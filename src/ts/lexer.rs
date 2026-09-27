//! TS-M1 lexer (S0): zero-dependency TypeScript tokenizer.
//!
//! Emits a flat token stream; every token carries line/col and `nl_before`
//! (a line terminator occurred before it — including inside block comments),
//! which is what the parser needs for automatic semicolon insertion.
//!
//! S0 scope: full token set (identifiers, reserved keywords, numbers,
//! strings, no-substitution template literals, punctuators, comments).
//! Template substitutions (`${...}`), regex literals and BigInt literals
//! are rejected with a TS-M1 diagnostic.

use crate::Diag;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Kw(&'static str),
    Number(String),   // raw literal text (value decoding is S2's job)
    Str(String),      // decoded string value
    Template(Vec<TplPart>), // template literal: literal chunks + interpolated token streams
    Punct(&'static str),
    Eof,
}

/// one segment of a template literal: a decoded literal chunk, or the
/// token stream of a `${...}` interpolation (delimiters stripped)
#[derive(Debug, Clone, PartialEq)]
pub enum TplPart {
    Lit(String),
    Expr(Vec<Token>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
    pub col: usize,
    pub nl_before: bool,
}

/// JS/TS reserved words. Contextual keywords (`from`, `as`, `type`, `of`,
/// `any`, `number`, ...) stay `Ident` — the parser matches them by text,
/// which keeps them legal as ordinary identifiers.
const KEYWORDS: &[&str] = &[
    "await", "break", "case", "catch", "class", "const", "continue", "debugger", "default",
    "delete", "do", "else", "enum", "export", "extends", "false", "finally", "for", "function",
    "if", "implements", "import", "in", "instanceof", "interface", "let", "new", "null",
    "package", "private", "protected", "public", "return", "super", "switch", "this", "throw",
    "true", "try", "typeof", "var", "void", "while", "with", "yield",
];

/// Longest-match table; matching walks lengths 4,3,2,1 so order inside the
/// table does not matter.
const PUNCTS: &[&str] = &[
    ">>>=", "...", "===", "!==", ">>>", "<<=", ">>=", "**=", "&&=", "||=", "??=",
    "=>", "==", "!=", "<=", ">=", "&&", "||", "??", "**", "++", "--", "+=", "-=", "*=", "/=",
    "%=", "&=", "|=", "^=", "<<", ">>", "?.",
    "{", "}", "(", ")", "[", "]", ";", ",", "<", ">", "+", "-", "*", "/", "%", "!", "~", "&",
    "|", "^", "?", ":", "=", ".",
];

struct Lexer<'a> {
    chars: Vec<char>,
    i: usize,
    line: usize,
    col: usize,
    file: u32,
    nl: bool,
    _src: &'a str,
}

/// Tokenize a TypeScript source unit. The stream always ends with `Eof`.
pub fn lex(file: u32, src: &str) -> Result<Vec<Token>, Diag> {
    let mut lx = Lexer {
        chars: src.chars().collect(),
        i: 0,
        line: 1,
        col: 1,
        file,
        nl: false,
        _src: src,
    };
    let mut out = Vec::new();
    loop {
        lx.skip_trivia()?;
        let line = lx.line;
        let col = lx.col;
        let nl_before = lx.nl;
        lx.nl = false;
        let tok = lx.next_token()?;
        let eof = tok == Tok::Eof;
        out.push(Token { tok, line, col, nl_before });
        if eof {
            return Ok(out);
        }
    }
}

impl<'a> Lexer<'a> {
    fn err(&self, line: usize, col: usize, msg: impl Into<String>) -> Diag {
        Diag::at("lex", self.file, line, col, msg)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).copied()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.chars.get(self.i + n).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.i).copied()?;
        self.i += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else if c == '\r' {
            // \r\n counts as one line break
            if self.peek() != Some('\n') {
                self.line += 1;
                self.col = 1;
            }
        } else {
            self.col += 1;
        }
        Some(c)
    }

    /// whitespace and comments; records whether a line terminator was crossed
    fn skip_trivia(&mut self) -> Result<(), Diag> {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    if c == '\n' || c == '\r' {
                        self.nl = true;
                    }
                    self.bump();
                }
                Some('/') if self.peek_at(1) == Some('/') => {
                    while let Some(c) = self.peek() {
                        if c == '\n' || c == '\r' {
                            break;
                        }
                        self.bump();
                    }
                }
                Some('/') if self.peek_at(1) == Some('*') => {
                    let (line, col) = (self.line, self.col);
                    self.bump();
                    self.bump();
                    loop {
                        match self.peek() {
                            None => return Err(self.err(line, col, "unterminated block comment")),
                            Some('*') if self.peek_at(1) == Some('/') => {
                                self.bump();
                                self.bump();
                                break;
                            }
                            Some(c) => {
                                if c == '\n' || c == '\r' {
                                    self.nl = true;
                                }
                                self.bump();
                            }
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn next_token(&mut self) -> Result<Tok, Diag> {
        let c = match self.peek() {
            None => return Ok(Tok::Eof),
            Some(c) => c,
        };
        if c.is_alphabetic() || c == '_' || c == '$' {
            return Ok(self.ident());
        }
        if c.is_ascii_digit() {
            return self.number();
        }
        if c == '.' && self.peek_at(1).map(|d| d.is_ascii_digit()).unwrap_or(false) {
            return self.number();
        }
        if c == '"' || c == '\'' {
            return self.string(c);
        }
        if c == '`' {
            return self.template();
        }
        // `?.5` is a ternary followed by a number, not optional chaining
        if c == '?' && self.peek_at(1) == Some('.') && self.peek_at(2).map(|d| d.is_ascii_digit()).unwrap_or(false) {
            self.bump();
            return Ok(Tok::Punct("?"));
        }
        if let Some(p) = self.match_punct() {
            for _ in 0..p.chars().count() {
                self.bump();
            }
            return Ok(Tok::Punct(p));
        }
        Err(self.err(self.line, self.col, format!("unexpected character '{c}'")))
    }

    fn ident(&mut self) -> Tok {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' || c == '$' {
                s.push(c);
                self.bump();
            } else {
                break;
            }
        }
        if let Some(k) = KEYWORDS.iter().find(|k| **k == s) {
            Tok::Kw(k)
        } else {
            Tok::Ident(s)
        }
    }

    fn match_punct(&self) -> Option<&'static str> {
        for len in [4usize, 3, 2, 1] {
            if self.i + len <= self.chars.len() {
                let cand: String = self.chars[self.i..self.i + len].iter().collect();
                if let Some(p) = PUNCTS.iter().find(|p| p.len() == len && **p == cand) {
                    return Some(p);
                }
            }
        }
        None
    }

    fn number(&mut self) -> Result<Tok, Diag> {
        let (line, col) = (self.line, self.col);
        let mut s = String::new();
        if self.peek() == Some('0') {
            if let Some(radix) = self.peek_at(1) {
                if matches!(radix, 'x' | 'X' | 'o' | 'O' | 'b' | 'B') {
                    s.push('0');
                    s.push(radix);
                    self.bump();
                    self.bump();
                    let mut any = false;
                    while let Some(c) = self.peek() {
                        if c.is_ascii_alphanumeric() || c == '_' {
                            any = true;
                            s.push(c);
                            self.bump();
                        } else {
                            break;
                        }
                    }
                    if !any {
                        return Err(self.err(line, col, "missing digits after radix prefix"));
                    }
                    return self.number_suffix(s, line, col);
                }
            }
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '_' {
                s.push(c);
                self.bump();
            } else {
                break;
            }
        }
        if self.peek() == Some('.') {
            s.push('.');
            self.bump();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() || c == '_' {
                    s.push(c);
                    self.bump();
                } else {
                    break;
                }
            }
        }
        if matches!(self.peek(), Some('e') | Some('E')) {
            s.push('e');
            self.bump();
            if matches!(self.peek(), Some('+') | Some('-')) {
                s.push(self.bump().unwrap());
            }
            let mut any = false;
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() || c == '_' {
                    any = true;
                    s.push(c);
                    self.bump();
                } else {
                    break;
                }
            }
            if !any {
                return Err(self.err(line, col, "missing digits in exponent"));
            }
        }
        self.number_suffix(s, line, col)
    }

    fn number_suffix(&mut self, s: String, line: usize, col: usize) -> Result<Tok, Diag> {
        if self.peek() == Some('n') {
            return Err(self.err(line, col, "BigInt literals are not supported in TS-M1"));
        }
        Ok(Tok::Number(s))
    }

    /// shared escape decoding for strings and template bodies; `None` is a
    /// line continuation (backslash + newline contributes no character)
    fn string_body(&mut self, quote: char) -> Result<String, Diag> {
        let mut out = String::new();
        loop {
            let (line, col) = (self.line, self.col);
            match self.bump() {
                None => return Err(self.err(line, col, "unterminated string literal")),
                Some(c) if c == quote => return Ok(out),
                Some('\\') => {
                    if let Some(ch) = self.escape(line, col)? {
                        out.push(ch);
                    }
                }
                Some(c) => out.push(c),
            }
        }
    }

    fn escape(&mut self, line: usize, col: usize) -> Result<Option<char>, Diag> {
        let e = self
            .bump()
            .ok_or_else(|| self.err(line, col, "unterminated escape sequence"))?;
        Ok(Some(match e {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'b' => '\u{8}',
            'f' => '\u{c}',
            'v' => '\u{b}',
            '0' => '\0',
            '\'' => '\'',
            '"' => '"',
            '`' => '`',
            '\\' => '\\',
            '$' => '$',
            '\n' | '\r' => return Ok(None), // line continuation: nothing
            'x' => {
                let h = self.hex_digits(2, line, col)?;
                char::from_u32(h).ok_or_else(|| self.err(line, col, "invalid \\x escape"))?
            }
            'u' => {
                if self.peek() == Some('{') {
                    self.bump();
                    let mut n: u32 = 0;
                    let mut any = false;
                    while let Some(c) = self.peek() {
                        if c == '}' {
                            break;
                        }
                        let d = c.to_digit(16).ok_or_else(|| self.err(line, col, "invalid \\u{...} escape"))?;
                        n = n.saturating_mul(16).saturating_add(d);
                        any = true;
                        self.bump();
                    }
                    if !any || self.bump() != Some('}') {
                        return Err(self.err(line, col, "invalid \\u{...} escape"));
                    }
                    char::from_u32(n).ok_or_else(|| self.err(line, col, "invalid \\u{...} escape"))?
                } else {
                    let h = self.hex_digits(4, line, col)?;
                    char::from_u32(h).ok_or_else(|| self.err(line, col, "invalid \\u escape"))?
                }
            }
            other => return Err(self.err(line, col, format!("unknown escape sequence '\\{other}'"))),
        }))
    }

    fn hex_digits(&mut self, count: usize, line: usize, col: usize) -> Result<u32, Diag> {
        let mut n: u32 = 0;
        for _ in 0..count {
            let c = self.bump().ok_or_else(|| self.err(line, col, "unterminated escape sequence"))?;
            let d = c.to_digit(16).ok_or_else(|| self.err(line, col, "invalid hex escape"))?;
            n = n * 16 + d;
        }
        Ok(n)
    }

    fn string(&mut self, quote: char) -> Result<Tok, Diag> {
        self.bump(); // opening quote
        Ok(Tok::Str(self.string_body(quote)?))
    }

    fn template(&mut self) -> Result<Tok, Diag> {
        self.bump(); // opening backtick
        let mut parts: Vec<TplPart> = Vec::new();
        let mut lit = String::new();
        loop {
            let (l, c) = (self.line, self.col);
            match self.bump() {
                None => return Err(self.err(l, c, "unterminated template literal")),
                Some('`') => break,
                Some('$') if self.peek() == Some('{') => {
                    self.bump(); // `{`
                    parts.push(TplPart::Lit(std::mem::take(&mut lit)));
                    let toks = self.template_expr(l, c)?;
                    parts.push(TplPart::Expr(toks));
                }
                Some('\\') => {
                    if let Some(ch) = self.escape(l, c)? {
                        lit.push(ch);
                    }
                }
                Some(ch) => lit.push(ch),
            }
        }
        parts.push(TplPart::Lit(lit));
        Ok(Tok::Template(parts))
    }

    /// token stream of one `${...}` interpolation up to its matching `}`
    fn template_expr(&mut self, line: usize, col: usize) -> Result<Vec<Token>, Diag> {
        let mut out = Vec::new();
        let mut depth: i32 = 0;
        loop {
            self.skip_trivia()?;
            let (l, c) = (self.line, self.col);
            let nl = std::mem::take(&mut self.nl);
            let tok = self.next_token()?;
            if tok == Tok::Eof {
                return Err(self.err(line, col, "unterminated template substitution"));
            }
            if tok == Tok::Punct("}") && depth == 0 {
                out.push(Token { tok: Tok::Eof, line: l, col: c, nl_before: false });
                return Ok(out);
            }
            if tok == Tok::Punct("{") {
                depth += 1;
            } else if tok == Tok::Punct("}") {
                depth -= 1;
            }
            out.push(Token { tok, line: l, col: c, nl_before: nl });
        }
    }
}
