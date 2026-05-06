//! tree-walking interpreter for the .tape language. dispatches the eight
//! built-in effect calls straight to the host Runtime so the same trace
//! machinery used by built-in programs records and replays scripts.
//!
//! scope is one global env for v0.2: no user-defined functions, no closures.
//! `let` inside a block shadows for the rest of the block via a stack of
//! frames. that's the whole story.

use super::ast::{BinOp, Expr, Program, Stmt, UnOp};
use crate::runtime::Runtime;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Nil,
    Bool(bool),
    Int(i64),
    Bytes(Vec<u8>),
    Str(String),
    List(Vec<Value>),
}

impl Value {
    pub fn truthy(&self) -> bool {
        match self {
            Value::Nil => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Bytes(b) => !b.is_empty(),
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => !l.is_empty(),
        }
    }

    pub fn to_display(&self) -> String {
        match self {
            Value::Nil => "nil".to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Int(i) => i.to_string(),
            Value::Bytes(b) => String::from_utf8_lossy(b).to_string(),
            Value::Str(s) => s.clone(),
            Value::List(items) => {
                let mut s = String::from("[");
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    s.push_str(&v.to_display());
                }
                s.push(']');
                s
            }
        }
    }

    /// coerce to bytes for io / fs effect calls. strings become utf-8 bytes;
    /// bytes pass through; everything else is rendered via to_display.
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Value::Bytes(b) => b.clone(),
            Value::Str(s) => s.as_bytes().to_vec(),
            other => other.to_display().into_bytes(),
        }
    }
}

#[derive(Debug)]
pub enum InterpErr {
    Runtime(String),
}

impl std::fmt::Display for InterpErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InterpErr::Runtime(s) => write!(f, "runtime error: {s}"),
        }
    }
}

impl std::error::Error for InterpErr {}

/// one flat global scope. blocks DO NOT introduce a new lexical frame for
/// `let` in v0.2 -- it makes scripts like
///
/// ```text
/// let name = env.get("NAME");
/// if name == "" { let name = "stranger"; }
/// io.write(name);
/// ```
///
/// do the obvious thing (rebind, not shadow-and-evaporate). callers still
/// see push/pop on the env so the for-loop iterator variable can be cleaned
/// up after the loop, but those scopes are only used for explicit pushes
/// from the for-loop -- `let` writes to the global frame.
/// TODO(v0.3): give the language proper lexical block scoping once user
/// functions land.
struct Env {
    globals: HashMap<String, Value>,
    locals: Vec<HashMap<String, Value>>,
}

impl Env {
    fn new() -> Self {
        Self {
            globals: HashMap::new(),
            locals: Vec::new(),
        }
    }
    fn push(&mut self) {
        self.locals.push(HashMap::new());
    }
    fn pop(&mut self) {
        self.locals.pop();
    }
    /// set a local binding (for `for i in ..` loop variables only).
    fn set_local(&mut self, name: &str, v: Value) {
        if let Some(top) = self.locals.last_mut() {
            top.insert(name.to_string(), v);
        } else {
            self.globals.insert(name.to_string(), v);
        }
    }
    /// `let` writes to the global frame.
    fn set(&mut self, name: &str, v: Value) {
        self.globals.insert(name.to_string(), v);
    }
    fn get(&self, name: &str) -> Option<Value> {
        for f in self.locals.iter().rev() {
            if let Some(v) = f.get(name) {
                return Some(v.clone());
            }
        }
        self.globals.get(name).cloned()
    }
}

enum Flow {
    Normal,
    Return(Value),
}

pub struct Interp<'a> {
    rt: &'a mut dyn Runtime,
    file_path: String,
    env: Env,
}

impl<'a> Interp<'a> {
    pub fn new(rt: &'a mut dyn Runtime, file_path: String) -> Self {
        Self {
            rt,
            file_path,
            env: Env::new(),
        }
    }

    pub fn run(&mut self, prog: &Program) -> Result<i32, InterpErr> {
        match self.exec_block(&prog.stmts)? {
            Flow::Return(Value::Int(n)) => Ok(n as i32),
            _ => Ok(0),
        }
    }

    fn exec_block(&mut self, stmts: &[Stmt]) -> Result<Flow, InterpErr> {
        for s in stmts {
            match self.exec_stmt(s)? {
                Flow::Normal => {}
                f @ Flow::Return(_) => return Ok(f),
            }
        }
        Ok(Flow::Normal)
    }

