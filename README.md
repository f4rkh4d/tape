# tape

deterministic record + replay runtime. capture any program's run once, then
replay it byte-identically forever — even if the clock has moved, the
random number generator is fresh, the file you read has been deleted, or
the env var has changed value.

> the bet: putting record-and-replay in the *language* — not in a debugger
> overlay — changes how people write programs. a program plus a trace is a
> reproducible run that you can hand to a colleague, attach to a bug
> report, or replay six months later and have it produce the same answer.

## the rationale demo

every team has a flaky test or a flaky job that fails one run in fifty,
nobody can reproduce it, and it ships to prod. tape makes that bug
catchable:

```sh
$ tape record flaky --out flaky.tape
ok: roll was 102, no flake this time
$ tape record flaky --out flaky.tape
ok: roll was 198, no flake this time
$ tape record flaky --out flaky.tape
FAIL: expected the answer to be 42, got 7 — this is the bug

$ tape replay flaky --trace flaky.tape
FAIL: expected the answer to be 42, got 7 — this is the bug
$ tape replay flaky --trace flaky.tape
FAIL: expected the answer to be 42, got 7 — this is the bug
```

the failure is now an artifact you can attach to a bug report. every
replay reproduces it byte-for-byte. fix the bug, re-record to verify the
new code does not fail on the same input, ship.

## install

clone and build for now. crates.io publish is queued (the `tape` name is
held by an unrelated tar-archive crate; the package will publish as
`tape-rt`).

```sh
git clone https://github.com/f4rkh4d/tape && cd tape
cargo build --release
./target/release/tape list
```

## what's in the box

```
tape list                                show built-in demo programs
tape record <program> [--out FILE]       run + record into FILE (default: trace.bin)
tape replay <program> --trace FILE       replay program against FILE
tape inspect <trace.bin>                 pretty-print the events in FILE
tape diff <a.tape> <b.tape>              show the first divergence between two traces
tape bench [--events N] [--effect KIND]  measure record / replay overhead
```

six demo programs ship in this build:

| program | effects exercised | what it shows |
|---|---|---|
| `dice` | clock + random + write | smallest happy path |
| `counter` | five writes | trace size for a tight write loop |
| `entropy` | random + write | "deterministic output from non-deterministic input" |
| `flaky` | random + write | the rationale: one run in 32 fails, replay reproduces it |
| `wordcount` | env + fs.read + write | reads a file, prints lines/words/chars |
| `greet` | env + write | env var → recorded → replay sees old value even after `export NAME=...` |

## the seven effects this build records

| effect | record | replay |
|---|---|---|
| `clock.now` | actual unix time | recorded value |
| `random.bits` | reads `/dev/urandom` | recorded bytes |
| `io.write` | writes to stdout | re-emits to stdout (same bytes) |
| `fs.read` | reads from disk | recorded contents (file may not exist) |
| `fs.write` | writes to disk | **does NOT touch disk** — destructive replay would be a footgun |
| `env.get` | reads env | recorded value (env may have changed) |
| `args.get` | reads argv | recorded argv |

every effect call records `(seq, site, kind, args, result)`. on replay we
match every call against the next event in the trace and abort if any
field has drifted. there is no "best effort" mode — drift always aborts.

## what makes replay refuse to run

| condition | when it fires |
|---|---|
| `EndOfTrace`        | the program tried more effect calls than the recording captured |
| `SiteMismatch`      | a call landed at a different source location (file:line:col) than the recording |
| `KindMismatch`      | site matches but the program now calls a different effect (e.g. `random.bits` where `clock.now` was recorded) |
| `ArgsMismatch`      | site + kind match but the args differ (e.g. `random_bits(100)` where the recording asked for `random_bits(8)`) |
| `CodeHashMismatch`  | any `.rs` file under `src/` was edited between record and replay |
| `UnsupportedSchema` | trace was made by a different schema version of tape |

every variant carries enough context (`seq`, `site`, expected vs got) to
identify the divergence without grepping. there is no silent recovery.

## try the negative case yourself

