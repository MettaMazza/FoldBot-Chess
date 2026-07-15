/// C → ErnosPlain Transpiler
///
/// Parses a subset of C source code and emits equivalent ErnosPlain (.ep) code.
///
/// Architecture:
///   C source → C tokenizer → C AST → ErnosPlain emitter → .ep file

use std::collections::HashMap;

// ============ C Tokenizer ============

#[derive(Debug, Clone, PartialEq)]
pub enum CTok {
    Ident(String),
    IntLit(i64),
    FloatLit(f64),
    StrLit(String),
    CharLit(char),
    // Operators
    Plus, Minus, Star, Slash, Percent, Amp, Pipe, Caret, Tilde,
    PlusPlus, MinusMinus,
    PlusEq, MinusEq, StarEq, SlashEq, PercentEq,
    AmpAmp, PipePipe, Bang,
    Eq, EqEq, NotEq, Lt, Gt, LtEq, GtEq,
    LShift, RShift,
    Arrow, Dot,
    Question,
    // Delimiters
    LParen, RParen, LBracket, RBracket, LBrace, RBrace,
    Comma, Semi, Colon,
    // Preprocessor
    Hash,
    // Special
    Newline,
    Eof,
}

pub struct CLexer {
    chars: Vec<char>,
    pos: usize,
}

impl CLexer {
    pub fn new(input: &str) -> Self {
        CLexer { chars: input.chars().collect(), pos: 0 }
    }

    fn peek(&self) -> Option<char> { self.chars.get(self.pos).copied() }
    fn advance(&mut self) -> Option<char> { let c = self.chars.get(self.pos).copied(); self.pos += 1; c }
    fn peek_at(&self, off: usize) -> Option<char> { self.chars.get(self.pos + off).copied() }

    pub fn tokenize(&mut self) -> Vec<CTok> {
        let mut tokens = Vec::new();
        loop {
            // Skip whitespace (except newlines for preprocessor)
            while matches!(self.peek(), Some(' ') | Some('\t') | Some('\r')) { self.advance(); }

            match self.peek() {
                None => break,
                Some('\n') => { self.advance(); tokens.push(CTok::Newline); }
                Some('/') if self.peek_at(1) == Some('/') => {
                    while self.peek() != Some('\n') && self.peek().is_some() { self.advance(); }
                }
                Some('/') if self.peek_at(1) == Some('*') => {
                    self.advance(); self.advance();
                    loop {
                        match self.advance() {
                            Some('*') if self.peek() == Some('/') => { self.advance(); break; }
                            None => break,
                            _ => {}
                        }
                    }
                }
                Some('#') => { self.advance(); tokens.push(CTok::Hash); }
                Some('"') => {
                    self.advance();
                    let mut s = String::new();
                    loop {
                        match self.advance() {
                            Some('"') => break,
                            Some('\\') => {
                                match self.advance() {
                                    Some('n') => s.push('\n'),
                                    Some('t') => s.push('\t'),
                                    Some('\\') => s.push('\\'),
                                    Some('"') => s.push('"'),
                                    Some('0') => s.push('\0'),
                                    Some(c) => { s.push('\\'); s.push(c); }
                                    None => break,
                                }
                            }
                            Some(c) => s.push(c),
                            None => break,
                        }
                    }
                    tokens.push(CTok::StrLit(s));
                }
                Some('\'') => {
                    self.advance();
                    let ch = match self.advance() {
                        Some('\\') => match self.advance() {
                            Some('n') => '\n', Some('t') => '\t', Some('\\') => '\\',
                            Some('0') => '\0', Some(c) => c, None => '\0',
                        },
                        Some(c) => c,
                        None => '\0',
                    };
                    if self.peek() == Some('\'') { self.advance(); }
                    tokens.push(CTok::CharLit(ch));
                }
                Some(c) if c.is_ascii_digit() => {
                    let mut num = String::new();
                    // Handle hex
                    if c == '0' && matches!(self.peek_at(1), Some('x') | Some('X')) {
                        self.advance(); self.advance();
                        while let Some(c) = self.peek() {
                            if c.is_ascii_hexdigit() { num.push(c); self.advance(); } else { break; }
                        }
                        tokens.push(CTok::IntLit(i64::from_str_radix(&num, 16).unwrap_or(0)));
                        continue;
                    }
                    while let Some(c) = self.peek() {
                        if c.is_ascii_digit() || c == '_' { if c != '_' { num.push(c); } self.advance(); } else { break; }
                    }
                    if self.peek() == Some('.') && self.peek_at(1).map_or(false, |c| c.is_ascii_digit()) {
                        num.push('.'); self.advance();
                        while let Some(c) = self.peek() {
                            if c.is_ascii_digit() { num.push(c); self.advance(); } else { break; }
                        }
                        tokens.push(CTok::FloatLit(num.parse().unwrap_or(0.0)));
                    } else {
                        // Skip type suffixes (L, LL, U, UL, etc.)
                        while matches!(self.peek(), Some('L') | Some('l') | Some('U') | Some('u')) { self.advance(); }
                        tokens.push(CTok::IntLit(num.parse().unwrap_or(0)));
                    }
                }
                Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                    let mut ident = String::new();
                    while let Some(c) = self.peek() {
                        if c.is_ascii_alphanumeric() || c == '_' { ident.push(c); self.advance(); } else { break; }
                    }
                    tokens.push(CTok::Ident(ident));
                }
                Some('+') => { self.advance(); match self.peek() { Some('+') => { self.advance(); tokens.push(CTok::PlusPlus); } Some('=') => { self.advance(); tokens.push(CTok::PlusEq); } _ => tokens.push(CTok::Plus) } }
                Some('-') => { self.advance(); match self.peek() { Some('-') => { self.advance(); tokens.push(CTok::MinusMinus); } Some('=') => { self.advance(); tokens.push(CTok::MinusEq); } Some('>') => { self.advance(); tokens.push(CTok::Arrow); } _ => tokens.push(CTok::Minus) } }
                Some('*') => { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(CTok::StarEq); } else { tokens.push(CTok::Star); } }
                Some('/') => { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(CTok::SlashEq); } else { tokens.push(CTok::Slash); } }
                Some('%') => { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(CTok::PercentEq); } else { tokens.push(CTok::Percent); } }
                Some('=') => { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(CTok::EqEq); } else { tokens.push(CTok::Eq); } }
                Some('!') => { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(CTok::NotEq); } else { tokens.push(CTok::Bang); } }
                Some('<') => { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(CTok::LtEq); } else if self.peek() == Some('<') { self.advance(); tokens.push(CTok::LShift); } else { tokens.push(CTok::Lt); } }
                Some('>') => { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(CTok::GtEq); } else if self.peek() == Some('>') { self.advance(); tokens.push(CTok::RShift); } else { tokens.push(CTok::Gt); } }
                Some('&') => { self.advance(); if self.peek() == Some('&') { self.advance(); tokens.push(CTok::AmpAmp); } else { tokens.push(CTok::Amp); } }
                Some('|') => { self.advance(); if self.peek() == Some('|') { self.advance(); tokens.push(CTok::PipePipe); } else { tokens.push(CTok::Pipe); } }
                Some('^') => { self.advance(); tokens.push(CTok::Caret); }
                Some('~') => { self.advance(); tokens.push(CTok::Tilde); }
                Some('?') => { self.advance(); tokens.push(CTok::Question); }
                Some('(') => { self.advance(); tokens.push(CTok::LParen); }
                Some(')') => { self.advance(); tokens.push(CTok::RParen); }
                Some('[') => { self.advance(); tokens.push(CTok::LBracket); }
                Some(']') => { self.advance(); tokens.push(CTok::RBracket); }
                Some('{') => { self.advance(); tokens.push(CTok::LBrace); }
                Some('}') => { self.advance(); tokens.push(CTok::RBrace); }
                Some(',') => { self.advance(); tokens.push(CTok::Comma); }
                Some(';') => { self.advance(); tokens.push(CTok::Semi); }
                Some(':') => { self.advance(); tokens.push(CTok::Colon); }
                Some('.') => { self.advance(); tokens.push(CTok::Dot); }
                _ => { self.advance(); }
            }
        }
        tokens.push(CTok::Eof);
        tokens
    }
}