    fn exec_stmt(&mut self, s: &Stmt) -> Result<Flow, InterpErr> {
        match s {
            Stmt::Let(name, e) => {
                let v = self.eval_expr(e)?;
                self.env.set(name, v);
                Ok(Flow::Normal)
            }
            Stmt::If(cond, then_b, else_b) => {
                let c = self.eval_expr(cond)?;
                let res = if c.truthy() {
                    self.exec_block(then_b)?
                } else if let Some(b) = else_b {
                    self.exec_block(b)?
                } else {
                    Flow::Normal
                };
                Ok(res)
            }
            Stmt::While(cond, body) => {
                loop {
                    let c = self.eval_expr(cond)?;
                    if !c.truthy() {
                        break;
                    }
                    let res = self.exec_block(body)?;
                    if let Flow::Return(v) = res {
                        return Ok(Flow::Return(v));
                    }
                }
                Ok(Flow::Normal)
            }
            Stmt::For(name, lo, hi, body) => {
                let lo_v = self.eval_expr(lo)?;
                let hi_v = self.eval_expr(hi)?;
                let (a, b) = match (lo_v, hi_v) {
                    (Value::Int(a), Value::Int(b)) => (a, b),
                    _ => return Err(InterpErr::Runtime("for range needs ints".into())),
                };
                let mut i = a;
                while i < b {
                    self.env.push();
                    self.env.set_local(name, Value::Int(i));
                    let res = self.exec_block(body)?;
                    self.env.pop();
                    if let Flow::Return(v) = res {
                        return Ok(Flow::Return(v));
                    }
                    i += 1;
                }
                Ok(Flow::Normal)
            }
            Stmt::Return(e) => {
                let v = match e {
                    Some(e) => self.eval_expr(e)?,
                    None => Value::Nil,
                };
                Ok(Flow::Return(v))
            }
            Stmt::Expr(e) => {
                self.eval_expr(e)?;
                Ok(Flow::Normal)
            }
        }
    }

    fn eval_expr(&mut self, e: &Expr) -> Result<Value, InterpErr> {
        match e {
            Expr::Nil => Ok(Value::Nil),
            Expr::Bool(b) => Ok(Value::Bool(*b)),
            Expr::Int(n) => Ok(Value::Int(*n)),
            Expr::Str(s) => Ok(Value::Str(s.clone())),
            Expr::Ident(name) => self
                .env
                .get(name)
                .ok_or_else(|| InterpErr::Runtime(format!("unbound name: {name}"))),
            Expr::Call { callee, args, off } => self.do_call(callee, args, *off),
            Expr::Binary { op, lhs, rhs } => {
                let l = self.eval_expr(lhs)?;
                // short-circuit on logical ops
                match op {
                    BinOp::And => {
                        if !l.truthy() {
                            return Ok(Value::Bool(false));
                        }
                        let r = self.eval_expr(rhs)?;
                        return Ok(Value::Bool(r.truthy()));
                    }
                    BinOp::Or => {
                        if l.truthy() {
                            return Ok(Value::Bool(true));
                        }
                        let r = self.eval_expr(rhs)?;
                        return Ok(Value::Bool(r.truthy()));
                    }
                    _ => {}
                }
                let r = self.eval_expr(rhs)?;
                apply_binop(*op, l, r)
            }
            Expr::Unary { op, rhs } => {
                let v = self.eval_expr(rhs)?;
                match (op, v) {
                    (UnOp::Neg, Value::Int(n)) => Ok(Value::Int(-n)),
                    (UnOp::Not, v) => Ok(Value::Bool(!v.truthy())),
                    _ => Err(InterpErr::Runtime("bad unary operand".into())),
                }
            }
            Expr::Index { target, index } => {
                let t = self.eval_expr(target)?;
                let i = self.eval_expr(index)?;
                let i = match i {
                    Value::Int(n) => n,
                    _ => return Err(InterpErr::Runtime("index must be int".into())),
                };
                match t {
                    Value::Bytes(b) => {
                        let n = b
                            .get(i as usize)
                            .ok_or_else(|| InterpErr::Runtime("index out of range".into()))?;
                        Ok(Value::Int(*n as i64))
                    }
                    Value::Str(s) => {
                        let bytes = s.as_bytes();
                        let n = bytes
                            .get(i as usize)
                            .ok_or_else(|| InterpErr::Runtime("index out of range".into()))?;
                        Ok(Value::Int(*n as i64))
                    }
                    Value::List(l) => l
                        .get(i as usize)
                        .cloned()
                        .ok_or_else(|| InterpErr::Runtime("index out of range".into())),
                    _ => Err(InterpErr::Runtime("cannot index this value".into())),
                }
            }
        }
    }

