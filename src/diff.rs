//! diff two traces. the use case: a CI run records a failing test, you
//! pull the trace, you replay it locally, you also record a fresh local
//! run on the same code. the two traces should be identical. when they
//! aren't — `tape diff ci.tape local.tape` shows you exactly where the
//! divergence starts.
//!
//! this is also the workflow for narrowing down "did anything change
//! between v0.3 and v0.4 of my program?": record on each, diff.

use crate::event::Trace;

pub fn render(a: &Trace, b: &Trace, label_a: &str, label_b: &str) -> String {
    let mut out = String::new();

    if a.header.code_hash != b.header.code_hash {
        out.push_str(
            "⚠ code_hash differs — these traces were recorded against different builds.\n",
        );
        out.push_str(&format!(
            "  {label_a}: {}\n  {label_b}: {}\n\n",
            short_hex(&a.header.code_hash),
            short_hex(&b.header.code_hash)
        ));
    }

    let pad = label_a.len().max(label_b.len());

    let n = a.events.len().max(b.events.len());
    let mut diverged = false;
    let mut shown = 0usize;
    for i in 0..n {
        let ea = a.events.get(i);
        let eb = b.events.get(i);
        match (ea, eb) {
            (Some(x), Some(y)) => {
                let same = x.kind == y.kind
                    && x.site == y.site
                    && x.args == y.args
                    && x.result == y.result;
                if !same {
                    if !diverged {
                        out.push_str(&format!("first divergence at seq {i}:\n"));
                        diverged = true;
                    }
                    out.push_str(&format!(
                        "  {:>pad$}  {} site={:#010x} args={}b result={}b\n",
                        label_a,
                        x.kind.name(),
                        x.site,
                        x.args.len(),
                        x.result.len(),
                        pad = pad
                    ));
                    out.push_str(&format!(
                        "  {:>pad$}  {} site={:#010x} args={}b result={}b\n",
                        label_b,
                        y.kind.name(),
                        y.site,
                        y.args.len(),
                        y.result.len(),
                        pad = pad
                    ));
                    out.push_str(&format!("  {}\n", explain_divergence(x, y)));
                    shown += 1;
                    if shown >= 5 {
                        out.push_str(&format!(
                            "(suppressing further differences after {} shown)\n",
                            shown
                        ));
                        break;
                    }
                }
            }
            (Some(x), None) => {
                out.push_str(&format!(
                    "  {:>pad$} has extra event at seq {i}: {} site={:#010x}\n",
                    label_a,
                    x.kind.name(),
                    x.site,
                    pad = pad
                ));
                break;
            }
            (None, Some(y)) => {
                out.push_str(&format!(
                    "  {:>pad$} has extra event at seq {i}: {} site={:#010x}\n",
                    label_b,
                    y.kind.name(),
                    y.site,
                    pad = pad
                ));
                break;
            }
            (None, None) => break,
        }
    }

    if !diverged && a.events.len() == b.events.len() {
        out.push_str(&format!(
            "no divergence. {} events match byte-for-byte.\n",
            a.events.len()
        ));
    }
    out
}

fn explain_divergence(a: &crate::event::Event, b: &crate::event::Event) -> &'static str {
    if a.kind != b.kind {
        "(different effect kinds — programs took different control-flow branches)"
    } else if a.site != b.site {
        "(same kind, different sites — code edited between recordings)"
    } else if a.args != b.args {
        "(same site, different args — programs computed different arguments)"
    } else {
        "(same kind/site/args, different result — outside world answered differently)"
    }
}

fn short_hex(b: &[u8]) -> String {
    let s: String = b.iter().take(8).map(|x| format!("{x:02x}")).collect();
    format!("{s}…")
}
