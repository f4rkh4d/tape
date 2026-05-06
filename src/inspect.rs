//! human-readable trace dumper. used by `tape inspect <trace.bin>` to
//! show what a recording actually captured. nothing in here mutates the
//! trace; it only formats.

use crate::event::{Event, Trace};

pub fn render(trace: &Trace) -> String {
    let mut out = String::new();
    out.push_str("== header ==\n");
    out.push_str(&format!("schema version: {}\n", trace.header.version));
    out.push_str(&format!("started_at:     {}\n", trace.header.started_at));
    out.push_str(&format!(
        "code_hash:      {}\n",
        hex(&trace.header.code_hash)
    ));
    out.push_str(&format!("events:         {}\n\n", trace.events.len()));

    if trace.events.is_empty() {
        out.push_str("(no events)\n");
        return out;
    }

    out.push_str("== events ==\n");
    out.push_str(&format!(
        "{:>4}  {:<11}  {:<10}  {:>6}  {:>6}  description\n",
        "seq", "kind", "site", "args_b", "res_b"
    ));
    out.push_str(&"-".repeat(78));
    out.push('\n');
    for ev in &trace.events {
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
    out
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
            Ok(b) => {
                let preview = String::from_utf8_lossy(&b);
                let preview = preview.trim_end();
                let snippet: String = preview.chars().take(40).collect();
                if preview.chars().count() > 40 {
                    format!("{snippet}…")
                } else {
                    snippet
                }
            }
            Err(_) => "?".to_string(),
        },
    }
}