// ============ C AST (Simplified) ============

#[derive(Debug, Clone)]
pub enum CExpr {
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
    Name(String),
    BinOp(Box<CExpr>, String, Box<CExpr>),
    UnaryOp(String, Box<CExpr>),
    Call(Box<CExpr>, Vec<CExpr>),
    Member(Box<CExpr>, String, bool), // obj.field or obj->field
    Index(Box<CExpr>, Box<CExpr>),
    Cast(String, Box<CExpr>),
    Sizeof(String),
    Ternary(Box<CExpr>, Box<CExpr>, Box<CExpr>),
    Assign(Box<CExpr>, Box<CExpr>),
    Null,
}

#[derive(Debug, Clone)]
pub enum CStmt {
    VarDecl(String, String, Option<CExpr>), // type, name, init
    Expr(CExpr),
    Return(Option<CExpr>),
    If(CExpr, Vec<CStmt>, Option<Vec<CStmt>>),
    While(CExpr, Vec<CStmt>),
    For(Option<Box<CStmt>>, Option<CExpr>, Option<CExpr>, Vec<CStmt>),
    Switch(CExpr, Vec<(CExpr, Vec<CStmt>)>, Option<Vec<CStmt>>),
    Break,
    Continue,
    FuncDef(String, String, Vec<(String, String)>, Vec<CStmt>), // ret_type, name, params, body
    StructDef(String, Vec<(String, String)>), // name, fields
    Typedef(String, String), // old_name, new_name
    Preprocessor(String, String), // directive, content
    Block(Vec<CStmt>),
}

// ============ C Parser ============

pub struct CParser {
    tokens: Vec<CTok>,
    pos: usize,
    typedefs: HashMap<String, bool>,
}

impl CParser {
    pub fn new(tokens: Vec<CTok>) -> Self {
        let mut typedefs = HashMap::new();
        // Standard types
        for t in &["size_t", "ssize_t", "uint8_t", "uint16_t", "uint32_t", "uint64_t",
                   "int8_t", "int16_t", "int32_t", "int64_t", "bool", "FILE",
                   "ptrdiff_t", "intptr_t", "uintptr_t", "time_t", "pid_t"] {
            typedefs.insert(t.to_string(), true);
        }
        CParser { tokens, pos: 0, typedefs }
    }

    fn peek(&self) -> &CTok { self.tokens.get(self.pos).unwrap_or(&CTok::Eof) }
    fn advance(&mut self) -> CTok { let t = self.tokens.get(self.pos).cloned().unwrap_or(CTok::Eof); self.pos += 1; t }
    fn skip_newlines(&mut self) { while matches!(self.peek(), CTok::Newline) { self.advance(); } }

    fn is_type_keyword(s: &str) -> bool {
        matches!(s, "int" | "long" | "short" | "char" | "void" | "float" | "double"
            | "unsigned" | "signed" | "const" | "static" | "extern" | "volatile"
            | "auto" | "register" | "inline" | "struct" | "union" | "enum" | "bool"
            | "_Bool" | "restrict" | "__restrict")
    }

    fn is_type_start(&self) -> bool {
        match self.peek() {
            CTok::Ident(s) => Self::is_type_keyword(s) || self.typedefs.contains_key(s),
            _ => false,
        }
    }

