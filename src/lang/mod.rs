//! scripting frontend for tape. parses a .tape source file and walks the
//! ast against a Runtime, so the same record / replay path used by built-in
//! programs also drives user scripts.
//!
//! the language is small on purpose: literals, ints, strings, bytes, lists,
//! `let`, `if/else`, `while`, `for ... in lo..hi`, the usual operators, and
//! a fixed set of host calls that map one-to-one to Runtime methods. no
//! user-defined functions, no closures, no module system. add those when
//! a real script needs them.

pub mod ast;
pub mod interp;
pub mod lexer;
pub mod parser;

pub use ast::{Expr, Program, Stmt};
pub use interp::{site_id, Interp, InterpErr, Value};
pub use lexer::{lex, LexErr, Tok, Token};
pub use parser::{ParseErr, Parser};

use crate::runtime::Runtime;

/// one-shot helper: lex, parse, and run `src` against `rt`. returns the
/// program's exit code (0 if it finished without an explicit `return N;`).
///
/// `file_path` is what site ids hash against. pass the path the user wrote
/// on the cli ("examples/scripts/hello.tape") so record and replay produce
/// identical site ids on the same machine.
pub fn run_source(rt: &mut dyn Runtime, file_path: &str, src: &str) -> Result<i32, ScriptErr> {
    let toks = lex(src).map_err(ScriptErr::Lex)?;
    let mut p = Parser::new(toks);
    let prog = p.parse_program().map_err(ScriptErr::Parse)?;
    let mut interp = Interp::new(rt, file_path.to_string());
    interp.run(&prog).map_err(ScriptErr::Interp)
}

#[derive(Debug)]
pub enum ScriptErr {
    Lex(LexErr),
    Parse(ParseErr),
    Interp(InterpErr),
}

impl std::fmt::Display for ScriptErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScriptErr::Lex(e) => write!(f, "{e}"),
            ScriptErr::Parse(e) => write!(f, "{e}"),
            ScriptErr::Interp(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ScriptErr {}
