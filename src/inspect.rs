//! human-readable trace dumper. used by `tape inspect <trace.bin>` to
//! show what a recording actually captured. nothing in here mutates the
//! trace; it only formats.

use crate::event::{EffectKind, Event, Trace};

/// optional filters used by `tape inspect` to narrow the dump. None on a
/// field means "no filter for this dimension".
#[derive(Debug, Default, Clone)]
pub struct Filter {
    pub kind: Option<EffectKind>,
    pub site: Option<u32>,
    pub since: Option<u64>,
    pub limit: Option<usize>,
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        self.kind.is_none() && self.site.is_none() && self.since.is_none() && self.limit.is_none()
    }
}

pub fn render(trace: &Trace) -> String {
    render_filtered(trace, &Filter::default())
}

pub fn render_filtered(trace: &Trace, f: &Filter) -> String {
    let mut out = String::new();
    out.push_str("== header ==\n");
    out.push_str(&format!("schema version: {}\n", trace.header.version));
    out.push_str(&format!("started_at:     {}\n", trace.header.started_at));
    out.push_str(&format!(
        "code_hash:      {}\n",
        hex(&trace.header.code_hash)
    ));
    out.push_str(&format!("events:         {}\n", trace.events.len()));
    if !f.is_empty() {
        out.push_str(&format!("filter:         {}\n", describe_filter(f)));
    }
    out.push('\n');

    if trace.events.is_empty() {
        out.push_str("(no events)\n");
        return out;
    }

    let matches: Vec<&Event> = trace
        .events
        .iter()
        .filter(|ev| {
            f.kind.is_none_or(|k| ev.kind == k)
                && f.site.is_none_or(|s| ev.site == s)
                && f.since.is_none_or(|s| ev.seq >= s)
        })
        .take(f.limit.unwrap_or(usize::MAX))
        .collect();

    out.push_str("== events ==\n");
    out.push_str(&format!(
        "{:>4}  {:<11}  {:<10}  {:>6}  {:>6}  description\n",
        "seq", "kind", "site", "args_b", "res_b"
    ));
    out.push_str(&"-".repeat(78));
    out.push('\n');
    for ev in &matches {
        out.push_str(&format!(
            "{:>4}  {:<11}  {:<#010x}  {:>6}  {:>6}  {}\n",
            ev.seq,
            ev.kind.name(),
            ev.site,
            ev.args.len(),
            ev.result.len(),
            describe(ev),
        ));
    }
    if !f.is_empty() {
        out.push_str(&format!(
            "\n({} of {} events shown)\n",
            matches.len(),
            trace.events.len()
        ));
    }
    out
}

fn describe_filter(f: &Filter) -> String {
    let mut parts = Vec::new();
    if let Some(k) = f.kind {
        parts.push(format!("kind={}", k.name()));
    }
    if let Some(s) = f.site {
        parts.push(format!("site={s:#010x}"));
    }
    if let Some(s) = f.since {
        parts.push(format!("since={s}"));
    }
    if let Some(l) = f.limit {
        parts.push(format!("limit={l}"));
    }
    parts.join(", ")
}

/// parse a kind name as written by `EffectKind::name` (e.g. "fs.read").
pub fn parse_kind(s: &str) -> Option<EffectKind> {
    match s {
        "clock.now" => Some(EffectKind::ClockNow),
        "random.bits" => Some(EffectKind::RandomBits),
        "io.write" => Some(EffectKind::IoWrite),
        "fs.read" => Some(EffectKind::FsRead),
        "fs.write" => Some(EffectKind::FsWrite),
        "env.get" => Some(EffectKind::EnvGet),
        "args.get" => Some(EffectKind::ArgsGet),
        _ => None,
    }
}

fn hex(b: &[u8]) -> String {
    let s: String = b.iter().take(8).map(|x| format!("{x:02x}")).collect();
    format!("{s}…")
}

fn describe(ev: &Event) -> String {
    use crate::event::EffectKind::*;
    match ev.kind {
        ClockNow => bincode::deserialize::<u64>(&ev.result)
            .map(|t| format!("returned {t}s"))
            .unwrap_or_else(|_| "returned ?".to_string()),
        RandomBits => {
            let len = bincode::deserialize::<u64>(&ev.args).unwrap_or(0);
            format!("{len} bytes of randomness")
        }
        IoWrite => match bincode::deserialize::<Vec<u8>>(&ev.args) {
            Ok(b) => preview(&b),
            Err(_) => "?".to_string(),
        },
        FsRead => bincode::deserialize::<String>(&ev.args)
            .map(|p| format!("read {p}"))
            .unwrap_or_else(|_| "read ?".to_string()),
        FsWrite => bincode::deserialize::<(String, Vec<u8>)>(&ev.args)
            .map(|(p, b)| format!("write {p} ({} bytes)", b.len()))
            .unwrap_or_else(|_| "write ?".to_string()),
        EnvGet => bincode::deserialize::<String>(&ev.args)
            .map(|n| format!("env {n}"))
            .unwrap_or_else(|_| "env ?".to_string()),
        ArgsGet => "process argv".to_string(),
    }
}

fn preview(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    let trimmed = s.trim_end();
    let snippet: String = trimmed.chars().take(40).collect();
    if trimmed.chars().count() > 40 {
        format!("{snippet}…")
    } else {
        snippet
    }
}
