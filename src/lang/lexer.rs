//! tokenizer for .tape source files. small on purpose.
//!
//! tokens carry their byte offset into the source string. the interpreter
//! later combines (file_path, byte_offset_of_call) into a stable site id
//! via the same FNV-1a used by the site!() macro.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    // literals
    Int(i64),
    Str(String),
    Ident(String),

    // keywords
    Let,
    If,
    Else,
    While,
    For,
    In,
    Return,
    True,
    False,
    Nil,

    // punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semicolon,
    Assign,
    DotDot,

    // operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,
    Neq,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    Bang,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub off: usize,
}

#[derive(Debug)]
pub struct LexErr {
    pub off: usize,
    pub msg: String,
}

impl std::fmt::Display for LexErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "lex error at byte {}: {}", self.off, self.msg)
    }
}

impl std::error::Error for LexErr {}

pub fn lex(src: &str) -> Result<Vec<Token>, LexErr> {
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let mut out = Vec::new();
    while i < bytes.len() {
        let c = bytes[i];
        // whitespace
        if c == b' ' || c == b'\t' || c == b'\r' || c == b'\n' {
            i += 1;
            continue;
        }
        // comment: # to end of line
        if c == b'#' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // string literal
        if c == b'"' {
            let start = i;
            i += 1;
            let mut s = String::new();
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' {
                    if i + 1 >= bytes.len() {
                        return Err(LexErr {
                            off: i,
                            msg: "unterminated escape".into(),
                        });
                    }
                    let esc = bytes[i + 1];
                    match esc {
                        b'n' => s.push('\n'),
                        b't' => s.push('\t'),
                        b'"' => s.push('"'),
                        b'\\' => s.push('\\'),
                        _ => {
                            return Err(LexErr {
                                off: i,
                                msg: format!("unknown escape \\{}", esc as char),
                            })
                        }
                    }
                    i += 2;
                } else {
                    s.push(bytes[i] as char);
                    i += 1;
                }
            }
            if i >= bytes.len() {
                return Err(LexErr {
                    off: start,
                    msg: "unterminated string".into(),
                });
            }
            i += 1; // closing quote
            out.push(Token {
                tok: Tok::Str(s),
                off: start,
            });
            continue;
        }
        // number
        if c.is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let num: i64 = src[start..i].parse().map_err(|_| LexErr {
                off: start,
                msg: "bad integer".into(),
            })?;
            out.push(Token {
                tok: Tok::Int(num),
                off: start,
            });
            continue;
        }
        // identifier / keyword. dots allowed inside so "clock.now" is one ident.
        if c.is_ascii_alphabetic() || c == b'_' {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'.')
            {
                i += 1;
            }
            let s = &src[start..i];
            let tok = match s {
                "let" => Tok::Let,
                "if" => Tok::If,
                "else" => Tok::Else,
                "while" => Tok::While,
                "for" => Tok::For,
                "in" => Tok::In,
                "return" => Tok::Return,
                "true" => Tok::True,
                "false" => Tok::False,
                "nil" => Tok::Nil,
                _ => Tok::Ident(s.to_string()),
            };
            out.push(Token { tok, off: start });
            continue;
        }
        // punctuation / operators
        let start = i;
        let two = if i + 1 < bytes.len() {
            Some(&src[i..i + 2])
        } else {
            None
        };
        let (tok, len) = match two {
            Some("==") => (Tok::Eq, 2),
            Some("!=") => (Tok::Neq, 2),
            Some("<=") => (Tok::Le, 2),
            Some(">=") => (Tok::Ge, 2),
            Some("&&") => (Tok::AndAnd, 2),
            Some("||") => (Tok::OrOr, 2),
            Some("..") => (Tok::DotDot, 2),
            _ => match c {
                b'(' => (Tok::LParen, 1),
                b')' => (Tok::RParen, 1),
                b'{' => (Tok::LBrace, 1),
                b'}' => (Tok::RBrace, 1),
                b'[' => (Tok::LBracket, 1),
                b']' => (Tok::RBracket, 1),
                b',' => (Tok::Comma, 1),
                b';' => (Tok::Semicolon, 1),
                b'=' => (Tok::Assign, 1),
                b'+' => (Tok::Plus, 1),
                b'-' => (Tok::Minus, 1),
                b'*' => (Tok::Star, 1),
                b'/' => (Tok::Slash, 1),
                b'%' => (Tok::Percent, 1),
                b'<' => (Tok::Lt, 1),
                b'>' => (Tok::Gt, 1),
                b'!' => (Tok::Bang, 1),
                _ => {
                    return Err(LexErr {
                        off: i,
                        msg: format!("unexpected character {:?}", c as char),
                    })
                }
            },
        };
        out.push(Token { tok, off: start });
        i += len;
    }
    Ok(out)
}
