//! tape cli — record / replay / inspect demo programs.
//!
//!   tape list
//!   tape record <program>  [--out trace.bin]
//!   tape replay <program>  --trace trace.bin
//!   tape inspect <trace.bin>
//!
//! the program argument names a demo built into this binary (see `tape list`).
//! a real language frontend will replace this with arbitrary user programs;
//! for the weekend mvp we keep the program set static and small.

use std::process::ExitCode;
use tape::event::Outcome;
use tape::{diff, inspect, programs, stats, Recording, Replaying, Trace};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str);
    let rest = &args[2..];
    match cmd {
        Some("list") => cmd_list(),
        Some("record") => cmd_record(rest),
        Some("replay") => cmd_replay(rest),
        Some("inspect") => cmd_inspect(rest),
        Some("diff") => cmd_diff(rest),
        Some("stats") => cmd_stats(rest),
        Some("bench") => cmd_bench(rest),
        Some("--version") | Some("-V") => {
            println!("tape {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--help") | Some("-h") | None => {
            print_usage();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("tape: unknown subcommand: {other}\n");
            print_usage();
            ExitCode::from(2)
        }
    }
}

fn print_usage() {
    println!("tape — deterministic record + replay runtime.\n");
    println!("usage:");
    println!("  tape list                                       show built-in programs");
    println!("  tape record <program> [--out FILE]              run + record into FILE (default: trace.bin)");
    println!("  tape replay <program> --trace FILE              replay program against FILE");
    println!("  tape inspect <trace.bin> [--filter KIND] [--site HEX] [--since N] [--limit N] [--json]");
    println!("                                                  pretty-print the events in FILE");
    println!("  tape stats <trace.bin> [--json]                 summary stats: count by kind + hot sites");
    println!("  tape diff <a.tape> <b.tape>                     show the first divergence between two traces");
    println!("  tape bench [--events N] [--effect KIND]         measure record / replay overhead");
    println!();
    println!("the same <program> name must be passed to record and replay.");
    println!("trace files are bincode v1.3; on-disk schema lives in src/event.rs.");
}

fn cmd_list() -> ExitCode {
    println!("built-in programs:\n");
    for (name, desc) in programs::CATALOG {
        println!("  {name:<10} {desc}");
    }
    ExitCode::SUCCESS
}

fn cmd_record(args: &[String]) -> ExitCode {
    let Some(name) = args.first() else {
        eprintln!("tape record: missing program name. try `tape list`.");
        return ExitCode::from(2);
    };
    let out = parse_named_arg(args, "--out").unwrap_or_else(|| "trace.bin".to_string());

    let Some(prog) = programs::lookup(name) else {
        eprintln!("tape record: no such program: {name}");
        return ExitCode::from(2);
    };

    let mut rec = Recording::new();
    let panic_loc = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    {
        let slot = panic_loc.clone();
        std::panic::set_hook(Box::new(move |info| {
            let loc = info
                .location()
                .map(|l| format!("{}:{}", l.file(), l.line()))
                .unwrap_or_default();
            if let Ok(mut g) = slot.lock() {
                *g = loc;
            }
        }));
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| prog(&mut rec)));
    let _ = std::panic::take_hook();
    let exit = match result {
        Ok(code) => {
            rec.set_outcome(Outcome::Exit(code));
            code
        }
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<panic with non-string payload>".to_string());
            let location = panic_loc.lock().map(|g| g.clone()).unwrap_or_default();
            eprintln!("[tape] program panicked: {message}");
            rec.set_outcome(Outcome::Panic { message, location });
            1
        }
    };
    let trace = rec.into_trace();
    let bytes = match bincode::serialize(&trace) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("tape record: encode error: {e}");
            return ExitCode::from(1);
        }
    };
    if let Err(e) = std::fs::write(&out, &bytes) {
        eprintln!("tape record: write error: {e}");
        return ExitCode::from(1);
    }
    eprintln!(
        "[tape] recorded {} events ({} bytes) into {}",
        trace.events.len(),
        bytes.len(),
        out
    );
    if exit != 0 {
        ExitCode::from(exit as u8)
    } else {
        ExitCode::SUCCESS
    }
}

