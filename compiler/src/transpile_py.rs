/// Python → ErnosPlain Transpiler
///
/// Parses a subset of Python 3 source code and emits equivalent ErnosPlain (.ep) code.
///
/// Architecture:
///   Python source → Python tokenizer → Python AST → ErnosPlain emitter → .ep file
///
/// Supported Python constructs:
///   - Variables and assignments (including augmented: +=, -=, *=, //=, %=)
///   - Functions (def) with type hints (optional)
///   - Control flow: if/elif/else, for, while, break, continue
///   - Print statements → display
///   - Input() → read_line()
///   - Lists: [], append, len, pop, indexing
///   - Dicts: {}, d[k] = v, len(d), del d[k], 'in'
///   - Strings: f-strings, +, len, upper, lower, strip, split, replace, find, in
///   - Arithmetic: +, -, *, //, %, **
///   - Comparisons: ==, !=, <, >, <=, >=, and, or, not
///   - Return statements
///   - Classes (basic) → structs
///   - Try/except → try/check
///   - Import statements (noted as comments)
///   - Comments preserved
///   - Type hints mapped to Ernos types

use std::collections::HashMap;

// ============ Python Tokenizer ============

#[derive(Debug, Clone, PartialEq)]
pub enum PyToken {
    Ident(String),
    IntLit(i64),
    FloatLit(f64),
    StrLit(String),
    FStrLit(String),
    // Operators
    Plus, Minus, Star, DoubleStar, Slash, DoubleSlash, Percent,
    Eq, EqEq, NotEq, Lt, Gt, LtEq, GtEq,
    PlusEq, MinusEq, StarEq, SlashEq, PercentEq,
    // Delimiters
    LParen, RParen, LBracket, RBracket, LBrace, RBrace,
    Comma, Colon, Dot, Arrow, Semi, At,
    // Special
    Newline,
    Indent(usize),  // indentation level
    Hash,
    Eof,
}

pub struct PyLexer {
    chars: Vec<char>,
    pos: usize,
    at_line_start: bool,
}

impl PyLexer {
    pub fn new(input: &str) -> Self {
        PyLexer {
            chars: input.chars().collect(),
            pos: 0,
            at_line_start: true,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        self.pos += 1;
        c
    }

    pub fn tokenize(&mut self) -> Vec<PyToken> {
        let mut tokens = Vec::new();

        loop {
            if self.at_line_start {
                // Count indentation
                let mut indent = 0;
                while self.peek() == Some(' ') {
                    self.advance();
                    indent += 1;
                }
                while self.peek() == Some('\t') {
                    self.advance();
                    indent += 4;
                }
                // Skip blank lines
                if self.peek() == Some('\n') {
                    self.advance();
                    continue;
                }
                if self.peek() == Some('#') {
                    // Comment line — skip
                    while self.peek() != Some('\n') && self.peek().is_some() {
                        self.advance();
                    }
                    if self.peek() == Some('\n') { self.advance(); }
                    continue;
                }
                if self.peek().is_none() { break; }
                tokens.push(PyToken::Indent(indent));
                self.at_line_start = false;
            }

            // Skip inline whitespace
            while self.peek() == Some(' ') || self.peek() == Some('\t') {
                self.advance();
            }

            match self.peek() {
                None => break,
                Some('\n') => {
                    self.advance();
                    tokens.push(PyToken::Newline);
                    self.at_line_start = true;
                }
                Some('#') => {
                    // Skip comment to end of line
                    while self.peek() != Some('\n') && self.peek().is_some() {
                        self.advance();
                    }
                }
                Some('\\') => {
                    // Line continuation
                    self.advance();
                    if self.peek() == Some('\n') { self.advance(); }
                }
                Some('"') | Some('\'') => {
                    let quote = self.peek().unwrap();
                    self.advance();

                    // Check for triple quotes
                    let is_triple = if self.peek() == Some(quote) {
                        let save = self.pos;
                        self.advance();
                        if self.peek() == Some(quote) {
                            self.advance();
                            true
                        } else {
                            self.pos = save;
                            false
                        }
                    } else {
                        false
                    };

                    let mut s = String::new();
                    if is_triple {
                        loop {
                            match self.advance() {
                                Some(c) if c == quote => {
                                    if self.peek() == Some(quote) {
                                        self.advance();
                                        if self.peek() == Some(quote) {
                                            self.advance();
                                            break;
                                        }
                                        s.push(c);
                                        s.push(c);
                                    } else {
                                        s.push(c);
                                    }
                                }
                                Some('\\') => {
                                    if let Some(esc) = self.advance() {
                                        match esc {
                                            'n' => s.push('\n'),
                                            't' => s.push('\t'),
                                            '\\' => s.push('\\'),
                                            '\'' => s.push('\''),
                                            '"' => s.push('"'),
                                            _ => { s.push('\\'); s.push(esc); }
                                        }
                                    }
                                }
                                Some(c) => s.push(c),
                                None => break,
                            }
                        }
                    } else {
                        loop {
                            match self.advance() {
                                Some(c) if c == quote => break,
                                Some('\\') => {
                                    if let Some(esc) = self.advance() {
                                        match esc {
                                            'n' => s.push('\n'),
                                            't' => s.push('\t'),
                                            '\\' => s.push('\\'),
                                            '\'' => s.push('\''),
                                            '"' => s.push('"'),
                                            _ => { s.push('\\'); s.push(esc); }
                                        }
                                    }
                                }
                                Some(c) => s.push(c),
                                None => break,
                            }
                        }
                    }
                    tokens.push(PyToken::StrLit(s));
                }
                Some('f') if matches!(self.chars.get(self.pos + 1), Some('"') | Some('\'')) => {
                    self.advance(); // f
                    let quote = self.peek().unwrap();
                    self.advance();
                    let mut s = String::new();
                    loop {
                        match self.advance() {
                            Some(c) if c == quote => break,
                            Some('\\') => {
                                if let Some(esc) = self.advance() {
                                    match esc {
                                        'n' => s.push('\n'),
                                        't' => s.push('\t'),
                                        '\\' => s.push('\\'),
                                        _ => { s.push('\\'); s.push(esc); }
                                    }
                                }
                            }
                            Some(c) => s.push(c),
                            None => break,
                        }
                    }
                    tokens.push(PyToken::FStrLit(s));
                }
                Some(c) if c.is_ascii_digit() => {
                    let mut num = String::new();
                    while let Some(c) = self.peek() {
                        if c.is_ascii_digit() || c == '_' {
                            if c != '_' { num.push(c); }
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    if self.peek() == Some('.') && self.chars.get(self.pos + 1).map_or(false, |c| c.is_ascii_digit()) {
                        num.push('.');
                        self.advance();
                        while let Some(c) = self.peek() {
                            if c.is_ascii_digit() || c == '_' {
                                if c != '_' { num.push(c); }
                                self.advance();
                            } else {
                                break;
                            }
                        }
                        tokens.push(PyToken::FloatLit(num.parse().unwrap_or(0.0)));
                    } else {
                        tokens.push(PyToken::IntLit(num.parse().unwrap_or(0)));
                    }
                }
                Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                    let mut ident = String::new();
                    while let Some(c) = self.peek() {
                        if c.is_ascii_alphanumeric() || c == '_' {
                            ident.push(c);
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    tokens.push(PyToken::Ident(ident));
                }
                Some('+') => { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(PyToken::PlusEq); } else { tokens.push(PyToken::Plus); } }
                Some('-') => {
                    self.advance();
                    if self.peek() == Some('=') { self.advance(); tokens.push(PyToken::MinusEq); }
                    else if self.peek() == Some('>') { self.advance(); tokens.push(PyToken::Arrow); }
                    else { tokens.push(PyToken::Minus); }
                }
                Some('*') => {
                    self.advance();
                    if self.peek() == Some('*') { self.advance(); tokens.push(PyToken::DoubleStar); }
                    else if self.peek() == Some('=') { self.advance(); tokens.push(PyToken::StarEq); }
                    else { tokens.push(PyToken::Star); }
                }
                Some('/') => {
                    self.advance();
                    if self.peek() == Some('/') { self.advance(); tokens.push(PyToken::DoubleSlash); }
                    else if self.peek() == Some('=') { self.advance(); tokens.push(PyToken::SlashEq); }
                    else { tokens.push(PyToken::Slash); }
                }
                Some('%') => { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(PyToken::PercentEq); } else { tokens.push(PyToken::Percent); } }
                Some('=') => { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(PyToken::EqEq); } else { tokens.push(PyToken::Eq); } }
                Some('!') => { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(PyToken::NotEq); } else { tokens.push(PyToken::Ident("not".into())); } }
                Some('<') => { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(PyToken::LtEq); } else { tokens.push(PyToken::Lt); } }
                Some('>') => { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(PyToken::GtEq); } else { tokens.push(PyToken::Gt); } }
                Some('(') => { self.advance(); tokens.push(PyToken::LParen); }
                Some(')') => { self.advance(); tokens.push(PyToken::RParen); }
                Some('[') => { self.advance(); tokens.push(PyToken::LBracket); }
                Some(']') => { self.advance(); tokens.push(PyToken::RBracket); }
                Some('{') => { self.advance(); tokens.push(PyToken::LBrace); }
                Some('}') => { self.advance(); tokens.push(PyToken::RBrace); }
                Some(',') => { self.advance(); tokens.push(PyToken::Comma); }
                Some(':') => { self.advance(); tokens.push(PyToken::Colon); }
                Some('.') => { self.advance(); tokens.push(PyToken::Dot); }
                Some(';') => { self.advance(); tokens.push(PyToken::Semi); }
                Some('@') => { self.advance(); tokens.push(PyToken::At); }
                Some(_) => { self.advance(); }
            }
        }

        tokens.push(PyToken::Newline);
        tokens.push(PyToken::Eof);
        tokens
    }
}

// ============ Python AST (Simplified) ============

#[derive(Debug, Clone)]
pub enum PyExpr {
    Int(i64),
    Float(f64),
    Str(String),
    FStr(String),
    Bool(bool),
    None,
    Name(String),
    BinOp(Box<PyExpr>, String, Box<PyExpr>),
    UnaryOp(String, Box<PyExpr>),
    Compare(Box<PyExpr>, Vec<(String, PyExpr)>),
    BoolOp(String, Vec<PyExpr>),
    Call(Box<PyExpr>, Vec<PyExpr>, Vec<(String, PyExpr)>),
    Attribute(Box<PyExpr>, String),
    Subscript(Box<PyExpr>, Box<PyExpr>),
    List(Vec<PyExpr>),
    Dict(Vec<(PyExpr, PyExpr)>),
    IfExpr(Box<PyExpr>, Box<PyExpr>, Box<PyExpr>),
    ListComp(Box<PyExpr>, String, Box<PyExpr>, Option<Box<PyExpr>>),
    Await(Box<PyExpr>),
}

