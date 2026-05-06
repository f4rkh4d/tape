use crate::error::ReplayErr;
use crate::event::{EffectKind, Trace};
use crate::runtime::Runtime;

/// runtime impl that does NOT touch the OS. every effect call is matched
/// against the next event in the trace. on any drift (site, kind, args), the
/// matching call panics with a ReplayErr — replay must abort the moment it
/// stops being faithful, otherwise the whole point is gone.
///
/// for tests and tooling that want to handle the error instead of panicking,
/// use the lower-level `next_event` API directly.
#[derive(Debug)]
pub struct Replaying {
    trace: Trace,
    idx: usize,
}

impl Replaying {
    /// build a replayer from a trace. validates schema version + code hash
    /// up front. we DO NOT defer either check to first call: a trace whose
    /// code or schema is wrong is a no-go before we even start.
    pub fn new(trace: Trace) -> Result<Self, ReplayErr> {
        if trace.header.version != Trace::SCHEMA_VERSION {
            return Err(ReplayErr::UnsupportedSchema {
                expected: Trace::SCHEMA_VERSION,
                got: trace.header.version,
            });
        }
        let expected_hash = crate::code_hash_bytes();
        // an all-zero hash means "skip this check" — used by tests that
        // construct traces directly via Trace::empty() without going through
        // Recording. real recordings always carry a non-zero hash.
        if trace.header.code_hash != [0u8; 32] && trace.header.code_hash != expected_hash {
            return Err(ReplayErr::CodeHashMismatch {
                expected: expected_hash,
                got: trace.header.code_hash,
            });
        }
        Ok(Self { trace, idx: 0 })
    }

    /// the heart of replay: advance to the next event and validate that the
    /// program's call matches what the recording made. on success, return
    /// the result bytes from the recording so the caller can deserialize.
    pub fn next_event(
        &mut self,
        site: u32,
        kind: EffectKind,
        args: &[u8],
    ) -> Result<&[u8], ReplayErr> {
        let Some(ev) = self.trace.events.get(self.idx) else {
            return Err(ReplayErr::EndOfTrace {
                seq: self.idx as u64,
                site,
                kind,
            });
        };

        if ev.site != site {
            return Err(ReplayErr::SiteMismatch {
                seq: ev.seq,
                expected: ev.site,
                got: site,
            });
        }
        if ev.kind != kind {
            return Err(ReplayErr::KindMismatch {
                seq: ev.seq,
                site,
                expected: ev.kind,
                got: kind,
            });
        }
        if ev.args.as_slice() != args {
            return Err(ReplayErr::ArgsMismatch {
                seq: ev.seq,
                site,
                kind,
            });
        }

        self.idx += 1;
        // SAFETY: we just advanced past idx; ev still borrows self.trace.events,
        // so the returned slice is valid until self is mutated again.
        let result = self.trace.events[self.idx - 1].result.as_slice();
        Ok(result)
    }

    /// number of events the replayer has consumed so far.
    pub fn position(&self) -> usize {
        self.idx
    }

    /// total number of events in the trace.
    pub fn len(&self) -> usize {
        self.trace.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.trace.events.is_empty()
    }
}

impl Runtime for Replaying {
    fn now(&mut self, site: u32) -> u64 {
        let result = self
            .next_event(site, EffectKind::ClockNow, &[])
            .unwrap_or_else(|e| panic!("replay: {e}"));
        bincode::deserialize(result).expect("malformed trace: clock.now result")
    }

    fn random_bits(&mut self, site: u32, len: usize) -> Vec<u8> {
        let args = bincode::serialize(&(len as u64)).expect("serialize len");
        let result = self
            .next_event(site, EffectKind::RandomBits, &args)
            .unwrap_or_else(|e| panic!("replay: {e}"));
        bincode::deserialize(result).expect("malformed trace: random.bits result")
    }

    fn io_write(&mut self, site: u32, buf: &[u8]) -> usize {
        let args = bincode::serialize(&buf.to_vec()).expect("serialize buf");
        let result = self
            .next_event(site, EffectKind::IoWrite, &args)
            .unwrap_or_else(|e| panic!("replay: {e}"));
        // intentionally re-emit to stdout during replay. the killer demo is
        // "record once, replay produces byte-identical output every time" —
        // hiding the output makes the demo opaque. semantically this is fine:
        // io.write is an external action; replay reproduces it. if you want
        // a quiet replay later, add a flag, don't change the default.
        use std::io::Write;
        let _ = std::io::stdout().write_all(buf);
        let _ = std::io::stdout().flush();
        bincode::deserialize::<u64>(result).expect("malformed trace: io.write result") as usize
    }
}
