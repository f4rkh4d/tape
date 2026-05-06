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
use tape::{inspect, programs, Recording, Replaying, Trace};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str);
    let rest = &args[2..];
    match cmd {
        Some("list") => cmd_list(),
        Some("record") => cmd_record(rest),
        Some("replay") => cmd_replay(rest),
        Some("inspect") => cmd_inspect(rest),
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
    println!("  tape inspect <trace.bin>                        pretty-print the events in FILE");
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
    let exit = prog(&mut rec);
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
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("tape inspect: read error: {e}");
            return ExitCode::from(1);
        }
    };
    let trace: Trace = match bincode::deserialize(&bytes) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("tape inspect: decode error: {e}");
            return ExitCode::from(1);
        }
    };
    print!("{}", inspect::render(&trace));
    ExitCode::SUCCESS
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