    /// Parse an entire C file
    pub fn parse(&mut self) -> Vec<CStmt> {
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek(), CTok::Eof) { break; }
            if let Some(s) = self.parse_toplevel() {
                stmts.push(s);
            }
        }
        stmts
    }

    fn parse_toplevel(&mut self) -> Option<CStmt> {
        // Preprocessor
        if matches!(self.peek(), CTok::Hash) {
            return self.parse_preprocessor();
        }

        // Typedef
        if let CTok::Ident(s) = self.peek() {
            if s == "typedef" {
                return self.parse_typedef();
            }
        }

        // Struct definition
        if let CTok::Ident(s) = self.peek() {
            if s == "struct" || s == "union" {
                let save = self.pos;
                self.advance(); // struct
                let name = if let CTok::Ident(n) = self.peek() { let n = n.clone(); self.advance(); n } else { "anon".into() };
                if matches!(self.peek(), CTok::LBrace) {
                    return self.parse_struct_def(&name);
                }
                self.pos = save;
            }
        }

        // Enum
        if let CTok::Ident(s) = self.peek() {
            if s == "enum" {
                return self.parse_enum();
            }
        }

        // Try function or variable declaration
        if self.is_type_start() {
            return self.parse_decl_or_func();
        }

        // Skip unknown
        self.advance();
        None
    }

    fn parse_preprocessor(&mut self) -> Option<CStmt> {
        self.advance(); // #
        if let CTok::Ident(dir) = self.advance() {
            let mut content = String::new();
            while !matches!(self.peek(), CTok::Newline | CTok::Eof) {
                match self.advance() {
                    CTok::Ident(s) => { if !content.is_empty() { content.push(' '); } content.push_str(&s); }
                    CTok::IntLit(n) => { if !content.is_empty() { content.push(' '); } content.push_str(&n.to_string()); }
                    CTok::FloatLit(f) => { if !content.is_empty() { content.push(' '); } content.push_str(&f.to_string()); }
                    CTok::StrLit(s) => { if !content.is_empty() { content.push(' '); } content.push('"'); content.push_str(&s); content.push('"'); }
                    CTok::LParen => { content.push('('); }
                    CTok::RParen => { content.push(')'); }
                    CTok::Plus => { content.push('+'); }
                    CTok::Minus => { content.push('-'); }
                    CTok::Star => { content.push('*'); }
                    CTok::Slash => { content.push('/'); }
                    CTok::Comma => { content.push(','); }
                    _ => {}
                }
            }
            self.skip_newlines();
            Some(CStmt::Preprocessor(dir, content))
        } else {
            None
        }
    }

    fn parse_typedef(&mut self) -> Option<CStmt> {
        self.advance(); // typedef
        // Skip until semicolon, capture last ident as new type name
        let mut last_ident = String::new();
        let mut depth = 0;
        loop {
            match self.advance() {
                CTok::LBrace => depth += 1,
                CTok::RBrace => depth -= 1,
                CTok::Ident(s) if depth == 0 => last_ident = s,
                CTok::Semi if depth == 0 => break,
                CTok::Eof => break,
                _ => {}
            }
        }
        if !last_ident.is_empty() {
            self.typedefs.insert(last_ident.clone(), true);
            Some(CStmt::Typedef("type".into(), last_ident))
        } else {
            None
        }
    }

    fn parse_struct_def(&mut self, name: &str) -> Option<CStmt> {
        self.advance(); // {
        let mut fields = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek(), CTok::RBrace | CTok::Eof) { break; }
            let type_name = self.consume_type_name();
            if let CTok::Ident(field_name) = self.advance() {
                // Skip array dims
                while matches!(self.peek(), CTok::LBracket) {
                    self.advance();
                    while !matches!(self.peek(), CTok::RBracket | CTok::Eof) { self.advance(); }
                    if matches!(self.peek(), CTok::RBracket) { self.advance(); }
                }
                fields.push((type_name, field_name));
            }
            while !matches!(self.peek(), CTok::Semi | CTok::RBrace | CTok::Eof) { self.advance(); }
            if matches!(self.peek(), CTok::Semi) { self.advance(); }
        }
        if matches!(self.peek(), CTok::RBrace) { self.advance(); }
        if matches!(self.peek(), CTok::Semi) { self.advance(); }
        self.typedefs.insert(name.to_string(), true);
        Some(CStmt::StructDef(name.to_string(), fields))
    }

    fn parse_enum(&mut self) -> Option<CStmt> {
        self.advance(); // enum
        let name = if let CTok::Ident(n) = self.peek() { let n = n.clone(); self.advance(); n } else { "anon".into() };
        if !matches!(self.peek(), CTok::LBrace) {
            // Forward declaration
            while !matches!(self.peek(), CTok::Semi | CTok::Eof) { self.advance(); }
            if matches!(self.peek(), CTok::Semi) { self.advance(); }
            return None;
        }
        self.advance(); // {
        let mut stmts = Vec::new();
        let mut val = 0i64;
        loop {
            self.skip_newlines();
            if matches!(self.peek(), CTok::RBrace | CTok::Eof) { break; }
            if let CTok::Ident(ename) = self.advance() {
                if matches!(self.peek(), CTok::Eq) {
                    self.advance();
                    if let CTok::IntLit(n) = self.advance() { val = n; }
                }
                stmts.push(CStmt::VarDecl("enum".into(), format!("{}_{}", name, ename), Some(CExpr::Int(val))));
                val += 1;
            }
            if matches!(self.peek(), CTok::Comma) { self.advance(); }
        }
        if matches!(self.peek(), CTok::RBrace) { self.advance(); }
        if matches!(self.peek(), CTok::Semi) { self.advance(); }
        Some(CStmt::Block(stmts))
    }

    fn consume_type_name(&mut self) -> String {
        let mut parts = Vec::new();
        loop {
            match self.peek() {
                CTok::Ident(s) if Self::is_type_keyword(s) || self.typedefs.contains_key(s) => {
                    parts.push(s.clone());
                    self.advance();
                }
                CTok::Star => { parts.push("*".into()); self.advance(); }
                _ => break,
            }
        }
        if parts.is_empty() { "int".into() } else { parts.join(" ") }
    }

    fn parse_decl_or_func(&mut self) -> Option<CStmt> {
        let type_name = self.consume_type_name();
        let name = if let CTok::Ident(n) = self.advance() { n } else { return None };

        // Function definition/declaration
        if matches!(self.peek(), CTok::LParen) {
            self.advance(); // (
            let mut params = Vec::new();
            while !matches!(self.peek(), CTok::RParen | CTok::Eof) {
                if let CTok::Ident(s) = self.peek() {
                    if s == "void" {
                        self.advance();
                        break;
                    }
                }
                let ptype = self.consume_type_name();
                let pname = if let CTok::Ident(n) = self.peek() {
                    let n = n.clone(); self.advance(); n
                } else {
                    format!("arg{}", params.len())
                };
                // Skip array dims in params
                while matches!(self.peek(), CTok::LBracket) {
                    self.advance();
                    while !matches!(self.peek(), CTok::RBracket | CTok::Eof) { self.advance(); }
                    if matches!(self.peek(), CTok::RBracket) { self.advance(); }
                }
                params.push((ptype, pname));
                if matches!(self.peek(), CTok::Comma) { self.advance(); }
            }
            if matches!(self.peek(), CTok::RParen) { self.advance(); }

            // Function body or declaration
            if matches!(self.peek(), CTok::LBrace) {
                let body = self.parse_block();
                return Some(CStmt::FuncDef(type_name, name, params, body));
            } else {
                // Forward declaration
                if matches!(self.peek(), CTok::Semi) { self.advance(); }
                return Some(CStmt::FuncDef(type_name, name, params, vec![]));
            }
        }

        // Variable declaration
        let init = if matches!(self.peek(), CTok::Eq) {
            self.advance();
            self.parse_expr()
        } else {
            None
        };
        // Skip array dims
        while matches!(self.peek(), CTok::LBracket) {
            self.advance();
            while !matches!(self.peek(), CTok::RBracket | CTok::Eof) { self.advance(); }
            if matches!(self.peek(), CTok::RBracket) { self.advance(); }
        }
        if matches!(self.peek(), CTok::Semi) { self.advance(); }
        Some(CStmt::VarDecl(type_name, name, init))
    }

    fn parse_block(&mut self) -> Vec<CStmt> {
        let mut stmts = Vec::new();
        if !matches!(self.peek(), CTok::LBrace) { return stmts; }
        self.advance(); // {
        loop {
            self.skip_newlines();
            if matches!(self.peek(), CTok::RBrace | CTok::Eof) { break; }
            if let Some(s) = self.parse_stmt() {
                stmts.push(s);
            }
        }
        if matches!(self.peek(), CTok::RBrace) { self.advance(); }
        stmts
    }

    fn parse_stmt(&mut self) -> Option<CStmt> {
        self.skip_newlines();
        match self.peek().clone() {
            CTok::Ident(ref s) => {
                match s.as_str() {
                    "return" => {
                        self.advance();
                        if matches!(self.peek(), CTok::Semi) {
                            self.advance();
                            Some(CStmt::Return(None))
                        } else {
                            let expr = self.parse_expr();
                            if matches!(self.peek(), CTok::Semi) { self.advance(); }
                            Some(CStmt::Return(expr))
                        }
                    }
                    "if" => self.parse_if(),
                    "while" => self.parse_while(),
                    "for" => self.parse_for(),
                    "switch" => self.parse_switch(),
                    "break" => { self.advance(); if matches!(self.peek(), CTok::Semi) { self.advance(); } Some(CStmt::Break) }
                    "continue" => { self.advance(); if matches!(self.peek(), CTok::Semi) { self.advance(); } Some(CStmt::Continue) }
                    "do" => {
                        self.advance();
                        let body = self.parse_block();
                        // Skip 'while (cond);'
                        if let CTok::Ident(s) = self.peek() { if s == "while" { self.advance(); } }
                        if matches!(self.peek(), CTok::LParen) {
                            self.advance();
                            let _cond = self.parse_expr();
                            if matches!(self.peek(), CTok::RParen) { self.advance(); }
                        }
                        if matches!(self.peek(), CTok::Semi) { self.advance(); }
                        Some(CStmt::While(CExpr::Int(1), body))
                    }
                    _ if self.is_type_start() => self.parse_local_decl(),
                    _ => self.parse_expr_stmt(),
                }
            }
            CTok::LBrace => {
                let block = self.parse_block();
                Some(CStmt::Block(block))
            }
            CTok::Hash => {
                self.parse_preprocessor()
            }
            _ if self.is_type_start() => self.parse_local_decl(),
            _ => self.parse_expr_stmt(),
        }
    }

    fn parse_local_decl(&mut self) -> Option<CStmt> {
        let type_name = self.consume_type_name();
        let name = if let CTok::Ident(n) = self.advance() { n } else { return None };
        // Array
        if matches!(self.peek(), CTok::LBracket) {
            self.advance();
            while !matches!(self.peek(), CTok::RBracket | CTok::Eof) { self.advance(); }
            if matches!(self.peek(), CTok::RBracket) { self.advance(); }
            let init = if matches!(self.peek(), CTok::Eq) {
                self.advance();
                // Skip initializer
                if matches!(self.peek(), CTok::LBrace) {
                    let mut depth = 0;
                    loop {
                        match self.advance() {
                            CTok::LBrace => depth += 1,
                            CTok::RBrace => { depth -= 1; if depth == 0 { break; } }
                            CTok::Eof => break,
                            _ => {}
                        }
                    }
                }
                None
            } else { None };
            if matches!(self.peek(), CTok::Semi) { self.advance(); }
            return Some(CStmt::VarDecl(type_name, name, init));
        }
        let init = if matches!(self.peek(), CTok::Eq) {
            self.advance();
            self.parse_expr()
        } else { None };
        if matches!(self.peek(), CTok::Semi) { self.advance(); }
        Some(CStmt::VarDecl(type_name, name, init))
    }

    fn parse_expr_stmt(&mut self) -> Option<CStmt> {
        let expr = self.parse_expr()?;
        if matches!(self.peek(), CTok::Semi) { self.advance(); }
        Some(CStmt::Expr(expr))
    }

    fn parse_if(&mut self) -> Option<CStmt> {
        self.advance(); // if
        if matches!(self.peek(), CTok::LParen) { self.advance(); }
        let cond = self.parse_expr()?;
        if matches!(self.peek(), CTok::RParen) { self.advance(); }
        let body = if matches!(self.peek(), CTok::LBrace) {
            self.parse_block()
        } else {
            let s = self.parse_stmt();
            s.into_iter().collect()
        };
        let else_body = if let CTok::Ident(s) = self.peek() {
            if s == "else" {
                self.advance();
                if matches!(self.peek(), CTok::LBrace) {
                    Some(self.parse_block())
                } else {
                    let s = self.parse_stmt();
                    Some(s.into_iter().collect())
                }
            } else { None }
        } else { None };
        Some(CStmt::If(cond, body, else_body))
    }

    fn parse_while(&mut self) -> Option<CStmt> {
        self.advance(); // while
        if matches!(self.peek(), CTok::LParen) { self.advance(); }
        let cond = self.parse_expr()?;
        if matches!(self.peek(), CTok::RParen) { self.advance(); }
        let body = if matches!(self.peek(), CTok::LBrace) {
            self.parse_block()
        } else {
            self.parse_stmt().into_iter().collect()
        };
        Some(CStmt::While(cond, body))
    }

    fn parse_for(&mut self) -> Option<CStmt> {
        self.advance(); // for
        if matches!(self.peek(), CTok::LParen) { self.advance(); }
        let init = if matches!(self.peek(), CTok::Semi) { self.advance(); None }
        else if self.is_type_start() { let d = self.parse_local_decl(); d.map(Box::new) }
        else { let e = self.parse_expr_stmt(); e.map(Box::new) };
        let cond = if matches!(self.peek(), CTok::Semi) { None } else { self.parse_expr() };
        if matches!(self.peek(), CTok::Semi) { self.advance(); }
        let step = if matches!(self.peek(), CTok::RParen) { None } else { self.parse_expr() };
        if matches!(self.peek(), CTok::RParen) { self.advance(); }
        let body = if matches!(self.peek(), CTok::LBrace) {
            self.parse_block()
        } else {
            self.parse_stmt().into_iter().collect()
        };
        Some(CStmt::For(init, cond, step, body))
    }

    fn parse_switch(&mut self) -> Option<CStmt> {
        self.advance(); // switch
        if matches!(self.peek(), CTok::LParen) { self.advance(); }
        let expr = self.parse_expr()?;
        if matches!(self.peek(), CTok::RParen) { self.advance(); }
        if !matches!(self.peek(), CTok::LBrace) { return None; }
        self.advance(); // {
        let mut cases = Vec::new();
        let mut default = None;
        loop {
            self.skip_newlines();
            if matches!(self.peek(), CTok::RBrace | CTok::Eof) { break; }
            if let CTok::Ident(s) = self.peek() {
                if s == "case" {
                    self.advance();
                    let val = self.parse_expr().unwrap_or(CExpr::Int(0));
                    if matches!(self.peek(), CTok::Colon) { self.advance(); }
                    let mut body = Vec::new();
                    loop {
                        self.skip_newlines();
                        match self.peek() {
                            CTok::Ident(s) if s == "case" || s == "default" => break,
                            CTok::RBrace | CTok::Eof => break,
                            _ => { if let Some(s) = self.parse_stmt() { body.push(s); } }
                        }
                    }
                    cases.push((val, body));
                    continue;
                } else if s == "default" {
                    self.advance();
                    if matches!(self.peek(), CTok::Colon) { self.advance(); }
                    let mut body = Vec::new();
                    loop {
                        self.skip_newlines();
                        match self.peek() {
                            CTok::RBrace | CTok::Eof => break,
                            CTok::Ident(s) if s == "case" => break,
                            _ => { if let Some(s) = self.parse_stmt() { body.push(s); } }
                        }
                    }
                    default = Some(body);
                    continue;
                }
            }
            self.advance(); // skip unknown
        }
        if matches!(self.peek(), CTok::RBrace) { self.advance(); }
        Some(CStmt::Switch(expr, cases, default))
    }

    // ============ Expression Parsing ============

    fn parse_expr(&mut self) -> Option<CExpr> {
        let left = self.parse_assign_expr()?;
        Some(left)
    }

    fn parse_assign_expr(&mut self) -> Option<CExpr> {
        let left = self.parse_ternary()?;
        match self.peek() {
            CTok::Eq => {
                self.advance();
                let right = self.parse_assign_expr()?;
                Some(CExpr::Assign(Box::new(left), Box::new(right)))
            }
            CTok::PlusEq => { self.advance(); let r = self.parse_assign_expr()?;
                Some(CExpr::Assign(Box::new(left.clone()), Box::new(CExpr::BinOp(Box::new(left), "+".into(), Box::new(r))))) }
            CTok::MinusEq => { self.advance(); let r = self.parse_assign_expr()?;
                Some(CExpr::Assign(Box::new(left.clone()), Box::new(CExpr::BinOp(Box::new(left), "-".into(), Box::new(r))))) }
            _ => Some(left),
        }
    }

    fn parse_ternary(&mut self) -> Option<CExpr> {
        let cond = self.parse_or()?;
        if matches!(self.peek(), CTok::Question) {
            self.advance();
            let then = self.parse_expr()?;
            if matches!(self.peek(), CTok::Colon) { self.advance(); }
            let alt = self.parse_ternary()?;
            Some(CExpr::Ternary(Box::new(cond), Box::new(then), Box::new(alt)))
        } else {
            Some(cond)
        }
    }

    fn parse_or(&mut self) -> Option<CExpr> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), CTok::PipePipe) {
            self.advance();
            let right = self.parse_and()?;
            left = CExpr::BinOp(Box::new(left), "||".into(), Box::new(right));
        }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<CExpr> {
        let mut left = self.parse_comparison()?;
        while matches!(self.peek(), CTok::AmpAmp) {
            self.advance();
            let right = self.parse_comparison()?;
            left = CExpr::BinOp(Box::new(left), "&&".into(), Box::new(right));
        }
        Some(left)
    }

    fn parse_comparison(&mut self) -> Option<CExpr> {
        let mut left = self.parse_addition()?;
        loop {
            let op = match self.peek() {
                CTok::EqEq => "==", CTok::NotEq => "!=",
                CTok::Lt => "<", CTok::Gt => ">",
                CTok::LtEq => "<=", CTok::GtEq => ">=",
                _ => break,
            };
            let op_s = op.to_string();
            self.advance();
            let right = self.parse_addition()?;
            left = CExpr::BinOp(Box::new(left), op_s, Box::new(right));
        }
        Some(left)
    }

    fn parse_addition(&mut self) -> Option<CExpr> {
        let mut left = self.parse_multiplication()?;
        loop {
            let op = match self.peek() {
                CTok::Plus => "+", CTok::Minus => "-",
                CTok::Pipe => "|", CTok::Caret => "^",
                _ => break,
            };
            let op_s = op.to_string();
            self.advance();
            let right = self.parse_multiplication()?;
            left = CExpr::BinOp(Box::new(left), op_s, Box::new(right));
        }
        Some(left)
    }

    fn parse_multiplication(&mut self) -> Option<CExpr> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                CTok::Star => "*", CTok::Slash => "/", CTok::Percent => "%",
                CTok::Amp => "&", CTok::LShift => "<<", CTok::RShift => ">>",
                _ => break,
            };
            let op_s = op.to_string();
            self.advance();
            let right = self.parse_unary()?;
            left = CExpr::BinOp(Box::new(left), op_s, Box::new(right));
        }
        Some(left)
    }

    fn parse_unary(&mut self) -> Option<CExpr> {
        match self.peek() {
            CTok::Minus => { self.advance(); let e = self.parse_postfix()?; Some(CExpr::UnaryOp("-".into(), Box::new(e))) }
            CTok::Bang => { self.advance(); let e = self.parse_postfix()?; Some(CExpr::UnaryOp("!".into(), Box::new(e))) }
            CTok::Tilde => { self.advance(); let e = self.parse_postfix()?; Some(CExpr::UnaryOp("~".into(), Box::new(e))) }
            CTok::Amp => { self.advance(); let e = self.parse_postfix()?; Some(CExpr::UnaryOp("&".into(), Box::new(e))) }
            CTok::Star => { self.advance(); let e = self.parse_postfix()?; Some(CExpr::UnaryOp("*".into(), Box::new(e))) }
            CTok::PlusPlus => { self.advance(); let e = self.parse_postfix()?; Some(CExpr::UnaryOp("++".into(), Box::new(e))) }
            CTok::MinusMinus => { self.advance(); let e = self.parse_postfix()?; Some(CExpr::UnaryOp("--".into(), Box::new(e))) }
            // sizeof
            CTok::Ident(s) if s == "sizeof" => {
                self.advance();
                if matches!(self.peek(), CTok::LParen) {
                    self.advance();
                    let mut tn = String::new();
                    while !matches!(self.peek(), CTok::RParen | CTok::Eof) {
                        match self.advance() { CTok::Ident(s) => tn.push_str(&s), _ => {} }
                    }
                    if matches!(self.peek(), CTok::RParen) { self.advance(); }
                    Some(CExpr::Sizeof(tn))
                } else { Some(CExpr::Int(0)) }
            }
            // Cast: (type)expr
            CTok::LParen => {
                let save = self.pos;
                self.advance();
                if self.is_type_start() {
                    let tn = self.consume_type_name();
                    if matches!(self.peek(), CTok::RParen) {
                        self.advance();
                        let e = self.parse_unary()?;
                        return Some(CExpr::Cast(tn, Box::new(e)));
                    }
                }
                self.pos = save;
                self.parse_postfix()
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Option<CExpr> {
        let mut expr = self.parse_atom()?;
        loop {
            match self.peek() {
                CTok::LParen => {
                    self.advance();
                    let mut args = Vec::new();
                    while !matches!(self.peek(), CTok::RParen | CTok::Eof) {
                        if let Some(a) = self.parse_expr() { args.push(a); }
                        if matches!(self.peek(), CTok::Comma) { self.advance(); }
                    }
                    if matches!(self.peek(), CTok::RParen) { self.advance(); }
                    expr = CExpr::Call(Box::new(expr), args);
                }
                CTok::LBracket => {
                    self.advance();
                    let idx = self.parse_expr()?;
                    if matches!(self.peek(), CTok::RBracket) { self.advance(); }
                    expr = CExpr::Index(Box::new(expr), Box::new(idx));
                }
                CTok::Dot => {
                    self.advance();
                    if let CTok::Ident(f) = self.advance() {
                        expr = CExpr::Member(Box::new(expr), f, false);
                    }
                }
                CTok::Arrow => {
                    self.advance();
                    if let CTok::Ident(f) = self.advance() {
                        expr = CExpr::Member(Box::new(expr), f, true);
                    }
                }
                CTok::PlusPlus => { self.advance(); expr = CExpr::UnaryOp("post++".into(), Box::new(expr)); }
                CTok::MinusMinus => { self.advance(); expr = CExpr::UnaryOp("post--".into(), Box::new(expr)); }
                _ => break,
            }
        }
        Some(expr)
    }

    fn parse_atom(&mut self) -> Option<CExpr> {
        match self.peek().clone() {
            CTok::IntLit(n) => { self.advance(); Some(CExpr::Int(n)) }
            CTok::FloatLit(f) => { self.advance(); Some(CExpr::Float(f)) }
            CTok::StrLit(s) => { self.advance(); Some(CExpr::Str(s)) }
            CTok::CharLit(c) => { self.advance(); Some(CExpr::Char(c)) }
            CTok::Ident(s) => {
                match s.as_str() {
                    "NULL" | "nullptr" => { self.advance(); Some(CExpr::Null) }
                    "true" => { self.advance(); Some(CExpr::Int(1)) }
                    "false" => { self.advance(); Some(CExpr::Int(0)) }
                    _ => { self.advance(); Some(CExpr::Name(s)) }
                }
            }
            CTok::LParen => {
                self.advance();
                let e = self.parse_expr()?;
                if matches!(self.peek(), CTok::RParen) { self.advance(); }
                Some(e)
            }
            _ => { self.advance(); None }
        }
    }
}

// ============ C → Ernos Emitter ============

fn c_type_to_ernos(t: &str) -> &str {
    if t.contains("char") && t.contains('*') { return "Str"; }
    if t.contains("void") { return "Int"; }
    if t.contains("float") || t.contains("double") { return "Int"; }
    "Int"
}

pub fn emit_ernos_from_c(filename: &str, source: &str) -> String {
    let mut lexer = CLexer::new(source);
    let tokens = lexer.tokenize();
    let mut parser = CParser::new(tokens);
    let stmts = parser.parse();

    let mut out = String::new();
    out.push_str(&format!("# Auto-transpiled from C: {}\n", filename));
    out.push_str("# Generated by: ernos transpile\n\n");

    for stmt in &stmts {
        emit_c_stmt(&mut out, stmt, 0);
    }

    out
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

fn emit_indent(out: &mut String, depth: usize) {
    for _ in 0..depth { out.push_str("    "); }
}

fn emit_c_stmt(out: &mut String, stmt: &CStmt, depth: usize) {
    match stmt {
        CStmt::Preprocessor(dir, content) => {
            match dir.as_str() {
                "include" => {
                    emit_indent(out, depth);
                    let module = content.replace('<', "").replace('>', "")
                        .replace(".h", "").replace('"', "").trim().to_string();
                    out.push_str(&format!("# include {} — use ernos bind to generate bindings\n", module));
                }
                "define" => {
                    let parts: Vec<&str> = content.splitn(2, ' ').collect();
                    if parts.len() == 2 {
                        emit_indent(out, depth);
                        let name = parts[0].trim();
                        let val = parts[1].trim();
                        if val.starts_with('"') {
                            out.push_str(&format!("set {} to {}\n", name, val));
                        } else if let Ok(n) = val.parse::<i64>() {
                            out.push_str(&format!("set {} to {}\n", name, n));
                        } else {
                            out.push_str(&format!("set {} to {} # macro\n", name, val));
                        }
                    } else {
                        emit_indent(out, depth);
                        out.push_str(&format!("# #define {}\n", content));
                    }
                }
                _ => {
                    emit_indent(out, depth);
                    out.push_str(&format!("# #{} {}\n", dir, content));
                }
            }
        }

        CStmt::FuncDef(ret_type, name, params, body) => {
            emit_indent(out, depth);
            if params.is_empty() || (params.len() == 1 && params[0].0.contains("void")) {
                if ret_type.contains("void") {
                    out.push_str(&format!("define {}:\n", sanitize_ernos_ident(name)));
                } else {
                    out.push_str(&format!("define {} returning {}:\n", sanitize_ernos_ident(name), c_type_to_ernos(ret_type)));
                }
            } else {
                let param_parts: Vec<String> = params.iter()
                    .map(|(t, n)| format!("{} as {}", sanitize_ernos_ident(n), c_type_to_ernos(t)))
                    .collect();
                if ret_type.contains("void") {
                    out.push_str(&format!("define {} with {}:\n", sanitize_ernos_ident(name), param_parts.join(" and ")));
                } else {
                    out.push_str(&format!("define {} with {} returning {}:\n",
                        sanitize_ernos_ident(name), param_parts.join(" and "), c_type_to_ernos(ret_type)));
                }
            }
            for s in body {
                emit_c_stmt(out, s, depth + 1);
            }
            if body.is_empty() {
                emit_indent(out, depth + 1);
                out.push_str("# forward declaration\n");
            }
            out.push('\n');
        }

        CStmt::StructDef(name, fields) => {
            emit_indent(out, depth);
            out.push_str(&format!("define structure {}:\n", name));
            for (ftype, fname) in fields {
                emit_indent(out, depth + 1);
                out.push_str(&format!("field {} as {}\n", fname, c_type_to_ernos(ftype)));
            }
            out.push('\n');
        }

        CStmt::VarDecl(_type_name, name, init) => {
            emit_indent(out, depth);
            if let Some(val) = init {
                out.push_str(&format!("set {} to ", name));
                emit_c_expr(out, val);
                out.push('\n');
            } else {
                out.push_str(&format!("set {} to 0\n", name));
            }
        }

        CStmt::Expr(expr) => {
            emit_indent(out, depth);
            // Handle printf/puts as display
            if let CExpr::Call(func, args) = expr {
                if let CExpr::Name(fname) = func.as_ref() {
                    match fname.as_str() {
                        "printf" | "puts" | "fprintf" => {
                            if !args.is_empty() {
                                let start = if fname == "fprintf" { 1 } else { 0 };
                                if start < args.len() {
                                    // Check if the first arg is a format string with specifiers
                                    let has_format_args = args.len() > start + 1;
                                    if has_format_args {
                                        // printf("%d\n", val) → display val
                                        // printf("%s: %d\n", name, val) → display f"{name}: {val}"
                                        if let CExpr::Str(fmt) = &args[start] {
                                            let specifier_count = fmt.matches("%d").count()
                                                + fmt.matches("%s").count()
                                                + fmt.matches("%ld").count()
                                                + fmt.matches("%lld").count()
                                                + fmt.matches("%lu").count()
                                                + fmt.matches("%llu").count()
                                                + fmt.matches("%i").count()
                                                + fmt.matches("%u").count()
                                                + fmt.matches("%x").count()
                                                + fmt.matches("%f").count()
                                                + fmt.matches("%c").count()
                                                + fmt.matches("%p").count();
                                            if specifier_count == 1 && args.len() == start + 2 {
                                                // Single specifier with single value — just display the value
                                                out.push_str("display ");
                                                emit_c_expr(out, &args[start + 1]);
                                                out.push('\n');
                                                return;
                                            } else if specifier_count > 0 {
                                                // Multiple specifiers — build f-string
                                                let mut fstr = fmt.clone();
                                                let mut arg_idx = start + 1;
                                                for spec in ["%lld", "%llu", "%ld", "%lu", "%d", "%i", "%u", "%s", "%x", "%f", "%c", "%p"] {
                                                    while let Some(pos) = fstr.find(spec) {
                                                        if arg_idx < args.len() {
                                                            let mut arg_str = String::new();
                                                            emit_c_expr(&mut arg_str, &args[arg_idx]);
                                                            fstr = format!("{}{{{}}}{}", &fstr[..pos], arg_str, &fstr[pos + spec.len()..]);
                                                            arg_idx += 1;
                                                        } else {
                                                            break;
                                                        }
                                                    }
                                                }
                                                // Remove trailing \n from format string
                                                let fstr = fstr.replace("\\n", "");
                                                out.push_str(&format!("display f\"{}\"", fstr));
                                                out.push('\n');
                                                return;
                                            }
                                        }
                                        // Fallback: emit all value args as separate display calls
                                        for i in (start + 1)..args.len() {
                                            out.push_str("display ");
                                            emit_c_expr(out, &args[i]);
                                            out.push('\n');
                                            if i < args.len() - 1 { emit_indent(out, depth); }
                                        }
                                        return;
                                    } else {
                                        // No format args — just display the string
                                        out.push_str("display ");
                                        emit_c_expr(out, &args[start]);
                                        out.push('\n');
                                        return;
                                    }
                                }
                            }
                        }
                        "scanf" | "fscanf" => {
                            out.push_str("# scanf → use read_line() or read_int()\n");
                            return;
                        }
                        "free" => {
                            out.push_str("# free(");
                            if !args.is_empty() { emit_c_expr(out, &args[0]); }
                            out.push_str(") — memory managed by GC\n");
                            return;
                        }
                        "malloc" | "calloc" | "realloc" => {
                            out.push_str("# malloc → use create_list() or create_map()\n");
                            return;
                        }
                        _ => {}
                    }
                }
            }
            emit_c_expr(out, expr);
            out.push('\n');
        }

        CStmt::Return(val) => {
            emit_indent(out, depth);
            if let Some(v) = val {
                out.push_str("return ");
                emit_c_expr(out, v);
                out.push('\n');
            } else {
                out.push_str("return 0\n");
            }
        }

        CStmt::If(cond, body, else_body) => {
            emit_indent(out, depth);
            out.push_str("if ");
            emit_c_cond(out, cond);
            out.push_str(":\n");
            for s in body { emit_c_stmt(out, s, depth + 1); }
            if let Some(eb) = else_body {
                emit_indent(out, depth);
                out.push_str("else:\n");
                for s in eb { emit_c_stmt(out, s, depth + 1); }
            }
        }

        CStmt::While(cond, body) => {
            emit_indent(out, depth);
            out.push_str("repeat while ");
            emit_c_cond(out, cond);
            out.push_str(":\n");
            for s in body { emit_c_stmt(out, s, depth + 1); }
        }

        CStmt::For(init, cond, step, body) => {
            // Translate C for to while
            if let Some(init_stmt) = init {
                emit_c_stmt(out, init_stmt, depth);
            }
            emit_indent(out, depth);
            out.push_str("repeat while ");
            if let Some(c) = cond {
                emit_c_cond(out, c);
            } else {
                out.push_str("true");
            }
            out.push_str(":\n");
            for s in body { emit_c_stmt(out, s, depth + 1); }
            if let Some(s) = step {
                emit_indent(out, depth + 1);
                emit_c_expr(out, s);
                out.push('\n');
            }
        }

        CStmt::Switch(expr, cases, default) => {
            emit_indent(out, depth);
            out.push_str("check ");
            emit_c_expr(out, expr);
            out.push_str(":\n");
            for (val, body) in cases {
                emit_indent(out, depth + 1);
                out.push_str("when ");
                emit_c_expr(out, val);
                out.push_str(":\n");
                for s in body {
                    if matches!(s, CStmt::Break) { continue; } // skip break in switch
                    emit_c_stmt(out, s, depth + 2);
                }
            }
            if let Some(db) = default {
                emit_indent(out, depth + 1);
                out.push_str("when _:\n");
                for s in db {
                    if matches!(s, CStmt::Break) { continue; }
                    emit_c_stmt(out, s, depth + 2);
                }
            }
        }

        CStmt::Break => { emit_indent(out, depth); out.push_str("break\n"); }
        CStmt::Continue => { emit_indent(out, depth); out.push_str("continue\n"); }
        CStmt::Typedef(_, name) => { emit_indent(out, depth); out.push_str(&format!("# typedef → {}\n", name)); }
        CStmt::Block(stmts) => { for s in stmts { emit_c_stmt(out, s, depth); } }
    }
}

fn emit_c_cond(out: &mut String, expr: &CExpr) {
    match expr {
        CExpr::BinOp(left, op, right) => {
            let ep_op = match op.as_str() {
                "==" => " equals ",
                "!=" => " != ",
                "&&" => " and ",
                "||" => " or ",
                _ => { emit_c_expr(out, left); out.push_str(&format!(" {} ", op)); emit_c_expr(out, right); return; }
            };
            emit_c_cond(out, left);
            out.push_str(ep_op);
            emit_c_cond(out, right);
        }
        CExpr::UnaryOp(op, e) if op == "!" => {
            out.push_str("not ");
            emit_c_cond(out, e);
        }
        _ => emit_c_expr(out, expr),
    }
}

fn emit_c_expr(out: &mut String, expr: &CExpr) {
    match expr {
        CExpr::Int(n) => out.push_str(&n.to_string()),
        CExpr::Float(f) => out.push_str(&(*f as i64).to_string()),
        CExpr::Str(s) => { out.push('"'); out.push_str(&s.replace('"', "\\\"").replace('\n', "\\n")); out.push('"'); }
        CExpr::Char(c) => out.push_str(&(*c as i64).to_string()),
        CExpr::Name(n) => out.push_str(&sanitize_ernos_ident(n)),
        CExpr::Null => out.push('0'),
        CExpr::Sizeof(t) => out.push_str(&format!("8 # sizeof({})", t)),

        CExpr::BinOp(left, op, right) => {
            emit_c_expr(out, left);
            out.push_str(&format!(" {} ", op));
            emit_c_expr(out, right);
        }

        CExpr::UnaryOp(op, e) => {
            match op.as_str() {
                "-" => { out.push_str("0 - "); emit_c_expr(out, e); }
                "!" => { out.push_str("not "); emit_c_expr(out, e); }
                "++" | "post++" => {
                    // i++ → set i to i + 1
                    emit_c_expr(out, e);
                    out.push_str(" + 1 # increment");
                }
                "--" | "post--" => {
                    emit_c_expr(out, e);
                    out.push_str(" - 1 # decrement");
                }
                "&" => { out.push_str("# &"); emit_c_expr(out, e); }
                "*" => { out.push_str("# *"); emit_c_expr(out, e); }
                _ => { out.push_str(op); emit_c_expr(out, e); }
            }
        }

        CExpr::Call(func, args) => {
            // Map C stdlib to Ernos
            if let CExpr::Name(fname) = func.as_ref() {
                match fname.as_str() {
                    "strlen" => { out.push_str("string_length("); if !args.is_empty() { emit_c_expr(out, &args[0]); } out.push(')'); return; }
                    "strcmp" => {
                        if args.len() >= 2 {
                            out.push_str("string_contains("); emit_c_expr(out, &args[0]);
                            out.push_str(" and "); emit_c_expr(out, &args[1]); out.push(')');
                        }
                        return;
                    }
                    "strcat" | "strncat" => {
                        if args.len() >= 2 {
                            out.push_str("concat("); emit_c_expr(out, &args[0]);
                            out.push_str(" and "); emit_c_expr(out, &args[1]); out.push(')');
                        }
                        return;
                    }
                    "strcpy" | "strncpy" => {
                        if args.len() >= 2 { emit_c_expr(out, &args[1]); }
                        return;
                    }
                    "atoi" | "strtol" => {
                        out.push_str("string_to_int("); if !args.is_empty() { emit_c_expr(out, &args[0]); } out.push(')');
                        return;
                    }
                    "abs" => { out.push_str("ep_abs("); if !args.is_empty() { emit_c_expr(out, &args[0]); } out.push(')'); return; }
                    "rand" => { out.push_str("ep_random_int(0 and 32767)"); return; }
                    "srand" => { out.push_str("# srand"); return; }
                    "exit" => { out.push_str("# exit("); if !args.is_empty() { emit_c_expr(out, &args[0]); } out.push(')'); return; }
                    "printf" | "puts" => {
                        out.push_str("display ");
                        if args.len() > 1 {
                            // Has format args — emit the value argument
                            emit_c_expr(out, &args[1]);
                        } else if !args.is_empty() {
                            emit_c_expr(out, &args[0]);
                        }
                        return;
                    }
                    "sprintf" | "snprintf" => {
                        out.push_str("# sprintf → use f-strings");
                        return;
                    }
                    _ => {}
                }
            }

            emit_c_expr(out, func);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 { out.push_str(" and "); }
                emit_c_expr(out, a);
            }
            out.push(')');
        }

        CExpr::Member(obj, field, _is_arrow) => {
            out.push_str("the ");
            out.push_str(field);
            out.push_str(" of ");
            emit_c_expr(out, obj);
        }

        CExpr::Index(obj, idx) => {
            out.push_str("get_list(");
            emit_c_expr(out, obj);
            out.push_str(" and ");
            emit_c_expr(out, idx);
            out.push(')');
        }

        CExpr::Cast(_type_name, e) => {
            emit_c_expr(out, e);
        }

        CExpr::Ternary(cond, then, alt) => {
            out.push_str("# ternary: ");
            emit_c_expr(out, then);
            out.push_str(" if ");
            emit_c_expr(out, cond);
            out.push_str(" else ");
            emit_c_expr(out, alt);
        }

        CExpr::Assign(target, val) => {
            out.push_str("set ");
            emit_c_expr(out, target);
            out.push_str(" to ");
            emit_c_expr(out, val);
        }
    }
}
