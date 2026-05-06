//! the trace footer captures how the recorded program ended: clean exit,
//! panic, or aborted. these tests exercise the library API directly; the
//! cli wraps prog() in catch_unwind and feeds the result through
//! `set_outcome`. tests below stand in for that wrapper.

use tape::event::Outcome;
use tape::Recording;

#[test]
fn into_trace_without_set_outcome_records_aborted() {
    let rec = Recording::new();
    let trace = rec.into_trace();
    assert_eq!(trace.footer.outcome, Outcome::Aborted);
    assert_eq!(trace.footer.last_seq, 0);
}

#[test]
fn set_outcome_exit_is_persisted_in_trace_footer() {
    let mut rec = Recording::new();
    rec.set_outcome(Outcome::Exit(42));
    let trace = rec.into_trace();
    assert_eq!(trace.footer.outcome, Outcome::Exit(42));
}

#[test]
fn set_outcome_panic_carries_message_and_location() {
    let mut rec = Recording::new();
    rec.set_outcome(Outcome::Panic {
        message: "boom".to_string(),
        location: "src/foo.rs:10".to_string(),
    });
    let trace = rec.into_trace();
    match trace.footer.outcome {
        Outcome::Panic { message, location } => {
            assert_eq!(message, "boom");
            assert_eq!(location, "src/foo.rs:10");
        }
        other => panic!("expected Panic, got {:?}", other),
    }
}

#[test]
fn last_seq_in_footer_matches_event_count() {
    use tape::Runtime;
    let mut rec = Recording::new();
    rec.now(0xC0DE_0001);
    rec.now(0xC0DE_0002);
    rec.now(0xC0DE_0003);
    rec.set_outcome(Outcome::Exit(0));
    let trace = rec.into_trace();
    assert_eq!(trace.footer.last_seq, 3);
    assert_eq!(trace.events.len(), 3);
}
