//! integration tests for `tape stats` and the inspect filter API. these
//! are pure renderer tests: they construct a Trace by hand and check the
//! output strings, so they don't depend on any program or runtime.

use tape::event::{EffectKind, Event, Trace};
use tape::{inspect, stats};

fn ev(seq: u64, site: u32, kind: EffectKind) -> Event {
    Event {
        seq,
        site,
        kind,
        args: vec![0u8; 4],
        result: vec![0u8; 8],
    }
}

fn fixture() -> Trace {
    let mut t = Trace::empty();
    t.events.push(ev(0, 0xA, EffectKind::ClockNow));
    t.events.push(ev(1, 0xB, EffectKind::IoWrite));
    t.events.push(ev(2, 0xB, EffectKind::IoWrite));
    t.events.push(ev(3, 0xC, EffectKind::RandomBits));
    t
}

#[test]
fn stats_counts_kinds_and_finds_hot_site() {
    let out = stats::render(&fixture());
    assert!(out.contains("events:          4"));
    assert!(out.contains("io.write"));
    assert!(out.contains("clock.now"));
    assert!(out.contains("random.bits"));
    // io.write appears twice -> should be at the top of the by-kind table
    let io_pos = out.find("io.write").unwrap();
    let clock_pos = out.find("clock.now").unwrap();
    assert!(
        io_pos < clock_pos,
        "io.write (2 calls) should rank above clock.now (1 call) in by-kind"
    );
    // hot site is 0xB (io.write x2)
    assert!(out.contains("0x0000000b"));
}

#[test]
fn inspect_filter_by_kind_drops_other_events() {
    let f = inspect::Filter {
        kind: Some(EffectKind::IoWrite),
        ..Default::default()
    };
    let out = inspect::render_filtered(&fixture(), &f);
    assert!(out.contains("(2 of 4 events shown)"));
    assert!(out.contains("io.write"));
    assert!(!out.contains("clock.now"));
    assert!(!out.contains("random.bits"));
}

#[test]
fn inspect_filter_by_site_and_since() {
    let f = inspect::Filter {
        site: Some(0xB),
        since: Some(2),
        ..Default::default()
    };
    let out = inspect::render_filtered(&fixture(), &f);
    // only seq=2 (site=0xB, kind=io.write) survives both filters
    assert!(out.contains("(1 of 4 events shown)"));
}

#[test]
fn inspect_limit_truncates() {
    let f = inspect::Filter {
        limit: Some(2),
        ..Default::default()
    };
    let out = inspect::render_filtered(&fixture(), &f);
    assert!(out.contains("(2 of 4 events shown)"));
}

#[test]
fn parse_kind_round_trips_every_variant() {
    for k in [
        EffectKind::ClockNow,
        EffectKind::RandomBits,
        EffectKind::IoWrite,
        EffectKind::FsRead,
        EffectKind::FsWrite,
        EffectKind::EnvGet,
        EffectKind::ArgsGet,
    ] {
        assert_eq!(inspect::parse_kind(k.name()), Some(k));
    }
    assert_eq!(inspect::parse_kind("nonsense"), None);
}
