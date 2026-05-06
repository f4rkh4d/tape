# tape cookbook

short recipes for things people actually want to do with `tape`. each one
assumes you have built the binary (`cargo build --release`) and that
`./target/release/tape` is on your path or aliased to `tape`.

## reproduce a flaky failure

a test fails one run in fifty, ci shows it once, nobody can reproduce
locally. record the failing run; the trace becomes a deterministic
reproducer.

```sh
$ tape record flaky --out fail.tape
FAIL: expected the answer to be 42, got 7 -- this is the bug
$ tape replay flaky --trace fail.tape
FAIL: expected the answer to be 42, got 7 -- this is the bug
$ tape replay flaky --trace fail.tape   # forever
FAIL: expected the answer to be 42, got 7 -- this is the bug
```

`flaky` fails about 3% of recordings. you may need to run `record` a
handful of times before you catch one. once captured, the failure is an
artifact: replay reproduces it byte-for-byte.

## replay a trace recorded against an older build

a colleague hands you a `.tape` file from six months ago. if any `.rs`
file under `src/` has changed since, replay refuses.

```sh
$ tape replay dice --trace old.tape
tape replay: code hash mismatch -- trace recorded against 87c5d6cc...,
this build is 4f1e9c7a.... you have edited a source file since the
recording.
```

the trace is fine. tape is telling you the program described by the
trace is no longer the program running now. check out the commit that
matches the recorded hash, rebuild, replay there.

## find the call site responsible for the most events

a long trace, you want to know where the volume is coming from. `tape
stats` groups events by kind and site.

```sh
$ tape stats trace.bin
== summary ==
events:          12043
== events by kind ==
io.write   8001
random.bits 4002
clock.now    40
== hot sites ==
0xa8e664b9  random.bits   4002
0x1f3c20d1  io.write      8000
```

the site hash is fnv-1a of `file:line:col`. feed it back to `tape
inspect --site` to see the actual events.

## see only the io.write calls

`tape inspect` dumps every event by default, which is too much for
anything but a tiny trace. filter by kind.

```sh
$ tape inspect trace.bin --filter io.write --limit 20
seq=3   site=0x1f3c20d1  io.write   args=12b  result=12b  "count 1\n"
seq=4   site=0x1f3c20d1  io.write   args=12b  result=12b  "count 2\n"
...
```

`--filter` accepts any of `clock.now`, `random.bits`, `io.write`,
`fs.read`, `fs.write`, `env.get`, `args.get`. combine with `--site
0x...`, `--since N` (seq), `--limit N` to narrow further.

## diff two runs of the same program

you recorded the same program twice and got different output. diff
tells you whether the divergence is in the program's path (different
site or kind), its inputs (different args), or in what the world
returned (same args, different result).

```sh
$ tape diff a.tape b.tape
first divergence at seq 1:
  a.tape  random.bits site=0xa8e664b9 args=8b result=9b
  b.tape  random.bits site=0xa8e664b9 args=8b result=11b
  (same kind/site/args, different result -- outside world answered differently)
```

exit code is 0 if the traces are identical, 1 otherwise. cheap enough
to drop into ci.

## replay on a machine where the input file is gone

`wordcount` reads the file at `$TAPE_INPUT`. record once; the contents
are now in the trace. replay anywhere -- the file does not have to
exist on the replay machine.

```sh
$ TAPE_INPUT=/tmp/notes.txt tape record wordcount --out wc.tape
/tmp/notes.txt: 14 lines, 87 words, 503 chars
$ rm /tmp/notes.txt
$ tape replay wordcount --trace wc.tape
/tmp/notes.txt: 14 lines, 87 words, 503 chars
```

same applies to `greet` reading `NAME`: change the env var between
record and replay; replay sees the recorded value, not the new one.
this is the point.

## attach a trace to a bug report

a `.tape` file is a few kilobytes for short programs and self-contained
(bincode, no external refs). commit it next to the test that produced
it, or paste it into the issue.

```sh
$ tape record flaky --out examples/flaky-failed.tape
$ git add examples/flaky-failed.tape
$ git commit -m "repro: flaky fails when r < 8"
```

anyone who clones the repo and runs `tape replay flaky --trace
examples/flaky-failed.tape` against the same build sees the exact same
failure. that's the artifact.

## benchmark record vs replay overhead

`tape bench` runs a synthetic loop of one effect kind and reports
per-event timing plus trace size. useful for sanity-checking that
nothing has gotten 10x slower since the last release.

```sh
$ tape bench --events 100000 --effect clock
tape bench: effect=clock, events=100000
  record:  7 ms   (0.07 µs/event)
  replay:  0 ms   (0.00 µs/event)
  trace:   3906.2 KiB  (40.0 bytes/event)
```

`--effect` accepts `clock`, `random`, or `write`. replay runs much
faster than record because it does no syscalls -- no `/dev/urandom`,
no `gettimeofday`, nothing. a one-hour recording replays in seconds.

## see also

- [architecture.md](architecture.md) -- the model, the trace format, why
  drift aborts instead of recovering.
- [comparison.md](comparison.md) -- where tape sits relative to rr,
  pernosco, hermit, antithesis, replay.io, koka.
- [v0_2_roadmap.md](v0_2_roadmap.md) -- what's coming next (concurrency,
  network, smarter source hash).
