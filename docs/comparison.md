# tape vs prior art

record-replay is hard for one reason: a program's behavior depends on
everything it observes from the outside world, and the outside world is
not a small surface. every tool below picks a level at which to
intercept those observations, and the choice of level decides almost
everything else: trace size, what programs you can record, how loudly
replay fails, whether you need to recompile. rr intercepts at the
syscall boundary on linux. pernosco builds a UI on top of that.
hermit and antithesis sandbox the whole binary. replay.io and windbg
work at the vm or processor trace level. koka and pony work at the
language layer but for different reasons (effect typing, actor
isolation), not for record-replay. tape sits at the language-runtime
boundary: a small trait, seven methods, one event per call.

## rr (mozilla)

records linux x86-64 processes at the syscall + signal boundary, plus
shared-memory and rdtsc interception, with chunked checkpoints for
fast reverse-execution. strong at: recording arbitrary unmodified
binaries, reverse-step in gdb, stable enough that the firefox team
debugs production with it. weak relative to tape's design: the trace
is large (every syscall, every read of /proc, every signal), the
events are untyped bytes (you cannot ask "show me the random.bits
calls"), and replay reproduces the syscall stream rather than the
program's logical effect set. you would still pick rr when the program
is not yours, when you need to debug a real binary on linux without
rebuilding, or when you want reverse-step under gdb today. tape does
not give you any of those.

## pernosco

a hosted omniscient debugger that ingests rr traces and lets you query
program state at any moment, with cross-references for data flow.
strong at: the UI is the best thing in this space, and the
"omniscient" model (every value at every moment) is genuinely
different from step-debugging. weak relative to tape: same level as
rr (syscall granularity, large traces), and the value lives in the
hosted service rather than the trace format. you would still pick
pernosco when you have a hard production bug, an rr trace, and a
budget. tape is a runtime; pernosco is a debugger built on a
different runtime.

## hermit (facebook)

wraps a linux binary in a deterministic sandbox by intercepting
syscalls and replacing the nondeterministic ones (time, randomness,
scheduling) with deterministic substitutes. strong at: makes existing
programs reproducible without recompiling, and handles the threading
case that tape explicitly does not. weak relative to tape: hermit
gives you determinism, not record-replay - two runs of the same
hermit-wrapped program produce the same output, but there is no trace
artifact you can attach to a bug report and replay later against
modified code. you would still pick hermit when you need to make an
existing binary reproducible without changing its source, or when you
need deterministic concurrency today.

## antithesis

a hypervisor-level deterministic simulation platform that runs your
whole system (containers, network, storage) inside a custom hypervisor
and searches the state space for failures. strong at: finds bugs no
test suite would, replays the entire system not just one process,
covers networked distributed systems. weak relative to tape: enormous
operational footprint (you bring your system to their platform), the
trace is the entire vm state, and the unit of work is "a service
under load" not "a function call." you would still pick antithesis
when you are testing a distributed system and the bug requires
adversarial scheduling across nodes. tape records one program.

## replay.io

records browser sessions (chromium under instrumentation) so a
developer can scrub through a recorded user session and inspect
javascript state at any point. strong at: front-end debugging, the
"i recorded the bug, watch it happen" workflow for web apps. weak
relative to tape: tied to the browser runtime, the trace is a chromium
recording (not portable), and the surface is dom + js, not a typed
effect set. you would still pick replay.io when the bug is in a web
app and you want a shareable session recording.

## windbg time-travel debugging

windows-only, uses intel processor trace to record a user-mode process
and lets you step backward and forward in windbg. strong at: works on
unmodified windows binaries, integrates with the existing windbg
muscle memory of windows kernel and driver developers. weak relative
to tape: windows-only, requires intel pt or equivalent, traces are
large, and the replay surface is processor instructions rather than
typed effects. you would still pick ttd when debugging a windows
binary and you already live in windbg.

## koka, eff, frank

research languages with full algebraic effect systems: every effect
is a typed capability, handlers are first-class, the type system tracks
which effects each function can perform. strong at: the theory is
beautiful and the languages give you fine-grained control over which
parts of a program can do io. weak relative to tape: these systems
are about typing effects, not recording them. there is no built-in
trace format, no replay-against-old-trace, no drift detection.
you would still pick koka when the goal is to write code in an
effect-typed language and reason statically about effect usage. tape
borrows the word "effect" but the goal is determinism replay, not
type-level effect tracking.

## pony

actor-model language with reference capabilities that statically
prevent data races. strong at: concurrency safety by construction,
no shared mutable state, garbage collected per-actor. weak relative to
tape: pony is about safe concurrency, not recording. you cannot take
a pony program, record one run, and replay it later. you would still
pick pony when building a concurrent system from scratch and you want
the type system to enforce isolation. tape's concurrency story is
"not yet."

## where tape sits

tape records at the language-runtime boundary. higher than rr (which
sits at the syscall boundary and sees every getpid, every futex, every
read from /proc) and lower than koka (which types effects but does
not record them). the unit of recording is one method call on the
Runtime trait: seven kinds today (clock.now, random.bits, io.write,
fs.read, fs.write, env.get, args.get).

the tradeoff this buys: traces are small (40-56 bytes per event for
the simple effects), every event is typed (you can ask "show me the
random.bits calls" and get a real answer), and drift detection is
sharp (site mismatch, kind mismatch, args mismatch, code-hash
mismatch all named explicitly). the cost is that programs have to be
written against the Runtime trait. a stray std::time::SystemTime::now
call bypasses tape and replay drifts silently. the long-term answer
is a language frontend; the short-term discipline is "all effects
through rt."

honest weaknesses: no concurrency yet (single-threaded only, schema
reserves ActorSend / TaskSpawn / TaskEnd discriminants but the
runtime side is not built). no network effects yet. the source-hash
is conservative (any whitespace edit invalidates the trace). there is
no real syntax yet (programs are rust functions taking &mut dyn
Runtime). traces past a few minutes of dense activity will get
multi-megabyte before chunking lands.

pick tape today when: you have a short rust program, you can write
it against the Runtime trait, the bug is in how the program reacts to
clock or randomness or env or file contents, and you want a trace you
can attach to a bug report and replay six months later against
unmodified code. pick something else for unmodified binaries (rr,
hermit, ttd), distributed systems (antithesis), browser apps
(replay.io), or static effect typing (koka).

## faq

**why not just use rr?** rr is excellent and you should use it when
recording an unmodified linux binary is the goal. tape's bet is
different: a typed effect surface, much smaller traces, drift
detection that names the diverging effect by site and kind, and a
trace format that is portable across machines because it does not
record syscalls. the cost is that the program must go through the
Runtime trait. rr has no such requirement and never will.

**is this an effect system?** not in the koka sense. tape does not
do effect typing, effect handlers, or effect inference; the rust type
system does not know about tape's effects. what tape calls "effects"
is an effect *surface*: the set of methods on the Runtime trait at
which determinism is enforced and recording happens. it is the right
word for the role (every observation of the world goes through one of
these) but it is a runtime concept, not a type-system concept.

**can i use tape today?** yes for short rust programs that go through
the Runtime trait. the six demo programs (dice, counter, entropy,
flaky, wordcount, greet) record and replay byte-identically, drift is
detected, the cli works. no for arbitrary unmodified programs:
without a tape-aware runtime, there is nothing to record. no for
multi-threaded or networked programs: those effects are not built. if
your program fits the constraints, tape is real and works; if it does
not, the right tool is rr or hermit or antithesis depending on which
constraint you are breaking.
