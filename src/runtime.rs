/// the runtime contract. anything a tape program does that depends on the
/// outside world goes through here. recording captures it, replaying replays
/// it. adding a new effect = a new method here + a new EffectKind variant +
/// implementations in Recording and Replaying.
///
/// keep this trait small. every method on it is a thing that has to be
/// recorded, replayed, and reasoned about. growing the trait is a deliberate
/// act, not an accident.
pub trait Runtime {
    /// monotonic-ish wall-clock seconds since unix epoch.
    /// recorded as `u64`, no args.
    fn now(&mut self, site: u32) -> u64;

    /// `len` bytes of randomness. recording calls the os; replay returns the
    /// recorded bytes. args = `len` (so changing the requested size between
    /// record and replay trips ArgsMismatch).
    fn random_bits(&mut self, site: u32, len: usize) -> Vec<u8>;

    /// write the buffer to stdout (for now). recording actually writes;
    /// replay does NOT write — the trace already captured what was emitted.
    /// returns the number of bytes the original write reported.
    fn io_write(&mut self, site: u32, buf: &[u8]) -> usize;
}
