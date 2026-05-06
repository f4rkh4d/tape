//! integration tests for the .tape scripting frontend.
//!
//! 1. lexer round-trips on a handful of token shapes
//! 2. parser accepts a tiny program and the ast has the expected shape
//! 3. happy path: record + replay an interpreted script. byte-identical io.
//! 4. site stability: parsing the same source twice gives the same site ids
//! 5. drift: editing a recorded site to a different value makes replay panic
//!    with SiteMismatch (asserted via the lower-level next_event API).

use tape::event::EffectKind;
use tape::lang::{lex, site_id, Expr, Parser, Stmt, Tok};
use tape::{Recording, Replaying};

#[test]
fn lexer_round_trips_basic_tokens() {
    let src = r#"let x = 1; if x == 0 { return; } "hi\n""#;
    let toks = lex(src).expect("lex ok");
    let kinds: Vec<&Tok> = toks.iter().map(|t| &t.tok).collect();
    let expected = vec![
        Tok::Let,
        Tok::Ident("x".into()),
        Tok::Assign,
        Tok::Int(1),
        Tok::Semicolon,
        Tok::If,
        Tok::Ident("x".into()),
        Tok::Eq,
        Tok::Int(0),
        Tok::LBrace,
        Tok::Return,
        Tok::Semicolon,
        Tok::RBrace,
        Tok::Str("hi\n".into()),
    ];
    let expected_refs: Vec<&Tok> = expected.iter().collect();
    assert_eq!(kinds, expected_refs);
}

#[test]
fn lexer_skips_comments_and_recognises_dotted_ident() {
    let src = "# this is a comment\nclock.now()";
    let toks = lex(src).expect("lex ok");
    assert_eq!(toks.len(), 3);
    assert_eq!(toks[0].tok, Tok::Ident("clock.now".into()));
    assert_eq!(toks[1].tok, Tok::LParen);
    assert_eq!(toks[2].tok, Tok::RParen);
}

#[test]
fn lexer_two_char_operators() {
    let src = "a == b != c <= d >= e && f || g ..";
    let toks = lex(src).expect("lex ok");
    let ops: Vec<&Tok> = toks.iter().map(|t| &t.tok).filter(|t| !matches!(t, Tok::Ident(_))).collect();
    let want = [
        Tok::Eq,
        Tok::Neq,
        Tok::Le,
        Tok::Ge,
        Tok::AndAnd,
        Tok::OrOr,
        Tok::DotDot,
    ];
    let want_refs: Vec<&Tok> = want.iter().collect();
    assert_eq!(ops, want_refs);
}