```sh
$ tape record dice --out dice.tape

# now edit src/programs.rs and change a number
$ vim src/programs.rs

$ cargo build --release
$ ./target/release/tape replay dice --trace dice.tape
tape replay: code hash mismatch — trace recorded against 87c5d6cc948aacf3…,
this build is 4f1e9c7a2b8d33e1…. you have edited a source file since the
recording.
```

the trace is well-formed. tape just refuses to replay it because the
program described by the trace is no longer the program running now.

## what `tape diff` looks like

```sh
$ tape diff a.tape b.tape
first divergence at seq 1:
  a.tape  random.bits site=0xa8e664b9 args=8b result=9b
  b.tape  random.bits site=0xa8e664b9 args=8b result=9b
  (same kind/site/args, different result — outside world answered differently)
```

when two recordings of the same code differ, the diff is the cheapest
debugging tool you have: it tells you whether the program took a different
path (Site/Kind), passed different inputs (Args), or simply got a
different answer from the world (Result).

## performance

apple m-series, `cargo build --release`, all numbers via `tape bench`.

| effect | record / event | replay / event | trace size / event |
|---|---:|---:|---:|
| `clock.now`   |  0.07 µs |  0.00 µs | 40 B |
| `random.bits` |  8.10 µs |  0.04 µs | 56 B |

replay is **≥200× faster** than record because it does not touch the OS:
no `/dev/urandom`, no `gettimeofday`, no syscalls at all. a one-hour
recording replays in seconds. trace size is dominated by `result` payloads
on data-heavy effects (random bytes, file reads); pure clock loops are
40 B/event.

reproduce with `tape bench --events 100000 --effect clock|random|write`.

## architecture

deeper write-up in [`docs/architecture.md`](docs/architecture.md). short
version: a program is a deterministic function of (its code, the answers
it got from effect calls). recording captures the answers; replay feeds
them back; mismatches between "what the program is asking now" and "what
the trace recorded" are caught at the first call that drifts.

```
src/
  lib.rs           re-exports + site!() macro + FNV-1a hashing
  event.rs         on-disk schema (Event, Header, Trace, EffectKind)
  error.rs         RecordErr, ReplayErr (six variants of drift)
  runtime.rs       Runtime trait — seven effects
  recording.rs     real OS calls + push to event log
  replaying.rs     match each call against next event; abort on drift
  programs.rs      built-in demo programs
  inspect.rs       human-readable trace dumper
  diff.rs          two-way trace comparator
  main.rs          cli dispatcher
build.rs           hashes src/**/*.rs at build time -> TAPE_CODE_HASH
tests/             16 tests across kernel + effects + diff
```

## what's intentionally missing

- **a syntax / parser / compiler.** programs are rust functions taking
  `&mut dyn Runtime`. the cli calls them directly. real syntax can layer
  on top later as a thin wrapper around the same Runtime trait.
- **actors / concurrency.** single-threaded only. trace format reserves
  room for `ActorSend` / `TaskSpawn` / `TaskEnd` event kinds; the design
  is sketched but not built. v0.2 territory.
- **network effects.** trait reserves the design space; only file +
  stdout are wired today.
- **soft / partial replay.** drift always aborts. a future flag could let
  a program handle drift instead of crashing.
- **smarter source hash.** a whitespace-only edit invalidates a trace.
  a hash that ignores comments and whitespace would be reasonable; not
  shipped yet.
- **trace compaction.** an hour of dense recording will produce a
  multi-megabyte trace. fine for v0.1; needs a streaming + chunked
  format before serious workloads.

## related work

- **`rr`** does record-replay at the linux x86 binary level. the
  granularity is system calls; the hosting language is anything that runs
  on linux. tape's bet is that putting record-replay one level up — at
  the language runtime — gives much smaller traces and lets you reason
  about effects as types, at the cost of needing a tape-aware runtime.
- **pernosco** is a beautiful UI on top of `rr` for omniscient debugging.
  same level as `rr`.
- **koka, frank, eff** are research languages with full algebraic effect
  systems. tape's effect set is intentionally tiny — record + replay is
  the property, the effects are the surface where determinism is enforced.

## license

MIT.
