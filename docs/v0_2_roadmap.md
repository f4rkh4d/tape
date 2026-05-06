# v0.2 roadmap

v0.1 is honest about what it is: a weekend prototype that proves the
property "every effect call lands at the same site, kind, and args, or
replay aborts." it does that for seven effects on one machine in one
language. the roadmap below is what would push tape from "demo that
proves the idea" to "tool you'd actually adopt for a side project."
nothing in v0.1 needs to be redone; v0.2 is purely additive.

the headings are ordered by what unlocks the most new use cases per
unit of work, not by what's easiest. dependencies between items are
called out where they matter.

## 1. stable, versioned, portable trace format

**status today.** v0.1 serializes Trace via bincode v1.3 and uses a
build-time sha256 of `src/` as the fence. the bincode encoding is
fine on a single machine for a single build, but it is not stable
across rust toolchain changes, and the code-hash fence is intentionally
strict: any source edit invalidates every existing trace.

**what v0.2 does.**
- pin the on-disk schema to an explicit binary layout (see
  `docs/architecture.md` for the existing field list). bincode stays
  as the encoder, but the byte layout becomes a documented contract,
  not a side effect of the rust struct order.
- separate "schema version" (already present, currently 1) from "code
  hash" (already present) and add an optional "compat hash" that
  hashes only the runtime trait surface (effect names + signatures),
  not the whole `src/` tree. recordings that touch only effect code
  the trait describes can replay across most non-trait edits.
- add a tiny migration test: every supported old schema version round-
  trips to current via a documented upgrade path or is rejected with
  a clear `UnsupportedSchema` error.

**why first.** every other item depends on traces being durable. if a
trace from monday's build can't be opened on friday, nothing else is
worth building on top.

## 2. signal handling and panic capture

v0.1 records seven effects but a real program also receives
asynchronous events: SIGTERM, panics, OOM kills. rr handles signals
explicitly because without them the trace loses information that
explains why the program ended. tape should at minimum record:

- the exit status of the recorded program (clean exit code vs panic
  vs signal)
- in the panic case, the panic message and the location of the panic
- the seq number of the last effect call before exit, so replay can
  abort at the same logical point even if the program's main function
  never returned

this is small in code (one `Header` field, a panic hook, a wrapper
around `main`) and large in usefulness for the rationale demo: a
crashed run is exactly the kind of run you want to replay.

## 3. trace stats and indexing layer

`tape stats` (shipped in v0.1.1) is a one-pass scan. for traces with
millions of events, you want a precomputed index: per-site call
counts, per-kind histograms, byte-size distributions. pernosco and
windbg ttd both invest heavily in a separate index pass that runs
after recording; the recording stays cheap, the index is built once
and reused for every query.

v0.2 introduces `tape index <trace>` which writes a sibling
`<trace>.idx` file containing:
- counts per (site, kind) pair
- offsets of each event in the trace, so random access to event N
  becomes O(1) instead of O(N)
- a content hash of the trace so the index can detect a mismatched
  pairing

`tape stats` and `tape inspect` then prefer the index when present
and fall back to a linear scan when it is absent.

## 4. a small replay-side debugger

today, on a drift, replay panics with a `ReplayErr` that says "at seq
N we expected site X kind Y, got site X' kind Y'." that is the right
information at the right moment, but it lands in stderr. v0.2 adds an
opt-in REPL: `tape replay <program> --trace t.bin --interactive`
catches the drift and drops you into a tiny prompt where you can
- print the next K events from the trace
- print the program's call site context for the divergent call (file,
  line, function name) using DWARF lookups
- step forward one event at a time
- compare args byte-for-byte

this borrows the *shape* of pernosco / windbg ttd interaction while
staying inside a process: no client-server, no separate UI, no
network. the implementation is a `--features repl` cargo feature so
the default build stays small.

## 5. effect surface: the next two

seven effects are enough to demonstrate the idea but not to record a
realistic backend service. v0.2 adds:

- **net.http**: a single inbound or outbound http request modeled as
  one effect call with method + url + headers + body in args, status
  + headers + body in result. this turns tape into a useful tool for
  reproducing api flake.
- **time.sleep**: explicit duration arg, empty result. paired with
  `clock.now`, this makes time-virtualized replay possible: the
  recorded sleep returns immediately, and the program's notion of
  "now" advances by the recorded sleep duration.

deliberately not on the list yet: tcp sockets, file watching, threads.
those require deeper changes to the runtime model and belong in v0.3+.

## 6. chaos mode

every effect call already goes through the runtime trait, which means
the runtime can perturb results without the program noticing. a chaos
runtime returns valid-looking but adversarial values: empty argv, env
vars that exist but are empty strings, fs.read returning truncated
data, random.bits returning all zeros. record once with chaos enabled,
get a trace that captures a specific perturbation; replay deterministic
ally.

this is the cheapest way to find error-handling bugs in code that
already uses tape. antithesis built a whole company around the chaos-
on-deterministic-replay angle; tape inherits the substrate for free.

## 7. crates.io publish

`tape` on crates.io is taken by a tar archive library. the published
crate name will be `tape-rt`; the binary stays `tape`. before publish:

- bump version to 0.2.0 once items 1-3 land
- ship a small README example that builds against the published crate
  rather than against the workspace
- run `cargo publish --dry-run` and verify the package contains
  `examples/` and `docs/` but not `target/` or trace files outside
  examples/

## what's deliberately not in v0.2

- **windows**: works in principle (no platform-specific code outside
  /dev/urandom fallback) but untested. v0.3.
- **multithreaded recording**: each thread would need its own seq
  counter and the merge order would matter for replay. this is a real
  design problem, not a port. v0.3 or later.
- **language runtimes other than rust**: the trait is rust-shaped. a
  c abi or wasm shim is an interesting v1.0 question, not a v0.2
  question.
- **gui**: explicitly not. the cli plus an optional repl is the whole
  surface. if someone wants a web ui they can build it on top of
  `tape inspect --json` (which v0.2 will also add as a flag, since
  json output is a one-day item that unlocks any external tooling).

## tracking

each item above maps to a github issue under the `v0.2` milestone.
items 1-3 are blocking; 4-7 are independently shippable in any order
once the schema is stable.
