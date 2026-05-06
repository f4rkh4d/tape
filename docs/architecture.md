# tape architecture

a longer companion to the README. covers the model, the on-disk format,
the drift detection contract, and the design decisions that won't be
obvious from reading the source.

## the model

a tape program is a function of two inputs:

1. **its code** (the `.rs` files under `src/`)
2. **the sequence of values it received from the runtime's effect calls**

everything else — local variables, control flow, arithmetic — is
determined by those two inputs. that's not a wishful constraint; it's
enforced by routing every observation of the outside world through the
`Runtime` trait. if a program calls `std::time::SystemTime::now()`
directly instead of `rt.now(site!())`, that call is invisible to tape
and replay will diverge silently. the long-term answer is a language
that makes such calls impossible to write; the short-term discipline is
"all effects through the runtime."

## the runtime trait

```rust
pub trait Runtime {
    fn now(&mut self, site: u32) -> u64;
    fn random_bits(&mut self, site: u32, len: usize) -> Vec<u8>;
    fn io_write(&mut self, site: u32, buf: &[u8]) -> usize;
    fn fs_read(&mut self, site: u32, path: &str) -> Result<Vec<u8>, String>;
    fn fs_write(&mut self, site: u32, path: &str, buf: &[u8]) -> Result<usize, String>;
    fn env_get(&mut self, site: u32, name: &str) -> Option<String>;
    fn args_get(&mut self, site: u32) -> Vec<String>;
}
```

seven methods. each one represents a class of value the program needs
from the world. adding a new method is a deliberate act: every new
effect needs (1) a method here, (2) an `EffectKind` discriminant, (3)
record + replay impls, (4) a story for what replay does (re-emit?
silence? error?), and (5) ideally a test.

the temptation to grow this trait into a fifty-method "do everything"
api should be resisted. the smaller the trait, the easier it is to
reason about determinism.

## why a `site` parameter on every method

`site` is a 32-bit hash of the source location of the call. it is the
runtime's answer to "is the program still calling this effect from the
same place?" two examples of why this matters:

```rust
// version 1
let t = rt.now(site!());        // site=0xAAAA
let r = rt.random_bits(site!(), 8); // site=0xBBBB

// version 2 (someone swapped the order)
let r = rt.random_bits(site!(), 8); // site=0xCCCC (different file:line:col)
let t = rt.now(site!());        // site=0xDDDD
```

without site tracking, replay would happily feed the v1 clock value to
v2's `random_bits`, and the program would proceed with garbage. with
site tracking, the very first replay call trips `SiteMismatch` and we
know exactly why.

`site!()` expands to a const FNV-1a hash of `file!()+line!()+column!()`
at compile time. it costs nothing at runtime.

## the trace

on disk: bincode v1.3 of a `Trace` struct.

```rust
struct Trace { header: Header, events: Vec<Event> }
struct Header { version: u32, started_at: i64, code_hash: [u8; 32] }
struct Event {
    seq:    u64,         // 0, 1, 2, ... strict order
    site:   u32,         // FNV-1a hash of file:line:col
    kind:   EffectKind,  // u16 discriminant; never reused after release
    args:   Vec<u8>,     // bincode-encoded inputs to the effect
    result: Vec<u8>,     // bincode-encoded outputs
}
```

`code_hash` is computed at build time by `build.rs`. it hashes every
`.rs` file under `src/` (path + null + content + null, sorted). this is
the conservative option: any source edit invalidates traces against
that build. a smarter scheme would skip whitespace and comment changes;
that's a future refinement, not v0.1.

`EffectKind` discriminants are part of the on-disk format. once a
release ships with `EffectKind::FsRead = 4`, that value is reserved
forever. removing or renumbering an effect breaks every trace recorded
against earlier versions.

## the replay contract

property R, formal version:

> given a trace `T` recorded from execution `E` of program `P`, replay
> of `T` against `P` yields execution `E'` such that, for every event
> `e` at index `k` in `T`, the corresponding effect call in `E'`
> satisfies:
>
>     e.kind == call.kind
>     e.site == call.site
>     e.args == call.args
>
> when satisfied, the runtime returns `e.result` to the call. when any
> field differs, replay aborts with `ReplayErr` naming the divergence.

