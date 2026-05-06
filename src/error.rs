use crate::event::EffectKind;
use std::fmt;

#[derive(Debug)]
pub enum RecordErr {
    Encode(bincode::Error),
    Io(std::io::Error),
}

impl fmt::Display for RecordErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RecordErr::Encode(e) => write!(f, "encode error: {e}"),
            RecordErr::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl std::error::Error for RecordErr {}
impl From<bincode::Error> for RecordErr {
    fn from(e: bincode::Error) -> Self {
        RecordErr::Encode(e)
    }
}
impl From<std::io::Error> for RecordErr {
    fn from(e: std::io::Error) -> Self {
        RecordErr::Io(e)
    }
}

/// every way replay can detect that "the program is no longer faithful to the
/// recorded trace". each variant carries enough context to print a useful
/// error message: which seq, which site, what was expected, what we got.
#[derive(Debug)]
pub enum ReplayErr {
    /// trace ran out of events but the program tried to make another call.
    EndOfTrace {
        seq: u64,
        site: u32,
        kind: EffectKind,
    },
    /// the program tried an effect at the right step but at a different
    /// source location than the recording. usually means code edits between
    /// record and replay.
    SiteMismatch { seq: u64, expected: u32, got: u32 },
    /// the program reached the right step + site but called a different kind
    /// of effect (e.g. recording asked the clock, replay asks for random).
    KindMismatch {
        seq: u64,
        site: u32,
        expected: EffectKind,
        got: EffectKind,
    },
    /// site + kind match, but the args differ. that means the program is
    /// asking a *different* question, so the recorded answer is no longer
    /// valid — refusing to replay it as if it were.
    ArgsMismatch {
        seq: u64,
        site: u32,
        kind: EffectKind,
    },
    /// the trace's schema version is not what this build of tape supports.
    UnsupportedSchema { expected: u32, got: u32 },
    /// the trace was recorded against a different build of the source tree.
    /// edits to any .rs file under src/ between record and replay land here.
    /// the trace itself is well-formed; we just refuse to replay it because
    /// the program it described is not the program running now.
    CodeHashMismatch { expected: [u8; 32], got: [u8; 32] },
    /// bincode failed to decode trace bytes. trace is corrupt or wrong format.
    Decode(bincode::Error),
}

impl fmt::Display for ReplayErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReplayErr::EndOfTrace { seq, site, kind } => write!(
                f,
                "trace ended at seq {seq}, but the program then called {} at site {site:#010x}",
                kind.name()
            ),
            ReplayErr::SiteMismatch { seq, expected, got } => write!(
                f,
                "at seq {seq}: site mismatch — trace recorded {expected:#010x}, program now at {got:#010x}"
            ),
            ReplayErr::KindMismatch { seq, site, expected, got } => write!(
                f,
                "at seq {seq}, site {site:#010x}: kind mismatch — trace has {}, program now calls {}",
                expected.name(),
                got.name()
            ),
            ReplayErr::ArgsMismatch { seq, site, kind } => write!(
                f,
                "at seq {seq}, site {site:#010x}, kind {}: args differ from the recorded call",
                kind.name()
            ),
            ReplayErr::UnsupportedSchema { expected, got } => write!(
                f,
                "trace schema {got} not supported (this build expects {expected})"
            ),
            ReplayErr::CodeHashMismatch { expected, got } => {
                let exp_hex: String = expected.iter().take(8).map(|b| format!("{b:02x}")).collect();
                let got_hex: String = got.iter().take(8).map(|b| format!("{b:02x}")).collect();
                write!(
                    f,
                    "code hash mismatch — trace recorded against {exp_hex}…, this build is {got_hex}…. you have edited a source file since the recording."
                )
            }
            ReplayErr::Decode(e) => write!(f, "decode error: {e}"),
        }
    }
}

impl std::error::Error for ReplayErr {}
impl From<bincode::Error> for ReplayErr {
    fn from(e: bincode::Error) -> Self {
        ReplayErr::Decode(e)
    }
}
