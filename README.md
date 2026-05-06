# tape

deterministic record + replay runtime. weekend prototype.

a program in this model is a deterministic function from
1. its code, and
2. the sequence of values it received from the runtime's effect calls.

we record (2) into a trace. we replay against the trace and assert that every
effect call lands at the same source site, with the same effect kind, with
the same args. the moment the program drifts from the recording, replay
aborts with a readable error. that's the whole game.

## the killer demo

```
$ tape record dice --out dice.tape
at 1778050445s you rolled a 2
[tape] recorded 3 events (219 bytes) into dice.tape

$ tape replay dice --trace dice.tape
at 1778050445s you rolled a 2
[tape] replayed 3 / 3 events from dice.tape

$ tape replay dice --trace dice.tape
at 1778050445s you rolled a 2
[tape] replayed 3 / 3 events from dice.tape
```

byte-identical, every time. clock and randomness are no longer
non-deterministic — they are *recorded*.

## what's in the cli

```
tape list                                show built-in programs
tape record <program> [--out FILE]       run + record into FILE (default: trace.bin)
tape replay <program> --trace FILE       replay program against FILE
tape inspect <trace.bin>                 pretty-print the events in FILE
```

three demo programs ship in this build:

- `dice` — clock + 1 byte of randomness + a write to stdout
- `counter` — five writes via `io.write`, no randomness
- `entropy` — 64 bytes of randomness summed and printed

```
$ tape inspect dice.tape
== header ==
schema version: 1
started_at:     1778050445
code_hash:      87c5d6cc948aacf3…
events:         3

== events ==
 seq  kind         site        args_b   res_b  description
------------------------------------------------------------------------------
   0  clock.now    0x2f42227e       0       8  returned 1778050445s
   1  random.bits  0x6544b617       8       9  1 bytes of randomness
   2  io.write     0x5c10cc89      38       8  at 1778050445s you rolled a 2
```

## what makes replay refuse to run

every drift between record and replay falls into one of these. each one
has a precise error message — replay never proceeds silently with a wrong
assumption.

| condition | when it fires |
|---|---|
| `EndOfTrace`        | the program tried more effect calls than the recording captured |
| `SiteMismatch`      | a call landed at a different source location (file:line:col) than the recording |
| `KindMismatch`      | site matches but the program now calls a different effect (e.g. `random.bits` where `clock.now` was recorded) |
| `ArgsMismatch`      | site + kind match but the args differ (e.g. `random_bits(100)` where the recording asked for `random_bits(8)`) |
| `CodeHashMismatch`  | any `.rs` file under `src/` was edited between record and replay (full source tree hash, computed at build time) |
| `UnsupportedSchema` | trace was made by a different schema version of tape |

## try the negative case yourself

```
$ tape record dice --out dice.tape

# now edit src/programs.rs (e.g. swap a number)
$ vim src/programs.rs

# rebuild and try to replay the old trace
$ cargo build --release
$ ./target/release/tape replay dice --trace dice.tape
tape replay: code hash mismatch — trace recorded against 87c5d6cc948aacf3…,
this build is 4f1e9c7a2b8d33e1…. you have edited a source file since the
recording.
```

the trace is well-formed. tape just refuses to replay it because the
program described in the trace is no longer the program running now.

## architecture

```
src/
  lib.rs           re-exports + site!() macro + FNV-1a const fn
  event.rs         EffectKind, Event, Header, Trace (on-disk format lives here)
  error.rs         RecordErr, ReplayErr, Display impls
  runtime.rs       Runtime trait — three effects: now, random_bits, io_write
  recording.rs     real OS calls + push to event log
  replaying.rs     match each call against next event, abort on drift
  programs.rs      built-in demo programs (dice / counter / entropy)
  inspect.rs       pretty-printer for trace events
  main.rs          cli dispatcher
build.rs           hashes src/**/*.rs at build time -> TAPE_CODE_HASH
tests/kernel.rs    8 tests: happy path + 6 drift modes + 1 schema mismatch
```

## what's intentionally missing

- **a syntax / parser / compiler.** programs are rust functions taking
  `&mut dyn Runtime`. the cli calls them directly. real syntax can layer
  on top later as a thin wrapper around the same Runtime trait.
- **actors / concurrency.** single-threaded only for now. trace format
  reserves room for `ActorSend` / `TaskSpawn` / `TaskEnd` event kinds.
- **file / network effects.** trait reserves the design space; only
  stdout is wired today.
- **soft / partial replay.** drift always aborts. a future flag could let
  the program handle drift instead of crashing.
- **multi-build hash compatibility.** the source-file hash is conservative;
  a whitespace-only edit invalidates a trace. a smarter hash that ignores
  comment / whitespace changes is reasonable future work.

## why care

`rr` does this at the binary level on linux x86. `pernosco` puts a UI on
top of `rr`. they are wonderful and you cannot use them as a language
feature, only as a debugger overlay. the bet here is that putting record /
replay in the *language* — not in the debugger — changes how people write
programs.

programs become **artifacts**: a program plus a trace is a fully
reproducible run that you can hand to a colleague, attach to a bug
report, or replay six months later and have it produce the same answer.

## license

MIT.
