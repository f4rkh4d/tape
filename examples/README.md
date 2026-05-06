# examples

four committed traces produced by `tape record` against the built-in
demo programs. each one is a real binary trace you can replay,
inspect, or diff today, on your machine, against this exact build.
none of these are synthetic: every byte was emitted by the same
recording path that user programs use.

| file | program | what it captures |
| ---- | ------- | ---------------- |
| dice.tape | dice | one clock.now + one random.bits + one io.write |
| flaky.tape | flaky | a passing run of the rationale demo |
| flaky-failed.tape | flaky | a failing run (random.bits returned a "bug" byte). this is the trace you would attach to a CI failure report |
| greet.tape | greet | env.get("NAME") + io.write of the greeting |

## try it

    # see what's inside
    cargo run --release -- inspect examples/flaky-failed.tape

    # summary stats
    cargo run --release -- stats examples/flaky-failed.tape

    # replay the failure deterministically (will print the same FAIL line every time)
    cargo run --release -- replay flaky --trace examples/flaky-failed.tape

    # diff the passing vs failing run
    cargo run --release -- diff examples/flaky.tape examples/flaky-failed.tape

## why these files are committed

a record-replay runtime that ships without sample traces is asking the
reader to take its word that traces exist and round-trip. the files
here let you skip the recording step on first read and go straight to
"poke at a real trace". they are tiny (under 1 KB each) and rebuilt
deterministically from the source, so the cost of carrying them in
git is roughly zero.

## regenerating

if the source changes in a way that bumps the code hash, these traces
will fail to load with `CodeHashMismatch`. that is the system working
as intended. to refresh:

    cargo build --release
    ./target/release/tape record dice  --out examples/dice.tape
    ./target/release/tape record greet --out examples/greet.tape
    ./target/release/tape record flaky --out examples/flaky.tape
    # repeat the flaky record until you get a "FAIL" line, then save:
    cp trace.bin examples/flaky-failed.tape
