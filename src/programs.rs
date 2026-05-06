//! demo programs the cli can record and replay. each one exercises a
//! different mix of effects so a viewer of the trace can tell at a glance
//! whether record + replay round-trip preserves the right shape of work.
//!
//! adding a new program: drop a new function below with signature
//! `fn(&mut dyn Runtime) -> i32`, then add an entry to `lookup` and `list`.

use crate::runtime::Runtime;
use crate::site;

pub type ProgramFn = fn(&mut dyn Runtime) -> i32;

/// every program advertised on the cli. tuple is (name, one-line doc).
pub const CATALOG: &[(&str, &str)] = &[
    ("dice", "roll a 6-sided die using clock + random + write"),
    ("counter", "count 1..5 to stdout via io.write"),
    (
        "entropy",
        "draw 64 bytes of randomness, hash via sum, print",
    ),
    (
        "flaky",
        "fails ~1 in 32 runs at random — the kind of bug you cannot reproduce until you record it",
    ),
    (
        "wordcount",
        "read TAPE_INPUT (default: README.md), count words, print result",
    ),
    ("greet", "read NAME from env, write a greeting to stdout"),
    (
        "heartbeat",
        "sleep + tick three times; replay finishes instantly",
    ),
];

pub fn lookup(name: &str) -> Option<ProgramFn> {
    match name {
        "dice" => Some(dice),
        "counter" => Some(counter),
        "entropy" => Some(entropy),
        "flaky" => Some(flaky),
        "wordcount" => Some(wordcount),
        "greet" => Some(greet),
        "heartbeat" => Some(heartbeat),
        _ => None,
    }
}

fn dice(rt: &mut dyn Runtime) -> i32 {
    let t = rt.now(site!());
    let r = rt.random_bits(site!(), 1)[0];
    let face = (r % 6) + 1;
    let line = format!("at {t}s you rolled a {face}\n");
    rt.io_write(site!(), line.as_bytes());
    0
}

fn counter(rt: &mut dyn Runtime) -> i32 {
    for i in 1..=5 {
        let line = format!("count {i}\n");
        rt.io_write(site!(), line.as_bytes());
    }
    0
}

fn entropy(rt: &mut dyn Runtime) -> i32 {
    let bytes = rt.random_bits(site!(), 64);
    // simple sum so the demo doesn't pull in a hash crate. the point is
    // "deterministic output from non-deterministic input", not the strength
    // of the digest.
    let sum: u64 = bytes.iter().map(|b| *b as u64).sum();
    let line = format!("64 random bytes summed to {sum}\n");
    rt.io_write(site!(), line.as_bytes());
    0
}

/// the rationale demo. fails ~3% of runs at random with a non-zero exit code.
/// without record/replay this is the kind of bug a CI pipeline shows once,
/// nobody can reproduce locally, and the team lives with as a known flake
/// for months. with tape: record the failing run, replay it on your laptop,
/// see the exact same failure with the exact same byte values, fix the bug,
/// re-run the test, never ship the bug again.
fn flaky(rt: &mut dyn Runtime) -> i32 {
    let r = rt.random_bits(site!(), 1)[0];
    // 8 of 256 possible byte values fail = 3.125% flake rate.
    let failed = r < 8;
    if failed {
        let line = format!(
            "FAIL: expected the answer to be 42, got {} — this is the bug\n",
            r
        );
        rt.io_write(site!(), line.as_bytes());
        1
    } else {
        let line = format!("ok: roll was {r}, no flake this time\n");
        rt.io_write(site!(), line.as_bytes());
        0
    }
}

/// real-world-shaped demo: read a file, do work on its contents, write
/// the answer to stdout. uses three effects: env.get (for the input path),
/// fs.read (the file), io.write (the result). on replay the file does not
/// have to exist — the recorded contents replay verbatim.
fn wordcount(rt: &mut dyn Runtime) -> i32 {
    let path = rt
        .env_get(site!(), "TAPE_INPUT")
        .unwrap_or_else(|| "README.md".to_string());
    match rt.fs_read(site!(), &path) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            let words = text.split_whitespace().count();
            let lines = text.lines().count();
            let chars = text.chars().count();
            let out = format!("{path}: {lines} lines, {words} words, {chars} chars\n");
            rt.io_write(site!(), out.as_bytes());
            0
        }
        Err(e) => {
            let out = format!("could not read {path}: {e}\n");
            rt.io_write(site!(), out.as_bytes());
            1
        }
    }
}

/// the time.sleep demo. records as ~600ms wall-clock; replays in milliseconds.
/// that gap is the point: when a real test sleeps 30s waiting for a flake to
/// surface, you record once and then iterate against the trace at full speed.
fn heartbeat(rt: &mut dyn Runtime) -> i32 {
    for i in 1..=3 {
        rt.time_sleep(site!(), 200);
        let t = rt.now(site!());
        let line = format!("tick {i} at {t}s\n");
        rt.io_write(site!(), line.as_bytes());
    }
    0
}

/// minimal env-aware demo. reads NAME, writes a hello.
fn greet(rt: &mut dyn Runtime) -> i32 {
    let name = rt
        .env_get(site!(), "NAME")
        .unwrap_or_else(|| "stranger".to_string());
    let out = format!("hello, {name}\n");
    rt.io_write(site!(), out.as_bytes());
    0
}
