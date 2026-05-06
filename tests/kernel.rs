//! kernel tests for tape: prove that record + replay are byte-identical
//! along the happy path, and that every flavour of "drift" between record
//! and replay is caught — never tolerated, never silent.
//!
//! the negative tests are the ones that actually matter. a passing happy
//! path is easy; what makes the runtime trustworthy is that the moment the
//! program stops being faithful to the recording, replay aborts with a
//! readable error.

use tape::{site, EffectKind, Recording, ReplayErr, Replaying, Runtime};

// the fixed program both record and replay run. the values it returns depend
// on the runtime's effect responses, so identical record→replay must produce
// identical outputs.
fn sample_program<R: Runtime>(rt: &mut R) -> (u64, Vec<u8>, usize) {
    let t = rt.now(site!());
    let r = rt.random_bits(site!(), 8);
    let n = rt.io_write(site!(), b"tape\n");
    (t, r, n)
}

#[test]
fn happy_path_record_then_replay_is_byte_identical() {
    let mut rec = Recording::new();
    let original = sample_program(&mut rec);
    let trace = rec.into_trace();

    // serialize + deserialize so the trace really survives a round trip
    // through bytes (which is what real users will do — write to disk).
    let bytes = bincode::serialize(&trace).expect("encode trace");
    let trace2: tape::Trace = bincode::deserialize(&bytes).expect("decode trace");

    let mut rep = Replaying::new(trace2).expect("schema accepted");
    let replayed = sample_program(&mut rep);

    assert_eq!(original, replayed, "record and replay diverged");
    assert_eq!(
        rep.position(),
        rep.len(),
        "replay didn't consume the full trace"
    );
}

#[test]
fn end_of_trace_error_when_program_calls_more_than_recorded() {
    // an empty recording produces an empty trace; any replay call should
    // immediately hit EndOfTrace.
    let trace = Recording::new().into_trace();
    let mut rep = Replaying::new(trace).unwrap();
    match rep.next_event(0xDEAD, EffectKind::ClockNow, &[]) {
        Err(ReplayErr::EndOfTrace { seq, site, kind }) => {
            assert_eq!(seq, 0);
            assert_eq!(site, 0xDEAD);
            assert_eq!(kind, EffectKind::ClockNow);
        }
        other => panic!("expected EndOfTrace, got {:?}", other),
    }
}

#[test]
fn site_mismatch_when_a_call_moves_to_a_different_source_location() {
    let mut rec = Recording::new();
    rec.now(0xAAAA_AAAA);
    let trace = rec.into_trace();

    let mut rep = Replaying::new(trace).unwrap();
    match rep.next_event(0xBBBB_BBBB, EffectKind::ClockNow, &[]) {
        Err(ReplayErr::SiteMismatch { expected, got, .. }) => {
            assert_eq!(expected, 0xAAAA_AAAA);
            assert_eq!(got, 0xBBBB_BBBB);
        }
        other => panic!("expected SiteMismatch, got {:?}", other),
    }
}

#[test]
fn kind_mismatch_when_program_calls_a_different_effect_at_the_same_site() {
    let mut rec = Recording::new();
    rec.now(0xC0DE_C0DE); // recorded as ClockNow
    let trace = rec.into_trace();

    // replay tries to call random_bits at the same site → kind mismatch.
    // args for random_bits is bincode of u64 len; doesn't matter what value.
    let args = bincode::serialize(&8u64).unwrap();
    let mut rep = Replaying::new(trace).unwrap();
    match rep.next_event(0xC0DE_C0DE, EffectKind::RandomBits, &args) {
        Err(ReplayErr::KindMismatch { expected, got, .. }) => {
            assert_eq!(expected, EffectKind::ClockNow);
            assert_eq!(got, EffectKind::RandomBits);
        }
        other => panic!("expected KindMismatch, got {:?}", other),
    }
}

#[test]
fn args_mismatch_when_call_at_same_site_now_passes_different_args() {
    let mut rec = Recording::new();
    rec.random_bits(0x1234_5678, 8); // recorded with args = 8
    let trace = rec.into_trace();

    let mut rep = Replaying::new(trace).unwrap();
    let bigger_args = bincode::serialize(&100u64).unwrap();
    match rep.next_event(0x1234_5678, EffectKind::RandomBits, &bigger_args) {
        Err(ReplayErr::ArgsMismatch { kind, .. }) => {
            assert_eq!(kind, EffectKind::RandomBits);
        }
        other => panic!("expected ArgsMismatch, got {:?}", other),
    }
}

#[test]
fn code_hash_mismatch_is_rejected_at_load_time() {
    // simulate a trace recorded against a different build of the source.
    // forge a trace whose header hash != the hash this build produced.
    let mut trace = tape::Trace::empty();
    trace.header.version = tape::Trace::SCHEMA_VERSION;
    trace.header.code_hash = [0xAA; 32]; // not all-zero (so the skip case
                                         // doesn't fire), not the real hash
    match Replaying::new(trace) {
        Err(ReplayErr::CodeHashMismatch { expected, got }) => {
            assert_eq!(got, [0xAA; 32]);
            assert_eq!(expected, tape::code_hash_bytes());
        }
        other => panic!("expected CodeHashMismatch, got {:?}", other),
    }
}

#[test]
fn unsupported_schema_is_rejected_at_load_time() {
    let mut bogus = tape::Trace::empty();
    bogus.header.version = 999;
    match Replaying::new(bogus) {
        Err(ReplayErr::UnsupportedSchema { expected, got }) => {
            assert_eq!(expected, tape::Trace::SCHEMA_VERSION);
            assert_eq!(got, 999);
        }
        other => panic!("expected UnsupportedSchema, got {:?}", other),
    }
}

// the canonical "сломай это сам" test: record a program with N effect
// calls, then simulate an "edited" program with N+1 calls and verify the
// extra call trips EndOfTrace at exactly the right step. uses the low-level
// next_event API so the test controls both site values explicitly — the
// site!() macro generates a different hash per source position which would
// otherwise contaminate the test.
#[test]
fn editing_the_program_to_call_an_extra_effect_breaks_replay() {
    let mut rec = Recording::new();
    rec.now(0x1111);
    rec.now(0x2222);
    let trace = rec.into_trace();

    let mut rep = Replaying::new(trace).unwrap();
    rep.next_event(0x1111, EffectKind::ClockNow, &[]).unwrap();
    rep.next_event(0x2222, EffectKind::ClockNow, &[]).unwrap();

    // edited program tries one more call — there is no third event recorded.
    match rep.next_event(0x3333, EffectKind::ClockNow, &[]) {
        Err(ReplayErr::EndOfTrace { seq, .. }) => {
            assert_eq!(seq, 2, "third call should look for event at index 2");
        }
        other => panic!("expected EndOfTrace at step 3, got {:?}", other),
    }
}
