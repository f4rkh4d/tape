use serde::{Deserialize, Serialize};

/// every kind of "ask the world a question" the runtime exposes.
/// adding a new effect = adding a variant here + a method on Runtime +
/// implementations in Recording and Replaying. the discriminant numbers are
/// part of the on-disk trace format and must never be reused or renumbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u16)]
pub enum EffectKind {
    ClockNow = 1,
    RandomBits = 2,
    IoWrite = 3,
    FsRead = 4,
    FsWrite = 5,
    EnvGet = 6,
    ArgsGet = 7,
}

impl EffectKind {
    pub fn name(&self) -> &'static str {
        match self {
            EffectKind::ClockNow => "clock.now",
            EffectKind::RandomBits => "random.bits",
            EffectKind::IoWrite => "io.write",
            EffectKind::FsRead => "fs.read",
            EffectKind::FsWrite => "fs.write",
            EffectKind::EnvGet => "env.get",
            EffectKind::ArgsGet => "args.get",
        }
    }
}

/// one effect call, recorded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub seq: u64,
    pub site: u32,
    pub kind: EffectKind,
    pub args: Vec<u8>,
    pub result: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    pub version: u32,
    pub started_at: i64,
    /// sha256 of source artifact. day-6 fence; for now it's all zeros.
    pub code_hash: [u8; 32],
}

/// how the recorded program ended. captured by the cli wrapper around the
/// program function; see `Recording::set_outcome`. replay reproduces effect
/// calls but the wrapper-level exit/panic happens after the last event, so
/// it lives here rather than as an Event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// program returned normally with this exit code.
    Exit(i32),
    /// program panicked. message is the downcast payload, location is the
    /// "file:line" string from the panic hook (best-effort, may be empty).
    Panic { message: String, location: String },
    /// recording ended without an explicit outcome being set. either the
    /// process was killed externally, or the caller used the library API
    /// without calling set_outcome.
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Footer {
    pub outcome: Outcome,
    /// number of events captured before the program ended.
    pub last_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub header: Header,
    pub events: Vec<Event>,
    pub footer: Footer,
}

impl Trace {
    /// bumped from 1 to 2 in v0.2: added Footer (outcome + last_seq).
    /// v1 traces are rejected with `UnsupportedSchema` -- by design,
    /// since they cannot tell replay how the program ended.
    pub const SCHEMA_VERSION: u32 = 2;

    pub fn empty() -> Self {
        Self {
            header: Header {
                version: Self::SCHEMA_VERSION,
                started_at: 0,
                code_hash: [0u8; 32],
            },
            events: Vec::new(),
            footer: Footer {
                outcome: Outcome::Aborted,
                last_seq: 0,
            },
        }
    }
}