    fn do_call(&mut self, callee: &str, args: &[Expr], off: usize) -> Result<Value, InterpErr> {
        // evaluate args first (left to right), then dispatch.
        let mut vals: Vec<Value> = Vec::with_capacity(args.len());
        for a in args {
            vals.push(self.eval_expr(a)?);
        }
        let site = site_id(&self.file_path, off);

        match callee {
            // effect dispatch: every one of these adds an event to the trace.
            "clock.now" => {
                require_args(callee, &vals, 0)?;
                Ok(Value::Int(self.rt.now(site) as i64))
            }
            "random.bits" => {
                require_args(callee, &vals, 1)?;
                let len = as_int(&vals[0], "random.bits len")? as usize;
                let bytes = self.rt.random_bits(site, len);
                Ok(Value::Bytes(bytes))
            }
            "io.write" => {
                require_args(callee, &vals, 1)?;
                let buf = vals[0].to_bytes();
                let n = self.rt.io_write(site, &buf);
                Ok(Value::Int(n as i64))
            }
            "fs.read" => {
                require_args(callee, &vals, 1)?;
                let path = as_str(&vals[0], "fs.read path")?;
                match self.rt.fs_read(site, &path) {
                    Ok(b) => Ok(Value::Bytes(b)),
                    // keep it simple for v0.2: surface fs errors as runtime errors.
                    // TODO(v0.3): give scripts a way to handle fs errors gracefully.
                    Err(e) => Err(InterpErr::Runtime(format!("fs.read({path}): {e}"))),
                }
            }
            "fs.write" => {
                require_args(callee, &vals, 2)?;
                let path = as_str(&vals[0], "fs.write path")?;
                let buf = vals[1].to_bytes();
                match self.rt.fs_write(site, &path, &buf) {
                    Ok(n) => Ok(Value::Int(n as i64)),
                    Err(e) => Err(InterpErr::Runtime(format!("fs.write({path}): {e}"))),
                }
            }
            "env.get" => {
                require_args(callee, &vals, 1)?;
                let name = as_str(&vals[0], "env.get name")?;
                Ok(Value::Str(self.rt.env_get(site, &name).unwrap_or_default()))
            }
            "args.get" => {
                require_args(callee, &vals, 0)?;
                let v = self.rt.args_get(site);
                Ok(Value::List(v.into_iter().map(Value::Str).collect()))
            }
            "time.sleep" => {
                require_args(callee, &vals, 1)?;
                let ms = as_int(&vals[0], "time.sleep millis")? as u64;
                self.rt.time_sleep(site, ms);
                Ok(Value::Nil)
            }
            // host helpers (not effect calls -- no recording).
            "print" => {
                require_args(callee, &vals, 1)?;
                let mut buf = vals[0].to_bytes();
                buf.push(b'\n');
                let n = self.rt.io_write(site, &buf);
                Ok(Value::Int(n as i64))
            }
            "len" => {
                require_args(callee, &vals, 1)?;
                let n = match &vals[0] {
                    Value::Str(s) => s.len(),
                    Value::Bytes(b) => b.len(),
                    Value::List(l) => l.len(),
                    _ => return Err(InterpErr::Runtime("len: unsupported type".into())),
                };
                Ok(Value::Int(n as i64))
            }
            "int" => {
                require_args(callee, &vals, 1)?;
                let n = match &vals[0] {
                    Value::Int(n) => *n,
                    Value::Bool(b) => {
                        if *b {
                            1
                        } else {
                            0
                        }
                    }
                    Value::Str(s) => s
                        .trim()
                        .parse::<i64>()
                        .map_err(|_| InterpErr::Runtime(format!("int: cannot parse {:?}", s)))?,
                    _ => return Err(InterpErr::Runtime("int: unsupported type".into())),
                };
                Ok(Value::Int(n))
            }
            "str" => {
                require_args(callee, &vals, 1)?;
                Ok(Value::Str(vals[0].to_display()))
            }
            "byte_at" => {
                require_args(callee, &vals, 2)?;
                let i = as_int(&vals[1], "byte_at index")? as usize;
                let n = match &vals[0] {
                    Value::Bytes(b) => *b
                        .get(i)
                        .ok_or_else(|| InterpErr::Runtime("byte_at: out of range".into()))?,
                    Value::Str(s) => *s
                        .as_bytes()
                        .get(i)
                        .ok_or_else(|| InterpErr::Runtime("byte_at: out of range".into()))?,
                    _ => return Err(InterpErr::Runtime("byte_at: need bytes or string".into())),
                };
                Ok(Value::Int(n as i64))
            }
            other => Err(InterpErr::Runtime(format!("unknown function: {other}"))),
        }
    }
}