fn cmd_replay(args: &[String]) -> ExitCode {
    let Some(name) = args.first() else {
        eprintln!("tape replay: missing program name. try `tape list`.");
        return ExitCode::from(2);
    };
    let Some(path) = parse_named_arg(args, "--trace") else {
        eprintln!("tape replay: missing --trace FILE");
        return ExitCode::from(2);
    };

    let Some(prog) = programs::lookup(name) else {
        eprintln!("tape replay: no such program: {name}");
        return ExitCode::from(2);
    };

    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("tape replay: read error: {e}");
            return ExitCode::from(1);
        }
    };
    let trace: Trace = match bincode::deserialize(&bytes) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("tape replay: decode error: {e}");
            return ExitCode::from(1);
        }
    };
    let mut rep = match Replaying::new(trace) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("tape replay: {e}");
            return ExitCode::from(1);
        }
    };

    // catch panics from drift detection so the cli prints them nicely instead
    // of dumping a backtrace at the user.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| prog(&mut rep)));
    match result {
        Ok(exit) => {
            eprintln!(
                "[tape] replayed {} / {} events from {}",
                rep.position(),
                rep.len(),
                path
            );
            if exit != 0 {
                ExitCode::from(exit as u8)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "replay panicked".to_string());
            eprintln!("tape replay: {msg}");
            ExitCode::from(1)
        }
    }
}

fn cmd_inspect(args: &[String]) -> ExitCode {
    let Some(path) = args.first() else {
        eprintln!("tape inspect: missing trace file");
        return ExitCode::from(2);
    };
    let trace = match load_trace(path) {
        Ok(t) => t,
        Err(code) => return code,
    };

    let mut filter = inspect::Filter::default();
    if let Some(k) = parse_named_arg(args, "--filter") {
        match inspect::parse_kind(&k) {
            Some(kind) => filter.kind = Some(kind),
            None => {
                eprintln!("tape inspect: unknown --filter kind: {k}");
                return ExitCode::from(2);
            }
        }
    }
    if let Some(s) = parse_named_arg(args, "--site") {
        match parse_u32_flexible(&s) {
            Some(v) => filter.site = Some(v),
            None => {
                eprintln!("tape inspect: bad --site value: {s} (try 0xA0000001 or 2684354561)");
                return ExitCode::from(2);
            }
        }
    }
    if let Some(s) = parse_named_arg(args, "--since") {
        match s.parse::<u64>() {
            Ok(v) => filter.since = Some(v),
            Err(_) => {
                eprintln!("tape inspect: bad --since value: {s}");
                return ExitCode::from(2);
            }
        }
    }
    if let Some(s) = parse_named_arg(args, "--limit") {
        match s.parse::<usize>() {
            Ok(v) => filter.limit = Some(v),
            Err(_) => {
                eprintln!("tape inspect: bad --limit value: {s}");
                return ExitCode::from(2);
            }
        }
    }

    if has_flag(args, "--json") {
        println!("{}", inspect::render_json_filtered(&trace, &filter));
    } else {
        print!("{}", inspect::render_filtered(&trace, &filter));
    }
    ExitCode::SUCCESS
}

fn cmd_stats(args: &[String]) -> ExitCode {
    let Some(path) = args.first() else {
        eprintln!("tape stats: missing trace file");
        return ExitCode::from(2);
    };
    let trace = match load_trace(path) {
        Ok(t) => t,
        Err(code) => return code,
    };
    if has_flag(args, "--json") {
        println!("{}", stats::render_json(&trace));
    } else {
        print!("{}", stats::render(&trace));
    }
    ExitCode::SUCCESS
}

fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

/// accept either decimal or 0x-prefixed hex for u32 args.
fn parse_u32_flexible(s: &str) -> Option<u32> {
    if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(rest, 16).ok()
    } else {
        s.parse::<u32>().ok()
    }
}

