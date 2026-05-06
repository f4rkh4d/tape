/// the runtime contract. anything a tape program does that depends on the
/// outside world goes through here. recording captures it, replaying replays
/// it. adding a new effect = a new method here + a new EffectKind variant +
/// implementations in Recording and Replaying.
///
/// keep this trait small. every method on it is a thing that has to be
/// recorded, replayed, and reasoned about. growing the trait is a deliberate
/// act, not an accident.
///
/// the seven effects that ship in alpha 1 cover the majority of "real"
/// programs: tell time, get randomness, write to stdout, read a file, write
/// a file, read an env var, read process args. anything else (network,
/// subprocess, signals, threads) is a future effect that has to be designed
/// for determinism before it lands here.
pub trait Runtime {
    /// monotonic-ish wall-clock seconds since unix epoch. no args.
    fn now(&mut self, site: u32) -> u64;

    /// `len` bytes of randomness. args = `len` u64.
    fn random_bits(&mut self, site: u32, len: usize) -> Vec<u8>;

    /// write the buffer to stdout. args = the buffer; result = bytes written.
    /// during replay the runtime re-emits to stdout so the killer demo "byte-
    /// identical run on every replay" works visibly.
    fn io_write(&mut self, site: u32, buf: &[u8]) -> usize;

    /// read a file. args = path string. result = `Result<Vec<u8>, String>`
    /// where Err is the os error message at record time. replay returns the
    /// recorded bytes — even if the file no longer exists or has changed.
    fn fs_read(&mut self, site: u32, path: &str) -> Result<Vec<u8>, String>;

    /// write a buffer to a file. args = (path, contents). result = bytes
    /// written or os error at record time. recording actually writes the
    /// file. replay DOES NOT touch the filesystem (silenced by design — a
    /// destructive replay would be the worst footgun).
    fn fs_write(&mut self, site: u32, path: &str, buf: &[u8]) -> Result<usize, String>;

    /// look up an environment variable. args = name. result = value or empty.
    /// like all effects, the recorded answer wins on replay even if the env
    /// has since changed.
    fn env_get(&mut self, site: u32, name: &str) -> Option<String>;

    /// command-line args of the original process. args = empty (this is a
    /// "no args, give me everything" effect). result = `Vec<String>` of the
    /// process's argv at record time. usually called once at program start.
    fn args_get(&mut self, site: u32) -> Vec<String>;
}