fn require_args(name: &str, args: &[Value], n: usize) -> Result<(), InterpErr> {
    if args.len() != n {
        return Err(InterpErr::Runtime(format!(
            "{name}: expected {n} args, got {}",
            args.len()
        )));
    }
    Ok(())
}

fn as_int(v: &Value, ctx: &str) -> Result<i64, InterpErr> {
    match v {
        Value::Int(n) => Ok(*n),
        _ => Err(InterpErr::Runtime(format!("{ctx}: expected int"))),
    }
}

fn as_str(v: &Value, ctx: &str) -> Result<String, InterpErr> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        Value::Bytes(b) => Ok(String::from_utf8_lossy(b).to_string()),
        _ => Err(InterpErr::Runtime(format!("{ctx}: expected string"))),
    }
}

/// stable u32 site id for one call expression. mirrors the FNV-1a algorithm
/// used by the site!() macro in lib.rs so traces produced by scripts use the
/// same site-id space as built-in programs.
pub fn site_id(file_path: &str, off: usize) -> u32 {
    // fold (file_path, off) into FNV-1a. we pack `off` into two u32 mixes
    // (low, high) so files larger than 4 GiB still produce a distinct hash --
    // not that we expect any.
    let mut h: u32 = 0x811c_9dc5;
    for &b in file_path.as_bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    let lo = (off as u64 & 0xFFFF_FFFF) as u32;
    let hi = ((off as u64) >> 32) as u32;
    h ^= lo;
    h = h.wrapping_mul(0x0100_0193);
    h ^= hi;
    h.wrapping_mul(0x0100_0193)
}

fn apply_binop(op: BinOp, l: Value, r: Value) -> Result<Value, InterpErr> {
    use BinOp::*;
    match op {
        Add => match (l, r) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            (Value::Str(a), Value::Str(b)) => Ok(Value::Str(a + &b)),
            (Value::Str(a), b) => Ok(Value::Str(a + &b.to_display())),
            (a, Value::Str(b)) => Ok(Value::Str(a.to_display() + &b)),
            _ => Err(InterpErr::Runtime("+: unsupported operands".into())),
        },
        Sub => num_op(l, r, |a, b| a - b),
        Mul => num_op(l, r, |a, b| a * b),
        Div => match (l, r) {
            (_, Value::Int(0)) => Err(InterpErr::Runtime("division by zero".into())),
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a / b)),
            _ => Err(InterpErr::Runtime("/: need ints".into())),
        },
        Mod => match (l, r) {
            (_, Value::Int(0)) => Err(InterpErr::Runtime("mod by zero".into())),
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a % b)),
            _ => Err(InterpErr::Runtime("%: need ints".into())),
        },
        Eq => Ok(Value::Bool(l == r)),
        Neq => Ok(Value::Bool(l != r)),
        Lt => cmp_op(l, r, |o| o == std::cmp::Ordering::Less),
        Le => cmp_op(l, r, |o| o != std::cmp::Ordering::Greater),
        Gt => cmp_op(l, r, |o| o == std::cmp::Ordering::Greater),
        Ge => cmp_op(l, r, |o| o != std::cmp::Ordering::Less),
        And | Or => unreachable!("handled in eval_expr()"),
    }
}

fn num_op<F: Fn(i64, i64) -> i64>(l: Value, r: Value, f: F) -> Result<Value, InterpErr> {
    match (l, r) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(f(a, b))),
        _ => Err(InterpErr::Runtime("arithmetic: need ints".into())),
    }
}

fn cmp_op<F: Fn(std::cmp::Ordering) -> bool>(l: Value, r: Value, f: F) -> Result<Value, InterpErr> {
    let ord = match (l, r) {
        (Value::Int(a), Value::Int(b)) => a.cmp(&b),
        (Value::Str(a), Value::Str(b)) => a.cmp(&b),
        _ => return Err(InterpErr::Runtime("comparison: incompatible types".into())),
    };
    Ok(Value::Bool(f(ord)))
}