#[derive(Debug, Clone)]
pub enum PyStmt {
    Assign(PyExpr, PyExpr),
    AugAssign(PyExpr, String, PyExpr),
    Expr(PyExpr),
    Return(Option<PyExpr>),
    If(PyExpr, Vec<PyStmt>, Vec<(PyExpr, Vec<PyStmt>)>, Option<Vec<PyStmt>>),
    While(PyExpr, Vec<PyStmt>),
    For(String, PyExpr, Vec<PyStmt>),
    FuncDef(String, Vec<(String, Option<String>)>, Option<String>, Vec<PyStmt>, Vec<PyExpr>, bool),
    ClassDef(String, Vec<PyStmt>, Vec<PyExpr>),
    Import(String, Option<String>),
    FromImport(String, Vec<String>),
    Print(Vec<PyExpr>),
    Break,
    Continue,
    Pass,
    Comment(String),
    Del(PyExpr),
    Try(Vec<PyStmt>, Vec<(Option<String>, Option<String>, Vec<PyStmt>)>),
}

// ============ Python Parser ============

pub struct PyParser {
    tokens: Vec<PyToken>,
    pos: usize,
}

impl PyParser {
    pub fn new(tokens: Vec<PyToken>) -> Self {
        PyParser { tokens, pos: 0 }
    }

    fn peek(&self) -> &PyToken {
        self.tokens.get(self.pos).unwrap_or(&PyToken::Eof)
    }

