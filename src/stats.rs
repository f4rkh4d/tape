//! summary statistics over a trace. used by `tape stats <trace.bin>` to
//! answer the questions you ask of any captured workload at a glance:
//! how many calls, of what kinds, against which sites, how big on the wire.
//!
//! pure read-only. nothing here mutates the trace; nothing here calls the OS.

use crate::event::{EffectKind, Trace};
use std::collections::HashMap;

pub fn render(trace: &Trace) -> String {
    let mut out = String::new();
    let n = trace.events.len();
    out.push_str("== summary ==\n");
    out.push_str(&format!("schema version:  {}\n", trace.header.version));
    out.push_str(&format!("started_at:      {}\n", trace.header.started_at));
    out.push_str(&format!("events:          {}\n", n));
    out.push_str(&format!(
        "outcome:         {}\n",
        crate::inspect::format_outcome(&trace.footer.outcome)
    ));

    let total_args: usize = trace.events.iter().map(|e| e.args.len()).sum();
    let total_result: usize = trace.events.iter().map(|e| e.result.len()).sum();
    let total_payload = total_args + total_result;
    out.push_str(&format!(
        "payload bytes:   {} args + {} result = {} total\n",
        total_args, total_result, total_payload
    ));
    if n > 0 {
        out.push_str(&format!(
            "per-event mean:  {:.1} bytes\n",
            total_payload as f64 / n as f64
        ));
    }
    out.push('\n');

    if n == 0 {
        return out;
    }

    let mut by_kind: HashMap<EffectKind, (usize, usize)> = HashMap::new();
    for ev in &trace.events {
        let entry = by_kind.entry(ev.kind).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += ev.args.len() + ev.result.len();
    }
    let mut kinds: Vec<_> = by_kind.into_iter().collect();
    kinds.sort_by_key(|x| std::cmp::Reverse(x.1 .0));

    out.push_str("== by kind ==\n");
    out.push_str(&format!(
        "{:<12}  {:>6}  {:>6}  {:>10}\n",
        "kind", "count", "pct", "bytes"
    ));
    out.push_str(&"-".repeat(40));
    out.push('\n');
    for (kind, (count, bytes)) in &kinds {
        let pct = (*count as f64) * 100.0 / (n as f64);
        out.push_str(&format!(
            "{:<12}  {:>6}  {:>5.1}%  {:>10}\n",
            kind.name(),
            count,
            pct,
            bytes
        ));
    }
    out.push('\n');

    let mut by_site: HashMap<(u32, EffectKind), usize> = HashMap::new();
    for ev in &trace.events {
        *by_site.entry((ev.site, ev.kind)).or_insert(0) += 1;
    }
    let mut sites: Vec<_> = by_site.into_iter().collect();
    sites.sort_by_key(|x| std::cmp::Reverse(x.1));

    let top = sites.len().min(10);
    out.push_str(&format!("== top {} sites ==\n", top));
    out.push_str(&format!("{:<10}  {:<12}  {:>6}\n", "site", "kind", "count"));
    out.push_str(&"-".repeat(40));
    out.push('\n');
    for ((site, kind), count) in sites.iter().take(top) {
        out.push_str(&format!(
            "{:#010x}  {:<12}  {:>6}\n",
            site,
            kind.name(),
            count
        ));
    }
    out
}
