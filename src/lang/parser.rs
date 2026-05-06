//! pratt-ish recursive descent parser. tiny on purpose: there is no error
//! recovery, the first parse error aborts and the message names the byte
//! offset where things went wrong.

use super::ast::{BinOp, Expr, Program, Stmt, UnOp};
use super::lexer::{Tok, Token};

#[derive(Debug)]
pub struct ParseErr {
    pub off: usize,
    pub msg: String,
}

impl std::fmt::Display for ParseErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parse error at byte {}: {}", self.off, self.msg)
    }
}

impl std::error::Error for ParseErr {}

pub struct Parser {
    toks: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(toks: Vec<Token>) -> Self {
        Self { toks, pos: 0 }
    }

    pub fn parse_program(&mut self) -> Result<Program, ParseErr> {
        let mut stmts = Vec::new();
        while self.pos < self.toks.len() {
            stmts.push(self.parse_stmt()?);
        }
        Ok(Program { stmts })
    }

    fn peek(&self) -> Option<&Token> {
        self.toks.get(self.pos)
    }

    fn eat(&mut self) -> Option<Token> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn expect(&mut self, want: &Tok) -> Result<Token, ParseErr> {
        let off = self.peek().map(|t| t.off).unwrap_or(0);
        match self.peek() {
            Some(t) if &t.tok == want => Ok(self.eat().unwrap()),
            Some(t) => Err(ParseErr {
                off: t.off,
                msg: format!("expected {:?}, found {:?}", want, t.tok),
            }),
            None => Err(ParseErr {
                off,
                msg: format!("expected {:?}, found end of input", want),
            }),
        }
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseErr> {
        let t = self.peek().ok_or_else(|| ParseErr {
            off: 0,
            msg: "unexpected end".into(),
        })?;
        match &t.tok {
            Tok::Let => self.parse_let(),
            Tok::If => self.parse_if(),
            Tok::While => self.parse_while(),
            Tok::For => self.parse_for(),
            Tok::Return => self.parse_return(),
            _ => {
                let e = self.parse_expr()?;
                self.expect(&Tok::Semicolon)?;
                Ok(Stmt::Expr(e))
            }
        }
    }

    fn parse_let(&mut self) -> Result<Stmt, ParseErr> {
        self.expect(&Tok::Let)?;
        let name = match self.eat() {
            Some(Token {
                tok: Tok::Ident(n),
                ..
            }) => n,
            other => {
                return Err(ParseErr {
                    off: other.map(|t| t.off).unwrap_or(0),
                    msg: "expected identifier after let".into(),
                })
            }
        };
        self.expect(&Tok::Assign)?;
        let e = self.parse_expr()?;
        self.expect(&Tok::Semicolon)?;
        Ok(Stmt::Let(name, e))
    }

    fn parse_if(&mut self) -> Result<Stmt, ParseErr> {
        self.expect(&Tok::If)?;
        let cond = self.parse_expr()?;
        let then_block = self.parse_block()?;
        let else_block = if matches!(self.peek().map(|t| &t.tok), Some(Tok::Else)) {
            self.eat();
            Some(self.parse_block()?)
        } else {
            None
        };
        Ok(Stmt::If(cond, then_block, else_block))
    }

    fn parse_while(&mut self) -> Result<Stmt, ParseErr> {
        self.expect(&Tok::While)?;
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Stmt::While(cond, body))
    }

