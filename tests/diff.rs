//! integration tests for `tape::diff::render`. the diff function is the
//! debugging primitive: when two traces should match but don't, it must
//! point at the first divergence with a useful explanation.

use tape::{diff, site, Recording, Runtime};

fn record_dice() -> tape::Trace {
    let mut rec = Recording::new();
    let _ = rec.now(site!());
    let _ = rec.random_bits(site!(), 1);
    rec.io_write(site!(), b"hello\n");
    rec.into_trace()
}

#[test]
fn identical_traces_report_no_divergence() {
    let t = record_dice();
    let s = diff::render(&t, &t, "a", "b");
    assert!(s.contains("no divergence"), "got: {s}");
    assert!(s.contains("3 events"));
}

#[test]
fn different_traces_pinpoint_first_divergence() {
    // two recordings of the same code differ at random.bits result (random
    // bytes are different each run) and at clock.now (timestamps differ).
    // diff should point at seq 0 (clock.now) as the first divergence.
    let a = record_dice();
    std::thread::sleep(std::time::Duration::from_millis(1100)); // bump the clock by ≥1s
    let b = record_dice();

    let s = diff::render(&a, &b, "a", "b");
    assert!(s.contains("first divergence at seq"), "got: {s}");
    assert!(
        s.contains("different result")
            || s.contains("different sites")
            || s.contains("different effect kinds"),
        "diff must explain the kind of divergence; got: {s}"
    );
}

#[test]
fn unequal_lengths_show_which_side_has_extras() {
    let a = record_dice();
    let mut rec = Recording::new();
    let _ = rec.now(site!());
    let b = rec.into_trace();

    let s = diff::render(&a, &b, "a", "b");
    assert!(s.contains("extra event"), "got: {s}");
}
