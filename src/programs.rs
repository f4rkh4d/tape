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
];

pub fn lookup(name: &str) -> Option<ProgramFn> {
    match name {
        "dice" => Some(dice),
        "counter" => Some(counter),
        "entropy" => Some(entropy),
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