    fn parse_for(&mut self) -> Result<Stmt, ParseErr> {
        self.expect(&Tok::For)?;
        let name = match self.eat() {
            Some(Token {
                tok: Tok::Ident(n),
                ..
            }) => n,
            other => {
                return Err(ParseErr {
                    off: other.map(|t| t.off).unwrap_or(0),
                    msg: "expected identifier after for".into(),
                })
            }
        };
        self.expect(&Tok::In)?;
        let lo = self.parse_expr()?;
        self.expect(&Tok::DotDot)?;
        let hi = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Stmt::For(name, lo, hi, body))
    }

    fn parse_return(&mut self) -> Result<Stmt, ParseErr> {
        self.expect(&Tok::Return)?;
        if matches!(self.peek().map(|t| &t.tok), Some(Tok::Semicolon)) {
            self.eat();
            return Ok(Stmt::Return(None));
        }
        let e = self.parse_expr()?;
        self.expect(&Tok::Semicolon)?;
        Ok(Stmt::Return(Some(e)))
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseErr> {
        self.expect(&Tok::LBrace)?;
        let mut stmts = Vec::new();
        while !matches!(self.peek().map(|t| &t.tok), Some(Tok::RBrace)) {
            if self.peek().is_none() {
                return Err(ParseErr {
                    off: 0,
                    msg: "unterminated block".into(),
                });
            }
            stmts.push(self.parse_stmt()?);
        }
        self.expect(&Tok::RBrace)?;
        Ok(stmts)
    }

    // expression precedence, lowest to highest:
    //   ||
    //   &&
    //   == !=
    //   < <= > >=
    //   + -
    //   * / %
    //   unary ! -
    //   call / index / primary

    pub fn parse_expr(&mut self) -> Result<Expr, ParseErr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr, ParseErr> {
        let mut lhs = self.parse_and()?;
        while matches!(self.peek().map(|t| &t.tok), Some(Tok::OrOr)) {
            self.eat();
            let rhs = self.parse_and()?;
            lhs = Expr::Binary {
                op: BinOp::Or,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseErr> {
        let mut lhs = self.parse_eq()?;
        while matches!(self.peek().map(|t| &t.tok), Some(Tok::AndAnd)) {
            self.eat();
            let rhs = self.parse_eq()?;
            lhs = Expr::Binary {
                op: BinOp::And,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_eq(&mut self) -> Result<Expr, ParseErr> {
        let mut lhs = self.parse_cmp()?;
        loop {
            let op = match self.peek().map(|t| &t.tok) {
                Some(Tok::Eq) => BinOp::Eq,
                Some(Tok::Neq) => BinOp::Neq,
                _ => break,
            };
            self.eat();
            let rhs = self.parse_cmp()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_cmp(&mut self) -> Result<Expr, ParseErr> {
        let mut lhs = self.parse_add()?;
        loop {
            let op = match self.peek().map(|t| &t.tok) {
                Some(Tok::Lt) => BinOp::Lt,
                Some(Tok::Le) => BinOp::Le,
                Some(Tok::Gt) => BinOp::Gt,
                Some(Tok::Ge) => BinOp::Ge,
                _ => break,
            };
            self.eat();
            let rhs = self.parse_add()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_add(&mut self) -> Result<Expr, ParseErr> {
        let mut lhs = self.parse_mul()?;
        loop {
            let op = match self.peek().map(|t| &t.tok) {
                Some(Tok::Plus) => BinOp::Add,
                Some(Tok::Minus) => BinOp::Sub,
                _ => break,
            };
            self.eat();
            let rhs = self.parse_mul()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_mul(&mut self) -> Result<Expr, ParseErr> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek().map(|t| &t.tok) {
                Some(Tok::Star) => BinOp::Mul,
                Some(Tok::Slash) => BinOp::Div,
                Some(Tok::Percent) => BinOp::Mod,
                _ => break,
            };
            self.eat();
            let rhs = self.parse_unary()?;
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseErr> {
        match self.peek().map(|t| &t.tok) {
            Some(Tok::Bang) => {
                self.eat();
                let rhs = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnOp::Not,
                    rhs: Box::new(rhs),
                })
            }
            Some(Tok::Minus) => {
                self.eat();
                let rhs = self.parse_unary()?;
                Ok(Expr::Unary {
                    op: UnOp::Neg,
                    rhs: Box::new(rhs),
                })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, ParseErr> {
        let mut e = self.parse_primary()?;
        while let Some(Tok::LBracket) = self.peek().map(|t| &t.tok) {
            self.eat();
            let idx = self.parse_expr()?;
            self.expect(&Tok::RBracket)?;
            e = Expr::Index {
                target: Box::new(e),
                index: Box::new(idx),
            };
        }
        Ok(e)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseErr> {
        let t = self.eat().ok_or_else(|| ParseErr {
            off: 0,
            msg: "unexpected end of input".into(),
        })?;
        match t.tok {
            Tok::Nil => Ok(Expr::Nil),
            Tok::True => Ok(Expr::Bool(true)),
            Tok::False => Ok(Expr::Bool(false)),
            Tok::Int(n) => Ok(Expr::Int(n)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::LParen => {
                let e = self.parse_expr()?;
                self.expect(&Tok::RParen)?;
                Ok(e)
            }
            Tok::Ident(name) => {
                // call?
                if matches!(self.peek().map(|tk| &tk.tok), Some(Tok::LParen)) {
                    let lp = self.eat().unwrap();
                    let mut args = Vec::new();
                    if !matches!(self.peek().map(|tk| &tk.tok), Some(Tok::RParen)) {
                        loop {
                            args.push(self.parse_expr()?);
                            match self.peek().map(|tk| &tk.tok) {
                                Some(Tok::Comma) => {
                                    self.eat();
                                }
                                _ => break,
                            }
                        }
                    }
                    self.expect(&Tok::RParen)?;
                    Ok(Expr::Call {
                        callee: name,
                        args,
                        off: lp.off,
                    })
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            other => Err(ParseErr {
                off: t.off,
                msg: format!("unexpected token {:?}", other),
            }),
        }
    }
}
