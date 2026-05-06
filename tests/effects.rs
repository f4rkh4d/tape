//! integration tests for the seven effects: every one round-trips through
//! record + replay byte-for-byte. for fs.write the test specifically verifies
//! that replay does NOT touch the real filesystem (a destructive replay
//! would be a footgun).

use tape::{EffectKind, Recording, Replaying, Runtime};

// fixed site values used by tests below. real programs go through site!()
// which gives a per-source-position hash; tests use literals so record and
// replay calls stay at the same site without depending on whitespace.
const SITE_FS_READ: u32 = 0xA000_0001;
const SITE_FS_WRITE: u32 = 0xA000_0002;
const SITE_ENV: u32 = 0xA000_0003;
const SITE_ARGS: u32 = 0xA000_0004;
const SITE_RANDOM: u32 = 0xA000_0005;

// (no helper here — each test is small enough to inline its own setup,
// and the effects tests need explicit control over both recording and
// replay phases anyway.)

#[test]
fn fs_read_returns_recorded_bytes_even_if_file_disappears() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("tape-test-fsread-{}.txt", std::process::id()));
    std::fs::write(&path, b"hello tape").unwrap();
    let path_str = path.to_str().unwrap().to_string();

    let mut rec = Recording::new();
    let recorded = rec.fs_read(SITE_FS_READ, &path_str).expect("read ok");
    assert_eq!(recorded, b"hello tape");

    // delete the real file. replay should still return the recorded bytes.
    std::fs::remove_file(&path).unwrap();

    let trace = rec.into_trace();
    let mut rep = Replaying::new(trace).unwrap();
    let replayed = rep.fs_read(SITE_FS_READ, &path_str).expect("replay ok");
    assert_eq!(
        replayed, b"hello tape",
        "replay must use recorded bytes, not the real (now-missing) file"
    );
}

#[test]
fn fs_write_during_replay_does_not_touch_the_filesystem() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("tape-test-fswrite-{}.txt", std::process::id()));
    let path_str = path.to_str().unwrap().to_string();
    let _ = std::fs::remove_file(&path); // pre-clean

    let mut rec = Recording::new();
    rec.fs_write(SITE_FS_WRITE, &path_str, b"recorded payload")
        .expect("write ok");
    assert!(path.exists(), "recording must actually write to disk");
    assert_eq!(std::fs::read(&path).unwrap(), b"recorded payload");

    // delete the file, then replay. the file MUST stay deleted.
    std::fs::remove_file(&path).unwrap();
    let trace = rec.into_trace();
    let mut rep = Replaying::new(trace).unwrap();
    let n = rep
        .fs_write(SITE_FS_WRITE, &path_str, b"recorded payload")
        .unwrap();
    assert_eq!(n, "recorded payload".len());
    assert!(
        !path.exists(),
        "replay must NOT recreate the file; destructive replay would be the worst footgun"
    );
}

#[test]
fn env_get_replays_recorded_value_even_if_env_changes() {
    let key = format!("TAPE_TEST_ENV_{}", std::process::id());
    std::env::set_var(&key, "first");

    let mut rec = Recording::new();
    let v1 = rec.env_get(SITE_ENV, &key);
    assert_eq!(v1.as_deref(), Some("first"));
    let trace = rec.into_trace();

    // change the env. replay must still see "first".
    std::env::set_var(&key, "second");
    let mut rep = Replaying::new(trace).unwrap();
    let v2 = rep.env_get(SITE_ENV, &key);
    assert_eq!(
        v2.as_deref(),
        Some("first"),
        "replay must see the recorded value"
    );
    std::env::remove_var(&key);
}

#[test]
fn args_get_round_trips_argv() {
    let mut rec = Recording::new();
    let recorded = rec.args_get(SITE_ARGS);
    let trace = rec.into_trace();

    let mut rep = Replaying::new(trace).unwrap();
    let replayed = rep.args_get(SITE_ARGS);
    assert_eq!(recorded, replayed, "argv must round-trip byte-identically");
    assert!(
        !replayed.is_empty(),
        "test runner always has at least argv[0]"
    );
}

#[test]
fn random_bits_args_mismatch_on_size_change() {
    // record asks for 8 bytes; "edited" replay asks for 16 — must trip
    // ArgsMismatch, not silently return 8 bytes.
    let mut rec = Recording::new();
    rec.random_bits(SITE_RANDOM, 8);
    let trace = rec.into_trace();

    let mut rep = Replaying::new(trace).unwrap();
    let bigger_args = bincode::serialize(&16u64).unwrap();
    match rep.next_event(SITE_RANDOM, EffectKind::RandomBits, &bigger_args) {
        Err(tape::ReplayErr::ArgsMismatch { .. }) => {}
        other => panic!("expected ArgsMismatch, got {:?}", other),
    }
}