#[test]
fn lexer_string_escapes() {
    let toks = lex(r#""a\tb\nc\"d\\e""#).expect("lex ok");
    assert_eq!(toks.len(), 1);
    assert_eq!(toks[0].tok, Tok::Str("a\tb\nc\"d\\e".into()));
}

#[test]
fn lexer_rejects_unterminated_string() {
    assert!(lex("\"oops").is_err());
}

#[test]
fn parser_accepts_tiny_program() {
    let src = r#"let x = 1 + 2; if x == 3 { io.write("ok"); }"#;
    let toks = lex(src).expect("lex ok");
    let mut p = Parser::new(toks);
    let prog = p.parse_program().expect("parse ok");
    assert_eq!(prog.stmts.len(), 2);
    match &prog.stmts[0] {
        Stmt::Let(n, _) => assert_eq!(n, "x"),
        other => panic!("expected let, got {:?}", other),
    }
    match &prog.stmts[1] {
        Stmt::If(_, then_b, _) => {
            assert_eq!(then_b.len(), 1);
            match &then_b[0] {
                Stmt::Expr(Expr::Call { callee, args, .. }) => {
                    assert_eq!(callee, "io.write");
                    assert_eq!(args.len(), 1);
                }
                other => panic!("expected call, got {:?}", other),
            }
        }
        other => panic!("expected if, got {:?}", other),
    }
}

const HAPPY_SRC: &str = r#"
let x = 1 + 2;
io.write("answer: ");
io.write(str(x));
io.write("\n");
"#;

#[test]
fn record_then_replay_is_byte_identical() {
    let path = "tests/scripts/happy.tape";

    let mut rec = Recording::new();
    tape::lang::run_source(&mut rec, path, HAPPY_SRC).expect("script ran");
    let trace = rec.into_trace();

    // record path: collect the io.write payloads in seq order.
    let recorded_writes: Vec<Vec<u8>> = trace
        .events
        .iter()
        .filter(|e| e.kind == EffectKind::IoWrite)
        .map(|e| bincode::deserialize::<Vec<u8>>(&e.args).expect("decode"))
        .collect();

    // replay path. drift would panic; we just assert it runs cleanly.
    let mut rep = Replaying::new(trace).expect("replay accept");
    tape::lang::run_source(&mut rep, path, HAPPY_SRC).expect("replay ran");
    assert_eq!(rep.position(), rep.len(), "consumed every event");

    // sanity: the first three writes are the literal bytes the script emits.
    assert_eq!(recorded_writes[0], b"answer: ");
    assert_eq!(recorded_writes[1], b"3");
    assert_eq!(recorded_writes[2], b"\n");
}

#[test]
fn site_ids_are_stable_across_two_parses() {
    let src = "let x = clock.now(); io.write(\"hi\");";
    let path = "tests/scripts/stability.tape";

    let toks_a = lex(src).expect("lex");
    let mut pa = Parser::new(toks_a);
    let prog_a = pa.parse_program().expect("parse");

    let toks_b = lex(src).expect("lex");
    let mut pb = Parser::new(toks_b);
    let prog_b = pb.parse_program().expect("parse");

    fn call_offsets(prog: &tape::lang::Program) -> Vec<usize> {
        let mut out = Vec::new();
        for s in &prog.stmts {
            collect(s, &mut out);
        }
        out
    }
    fn collect(s: &Stmt, out: &mut Vec<usize>) {
        match s {
            Stmt::Let(_, e) => walk(e, out),
            Stmt::Expr(e) => walk(e, out),
            Stmt::If(c, t, el) => {
                walk(c, out);
                for s in t {
                    collect(s, out);
                }
                if let Some(b) = el {
                    for s in b {
                        collect(s, out);
                    }
                }
            }
            Stmt::While(c, b) => {
                walk(c, out);
                for s in b {
                    collect(s, out);
                }
            }
            Stmt::For(_, lo, hi, b) => {
                walk(lo, out);
                walk(hi, out);
                for s in b {
                    collect(s, out);
                }
            }
            Stmt::Return(Some(e)) => walk(e, out),
            Stmt::Return(None) => {}
        }
    }
    fn walk(e: &Expr, out: &mut Vec<usize>) {
        match e {
            Expr::Call { args, off, .. } => {
                out.push(*off);
                for a in args {
                    walk(a, out);
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                walk(lhs, out);
                walk(rhs, out);
            }
            Expr::Unary { rhs, .. } => walk(rhs, out),
            Expr::Index { target, index } => {
                walk(target, out);
                walk(index, out);
            }
            _ => {}
        }
    }

    let offs_a = call_offsets(&prog_a);
    let offs_b = call_offsets(&prog_b);
    assert_eq!(offs_a, offs_b, "call offsets must match across parses");
    assert!(!offs_a.is_empty());
    for off in &offs_a {
        // and the site_id derived from each is stable too.
        let s1 = site_id(path, *off);
        let s2 = site_id(path, *off);
        assert_eq!(s1, s2);
    }
}

#[test]
fn drift_at_a_call_site_is_caught_by_replay() {
    let path = "tests/scripts/drift.tape";
    let src = "let t = clock.now();";

    let mut rec = Recording::new();
    tape::lang::run_source(&mut rec, path, src).expect("ran");
    let mut trace = rec.into_trace();

    // tamper with the recorded call: bump the site id by one. now the script
    // still asks clock.now() at its real site, but the trace has a stale site.
    // replay must refuse via SiteMismatch.
    assert_eq!(trace.events.len(), 1);
    trace.events[0].site = trace.events[0].site.wrapping_add(1);

    let mut rep = Replaying::new(trace).expect("replay accept");
    // use the lower-level next_event so we get the typed error rather than a panic.
    let real_site = site_id(path, "let t = ".len() + "clock.now".len());
    let err = rep
        .next_event(real_site, EffectKind::ClockNow, &[])
        .unwrap_err();
    match err {
        tape::ReplayErr::SiteMismatch { .. } => {}
        other => panic!("expected SiteMismatch, got {:?}", other),
    }
}
