use crate::event::{EffectKind, Event, Header, Trace};
use crate::runtime::Runtime;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

/// runtime impl that calls the real OS for every effect AND saves the call
/// into a growing trace. when the program returns, hand the trace off via
/// `into_trace()` — that's the artifact replay needs.
pub struct Recording {
    next_seq: u64,
    events: Vec<Event>,
    started_at: i64,
}

impl Recording {
    pub fn new() -> Self {
        let started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Self {
            next_seq: 0,
            events: Vec::new(),
            started_at,
        }
    }

    /// finish recording and return the trace. callers serialize it to disk.
    pub fn into_trace(self) -> Trace {
        Trace {
            header: Header {
                version: Trace::SCHEMA_VERSION,
                started_at: self.started_at,
                code_hash: crate::code_hash_bytes(),
            },
            events: self.events,
        }
    }

    /// helper: encode args + result, push an Event, bump seq.
    fn record(&mut self, site: u32, kind: EffectKind, args: Vec<u8>, result: Vec<u8>) {
        self.events.push(Event {
            seq: self.next_seq,
            site,
            kind,
            args,
            result,
        });
        self.next_seq += 1;
    }
}

impl Default for Recording {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime for Recording {
    fn now(&mut self, site: u32) -> u64 {
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // args = empty; result = bincode(t)
        let result = bincode::serialize(&t).expect("serialize u64");
        self.record(site, EffectKind::ClockNow, Vec::new(), result);
        t
    }

    fn random_bits(&mut self, site: u32, len: usize) -> Vec<u8> {
        // for the weekend MVP we use /dev/urandom if available, otherwise we
        // fall back to a pid-mixed counter. either way the bytes go straight
        // into the trace, so replay doesn't care which source we used.
        let mut buf = vec![0u8; len];
        let read_from_urandom = std::fs::File::open("/dev/urandom")
            .and_then(|mut f| {
                use std::io::Read;
                f.read_exact(&mut buf).map(|_| true)
            })
            .unwrap_or(false);
        if !read_from_urandom {
            // fallback: time + pid mixed; not cryptographic, but deterministic
            // enough that the trace will faithfully replay it.
            let seed = (std::process::id() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                ^ self.next_seq.wrapping_mul(0xD1B5_4A32_D192_ED03);
            for (i, b) in buf.iter_mut().enumerate() {
                *b = ((seed >> ((i % 8) * 8)) & 0xff) as u8;
            }
        }
        let args = bincode::serialize(&(len as u64)).expect("serialize len");
        let result = bincode::serialize(&buf).expect("serialize random buf");
        self.record(site, EffectKind::RandomBits, args, result);
        buf
    }

    fn io_write(&mut self, site: u32, buf: &[u8]) -> usize {
        let n = std::io::stdout().write(buf).unwrap_or(0);
        let _ = std::io::stdout().flush();
        let args = bincode::serialize(&buf.to_vec()).expect("serialize buf");
        let result = bincode::serialize(&(n as u64)).expect("serialize n");
        self.record(site, EffectKind::IoWrite, args, result);
        n
    }
}