fn cmd_diff(args: &[String]) -> ExitCode {
    if args.len() < 2 {
        eprintln!("tape diff: need two trace files");
        return ExitCode::from(2);
    }
    let path_a = &args[0];
    let path_b = &args[1];
    let trace_a = match load_trace(path_a) {
        Ok(t) => t,
        Err(code) => return code,
    };
    let trace_b = match load_trace(path_b) {
        Ok(t) => t,
        Err(code) => return code,
    };
    let label_a = std::path::Path::new(path_a)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path_a)
        .to_string();
    let label_b = std::path::Path::new(path_b)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path_b)
        .to_string();
    print!("{}", diff::render(&trace_a, &trace_b, &label_a, &label_b));

    let same = trace_a.events.len() == trace_b.events.len()
        && trace_a.events.iter().zip(&trace_b.events).all(|(x, y)| {
            x.kind == y.kind && x.site == y.site && x.args == y.args && x.result == y.result
        });
    if same {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

/// `tape bench` records and replays a synthetic load of N effect calls and
/// reports wall-clock + per-event numbers. used to back the perf claims in
/// the readme. it is a microbenchmark — not a substitute for profiling a
/// real program — but it's the cheapest signal that nothing has gotten 10x
/// slower since the last release.
fn cmd_bench(args: &[String]) -> ExitCode {
    let n: usize = parse_named_arg(args, "--events")
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000);
    let effect = parse_named_arg(args, "--effect").unwrap_or_else(|| "clock".to_string());

    use std::time::Instant;
    use tape::Runtime;

    let mut rec = Recording::new();
    let t0 = Instant::now();
    match effect.as_str() {
        "clock" => {
            for _ in 0..n {
                rec.now(0xBEEF_0001);
            }
        }
        "random" => {
            for _ in 0..n {
                rec.random_bits(0xBEEF_0002, 8);
            }
        }
        "write" => {
            for _ in 0..n {
                rec.io_write(0xBEEF_0003, b".");
            }
        }
        other => {
            eprintln!("tape bench: unknown --effect: {other} (use clock|random|write)");
            return ExitCode::from(2);
        }
    }
    let record_ms = t0.elapsed().as_millis();
    let trace = rec.into_trace();

    let bytes = bincode::serialize(&trace).expect("encode trace");
    let trace_kb = bytes.len() as f64 / 1024.0;

    let trace2: Trace = bincode::deserialize(&bytes).expect("decode trace");
    let mut rep = Replaying::new(trace2).expect("replay accept");
    let t1 = Instant::now();
    match effect.as_str() {
        "clock" => {
            for _ in 0..n {
                rep.now(0xBEEF_0001);
            }
        }
        "random" => {
            for _ in 0..n {
                rep.random_bits(0xBEEF_0002, 8);
            }
        }
        "write" => {
            for _ in 0..n {
                rep.io_write(0xBEEF_0003, b".");
            }
        }
        _ => unreachable!(),
    }
    let replay_ms = t1.elapsed().as_millis();

    // for io.write the recorded path includes a real stdout flush per call;
    // bury that in stderr below so it doesn't confuse readers.
    if effect == "write" {
        eprintln!();
    }
    eprintln!("tape bench: effect={effect}, events={n}");
    eprintln!(
        "  record:  {record_ms} ms  ({:.2} µs/event)",
        record_ms as f64 * 1000.0 / n as f64
    );
    eprintln!(
        "  replay:  {replay_ms} ms  ({:.2} µs/event)",
        replay_ms as f64 * 1000.0 / n as f64
    );
    eprintln!(
        "  trace:   {:.1} KiB  ({:.1} bytes/event)",
        trace_kb,
        bytes.len() as f64 / n as f64
    );
    ExitCode::SUCCESS
}

fn load_trace(path: &str) -> Result<Trace, ExitCode> {
    let bytes = std::fs::read(path).map_err(|e| {
        eprintln!("tape: read {path}: {e}");
        ExitCode::from(1)
    })?;
    bincode::deserialize::<Trace>(&bytes).map_err(|e| {
        eprintln!("tape: decode {path}: {e}");
        ExitCode::from(1)
    })
}

/// pull `--name VALUE` out of args, ignoring everything else.
fn parse_named_arg(args: &[String], name: &str) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == name {
            return iter.next().cloned();
        }
        if let Some(rest) = arg.strip_prefix(&format!("{name}=")) {
            return Some(rest.to_string());
        }
    }
    None
}