note what's not in the contract: there is no claim that `E'` produces
the same wall-clock side effects (it doesn't; replay does not touch the
filesystem for `fs.write`). there is no claim that `E'` runs at the
same speed (it doesn't; replay is much faster). the property is about
the program's view of the world: every observation matches the recorded
one, in order.

## why replay aborts on drift instead of recovering

the alternative — "best effort" replay that tries to fall through
mismatches — defeats the property. the moment replay tolerates one
drift, the user has to wonder whether the value the program just
received is real or fabricated, and the trace is no longer a faithful
record of the original run. drift is a hard failure.

soft replay (programs that explicitly handle drift, like a fuzzer) is a
reasonable feature but it must be opt-in and explicit, not the default.

## why `fs.write` does not write during replay

the most painful potential footgun in this whole design. consider:

```rust
// recorded six months ago against a now-stale trace
fs.write("/etc/important-config", b"old contents");
```

replay should not reproduce that write. the recorded contents are
stale; if tape silently overwrote `/etc/important-config` with the
old payload during replay, that is "destructive replay" and the worst
property you could give a record-replay tool. so `fs.write` during
replay is silenced: the recorded result is returned, the disk is not
touched.

`io.write` (stdout) is different — it's transient, tied to the current
terminal, and re-emitting matches the killer-demo expectation
("byte-identical output every time"). when `io.write` grows into a
broader `io.stream` family that includes file handles, that distinction
will need to be expressed in the API; for v0.1 the line is simply
"stdout re-emits, anything persistent does not."

## why bincode v1, not v2 or CBOR

v0.1 uses bincode 1.3. bincode is deterministic by default for the
types used here (no float weirdness, no map ordering surprises). CBOR
canonical mode is more carefully designed for cross-language interop
but requires more discipline to use deterministically. when tape grows
beyond rust callers, switching to canonical CBOR (with explicit schema
versioning) is the logical move; for now the simpler choice ships.

## what's not here yet (and why)

**actors / concurrency.** single-threaded for v0.1. a deterministic
replay of multi-task code requires recording the inter-task message
order, which lifts the trace from "ordered list of effects" to
"interleaved log of effects + messages." the schema reserves the
discriminants (`ActorSend`, `TaskSpawn`, `TaskEnd`) but the runtime
side hasn't been written. v0.2.

**network.** every network round trip would be an effect; the design
is straightforward but the surface is big (TCP vs UDP, blocking vs
async, partial reads, timeouts). when this lands, it'll come with a
specific demo program and a careful set of effects, not a kitchen-sink
"net.*" namespace.

**a real language.** `tape` programs today are rust functions. the
language frontend is a separate project that will compile to the same
`Runtime` trait. building it before the runtime is solid would be
backwards.

**partial replay / time-travel debugging.** replay runs from event 0 to
the end. nothing stops you from stopping mid-replay and inspecting
state, but there's no UI for it yet. omniscient debugging on top of
tape is a clear future direction; pernosco-on-top-of-rr is the
reference here.

**trace compaction.** at ~50 bytes/event, a million-event trace is
50MB. that's fine for short workloads, painful for hour-long ones. a
chunked + compressed format is a future change.

## design decisions worth knowing

1. **panic on drift in trait methods, return `Result` from
   `next_event`.** the trait surface is ergonomic (you call
   `rt.now(site!())` like any other function); the lower-level
   `Replaying::next_event(...)` returns a typed error and is what tests
   use. ergonomic for users, recoverable for tools.

2. **every event has a unique `seq` even though it's just the index.**
   redundant on the wire but useful in error messages: "drift at seq
   147" is more readable than "drift at index 147 of an array".

3. **schema_version of 1 today, with explicit `UnsupportedSchema`
   error.** when v2 lands the runtime will refuse v1 traces by default
   and offer a `migrate` subcommand. the schema discriminant lives in
   the header for exactly this reason.

4. **`code_hash = [0; 32]` is treated as "skip this check".** tests
   that construct traces directly via `Trace::empty()` need this
   escape hatch. real recordings always carry a non-zero hash.

5. **the trait does not return `Result` from any effect method.**
   effects that can fail (file read, file write) return `Result<T,
   String>` from the method itself; the runtime doesn't add an outer
   `Result` for "replay failed". that ergonomic choice means programs
   look natural and replay errors come up as panics that the cli
   catches.
