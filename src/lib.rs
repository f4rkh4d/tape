//! tape — deterministic record + replay runtime, weekend prototype.
//!
//! a program in this model is a deterministic function from
//!   1. its code, and
//!   2. the sequence of values it received from the runtime's effect calls.
//!
//! we record (2) into a trace, then replay against the trace and assert that
//! every effect call lands at the same site, with the same kind, with the
//! same args. if any of those drift, replay aborts. that's the whole game.

pub mod diff;
pub mod error;
pub mod event;
pub mod inspect;
pub mod programs;
pub mod recording;
pub mod replaying;
pub mod runtime;

pub use error::{RecordErr, ReplayErr};
pub use event::{EffectKind, Event, Header, Trace};
pub use recording::Recording;
pub use replaying::Replaying;
pub use runtime::Runtime;

/// hex sha-256 of every .rs file under src/, computed at build time by build.rs.
/// recording embeds the bytes of this hash in the trace header; replay rejects
/// any trace whose hash doesn't match the current build.
pub const CODE_HASH_HEX: &str = env!("TAPE_CODE_HASH");

/// `CODE_HASH_HEX` decoded into 32 raw bytes. used by Recording / Replaying.
pub fn code_hash_bytes() -> [u8; 32] {
    let mut out = [0u8; 32];
    let s = CODE_HASH_HEX.as_bytes();
    let mut i = 0;
    while i < 32 {
        let hi = hex_nibble(s[i * 2]);
        let lo = hex_nibble(s[i * 2 + 1]);
        out[i] = (hi << 4) | lo;
        i += 1;
    }
    out
}

fn hex_nibble(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}

/// site identifier hashed from `file!()+line!()+column!()` at the call site.
/// stable within one cargo build of one source tree; intentionally unstable
/// across edits to source. that instability is what catches "you changed the
/// program and tried to replay an old trace" — the trace's `code_hash` (day 6)
/// catches the same thing at file granularity, this is the per-call cross-check.
#[macro_export]
macro_rules! site {
    () => {{
        const HASH: u32 = $crate::__site_hash(file!(), line!(), column!());
        HASH
    }};
}

/// FNV-1a 32-bit. const fn so the hash is a compile-time constant.
#[doc(hidden)]
pub const fn __site_hash(file: &str, line: u32, column: u32) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    let mut i = 0;
    let bytes = file.as_bytes();
    while i < bytes.len() {
        h ^= bytes[i] as u32;
        h = h.wrapping_mul(0x0100_0193);
        i += 1;
    }
    h ^= line;
    h = h.wrapping_mul(0x0100_0193);
    h ^= column;
    h.wrapping_mul(0x0100_0193)
}
