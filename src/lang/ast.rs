//! ast nodes for the .tape language. each call expression carries the byte
//! offset of its opening paren in the source so site ids stay stable.

#[derive(Debug, Clone)]
pub enum Expr {
    Nil,
    Bool(bool),
    Int(i64),
    Str(String),
    Ident(String),
    /// fn_name (call target as ident -- no first-class functions in v0.2),
    /// args, and the byte offset of the call's opening paren. that offset
    /// is what gets hashed into the site id.
    Call {
        callee: String,
        args: Vec<Expr>,
        off: usize,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Unary {
        op: UnOp,
        rhs: Box<Expr>,
    },
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Neq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let(String, Expr),
    If(Expr, Vec<Stmt>, Option<Vec<Stmt>>),
    While(Expr, Vec<Stmt>),
    For(String, Expr, Expr, Vec<Stmt>),
    Return(Option<Expr>),
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct Program {
    pub stmts: Vec<Stmt>,
}