    fn advance(&mut self) -> PyToken {
        let tok = self.tokens.get(self.pos).cloned().unwrap_or(PyToken::Eof);
        self.pos += 1;
        tok
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), PyToken::Newline) {
            self.advance();
        }
    }

    fn expect_newline(&mut self) {
        if matches!(self.peek(), PyToken::Newline) {
            self.advance();
        }
    }

    /// Parse an entire Python file → list of statements
    pub fn parse(&mut self) -> Vec<PyStmt> {
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek(), PyToken::Eof) { break; }
            if let PyToken::Indent(_) = self.peek() {
                self.advance(); // consume top-level indent (should be 0)
            }
            if matches!(self.peek(), PyToken::Newline | PyToken::Eof) { continue; }
            if let Some(s) = self.parse_stmt() {
                stmts.push(s);
            } else {
                let tok = self.peek().clone();
                stmts.push(PyStmt::Comment(format!("Unrecognized syntax near token: {:?}", tok)));
                while !matches!(self.peek(), PyToken::Newline | PyToken::Eof) {
                    self.advance();
                }
                self.expect_newline();
            }
        }
        stmts
    }

    fn parse_stmt(&mut self) -> Option<PyStmt> {
        let mut decorators = Vec::new();
        loop {
            self.skip_newlines();
            if let PyToken::Indent(0) = self.peek() {
                self.advance();
            }
            self.skip_newlines();
            if matches!(self.peek(), PyToken::At) {
                self.advance(); // consume @
                if let Some(dec) = self.parse_expr() {
                    decorators.push(dec);
                }
                self.expect_newline();
            } else {
                break;
            }
        }

        self.skip_newlines();
        if let PyToken::Indent(0) = self.peek() {
            self.advance();
        }
        self.skip_newlines();
        let mut is_async = false;
        if let PyToken::Ident(ref s) = self.peek() {
            if s == "async" {
                let mut next_pos = self.pos + 1;
                while next_pos < self.tokens.len() && matches!(self.tokens[next_pos], PyToken::Newline) {
                    next_pos += 1;
                }
                if next_pos < self.tokens.len() {
                    if let PyToken::Ident(ref next_s) = self.tokens[next_pos] {
                        if next_s == "def" {
                            self.advance(); // consume async
                            self.skip_newlines();
                            is_async = true;
                        }
                    }
                }
            }
        }

        let mut stmt = match self.peek().clone() {
            PyToken::Ident(ref s) => {
                match s.as_str() {
                    "def" => self.parse_funcdef(),
                    "class" => self.parse_classdef(),
                    "if" => self.parse_if(),
                    "while" => self.parse_while(),
                    "for" => self.parse_for(),
                    "return" => self.parse_return(),
                    "print" => { self.advance(); self.parse_print() }
                    "import" => self.parse_import(),
                    "from" => self.parse_from_import(),
                    "break" => { self.advance(); self.expect_newline(); Some(PyStmt::Break) }
                    "continue" => { self.advance(); self.expect_newline(); Some(PyStmt::Continue) }
                    "pass" => { self.advance(); self.expect_newline(); Some(PyStmt::Pass) }
                    "del" => self.parse_del(),
                    "try" => self.parse_try(),
                    _ => self.parse_assign_or_expr(),
                }
            }
            PyToken::Hash => {
                self.advance();
                let mut comment = String::new();
                while !matches!(self.peek(), PyToken::Newline | PyToken::Eof) {
                    match self.advance() {
                        PyToken::Ident(s) => { if !comment.is_empty() { comment.push(' '); } comment.push_str(&s); }
                        PyToken::IntLit(n) => { if !comment.is_empty() { comment.push(' '); } comment.push_str(&n.to_string()); }
                        _ => {}
                    }
                }
                self.expect_newline();
                Some(PyStmt::Comment(comment))
            }
            _ => {
                self.parse_assign_or_expr()
            }
        };

        if is_async {
            if let Some(PyStmt::FuncDef(_, _, _, _, _, ref mut async_flag)) = stmt {
                *async_flag = true;
            }
        }

        if !decorators.is_empty() {
            if let Some(ref mut s) = stmt {
                match s {
                    PyStmt::FuncDef(_, _, _, _, ref mut decs, _) => {
                        *decs = decorators;
                    }
                    PyStmt::ClassDef(_, _, ref mut decs) => {
                        *decs = decorators;
                    }
                    _ => {}
                }
            }
        }

        stmt
    }

    fn parse_funcdef(&mut self) -> Option<PyStmt> {
        self.advance(); // def
        let name = if let PyToken::Ident(s) = self.advance() { s } else { return None };
        if !matches!(self.advance(), PyToken::LParen) { return None; }

        let mut params = Vec::new();
        while !matches!(self.peek(), PyToken::RParen | PyToken::Eof) {
            self.skip_newlines();
            if let PyToken::Indent(_) = self.peek() { self.advance(); }
            self.skip_newlines();
            if matches!(self.peek(), PyToken::RParen | PyToken::Eof) { break; }

            if matches!(self.peek(), PyToken::Star | PyToken::DoubleStar) {
                self.advance();
            }
            if let PyToken::Ident(pname) = self.advance() {
                if pname == "self" {
                    // Skip self parameter for methods
                    self.skip_newlines();
                    if let PyToken::Indent(_) = self.peek() { self.advance(); }
                    self.skip_newlines();
                    if matches!(self.peek(), PyToken::Comma) { self.advance(); }
                    continue;
                }
                let type_hint = if matches!(self.peek(), PyToken::Colon) {
                    self.advance();
                    self.skip_newlines();
                    if let PyToken::Indent(_) = self.peek() { self.advance(); }
                    self.skip_newlines();
                    self.parse_type_hint()
                } else {
                    None
                };
                // Skip default value
                self.skip_newlines();
                if let PyToken::Indent(_) = self.peek() { self.advance(); }
                self.skip_newlines();
                if matches!(self.peek(), PyToken::Eq) {
                    self.advance();
                    self.skip_newlines();
                    if let PyToken::Indent(_) = self.peek() { self.advance(); }
                    self.skip_newlines();
                    let _ = self.parse_expr();
                }
                params.push((pname, type_hint));
            }
            self.skip_newlines();
            if let PyToken::Indent(_) = self.peek() { self.advance(); }
            self.skip_newlines();
            if matches!(self.peek(), PyToken::Comma) { self.advance(); }
        }
        self.skip_newlines();
        if let PyToken::Indent(_) = self.peek() { self.advance(); }
        self.skip_newlines();
        if matches!(self.peek(), PyToken::RParen) { self.advance(); }

        // Return type hint
        self.skip_newlines();
        if let PyToken::Indent(_) = self.peek() { self.advance(); }
        self.skip_newlines();
        let ret_hint = if matches!(self.peek(), PyToken::Arrow) {
            self.advance();
            self.skip_newlines();
            if let PyToken::Indent(_) = self.peek() { self.advance(); }
            self.skip_newlines();
            self.parse_type_hint()
        } else {
            None
        };

        self.skip_newlines();
        if let PyToken::Indent(_) = self.peek() { self.advance(); }
        self.skip_newlines();
        if matches!(self.peek(), PyToken::Colon) { self.advance(); }
        self.expect_newline();

        let body = self.parse_block();

        Some(PyStmt::FuncDef(name, params, ret_hint, body, Vec::new(), false))
    }

    fn parse_type_hint(&mut self) -> Option<String> {
        let mut parts = Vec::new();
        while let PyToken::Ident(s) = self.peek() {
            parts.push(s.clone());
            self.advance();
            if matches!(self.peek(), PyToken::Dot) {
                parts.push(".".to_string());
                self.advance();
            } else {
                break;
            }
        }
        if parts.is_empty() {
            return None;
        }

        let type_name = parts.concat();
        // Handle Optional[X], List[X], Dict[K,V], etc.
        if matches!(self.peek(), PyToken::LBracket) {
            self.advance();
            let mut depth = 1;
            while depth > 0 {
                match self.advance() {
                    PyToken::LBracket => depth += 1,
                    PyToken::RBracket => depth -= 1,
                    PyToken::Eof => break,
                    _ => {}
                }
            }
        }
        Some(type_name)
    }

    fn parse_classdef(&mut self) -> Option<PyStmt> {
        self.advance(); // class
        let _name = if let PyToken::Ident(s) = self.advance() { s } else { return None };

        // Skip parent classes
        if matches!(self.peek(), PyToken::LParen) {
            self.advance();
            let mut depth = 1;
            while depth > 0 {
                match self.advance() {
                    PyToken::LParen => depth += 1,
                    PyToken::RParen => depth -= 1,
                    PyToken::Eof => break,
                    _ => {}
                }
            }
        }
        if matches!(self.peek(), PyToken::Colon) { self.advance(); }
        self.expect_newline();

        let body = self.parse_block();
        Some(PyStmt::ClassDef(_name, body, Vec::new()))
    }

    fn parse_if(&mut self) -> Option<PyStmt> {
        self.advance(); // if
        let cond = self.parse_expr()?;
        if matches!(self.peek(), PyToken::Colon) { self.advance(); }
        self.expect_newline();
        let body = self.parse_block();

        let mut elifs = Vec::new();
        let mut else_body = None;

        loop {
            self.skip_newlines();
            if let PyToken::Indent(_) = self.peek() {
                let save = self.pos;
                self.advance();
                if let PyToken::Ident(s) = self.peek() {
                    if s == "elif" {
                        self.advance();
                        let econd = self.parse_expr().unwrap_or(PyExpr::Bool(true));
                        if matches!(self.peek(), PyToken::Colon) { self.advance(); }
                        self.expect_newline();
                        let ebody = self.parse_block();
                        elifs.push((econd, ebody));
                        continue;
                    } else if s == "else" {
                        self.advance();
                        if matches!(self.peek(), PyToken::Colon) { self.advance(); }
                        self.expect_newline();
                        else_body = Some(self.parse_block());
                        break;
                    }
                }
                self.pos = save;
                break;
            } else {
                break;
            }
        }

        Some(PyStmt::If(cond, body, elifs, else_body))
    }

    fn parse_while(&mut self) -> Option<PyStmt> {
        self.advance(); // while
        let cond = self.parse_expr()?;
        if matches!(self.peek(), PyToken::Colon) { self.advance(); }
        self.expect_newline();
        let body = self.parse_block();
        Some(PyStmt::While(cond, body))
    }

    fn parse_for(&mut self) -> Option<PyStmt> {
        self.advance(); // for
        let var = if let PyToken::Ident(s) = self.advance() { s } else { return None };
        // Skip 'in'
        if let PyToken::Ident(s) = self.peek() {
            if s == "in" { self.advance(); }
        }
        let iter = self.parse_expr()?;
        if matches!(self.peek(), PyToken::Colon) { self.advance(); }
        self.expect_newline();
        let body = self.parse_block();
        Some(PyStmt::For(var, iter, body))
    }

    fn parse_return(&mut self) -> Option<PyStmt> {
        self.advance(); // return
        if matches!(self.peek(), PyToken::Newline | PyToken::Eof) {
            self.expect_newline();
            return Some(PyStmt::Return(None));
        }
        let val = self.parse_expr();
        self.expect_newline();
        Some(PyStmt::Return(val))
    }

    fn parse_print(&mut self) -> Option<PyStmt> {
        if !matches!(self.peek(), PyToken::LParen) {
            // print without parens (Python 2 style)
            let expr = self.parse_expr()?;
            self.expect_newline();
            return Some(PyStmt::Print(vec![expr]));
        }
        self.advance(); // (
        let mut args = Vec::new();
        while !matches!(self.peek(), PyToken::RParen | PyToken::Eof) {
            self.skip_newlines();
            if let PyToken::Indent(_) = self.peek() { self.advance(); }
            self.skip_newlines();
            if matches!(self.peek(), PyToken::RParen | PyToken::Eof) { break; }

            // Skip keyword arguments like end=, sep=
            if let PyToken::Ident(s) = self.peek() {
                let _s = s.clone();
                let save = self.pos;
                self.advance();
                self.skip_newlines();
                if let PyToken::Indent(_) = self.peek() { self.advance(); }
                self.skip_newlines();
                if matches!(self.peek(), PyToken::Eq) {
                    self.advance();
                    self.skip_newlines();
                    if let PyToken::Indent(_) = self.peek() { self.advance(); }
                    self.skip_newlines();
                    let _ = self.parse_expr();
                    self.skip_newlines();
                    if let PyToken::Indent(_) = self.peek() { self.advance(); }
                    self.skip_newlines();
                    if matches!(self.peek(), PyToken::Comma) { self.advance(); }
                    continue;
                }
                self.pos = save;
            }
            if let Some(e) = self.parse_expr() {
                args.push(e);
            }
            self.skip_newlines();
            if let PyToken::Indent(_) = self.peek() { self.advance(); }
            self.skip_newlines();
            if matches!(self.peek(), PyToken::Comma) { self.advance(); }
        }
        self.skip_newlines();
        if let PyToken::Indent(_) = self.peek() { self.advance(); }
        self.skip_newlines();
        if matches!(self.peek(), PyToken::RParen) { self.advance(); }
        self.expect_newline();
        Some(PyStmt::Print(args))
    }

    fn parse_import(&mut self) -> Option<PyStmt> {
        self.advance(); // import
        let mut module = String::new();
        while let PyToken::Ident(s) = self.peek() {
            if !module.is_empty() { module.push('.'); }
            module.push_str(s);
            self.advance();
            if matches!(self.peek(), PyToken::Dot) { self.advance(); } else { break; }
        }
        let alias = if let PyToken::Ident(s) = self.peek() {
            if s == "as" {
                self.advance();
                if let PyToken::Ident(a) = self.advance() { Some(a) } else { None }
            } else { None }
        } else { None };
        self.expect_newline();
        Some(PyStmt::Import(module, alias))
    }

    fn parse_from_import(&mut self) -> Option<PyStmt> {
        self.advance(); // from
        let mut module = String::new();
        while let PyToken::Ident(s) = self.peek() {
            let s = s.clone();
            if s == "import" { break; }
            if !module.is_empty() { module.push('.'); }
            module.push_str(&s);
            self.advance();
            if matches!(self.peek(), PyToken::Dot) { self.advance(); } else { break; }
        }
        if let PyToken::Ident(s) = self.peek() {
            if s == "import" { self.advance(); }
        }
        let mut names = Vec::new();
        loop {
            match self.peek() {
                PyToken::Ident(s) => { names.push(s.clone()); self.advance(); }
                PyToken::Star => { names.push("*".into()); self.advance(); }
                _ => break,
            }
            if matches!(self.peek(), PyToken::Comma) { self.advance(); } else { break; }
        }
        self.expect_newline();
        Some(PyStmt::FromImport(module, names))
    }

    fn parse_del(&mut self) -> Option<PyStmt> {
        self.advance(); // del
        let expr = self.parse_expr()?;
        self.expect_newline();
        Some(PyStmt::Del(expr))
    }

    fn parse_try(&mut self) -> Option<PyStmt> {
        self.advance(); // try
        if matches!(self.peek(), PyToken::Colon) { self.advance(); }
        self.expect_newline();
        let body = self.parse_block();

        let mut handlers = Vec::new();
        loop {
            self.skip_newlines();
            if let PyToken::Indent(_) = self.peek() {
                let save = self.pos;
                self.advance();
                if let PyToken::Ident(s) = self.peek() {
                    if s == "except" {
                        self.advance();
                        let mut exc_type_parts = Vec::new();
                        while let PyToken::Ident(t) = self.peek() {
                            exc_type_parts.push(t.clone());
                            self.advance();
                            if matches!(self.peek(), PyToken::Dot) {
                                exc_type_parts.push(".".to_string());
                                self.advance();
                            } else {
                                break;
                            }
                        }
                        let exc_type = if exc_type_parts.is_empty() {
                            None
                        } else {
                            Some(exc_type_parts.concat())
                        };

                        let exc_name = if let PyToken::Ident(s) = self.peek() {
                            if s == "as" {
                                self.advance();
                                if let PyToken::Ident(n) = self.advance() { Some(n) } else { None }
                            } else { None }
                        } else { None };
                        if matches!(self.peek(), PyToken::Colon) { self.advance(); }
                        self.expect_newline();
                        let handler_body = self.parse_block();
                        handlers.push((exc_type, exc_name, handler_body));
                        continue;
                    }
                }
                self.pos = save;
            }
            break;
        }

        Some(PyStmt::Try(body, handlers))
    }

    fn parse_assign_or_expr(&mut self) -> Option<PyStmt> {
        let expr = self.parse_expr()?;

        // Check for assignment: name = expr, or augmented assignment
        match self.peek() {
            PyToken::Eq => {
                self.advance();
                let rhs = self.parse_expr()?;
                self.expect_newline();
                Some(PyStmt::Assign(expr, rhs))
            }
            PyToken::PlusEq => { self.advance(); let rhs = self.parse_expr()?; self.expect_newline();
                Some(PyStmt::AugAssign(expr, "+".into(), rhs))
            }
            PyToken::MinusEq => { self.advance(); let rhs = self.parse_expr()?; self.expect_newline();
                Some(PyStmt::AugAssign(expr, "-".into(), rhs))
            }
            PyToken::StarEq => { self.advance(); let rhs = self.parse_expr()?; self.expect_newline();
                Some(PyStmt::AugAssign(expr, "*".into(), rhs))
            }
            PyToken::SlashEq => { self.advance(); let rhs = self.parse_expr()?; self.expect_newline();
                Some(PyStmt::AugAssign(expr, "/".into(), rhs))
            }
            PyToken::PercentEq => { self.advance(); let rhs = self.parse_expr()?; self.expect_newline();
                Some(PyStmt::AugAssign(expr, "%".into(), rhs))
            }
            _ => {
                self.expect_newline();
                Some(PyStmt::Expr(expr))
            }
        }
    }

    fn parse_block(&mut self) -> Vec<PyStmt> {
        let mut stmts = Vec::new();
        self.skip_newlines();

        // Determine block indent
        let block_indent = if let PyToken::Indent(n) = self.peek() {
            *n
        } else {
            return stmts;
        };

        loop {
            self.skip_newlines();
            match self.peek() {
                PyToken::Indent(n) if *n >= block_indent => {
                    self.advance();
                    if matches!(self.peek(), PyToken::Newline | PyToken::Eof) { continue; }
                    if let Some(s) = self.parse_stmt() {
                        stmts.push(s);
                    } else {
                        let tok = self.peek().clone();
                        stmts.push(PyStmt::Comment(format!("Unrecognized block syntax near token: {:?}", tok)));
                        while !matches!(self.peek(), PyToken::Newline | PyToken::Eof) {
                            self.advance();
                        }
                        self.expect_newline();
                    }
                }
                _ => break,
            }
        }
        stmts
    }

    // ============ Expression Parsing ============

    fn parse_expr(&mut self) -> Option<PyExpr> {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> Option<PyExpr> {
        let expr = self.parse_or()?;
        let save = self.pos;
        if let PyToken::Ident(s) = self.peek() {
            if s == "if" {
                self.advance();
                if let Some(cond) = self.parse_or() {
                    self.skip_newlines();
                    if let PyToken::Indent(_) = self.peek() { self.advance(); }
                    self.skip_newlines();
                    if let PyToken::Ident(s2) = self.peek() {
                        if s2 == "else" {
                            self.advance();
                            if let Some(alt) = self.parse_or() {
                                return Some(PyExpr::IfExpr(Box::new(expr), Box::new(cond), Box::new(alt)));
                            }
                        }
                    }
                }
                self.pos = save; // backtrack!
            }
        }
        Some(expr)
    }

    fn parse_or(&mut self) -> Option<PyExpr> {
        let mut left = self.parse_and()?;
        while let PyToken::Ident(s) = self.peek() {
            if s != "or" { break; }
            self.advance();
            let right = self.parse_and()?;
            left = PyExpr::BoolOp("or".into(), vec![left, right]);
        }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<PyExpr> {
        let mut left = self.parse_not()?;
        while let PyToken::Ident(s) = self.peek() {
            if s != "and" { break; }
            self.advance();
            let right = self.parse_not()?;
            left = PyExpr::BoolOp("and".into(), vec![left, right]);
        }
        Some(left)
    }

    fn parse_not(&mut self) -> Option<PyExpr> {
        if let PyToken::Ident(s) = self.peek() {
            if s == "not" {
                self.advance();
                let expr = self.parse_not()?;
                return Some(PyExpr::UnaryOp("not".into(), Box::new(expr)));
            }
        }
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Option<PyExpr> {
        let left = self.parse_addition()?;
        let mut ops = Vec::new();

        loop {
            let op = match self.peek() {
                PyToken::EqEq => "==",
                PyToken::NotEq => "!=",
                PyToken::Lt => "<",
                PyToken::Gt => ">",
                PyToken::LtEq => "<=",
                PyToken::GtEq => ">=",
                PyToken::Ident(s) if s == "in" => "in",
                PyToken::Ident(s) if s == "not" => {
                    let save = self.pos;
                    self.advance();
                    if let PyToken::Ident(s2) = self.peek() {
                        if s2 == "in" {
                            self.advance();
                            let right = self.parse_addition()?;
                            ops.push(("not in".to_string(), right));
                            continue;
                        }
                    }
                    self.pos = save;
                    break;
                }
                _ => break,
            };
            let op_str = op.to_string();
            self.advance();
            let right = self.parse_addition()?;
            ops.push((op_str, right));
        }

        if ops.is_empty() {
            Some(left)
        } else {
            Some(PyExpr::Compare(Box::new(left), ops))
        }
    }

    fn parse_addition(&mut self) -> Option<PyExpr> {
        let mut left = self.parse_multiplication()?;
        loop {
            let op = match self.peek() {
                PyToken::Plus => "+",
                PyToken::Minus => "-",
                _ => break,
            };
            let op_str = op.to_string();
            self.advance();
            let right = self.parse_multiplication()?;
            left = PyExpr::BinOp(Box::new(left), op_str, Box::new(right));
        }
        Some(left)
    }

    fn parse_multiplication(&mut self) -> Option<PyExpr> {
        let mut left = self.parse_power()?;
        loop {
            let op = match self.peek() {
                PyToken::Star => "*",
                PyToken::DoubleSlash => "//",
                PyToken::Slash => "/",
                PyToken::Percent => "%",
                _ => break,
            };
            let op_str = op.to_string();
            self.advance();
            let right = self.parse_power()?;
            left = PyExpr::BinOp(Box::new(left), op_str, Box::new(right));
        }
        Some(left)
    }

    fn parse_power(&mut self) -> Option<PyExpr> {
        let left = self.parse_unary()?;
        if matches!(self.peek(), PyToken::DoubleStar) {
            self.advance();
            let right = self.parse_unary()?;
            Some(PyExpr::BinOp(Box::new(left), "**".into(), Box::new(right)))
        } else {
            Some(left)
        }
    }

    fn parse_unary(&mut self) -> Option<PyExpr> {
        match self.peek().clone() {
            PyToken::Minus => {
                self.advance();
                let expr = self.parse_postfix()?;
                Some(PyExpr::UnaryOp("-".into(), Box::new(expr)))
            }
            PyToken::Ident(ref s) if s == "await" => {
                self.advance();
                let expr = self.parse_unary()?;
                Some(PyExpr::Await(Box::new(expr)))
            }
            _ => self.parse_postfix()
        }
    }

    fn parse_postfix(&mut self) -> Option<PyExpr> {
        let mut expr = self.parse_atom()?;

        loop {
            match self.peek() {
                PyToken::LParen => {
                    self.advance();
                    let mut args = Vec::new();
                    let mut kwargs = Vec::new();
                    while !matches!(self.peek(), PyToken::RParen | PyToken::Eof) {
                        self.skip_newlines();
                        if let PyToken::Indent(_) = self.peek() { self.advance(); }
                        self.skip_newlines();
                        if matches!(self.peek(), PyToken::RParen | PyToken::Eof) { break; }

                        // Check for keyword argument
                        if let PyToken::Ident(s) = self.peek() {
                            let s = s.clone();
                            let save = self.pos;
                            self.advance();
                            self.skip_newlines();
                            if let PyToken::Indent(_) = self.peek() { self.advance(); }
                            self.skip_newlines();
                            if matches!(self.peek(), PyToken::Eq) {
                                self.advance();
                                self.skip_newlines();
                                if let PyToken::Indent(_) = self.peek() { self.advance(); }
                                self.skip_newlines();
                                let val = self.parse_expr().unwrap_or(PyExpr::None);
                                kwargs.push((s, val));
                                self.skip_newlines();
                                if let PyToken::Indent(_) = self.peek() { self.advance(); }
                                self.skip_newlines();
                                if matches!(self.peek(), PyToken::Comma) { self.advance(); }
                                continue;
                            }
                            self.pos = save;
                        }
                        if let Some(arg) = self.parse_expr() {
                            args.push(arg);
                        }
                        self.skip_newlines();
                        if let PyToken::Indent(_) = self.peek() { self.advance(); }
                        self.skip_newlines();
                        if matches!(self.peek(), PyToken::Comma) { self.advance(); }
                    }
                    self.skip_newlines();
                    if let PyToken::Indent(_) = self.peek() { self.advance(); }
                    self.skip_newlines();
                    if matches!(self.peek(), PyToken::RParen) { self.advance(); }
                    expr = PyExpr::Call(Box::new(expr), args, kwargs);
                }
                PyToken::LBracket => {
                    self.advance();
                    self.skip_newlines();
                    if let PyToken::Indent(_) = self.peek() { self.advance(); }
                    self.skip_newlines();
                    let idx = self.parse_expr()?;
                    self.skip_newlines();
                    if let PyToken::Indent(_) = self.peek() { self.advance(); }
                    self.skip_newlines();
                    if matches!(self.peek(), PyToken::RBracket) { self.advance(); }
                    expr = PyExpr::Subscript(Box::new(expr), Box::new(idx));
                }
                PyToken::Dot => {
                    self.advance();
                    if let PyToken::Ident(attr) = self.advance() {
                        expr = PyExpr::Attribute(Box::new(expr), attr);
                    }
                }
                _ => break,
            }
        }

        Some(expr)
    }

    fn parse_atom(&mut self) -> Option<PyExpr> {
        match self.peek().clone() {
            PyToken::IntLit(n) => { self.advance(); Some(PyExpr::Int(n)) }
            PyToken::FloatLit(f) => { self.advance(); Some(PyExpr::Float(f)) }
            PyToken::StrLit(s) => { self.advance(); Some(PyExpr::Str(s)) }
            PyToken::FStrLit(s) => { self.advance(); Some(PyExpr::FStr(s)) }
            PyToken::Ident(s) => {
                match s.as_str() {
                    "True" => { self.advance(); Some(PyExpr::Bool(true)) }
                    "False" => { self.advance(); Some(PyExpr::Bool(false)) }
                    "None" => { self.advance(); Some(PyExpr::None) }
                    _ => { self.advance(); Some(PyExpr::Name(s)) }
                }
            }
            PyToken::LParen => {
                self.advance();
                self.skip_newlines();
                if let PyToken::Indent(_) = self.peek() { self.advance(); }
                self.skip_newlines();
                let expr = self.parse_expr()?;
                self.skip_newlines();
                if let PyToken::Indent(_) = self.peek() { self.advance(); }
                self.skip_newlines();
                if matches!(self.peek(), PyToken::RParen) { self.advance(); }
                Some(expr)
            }
            PyToken::LBracket => {
                self.advance();
                let mut elems = Vec::new();
                while !matches!(self.peek(), PyToken::RBracket | PyToken::Eof) {
                    self.skip_newlines();
                    if let PyToken::Indent(_) = self.peek() { self.advance(); }
                    self.skip_newlines();
                    if matches!(self.peek(), PyToken::RBracket | PyToken::Eof) { break; }

                    if let Some(e) = self.parse_expr() {
                        self.skip_newlines();
                        if let PyToken::Indent(_) = self.peek() { self.advance(); }
                        self.skip_newlines();
                        // Check for list comprehension: [expr for x in iter]
                        if let PyToken::Ident(s) = self.peek() {
                            if s == "for" && elems.is_empty() {
                                self.advance();
                                self.skip_newlines();
                                if let PyToken::Indent(_) = self.peek() { self.advance(); }
                                self.skip_newlines();
                                let var = if let PyToken::Ident(v) = self.advance() { v } else { "_".into() };
                                self.skip_newlines();
                                if let PyToken::Indent(_) = self.peek() { self.advance(); }
                                self.skip_newlines();
                                if let PyToken::Ident(s) = self.peek() { if s == "in" { self.advance(); } }
                                self.skip_newlines();
                                if let PyToken::Indent(_) = self.peek() { self.advance(); }
                                self.skip_newlines();
                                let iter = self.parse_expr()?;
                                self.skip_newlines();
                                if let PyToken::Indent(_) = self.peek() { self.advance(); }
                                self.skip_newlines();
                                let filter = if let PyToken::Ident(s) = self.peek() {
                                    if s == "if" {
                                        self.advance();
                                        self.skip_newlines();
                                        if let PyToken::Indent(_) = self.peek() { self.advance(); }
                                        self.skip_newlines();
                                        self.parse_expr().map(Box::new)
                                    }
                                    else { None }
                                } else { None };
                                self.skip_newlines();
                                if let PyToken::Indent(_) = self.peek() { self.advance(); }
                                self.skip_newlines();
                                if matches!(self.peek(), PyToken::RBracket) { self.advance(); }
                                return Some(PyExpr::ListComp(Box::new(e), var, Box::new(iter), filter));
                            }
                        }
                        elems.push(e);
                    }
                    self.skip_newlines();
                    if let PyToken::Indent(_) = self.peek() { self.advance(); }
                    self.skip_newlines();
                    if matches!(self.peek(), PyToken::Comma) { self.advance(); }
                }
                self.skip_newlines();
                if let PyToken::Indent(_) = self.peek() { self.advance(); }
                self.skip_newlines();
                if matches!(self.peek(), PyToken::RBracket) { self.advance(); }
                Some(PyExpr::List(elems))
            }
            PyToken::LBrace => {
                self.advance();
                let mut pairs = Vec::new();
                while !matches!(self.peek(), PyToken::RBrace | PyToken::Eof) {
                    self.skip_newlines();
                    if let PyToken::Indent(_) = self.peek() { self.advance(); }
                    self.skip_newlines();
                    if matches!(self.peek(), PyToken::RBrace | PyToken::Eof) { break; }

                    let key = self.parse_expr()?;
                    self.skip_newlines();
                    if let PyToken::Indent(_) = self.peek() { self.advance(); }
                    self.skip_newlines();
                    if matches!(self.peek(), PyToken::Colon) {
                        self.advance();
                        self.skip_newlines();
                        if let PyToken::Indent(_) = self.peek() { self.advance(); }
                        self.skip_newlines();
                        let val = self.parse_expr()?;
                        pairs.push((key, val));
                    }
                    self.skip_newlines();
                    if let PyToken::Indent(_) = self.peek() { self.advance(); }
                    self.skip_newlines();
                    if matches!(self.peek(), PyToken::Comma) { self.advance(); }
                }
                self.skip_newlines();
                if let PyToken::Indent(_) = self.peek() { self.advance(); }
                self.skip_newlines();
                if matches!(self.peek(), PyToken::RBrace) { self.advance(); }
                Some(PyExpr::Dict(pairs))
            }
            PyToken::Minus => {
                self.advance();
                let expr = self.parse_atom()?;
                Some(PyExpr::UnaryOp("-".into(), Box::new(expr)))
            }
            _ => None,
        }
    }
}

// ============ Python → Ernos Emitter ============

fn py_type_to_ernos(hint: &str) -> &str {
    match hint {
        "int" => "Int",
        "str" => "Str",
        "bool" => "Int",
        "float" => "Int",
        "list" | "List" => "List",
        "dict" | "Dict" => "Int",
        "None" | "NoneType" => "Int",
        "Optional" => "Int",
        other if other.contains('.') || other.chars().next().map_or(false, |c| c.is_ascii_uppercase()) => "Any",
        _ => "Int",
    }
}

/// Sanitize identifiers that collide with Ernos keywords
fn sanitize_ernos_ident(name: &str) -> String {
    match name {
        "define" | "with" | "and" | "set" | "to" | "if" | "else" | "return"
        | "display" | "repeat" | "while" | "import" | "spawn" | "channel"
        | "send" | "receive" | "from" | "external" | "borrow" | "structure"
        | "field" | "as" | "is" | "create" | "returning" | "choice" | "variant"
        | "check" | "for" | "each" | "in" | "range" | "on" | "trait" | "implement"
        | "not" | "break" | "continue" | "of" | "try" | "given" | "true" | "false"
        | "async" | "await" | "plus" | "minus" | "equals" | "modulo" => {
            format!("{}_v", name)
        }
        _ => name.to_string(),
    }
}

fn expr_to_py_string(expr: &PyExpr) -> String {
    match expr {
        PyExpr::Int(n) => n.to_string(),
        PyExpr::Float(f) => f.to_string(),
        PyExpr::Str(s) => format!("\"{}\"", s.replace('"', "\\\"")),
        PyExpr::FStr(s) => format!("f\"{}\"", s.replace('"', "\\\"")),
        PyExpr::Bool(b) => if *b { "True".to_string() } else { "False".to_string() },
        PyExpr::None => "None".to_string(),
        PyExpr::Name(n) => n.clone(),
        PyExpr::BinOp(left, op, right) => {
            format!("({} {} {})", expr_to_py_string(left), op, expr_to_py_string(right))
        }
        PyExpr::UnaryOp(op, expr) => {
            format!("({}{})", op, expr_to_py_string(expr))
        }
        PyExpr::Compare(left, ops) => {
            let mut s = expr_to_py_string(left);
            for (op, right) in ops {
                s.push_str(&format!(" {} {}", op, expr_to_py_string(right)));
            }
            s
        }
        PyExpr::BoolOp(op, exprs) => {
            exprs.iter().map(expr_to_py_string).collect::<Vec<_>>().join(&format!(" {} ", op))
        }
        PyExpr::Call(func, args, kwargs) => {
            let mut parts = Vec::new();
            for arg in args {
                parts.push(expr_to_py_string(arg));
            }
            for (k, v) in kwargs {
                parts.push(format!("{}={}", k, expr_to_py_string(v)));
            }
            format!("{}({})", expr_to_py_string(func), parts.join(", "))
        }
        PyExpr::Attribute(obj, attr) => {
            format!("{}.{}", expr_to_py_string(obj), attr)
        }
        PyExpr::Subscript(obj, idx) => {
            format!("{}[{}]", expr_to_py_string(obj), expr_to_py_string(idx))
        }
        PyExpr::List(elems) => {
            format!("[{}]", elems.iter().map(expr_to_py_string).collect::<Vec<_>>().join(", "))
        }
        PyExpr::Dict(pairs) => {
            let items: Vec<String> = pairs.iter()
                .map(|(k, v)| format!("{}: {}", expr_to_py_string(k), expr_to_py_string(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
        PyExpr::IfExpr(val, cond, alt) => {
            format!("{} if {} else {}", expr_to_py_string(val), expr_to_py_string(cond), expr_to_py_string(alt))
        }
        PyExpr::ListComp(expr, var, iter, filter) => {
            let filt = filter.as_ref().map(|f| format!(" if {}", expr_to_py_string(f))).unwrap_or_default();
            format!("[{} for {} in {}{}]", expr_to_py_string(expr), var, expr_to_py_string(iter), filt)
        }
        PyExpr::Await(expr) => {
            format!("await {}", expr_to_py_string(expr))
        }
    }
}

fn desugar_fstring(s: &str) -> PyExpr {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut parts = Vec::new();
    let mut current_lit = String::new();

    while i < chars.len() {
        if chars[i] == '{' {
            if i + 1 < chars.len() && chars[i + 1] == '{' {
                current_lit.push('{');
                i += 2;
                continue;
            }
            if !current_lit.is_empty() {
                parts.push(PyExpr::Str(current_lit.clone()));
                current_lit.clear();
            }
            i += 1; // skip '{'
            let mut expr_str = String::new();
            let mut brace_depth = 1;
            while i < chars.len() {
                let c = chars[i];
                if c == '}' {
                    brace_depth -= 1;
                    if brace_depth == 0 {
                        i += 1; // skip '}'
                        break;
                    }
                } else if c == '{' {
                    brace_depth += 1;
                }
                expr_str.push(c);
                i += 1;
            }
            
            let mut lexer = PyLexer::new(&expr_str);
            let tokens = lexer.tokenize();
            let mut parser = PyParser::new(tokens);
            let parsed_expr = parser.parse_expr().unwrap_or(PyExpr::None);
            
            let wrapped = PyExpr::Call(
                Box::new(PyExpr::Name("ep_auto_to_string".to_string())),
                vec![parsed_expr],
                vec![]
            );
            parts.push(wrapped);
        } else if chars[i] == '}' {
            if i + 1 < chars.len() && chars[i + 1] == '}' {
                current_lit.push('}');
                i += 2;
            } else {
                current_lit.push('}');
                i += 1;
            }
        } else {
            current_lit.push(chars[i]);
            i += 1;
        }
    }

    if !current_lit.is_empty() {
        parts.push(PyExpr::Str(current_lit));
    }

    if parts.is_empty() {
        return PyExpr::Str(String::new());
    }

    let mut result = parts[0].clone();
    for part in parts.iter().skip(1) {
        result = PyExpr::Call(
            Box::new(PyExpr::Name("concat".to_string())),
            vec![result, part.clone()],
            vec![]
        );
    }
    result
}

fn simplify_expr(expr: &mut PyExpr, temp_counter: &mut usize, pre_stmts: &mut Vec<PyStmt>) {
    match expr {
        PyExpr::BinOp(left, _, right) => {
            simplify_expr(left, temp_counter, pre_stmts);
            simplify_expr(right, temp_counter, pre_stmts);
        }
        PyExpr::UnaryOp(_, inner) => {
            simplify_expr(inner, temp_counter, pre_stmts);
        }
        PyExpr::Compare(left, ops) => {
            simplify_expr(left, temp_counter, pre_stmts);
            for (_, right) in ops {
                simplify_expr(right, temp_counter, pre_stmts);
            }
        }
        PyExpr::BoolOp(_, exprs) => {
            for e in exprs {
                simplify_expr(e, temp_counter, pre_stmts);
            }
        }
        PyExpr::Call(func, args, kwargs) => {
            simplify_expr(func, temp_counter, pre_stmts);
            for arg in args {
                simplify_expr(arg, temp_counter, pre_stmts);
            }
            for (_, val) in kwargs {
                simplify_expr(val, temp_counter, pre_stmts);
            }
        }
        PyExpr::Attribute(obj, _) => {
            simplify_expr(obj, temp_counter, pre_stmts);
        }
        PyExpr::Subscript(obj, idx) => {
            simplify_expr(obj, temp_counter, pre_stmts);
            simplify_expr(idx, temp_counter, pre_stmts);
        }
        PyExpr::List(elems) => {
            for elem in elems {
                simplify_expr(elem, temp_counter, pre_stmts);
            }
        }
        PyExpr::Dict(pairs) => {
            if pairs.is_empty() {
                return;
            }
            for (k, v) in pairs.iter_mut() {
                simplify_expr(k, temp_counter, pre_stmts);
                simplify_expr(v, temp_counter, pre_stmts);
            }
            
            let temp_name = format!("_dict_{}", *temp_counter);
            *temp_counter += 1;
            
            pre_stmts.push(PyStmt::Assign(PyExpr::Name(temp_name.clone()), PyExpr::Call(Box::new(PyExpr::Name("create_map".to_string())), vec![], vec![])));
            for (k, v) in pairs.clone() {
                pre_stmts.push(PyStmt::Expr(PyExpr::Call(
                    Box::new(PyExpr::Name("map_insert".to_string())),
                    vec![PyExpr::Name(temp_name.clone()), k, v],
                    vec![]
                )));
            }
            *expr = PyExpr::Name(temp_name);
        }
        PyExpr::IfExpr(val, cond, alt) => {
            simplify_expr(val, temp_counter, pre_stmts);
            simplify_expr(cond, temp_counter, pre_stmts);
            simplify_expr(alt, temp_counter, pre_stmts);
            
            let temp_name = format!("_ternary_{}", *temp_counter);
            *temp_counter += 1;
            
            pre_stmts.push(PyStmt::If(
                *cond.clone(),
                vec![PyStmt::Assign(PyExpr::Name(temp_name.clone()), *val.clone())],
                vec![],
                Some(vec![PyStmt::Assign(PyExpr::Name(temp_name.clone()), *alt.clone())])
            ));
            
            *expr = PyExpr::Name(temp_name);
        }
        PyExpr::ListComp(item_expr, var, iter, filter) => {
            simplify_expr(item_expr, temp_counter, pre_stmts);
            simplify_expr(iter, temp_counter, pre_stmts);
            if let Some(f) = filter {
                simplify_expr(f, temp_counter, pre_stmts);
            }
            
            let temp_name = format!("_list_comp_{}", *temp_counter);
            *temp_counter += 1;
            
            pre_stmts.push(PyStmt::Assign(PyExpr::Name(temp_name.clone()), PyExpr::Call(Box::new(PyExpr::Name("create_list".to_string())), vec![], vec![])));
            
            let append_call = PyStmt::Expr(PyExpr::Call(
                Box::new(PyExpr::Name("append_list".to_string())),
                vec![PyExpr::Name(temp_name.clone()), *item_expr.clone()],
                vec![]
            ));
            
            let loop_body = if let Some(filt) = filter {
                vec![PyStmt::If(*filt.clone(), vec![append_call], vec![], None)]
            } else {
                vec![append_call]
            };
            
            pre_stmts.push(PyStmt::For(var.clone(), *iter.clone(), loop_body));
            
            *expr = PyExpr::Name(temp_name);
        }
        PyExpr::Await(inner) => {
            simplify_expr(inner, temp_counter, pre_stmts);
        }
        PyExpr::FStr(s) => {
            let desugared = desugar_fstring(s);
            *expr = desugared;
            simplify_expr(expr, temp_counter, pre_stmts);
        }
        _ => {}
    }
}

fn preprocess_stmt(stmt: PyStmt, temp_counter: &mut usize) -> Vec<PyStmt> {
    let mut pre_stmts = Vec::new();
    let mut mut_stmt = stmt;
    match &mut mut_stmt {
        PyStmt::Assign(lhs, val) => {
            simplify_expr(lhs, temp_counter, &mut pre_stmts);
            simplify_expr(val, temp_counter, &mut pre_stmts);
        }
        PyStmt::AugAssign(lhs, _, val) => {
            simplify_expr(lhs, temp_counter, &mut pre_stmts);
            simplify_expr(val, temp_counter, &mut pre_stmts);
        }
        PyStmt::Expr(val) => {
            simplify_expr(val, temp_counter, &mut pre_stmts);
        }
        PyStmt::Return(Some(val)) => {
            simplify_expr(val, temp_counter, &mut pre_stmts);
        }
        PyStmt::If(cond, body, elifs, else_body) => {
            simplify_expr(cond, temp_counter, &mut pre_stmts);
            let mut new_body = Vec::new();
            for s in body.drain(..) {
                new_body.extend(preprocess_stmt(s, temp_counter));
            }
            *body = new_body;
            for (econd, ebody) in elifs {
                simplify_expr(econd, temp_counter, &mut pre_stmts);
                let mut new_ebody = Vec::new();
                for s in ebody.drain(..) {
                    new_ebody.extend(preprocess_stmt(s, temp_counter));
                }
                *ebody = new_ebody;
            }
            if let Some(else_b) = else_body {
                let mut new_else = Vec::new();
                for s in else_b.drain(..) {
                    new_else.extend(preprocess_stmt(s, temp_counter));
                }
                *else_body = Some(new_else);
            }
        }
        PyStmt::While(cond, body) => {
            simplify_expr(cond, temp_counter, &mut pre_stmts);
            let mut new_body = Vec::new();
            for s in body.drain(..) {
                new_body.extend(preprocess_stmt(s, temp_counter));
            }
            *body = new_body;
        }
        PyStmt::For(_var, iter, body) => {
            simplify_expr(iter, temp_counter, &mut pre_stmts);
            let mut new_body = Vec::new();
            for s in body.drain(..) {
                new_body.extend(preprocess_stmt(s, temp_counter));
            }
            *body = new_body;
        }
        PyStmt::FuncDef(_, _, _, body, _, _) => {
            let mut new_body = Vec::new();
            for s in body.drain(..) {
                new_body.extend(preprocess_stmt(s, temp_counter));
            }
            *body = new_body;
        }
        PyStmt::ClassDef(_, body, _) => {
            let mut new_body = Vec::new();
            for s in body.drain(..) {
                new_body.extend(preprocess_stmt(s, temp_counter));
            }
            *body = new_body;
        }
        PyStmt::Print(args) => {
            for arg in args {
                simplify_expr(arg, temp_counter, &mut pre_stmts);
            }
        }
        PyStmt::Del(expr) => {
            simplify_expr(expr, temp_counter, &mut pre_stmts);
        }
        PyStmt::Try(body, handlers) => {
            let mut new_body = Vec::new();
            for s in body.drain(..) {
                new_body.extend(preprocess_stmt(s, temp_counter));
            }
            *body = new_body;
            for (_, _, hbody) in handlers {
                let mut new_hbody = Vec::new();
                for s in hbody.drain(..) {
                    new_hbody.extend(preprocess_stmt(s, temp_counter));
                }
                *hbody = new_hbody;
            }
        }
        _ => {}
    }
    pre_stmts.push(mut_stmt);
    pre_stmts
}

fn emit_indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("    ");
    }
}

pub fn emit_ernos_from_python(filename: &str, source: &str) -> String {
    let mut lexer = PyLexer::new(source);
    let tokens = lexer.tokenize();
    let mut parser = PyParser::new(tokens);
    let stmts = parser.parse();

    let mut temp_counter = 0;
    let mut processed_stmts = Vec::new();
    for stmt in stmts {
        processed_stmts.extend(preprocess_stmt(stmt, &mut temp_counter));
    }

    let mut out = String::new();
    out.push_str(&format!("# Auto-transpiled from Python: {}\n", filename));
    out.push_str("# Generated by: ernos transpile\n\n");

    let mut ctx = EmitCtx {
        vars: HashMap::new(),
        depth: 0,
        last_query_results: HashMap::new(),
    };

    for stmt in &processed_stmts {
        // Only emit function/class definitions at top level — ErnosPlain
        // doesn't allow bare statements (calls, assignments, etc.) outside of define blocks
        match stmt {
            PyStmt::FuncDef(..) | PyStmt::ClassDef(..) | PyStmt::Comment(..)
            | PyStmt::Import(..) | PyStmt::FromImport(..) => {
                emit_stmt(&mut out, &mut ctx, stmt);
            }
            _ => {
                // Skip bare top-level statements (calls like main(), assignments, etc.)
            }
        }
    }

    out
}

struct EmitCtx {
    vars: HashMap<String, bool>,  // name → is_set (for tracking first assignment)
    depth: usize,
    last_query_results: HashMap<String, String>, // cursor_name -> results_var_name
}

fn emit_stmt(out: &mut String, ctx: &mut EmitCtx, stmt: &PyStmt) {
    match stmt {
        PyStmt::FuncDef(name, params, ret_hint, body, decorators, is_async) => {
            for dec in decorators {
                emit_indent(out, ctx.depth);
                out.push_str(&format!("# @{}\n", expr_to_py_string(dec)));
            }

            emit_indent(out, ctx.depth);
            let ep_name = sanitize_ernos_ident(&name.replace("__init__", "create"));
            let prefix = if *is_async { "async define " } else { "define " };

            if params.is_empty() {
                if let Some(ret) = ret_hint {
                    out.push_str(&format!("{}{} returning {}:\n", prefix, ep_name, py_type_to_ernos(ret)));
                } else {
                    out.push_str(&format!("{}{}:\n", prefix, ep_name));
                }
            } else {
                let param_parts: Vec<String> = params.iter()
                    .map(|(pname, hint)| {
                        let ptype = hint.as_deref().map(py_type_to_ernos).unwrap_or("Int");
                        format!("{} as {}", sanitize_ernos_ident(pname), ptype)
                    })
                    .collect();

                if let Some(ret) = ret_hint {
                    out.push_str(&format!("{}{} with {} returning {}:\n",
                        prefix, ep_name, param_parts.join(" and "), py_type_to_ernos(ret)));
                } else {
                    out.push_str(&format!("{}{} with {}:\n",
                        prefix, ep_name, param_parts.join(" and ")));
                }
            }

            let mut inner_ctx = EmitCtx {
                vars: HashMap::new(),
                depth: ctx.depth + 1,
                last_query_results: HashMap::new(),
            };
            for p in params { inner_ctx.vars.insert(p.0.clone(), true); }
            for s in body {
                emit_stmt(out, &mut inner_ctx, s);
            }
            out.push('\n');
        }

        PyStmt::ClassDef(name, body, decorators) => {
            for dec in decorators {
                emit_indent(out, ctx.depth);
                out.push_str(&format!("# @{}\n", expr_to_py_string(dec)));
            }

            // Extract fields from __init__ method
            let mut fields = Vec::new();
            let mut methods = Vec::new();

            for s in body {
                if let PyStmt::FuncDef(fname, _params, _ret, fbody, _, _) = s {
                    if fname == "__init__" {
                        for init_stmt in fbody {
                            if let PyStmt::Assign(lhs, _val) = init_stmt {
                                if let PyExpr::Attribute(obj, field) = lhs {
                                    if let PyExpr::Name(obj_name) = obj.as_ref() {
                                        if obj_name == "self" {
                                            fields.push(field.clone());
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        methods.push(s.clone());
                    }
                }
            }

            emit_indent(out, ctx.depth);
            out.push_str(&format!("define structure {}:\n", name));
            for field in &fields {
                emit_indent(out, ctx.depth + 1);
                out.push_str(&format!("field {} as Int\n", field));
            }
            out.push('\n');

            // Emit methods
            for method in &methods {
                if let PyStmt::FuncDef(mname, params, _ret, mbody, _, is_async) = method {
                    emit_indent(out, ctx.depth);
                    let prefix = if *is_async { "async define " } else { "define " };
                    let param_parts: Vec<String> = params.iter()
                        .map(|(pname, hint)| {
                            let ptype = hint.as_deref().map(py_type_to_ernos).unwrap_or("Int");
                            format!("{} as {}", pname, ptype)
                        })
                        .collect();

                    if param_parts.is_empty() {
                        out.push_str(&format!("{}{} on {}:\n", prefix, mname, name));
                    } else {
                        out.push_str(&format!("{}{} on {} with {}:\n",
                            prefix, mname, name, param_parts.join(" and ")));
                    }

                    let mut inner_ctx = EmitCtx {
                        vars: HashMap::new(),
                        depth: ctx.depth + 1,
                        last_query_results: HashMap::new(),
                    };
                    for p in params { inner_ctx.vars.insert(p.0.clone(), true); }
                    for s in mbody {
                        emit_stmt(out, &mut inner_ctx, s);
                    }
                    out.push('\n');
                }
            }
        }

        PyStmt::Assign(lhs, val) => {
            emit_indent(out, ctx.depth);
            match lhs {
                PyExpr::Attribute(obj, attr) => {
                    out.push_str("set ");
                    emit_expr(out, obj, ctx);
                    out.push('.');
                    out.push_str(attr);
                    out.push_str(" to ");
                    emit_expr(out, val, ctx);
                    out.push('\n');
                }
                PyExpr::Subscript(obj, idx) => {
                    out.push_str("set_list(");
                    emit_expr(out, obj, ctx);
                    out.push_str(" and ");
                    emit_expr(out, idx, ctx);
                    out.push_str(" and ");
                    emit_expr(out, val, ctx);
                    out.push_str(")\n");
                }
                PyExpr::Name(target) => {
                    let sanitized = sanitize_ernos_ident(target);
                    if ctx.vars.contains_key(&sanitized) {
                        out.push_str(&format!("set {} to ", sanitized));
                    } else {
                        ctx.vars.insert(sanitized.clone(), true);
                        out.push_str(&format!("set {} to ", sanitized));
                    }
                    emit_expr(out, val, ctx);
                    out.push('\n');
                }
                _ => {
                    out.push_str("set ");
                    emit_expr(out, lhs, ctx);
                    out.push_str(" to ");
                    emit_expr(out, val, ctx);
                    out.push('\n');
                }
            }
        }

        PyStmt::AugAssign(lhs, op, val) => {
            emit_indent(out, ctx.depth);
            match lhs {
                PyExpr::Attribute(obj, attr) => {
                    out.push_str("set ");
                    emit_expr(out, obj, ctx);
                    out.push('.');
                    out.push_str(attr);
                    out.push_str(" to ");
                    emit_expr(out, obj, ctx);
                    out.push('.');
                    out.push_str(attr);
                    out.push_str(&format!(" {} ", op));
                    emit_expr(out, val, ctx);
                    out.push('\n');
                }
                PyExpr::Name(name) => {
                    let sanitized = sanitize_ernos_ident(name);
                    out.push_str(&format!("set {} to {} {} ", sanitized, sanitized, op));
                    emit_expr(out, val, ctx);
                    out.push('\n');
                }
                _ => {
                    out.push_str("set ");
                    emit_expr(out, lhs, ctx);
                    out.push_str(" to ");
                    emit_expr(out, lhs, ctx);
                    out.push_str(&format!(" {} ", op));
                    emit_expr(out, val, ctx);
                    out.push('\n');
                }
            }
        }

        PyStmt::Print(args) => {
            for arg in args {
                emit_indent(out, ctx.depth);
                out.push_str("display ");
                emit_expr(out, arg, ctx);
                out.push('\n');
            }
        }

        PyStmt::Return(val) => {
            emit_indent(out, ctx.depth);
            if let Some(v) = val {
                out.push_str("return ");
                emit_expr(out, v, ctx);
                out.push('\n');
            } else {
                out.push_str("return 0\n");
            }
        }

        PyStmt::If(cond, body, elifs, else_body) => {
            emit_indent(out, ctx.depth);
            out.push_str("if ");
            emit_cond(out, cond, ctx);
            out.push_str(":\n");
            ctx.depth += 1;
            for s in body { emit_stmt(out, ctx, s); }
            ctx.depth -= 1;

            for (econd, ebody) in elifs {
                emit_indent(out, ctx.depth);
                out.push_str("else if ");
                emit_cond(out, econd, ctx);
                out.push_str(":\n");
                ctx.depth += 1;
                for s in ebody { emit_stmt(out, ctx, s); }
                ctx.depth -= 1;
            }

            if let Some(eb) = else_body {
                emit_indent(out, ctx.depth);
                out.push_str("else:\n");
                ctx.depth += 1;
                for s in eb { emit_stmt(out, ctx, s); }
                ctx.depth -= 1;
            }
        }

        PyStmt::While(cond, body) => {
            emit_indent(out, ctx.depth);
            out.push_str("repeat while ");
            emit_cond(out, cond, ctx);
            out.push_str(":\n");
            ctx.depth += 1;
            for s in body { emit_stmt(out, ctx, s); }
            ctx.depth -= 1;
        }

        PyStmt::For(var, iter, body) => {
            emit_indent(out, ctx.depth);
            ctx.vars.insert(var.clone(), true);

            // Check if iterating over range()
            if let PyExpr::Call(func, args, _) = iter {
                if let PyExpr::Name(fname) = func.as_ref() {
                    if fname == "range" {
                        let (start_expr, end_expr) = match args.len() {
                            1 => (None, &args[0]),
                            2 => (Some(&args[0]), &args[1]),
                            _ => (None, &args[0]),
                        };
                        out.push_str(&format!("set {} to ", var));
                        if let Some(start) = start_expr {
                            emit_expr(out, start, ctx);
                        } else {
                            out.push('0');
                        }
                        out.push('\n');
                        emit_indent(out, ctx.depth);
                        out.push_str(&format!("repeat while {} < ", var));
                        emit_expr(out, end_expr, ctx);
                        out.push_str(":\n");
                        ctx.depth += 1;
                        for s in body { emit_stmt(out, ctx, s); }
                        emit_indent(out, ctx.depth);
                        out.push_str(&format!("set {} to {} + 1\n", var, var));
                        ctx.depth -= 1;
                        return;
                    }
                }
            }

            out.push_str(&format!("for each {} in ", var));
            emit_expr(out, iter, ctx);
            out.push_str(":\n");
            ctx.depth += 1;
            for s in body { emit_stmt(out, ctx, s); }
            ctx.depth -= 1;
        }

        PyStmt::Break => {
            emit_indent(out, ctx.depth);
            out.push_str("break\n");
        }

        PyStmt::Continue => {
            emit_indent(out, ctx.depth);
            out.push_str("continue\n");
        }

        PyStmt::Pass => {
            emit_indent(out, ctx.depth);
            out.push_str("# pass\n");
        }

        PyStmt::Import(module, alias) => {
            emit_indent(out, ctx.depth);
            if module == "sqlite3" {
                out.push_str("import \"sql\"\n");
            } else if let Some(a) = alias {
                out.push_str(&format!("# import {} as {} — manual translation needed\n", module, a));
            } else {
                out.push_str(&format!("# import {} — manual translation needed\n", module));
            }
        }

        PyStmt::FromImport(module, names) => {
            emit_indent(out, ctx.depth);
            out.push_str(&format!("# from {} import {} — manual translation needed\n", module, names.join(", ")));
        }

        PyStmt::Comment(text) => {
            emit_indent(out, ctx.depth);
            out.push_str(&format!("# {}\n", text));
        }

        PyStmt::Expr(expr) => {
            emit_indent(out, ctx.depth);
            match expr {
                PyExpr::Call(func, args, _) => {
                    if let PyExpr::Attribute(obj, method) = func.as_ref() {
                        if let PyExpr::Name(obj_name) = obj.as_ref() {
                            let sanitized_obj = sanitize_ernos_ident(obj_name);
                            if method == "execute" && !args.is_empty() {
                                let sql_arg = &args[0];
                                let mut is_write = false;
                                if let PyExpr::Str(sql_str) = sql_arg {
                                    let sql_lower = sql_str.to_lowercase();
                                    is_write = sql_lower.contains("insert")
                                        || sql_lower.contains("update")
                                        || sql_lower.contains("delete")
                                        || sql_lower.contains("create")
                                        || sql_lower.contains("drop")
                                        || sql_lower.contains("replace");
                                }
                                if is_write {
                                    out.push_str("sql_execute(");
                                    emit_expr(out, obj, ctx);
                                    out.push_str(" and ");
                                    emit_expr(out, sql_arg, ctx);
                                    out.push_str(")\n");
                                } else {
                                    let res_var = format!("_query_res_{}", sanitized_obj);
                                    ctx.vars.insert(res_var.clone(), true);
                                    ctx.last_query_results.insert(sanitized_obj.clone(), res_var.clone());
                                    
                                    out.push_str(&format!("set {} to sql_query(", res_var));
                                    emit_expr(out, obj, ctx);
                                    out.push_str(" and ");
                                    emit_expr(out, sql_arg, ctx);
                                    out.push_str(")\n");
                                }
                                return;
                            }
                        }
                    }
                    emit_call_stmt(out, func, args, ctx);
                    out.push('\n');
                }
                _ => {
                    emit_expr(out, expr, ctx);
                    out.push('\n');
                }
            }
        }

        PyStmt::Del(expr) => {
            emit_indent(out, ctx.depth);
            if let PyExpr::Subscript(obj, key) = expr {
                out.push_str("map_delete(");
                emit_expr(out, obj, ctx);
                out.push_str(" and ");
                emit_expr(out, key, ctx);
                out.push_str(")\n");
            } else {
                out.push_str("# del ");
                emit_expr(out, expr, ctx);
                out.push_str(" — manual translation needed\n");
            }
        }

        PyStmt::Try(body, handlers) => {
            emit_indent(out, ctx.depth);
            out.push_str("# try:\n");
            for s in body { emit_stmt(out, ctx, s); }
            for (exc_type, exc_name, handler_body) in handlers {
                emit_indent(out, ctx.depth);
                let exc_str = exc_type.as_deref().unwrap_or("Exception");
                let name_str = exc_name.as_ref().map(|n| format!(" as {}", n)).unwrap_or_default();
                out.push_str(&format!("# except {}{}:\n", exc_str, name_str));
                for s in handler_body { emit_stmt(out, ctx, s); }
            }
        }
    }
}

fn emit_call_stmt(out: &mut String, func: &PyExpr, args: &[PyExpr], ctx: &mut EmitCtx) {
    if let PyExpr::Attribute(obj, method) = func {
        match method.as_str() {
            "append" => {
                out.push_str("append_list(");
                emit_expr(out, obj, ctx);
                if !args.is_empty() {
                    out.push_str(" and ");
                    emit_expr(out, &args[0], ctx);
                }
                out.push(')');
                return;
            }
            "pop" => {
                out.push_str("pop_list(");
                emit_expr(out, obj, ctx);
                out.push(')');
                return;
            }
            "close" => {
                out.push_str("sql_close(");
                emit_expr(out, obj, ctx);
                out.push(')');
                return;
            }
            "commit" => {
                out.push_str("# conn.commit()");
                return;
            }
            _ => {}
        }
    }

    emit_expr(out, &PyExpr::Call(Box::new(func.clone()), args.to_vec(), vec![]), ctx);
}

fn emit_cond(out: &mut String, expr: &PyExpr, ctx: &mut EmitCtx) {
    match expr {
        PyExpr::Compare(left, ops) => {
            if ops.len() == 1 && (ops[0].0 == "in" || ops[0].0 == "not in") {
                let op = &ops[0].0;
                let right = &ops[0].1;
                if op == "in" {
                    out.push_str("string_index_of(");
                    emit_expr(out, right, ctx);
                    out.push_str(" and ");
                    emit_expr(out, left, ctx);
                    out.push_str(") != -1");
                } else {
                    out.push_str("string_index_of(");
                    emit_expr(out, right, ctx);
                    out.push_str(" and ");
                    emit_expr(out, left, ctx);
                    out.push_str(") equals -1");
                }
                return;
            }
            emit_expr(out, left, ctx);
            for (op, right) in ops {
                match op.as_str() {
                    "==" => out.push_str(" equals "),
                    "!=" => out.push_str(" != "),
                    "<" => out.push_str(" < "),
                    ">" => out.push_str(" > "),
                    "<=" => out.push_str(" <= "),
                    ">=" => out.push_str(" >= "),
                    other => { out.push(' '); out.push_str(other); out.push(' '); }
                };
                emit_expr(out, right, ctx);
            }
        }
        PyExpr::BoolOp(op, exprs) => {
            let ep_op = if op == "and" { " and " } else { " or " };
            for (i, e) in exprs.iter().enumerate() {
                if i > 0 { out.push_str(ep_op); }
                emit_cond(out, e, ctx);
            }
        }
        PyExpr::UnaryOp(op, expr) if op == "not" => {
            out.push_str("not ");
            emit_cond(out, expr, ctx);
        }
        PyExpr::Subscript(_, _) => {
            emit_expr(out, expr, ctx);
            out.push_str(" equals true");
        }
        _ => {
            emit_expr(out, expr, ctx);
        }
    }
}

fn emit_expr(out: &mut String, expr: &PyExpr, ctx: &mut EmitCtx) {
    match expr {
        PyExpr::Int(n) => out.push_str(&n.to_string()),
        PyExpr::Float(f) => out.push_str(&(*f as i64).to_string()),
        PyExpr::Str(s) => {
            out.push('"');
            out.push_str(&s.replace('"', "\\\"").replace('\n', "\\n"));
            out.push('"');
        }
        PyExpr::FStr(s) => {
            out.push_str("f\"");
            out.push_str(&s.replace('"', "\\\"").replace('\n', "\\n"));
            out.push('"');
        }
        PyExpr::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        PyExpr::None => out.push('0'),
        PyExpr::Name(n) => out.push_str(&sanitize_ernos_ident(n)),

        PyExpr::BinOp(left, op, right) => {
            match op.as_str() {
                "**" => {
                    out.push_str("# power operation: ");
                    emit_expr(out, left, ctx);
                    out.push_str(" ** ");
                    emit_expr(out, right, ctx);
                    return;
                }
                _ => {}
            }
            let ep_op = match op.as_str() {
                "+" => " + ",
                "-" => " - ",
                "*" => " * ",
                "//" => " / ",
                "/" => " / ",
                "%" => " % ",
                _ => {
                    emit_expr(out, left, ctx);
                    out.push(' '); out.push_str(op); out.push(' ');
                    emit_expr(out, right, ctx);
                    return;
                }
            };
            emit_expr(out, left, ctx);
            out.push_str(ep_op);
            emit_expr(out, right, ctx);
        }

        PyExpr::UnaryOp(op, expr) => {
            if op == "-" {
                out.push_str("0 - ");
                emit_expr(out, expr, ctx);
            } else if op == "not" {
                out.push_str("not ");
                emit_expr(out, expr, ctx);
            }
        }

        PyExpr::Compare(left, ops) => {
            if ops.len() == 1 && (ops[0].0 == "in" || ops[0].0 == "not in") {
                let op = &ops[0].0;
                let right = &ops[0].1;
                if op == "in" {
                    out.push_str("string_index_of(");
                    emit_expr(out, right, ctx);
                    out.push_str(" and ");
                    emit_expr(out, left, ctx);
                    out.push_str(") != -1");
                } else {
                    out.push_str("string_index_of(");
                    emit_expr(out, right, ctx);
                    out.push_str(" and ");
                    emit_expr(out, left, ctx);
                    out.push_str(") equals -1");
                }
                return;
            }
            emit_expr(out, left, ctx);
            for (op, right) in ops {
                match op.as_str() {
                    "==" => out.push_str(" equals "),
                    "!=" => out.push_str(" != "),
                    other => { out.push(' '); out.push_str(other); out.push(' '); }
                };
                emit_expr(out, right, ctx);
            }
        }

        PyExpr::BoolOp(op, exprs) => {
            let ep_op = if op == "and" { " and " } else { " or " };
            for (i, e) in exprs.iter().enumerate() {
                if i > 0 { out.push_str(ep_op); }
                emit_expr(out, e, ctx);
            }
        }

        PyExpr::Call(func, args, _kwargs) => {
            if let PyExpr::Name(fname) = func.as_ref() {
                match fname.as_str() {
                    "len" => {
                        if !args.is_empty() {
                            out.push_str("length_list(");
                            emit_expr(out, &args[0], ctx);
                            out.push(')');
                        }
                        return;
                    }
                    "int" => {
                        if !args.is_empty() {
                            out.push_str("string_to_int(");
                            emit_expr(out, &args[0], ctx);
                            out.push(')');
                        }
                        return;
                    }
                    "str" => {
                        if !args.is_empty() {
                            out.push_str("int_to_string(");
                            emit_expr(out, &args[0], ctx);
                            out.push(')');
                        }
                        return;
                    }
                    "input" => {
                        out.push_str("read_line()");
                        return;
                    }
                    "abs" => {
                        if !args.is_empty() {
                            out.push_str("ep_abs(");
                            emit_expr(out, &args[0], ctx);
                            out.push(')');
                        }
                        return;
                    }
                    "print" => {
                        if !args.is_empty() {
                            out.push_str("display ");
                            emit_expr(out, &args[0], ctx);
                        }
                        return;
                    }
                    _ => {}
                }
            }

            if let PyExpr::Attribute(obj, method) = func.as_ref() {
                if let PyExpr::Name(obj_name) = obj.as_ref() {
                    let sanitized_obj = sanitize_ernos_ident(obj_name);
                    match method.as_str() {
                        "cursor" => {
                            emit_expr(out, obj, ctx);
                            return;
                        }
                        "fetchall" => {
                            let res_var = ctx.last_query_results.get(&sanitized_obj)
                                .cloned()
                                .unwrap_or_else(|| format!("_query_res_{}", sanitized_obj));
                            out.push_str(&res_var);
                            return;
                        }
                        "fetchone" => {
                            let res_var = ctx.last_query_results.get(&sanitized_obj)
                                .cloned()
                                .unwrap_or_else(|| format!("_query_res_{}", sanitized_obj));
                            out.push_str("get_list(");
                            out.push_str(&res_var);
                            out.push_str(" and 0)");
                            return;
                        }
                        _ => {}
                    }
                }
            }

            if let PyExpr::Attribute(obj, method) = func.as_ref() {
                if let PyExpr::Name(obj_name) = obj.as_ref() {
                    if obj_name == "sqlite3" && method == "connect" {
                        out.push_str("sql_open(");
                        if !args.is_empty() {
                            emit_expr(out, &args[0], ctx);
                        }
                        out.push(')');
                        return;
                    }
                }
            }

            if let PyExpr::Attribute(obj, method) = func.as_ref() {
                match method.as_str() {
                    "append" => {
                        out.push_str("append_list(");
                        emit_expr(out, obj, ctx);
                        if !args.is_empty() {
                            out.push_str(" and ");
                            emit_expr(out, &args[0], ctx);
                        }
                        out.push(')');
                        return;
                    }
                    "pop" => {
                        out.push_str("pop_list(");
                        emit_expr(out, obj, ctx);
                        out.push(')');
                        return;
                    }
                    "close" => {
                        out.push_str("sql_close(");
                        emit_expr(out, obj, ctx);
                        out.push(')');
                        return;
                    }
                    "commit" => {
                        out.push_str("0");
                        return;
                    }
                    "upper" => {
                        out.push_str("string_upper(");
                        emit_expr(out, obj, ctx);
                        out.push(')');
                        return;
                    }
                    "lower" => {
                        out.push_str("string_lower(");
                        emit_expr(out, obj, ctx);
                        out.push(')');
                        return;
                    }
                    "strip" => {
                        out.push_str("string_trim(");
                        emit_expr(out, obj, ctx);
                        out.push(')');
                        return;
                    }
                    "split" => {
                        out.push_str("string_split(");
                        emit_expr(out, obj, ctx);
                        if !args.is_empty() {
                            out.push_str(" and ");
                            emit_expr(out, &args[0], ctx);
                        } else {
                            out.push_str(" and \" \"");
                        }
                        out.push(')');
                        return;
                    }
                    "replace" => {
                        if args.len() >= 2 {
                            out.push_str("string_replace(");
                            emit_expr(out, obj, ctx);
                            out.push_str(" and ");
                            emit_expr(out, &args[0], ctx);
                            out.push_str(" and ");
                            emit_expr(out, &args[1], ctx);
                            out.push(')');
                        }
                        return;
                    }
                    "find" | "index" => {
                        out.push_str("string_index_of(");
                        emit_expr(out, obj, ctx);
                        if !args.is_empty() {
                            out.push_str(" and ");
                            emit_expr(out, &args[0], ctx);
                        }
                        out.push(')');
                        return;
                    }
                    "join" => {
                        out.push_str("# .join() — manual translation needed: ");
                        emit_expr(out, obj, ctx);
                        out.push_str(".join(...)");
                        return;
                    }
                    "keys" => {
                        out.push_str("map_keys(");
                        emit_expr(out, obj, ctx);
                        out.push(')');
                        return;
                    }
                    "values" => {
                        out.push_str("map_values(");
                        emit_expr(out, obj, ctx);
                        out.push(')');
                        return;
                    }
                    "items" => {
                        out.push_str("# .items() — manual translation needed for ");
                        emit_expr(out, obj, ctx);
                        return;
                    }
                    "get" => {
                        out.push_str("map_get_val(");
                        emit_expr(out, obj, ctx);
                        if !args.is_empty() {
                            out.push_str(" and ");
                            emit_expr(out, &args[0], ctx);
                        }
                        out.push(')');
                        return;
                    }
                    _ => {}
                }
            }

            emit_expr(out, func, ctx);
            out.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 { out.push_str(" and "); }
                emit_expr(out, arg, ctx);
            }
            out.push(')');
        }

        PyExpr::Attribute(obj, attr) => {
            emit_expr(out, obj, ctx);
            out.push('.');
            out.push_str(attr);
        }

        PyExpr::Subscript(obj, idx) => {
            let is_map = match idx.as_ref() {
                PyExpr::Str(_) => true,
                PyExpr::Name(name) => name.contains("key") || name.contains("name") || name.contains("id"),
                _ => false,
            } || match obj.as_ref() {
                PyExpr::Name(name) => name.contains("dict") || name.contains("map") || name.contains("info"),
                _ => false,
            };

            if is_map {
                out.push_str("map_get_val(");
            } else {
                out.push_str("get_list(");
            }
            emit_expr(out, obj, ctx);
            out.push_str(" and ");
            emit_expr(out, idx, ctx);
            out.push(')');
        }

        PyExpr::List(elems) => {
            if elems.is_empty() {
                out.push_str("create_list()");
            } else {
                out.push('[');
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    emit_expr(out, e, ctx);
                }
                out.push(']');
            }
        }

        PyExpr::Dict(pairs) => {
            if pairs.is_empty() {
                out.push_str("create_map()");
            } else {
                out.push_str("create_map()");
                if !pairs.is_empty() {
                    out.push_str(" # dict literal — add map_insert calls");
                }
            }
        }

        PyExpr::IfExpr(val, cond, alt) => {
            out.push_str("# ternary: ");
            emit_expr(out, val, ctx);
            out.push_str(" if ");
            emit_expr(out, cond, ctx);
            out.push_str(" else ");
            emit_expr(out, alt, ctx);
        }

        PyExpr::ListComp(_expr, var, iter, filter) => {
            out.push_str(&format!("# list comprehension: [{} for {} in ", "...", var));
            emit_expr(out, iter, ctx);
            if let Some(f) = filter {
                out.push_str(" if ");
                emit_expr(out, f, ctx);
            }
            out.push(']');
        }

        PyExpr::Await(expr) => {
            out.push_str("await ");
            emit_expr(out, expr, ctx);
        }
    }
}
