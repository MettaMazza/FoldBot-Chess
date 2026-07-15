/// JavaScript → ErnosPlain Transpiler
///
/// Parses a subset of JavaScript/ES6+ source code and emits ErnosPlain (.ep) code.

// ============ JS Tokenizer ============

#[derive(Debug, Clone, PartialEq)]
pub enum JsTok {
    Ident(String),
    IntLit(i64),
    FloatLit(f64),
    StrLit(String),
    TemplateLit(String),
    RegexLit(String),
    // Operators
    Plus, Minus, Star, Slash, Percent, StarStar,
    PlusPlus, MinusMinus,
    PlusEq, MinusEq, StarEq, SlashEq, PercentEq,
    AmpAmp, PipePipe, Bang, Question, QuestionDot,
    Eq, EqEq, EqEqEq, NotEq, NotEqEq,
    Lt, Gt, LtEq, GtEq,
    Arrow, Spread,
    Amp, Pipe, Caret, Tilde, LShift, RShift,
    // Delimiters
    LParen, RParen, LBracket, RBracket, LBrace, RBrace,
    Comma, Semi, Colon, Dot,
    Newline, Eof,
}

pub struct JsLexer {
    chars: Vec<char>,
    pos: usize,
}

impl JsLexer {
    pub fn new(input: &str) -> Self {
        JsLexer { chars: input.chars().collect(), pos: 0 }
    }

    fn peek(&self) -> Option<char> { self.chars.get(self.pos).copied() }
    fn advance(&mut self) -> Option<char> { let c = self.chars.get(self.pos).copied(); self.pos += 1; c }
    fn peek_at(&self, off: usize) -> Option<char> { self.chars.get(self.pos + off).copied() }

    pub fn tokenize(&mut self) -> Vec<JsTok> {
        let mut tokens = Vec::new();
        loop {
            while matches!(self.peek(), Some(' ') | Some('\t') | Some('\r')) { self.advance(); }
            match self.peek() {
                None => break,
                Some('\n') => { self.advance(); tokens.push(JsTok::Newline); }
                Some('/') if self.peek_at(1) == Some('/') => {
                    while self.peek() != Some('\n') && self.peek().is_some() { self.advance(); }
                }
                Some('/') if self.peek_at(1) == Some('*') => {
                    self.advance(); self.advance();
                    loop {
                        match self.advance() {
                            Some('*') if self.peek() == Some('/') => { self.advance(); break; }
                            None => break, _ => {}
                        }
                    }
                }
                Some('"') | Some('\'') => {
                    let quote = self.peek().unwrap();
                    self.advance();
                    let mut s = String::new();
                    loop {
                        match self.advance() {
                            Some(c) if c == quote => break,
                            Some('\\') => match self.advance() {
                                Some('n') => s.push('\n'), Some('t') => s.push('\t'),
                                Some('\\') => s.push('\\'), Some(c) => { s.push('\\'); s.push(c); }
                                None => break,
                            },
                            Some(c) => s.push(c),
                            None => break,
                        }
                    }
                    tokens.push(JsTok::StrLit(s));
                }
                Some('`') => {
                    self.advance();
                    let mut s = String::new();
                    loop {
                        match self.advance() {
                            Some('`') => break,
                            Some('\\') => match self.advance() {
                                Some('n') => s.push('\n'), Some('t') => s.push('\t'),
                                Some(c) => { s.push('\\'); s.push(c); }
                                None => break,
                            },
                            Some('$') if self.peek() == Some('{') => {
                                self.advance(); // {
                                s.push('{');
                                let mut depth = 1;
                                while depth > 0 {
                                    match self.advance() {
                                        Some('{') => { depth += 1; s.push('{'); }
                                        Some('}') => { depth -= 1; if depth > 0 { s.push('}'); } else { s.push('}'); } }
                                        Some(c) => s.push(c),
                                        None => break,
                                    }
                                }
                            }
                            Some(c) => s.push(c),
                            None => break,
                        }
                    }
                    tokens.push(JsTok::TemplateLit(s));
                }
                Some(c) if c.is_ascii_digit() => {
                    let mut num = String::new();
                    while let Some(c) = self.peek() {
                        if c.is_ascii_digit() || c == '_' { if c != '_' { num.push(c); } self.advance(); } else { break; }
                    }
                    if self.peek() == Some('.') && self.peek_at(1).map_or(false, |c| c.is_ascii_digit()) {
                        num.push('.'); self.advance();
                        while let Some(c) = self.peek() {
                            if c.is_ascii_digit() { num.push(c); self.advance(); } else { break; }
                        }
                        tokens.push(JsTok::FloatLit(num.parse().unwrap_or(0.0)));
                    } else {
                        tokens.push(JsTok::IntLit(num.parse().unwrap_or(0)));
                    }
                }
                Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$' => {
                    let mut ident = String::new();
                    while let Some(c) = self.peek() {
                        if c.is_ascii_alphanumeric() || c == '_' || c == '$' { ident.push(c); self.advance(); } else { break; }
                    }
                    tokens.push(JsTok::Ident(ident));
                }
                Some('+') => { self.advance(); match self.peek() { Some('+') => { self.advance(); tokens.push(JsTok::PlusPlus); } Some('=') => { self.advance(); tokens.push(JsTok::PlusEq); } _ => tokens.push(JsTok::Plus) } }
                Some('-') => { self.advance(); match self.peek() { Some('-') => { self.advance(); tokens.push(JsTok::MinusMinus); } Some('=') => { self.advance(); tokens.push(JsTok::MinusEq); } _ => tokens.push(JsTok::Minus) } }
                Some('*') => { self.advance(); if self.peek() == Some('*') { self.advance(); tokens.push(JsTok::StarStar); } else if self.peek() == Some('=') { self.advance(); tokens.push(JsTok::StarEq); } else { tokens.push(JsTok::Star); } }
                Some('/') => { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(JsTok::SlashEq); } else { tokens.push(JsTok::Slash); } }
                Some('%') => { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(JsTok::PercentEq); } else { tokens.push(JsTok::Percent); } }
                Some('=') => { self.advance(); if self.peek() == Some('=') { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(JsTok::EqEqEq); } else { tokens.push(JsTok::EqEq); } } else if self.peek() == Some('>') { self.advance(); tokens.push(JsTok::Arrow); } else { tokens.push(JsTok::Eq); } }
                Some('!') => { self.advance(); if self.peek() == Some('=') { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(JsTok::NotEqEq); } else { tokens.push(JsTok::NotEq); } } else { tokens.push(JsTok::Bang); } }
                Some('<') => { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(JsTok::LtEq); } else if self.peek() == Some('<') { self.advance(); tokens.push(JsTok::LShift); } else { tokens.push(JsTok::Lt); } }
                Some('>') => { self.advance(); if self.peek() == Some('=') { self.advance(); tokens.push(JsTok::GtEq); } else if self.peek() == Some('>') { self.advance(); tokens.push(JsTok::RShift); } else { tokens.push(JsTok::Gt); } }
                Some('&') => { self.advance(); if self.peek() == Some('&') { self.advance(); tokens.push(JsTok::AmpAmp); } else { tokens.push(JsTok::Amp); } }
                Some('|') => { self.advance(); if self.peek() == Some('|') { self.advance(); tokens.push(JsTok::PipePipe); } else { tokens.push(JsTok::Pipe); } }
                Some('^') => { self.advance(); tokens.push(JsTok::Caret); }
                Some('~') => { self.advance(); tokens.push(JsTok::Tilde); }
                Some('?') => { self.advance(); if self.peek() == Some('.') { self.advance(); tokens.push(JsTok::QuestionDot); } else { tokens.push(JsTok::Question); } }
                Some('.') => { self.advance(); if self.peek() == Some('.') && self.peek_at(1) == Some('.') { self.advance(); self.advance(); tokens.push(JsTok::Spread); } else { tokens.push(JsTok::Dot); } }
                Some('(') => { self.advance(); tokens.push(JsTok::LParen); }
                Some(')') => { self.advance(); tokens.push(JsTok::RParen); }
                Some('[') => { self.advance(); tokens.push(JsTok::LBracket); }
                Some(']') => { self.advance(); tokens.push(JsTok::RBracket); }
                Some('{') => { self.advance(); tokens.push(JsTok::LBrace); }
                Some('}') => { self.advance(); tokens.push(JsTok::RBrace); }
                Some(',') => { self.advance(); tokens.push(JsTok::Comma); }
                Some(';') => { self.advance(); tokens.push(JsTok::Semi); }
                Some(':') => { self.advance(); tokens.push(JsTok::Colon); }
                _ => { self.advance(); }
            }
        }
        tokens.push(JsTok::Eof);
        tokens
    }
}

// ============ JS AST ============

#[derive(Debug, Clone)]
pub enum JsExpr {
    Int(i64),
    Float(f64),
    Str(String),
    Template(String),
    Bool(bool),
    Null,
    Undefined,
    Name(String),
    BinOp(Box<JsExpr>, String, Box<JsExpr>),
    UnaryOp(String, Box<JsExpr>),
    Call(Box<JsExpr>, Vec<JsExpr>),
    Member(Box<JsExpr>, String),
    Index(Box<JsExpr>, Box<JsExpr>),
    Array(Vec<JsExpr>),
    Object(Vec<(String, JsExpr)>),
    Arrow(Vec<String>, Box<JsExpr>),
    Ternary(Box<JsExpr>, Box<JsExpr>, Box<JsExpr>),
    Assign(Box<JsExpr>, Box<JsExpr>),
    New(Box<JsExpr>, Vec<JsExpr>),
}

#[derive(Debug, Clone)]
pub enum JsStmt {
    VarDecl(String, String, Option<JsExpr>), // kind (let/const/var), name, init
    Expr(JsExpr),
    Return(Option<JsExpr>),
    If(JsExpr, Vec<JsStmt>, Option<Vec<JsStmt>>),
    While(JsExpr, Vec<JsStmt>),
    For(Option<Box<JsStmt>>, Option<JsExpr>, Option<JsExpr>, Vec<JsStmt>),
    ForOf(String, JsExpr, Vec<JsStmt>),
    ForIn(String, JsExpr, Vec<JsStmt>),
    Switch(JsExpr, Vec<(JsExpr, Vec<JsStmt>)>, Option<Vec<JsStmt>>),
    FuncDecl(String, Vec<String>, Vec<JsStmt>),
    ClassDecl(String, Option<String>, Vec<JsStmt>),
    Break,
    Continue,
    Throw(JsExpr),
    Try(Vec<JsStmt>, Option<String>, Vec<JsStmt>),
    Import(String, Vec<String>),
    Export(Box<JsStmt>),
}

// ============ JS Parser ============

pub struct JsParser {
    tokens: Vec<JsTok>,
    pos: usize,
}

impl JsParser {
    pub fn new(tokens: Vec<JsTok>) -> Self {
        JsParser { tokens, pos: 0 }
    }

    fn peek(&self) -> &JsTok { self.tokens.get(self.pos).unwrap_or(&JsTok::Eof) }
    fn advance(&mut self) -> JsTok { let t = self.tokens.get(self.pos).cloned().unwrap_or(JsTok::Eof); self.pos += 1; t }
    fn skip_nl(&mut self) { while matches!(self.peek(), JsTok::Newline) { self.advance(); } }
    fn skip_semi(&mut self) { while matches!(self.peek(), JsTok::Semi | JsTok::Newline) { self.advance(); } }

    pub fn parse(&mut self) -> Vec<JsStmt> {
        let mut stmts = Vec::new();
        loop {
            self.skip_nl();
            if matches!(self.peek(), JsTok::Eof) { break; }
            if let Some(s) = self.parse_stmt() { stmts.push(s); }
            self.skip_semi();
        }
        stmts
    }

    fn parse_stmt(&mut self) -> Option<JsStmt> {
        self.skip_nl();
        match self.peek().clone() {
            JsTok::Ident(ref s) => match s.as_str() {
                "let" | "const" | "var" => self.parse_var_decl(),
                "function" => self.parse_func_decl(),
                "class" => self.parse_class_decl(),
                "if" => self.parse_if(),
                "while" => self.parse_while(),
                "for" => self.parse_for(),
                "switch" => self.parse_switch(),
                "return" => { self.advance(); if matches!(self.peek(), JsTok::Semi | JsTok::Newline | JsTok::Eof | JsTok::RBrace) { self.skip_semi(); return Some(JsStmt::Return(None)); } let e = self.parse_expr(); self.skip_semi(); Some(JsStmt::Return(e)) }
                "break" => { self.advance(); self.skip_semi(); Some(JsStmt::Break) }
                "continue" => { self.advance(); self.skip_semi(); Some(JsStmt::Continue) }
                "throw" => { self.advance(); let e = self.parse_expr().unwrap_or(JsExpr::Null); self.skip_semi(); Some(JsStmt::Throw(e)) }
                "try" => self.parse_try(),
                "import" => self.parse_import(),
                "export" => { self.advance(); self.skip_nl(); let s = self.parse_stmt(); s.map(|s| JsStmt::Export(Box::new(s))) }
                _ => self.parse_expr_stmt(),
            },
            _ => self.parse_expr_stmt(),
        }
    }

    fn parse_var_decl(&mut self) -> Option<JsStmt> {
        let kind = if let JsTok::Ident(s) = self.advance() { s } else { return None };
        let name = if let JsTok::Ident(n) = self.advance() { n } else { return None };
        let init = if matches!(self.peek(), JsTok::Eq) { self.advance(); self.parse_expr() } else { None };
        self.skip_semi();
        Some(JsStmt::VarDecl(kind, name, init))
    }

    fn parse_func_decl(&mut self) -> Option<JsStmt> {
        self.advance(); // function
        // Check for async
        let name = if let JsTok::Ident(n) = self.peek() { let n = n.clone(); self.advance(); n } else { "anon".into() };
        if !matches!(self.peek(), JsTok::LParen) { return None; }
        self.advance();
        let mut params = Vec::new();
        while !matches!(self.peek(), JsTok::RParen | JsTok::Eof) {
            if matches!(self.peek(), JsTok::Spread) { self.advance(); }
            if let JsTok::Ident(p) = self.advance() {
                // Skip default value
                if matches!(self.peek(), JsTok::Eq) { self.advance(); let _ = self.parse_expr(); }
                params.push(p);
            }
            if matches!(self.peek(), JsTok::Comma) { self.advance(); }
        }
        if matches!(self.peek(), JsTok::RParen) { self.advance(); }
        let body = self.parse_block();
        Some(JsStmt::FuncDecl(name, params, body))
    }

    fn parse_class_decl(&mut self) -> Option<JsStmt> {
        self.advance(); // class
        let name = if let JsTok::Ident(n) = self.peek() { let n = n.clone(); self.advance(); n } else { return None };
        let extends = if let JsTok::Ident(s) = self.peek() {
            if s == "extends" { self.advance(); if let JsTok::Ident(p) = self.advance() { Some(p) } else { None } } else { None }
        } else { None };
        if !matches!(self.peek(), JsTok::LBrace) { return None; }
        self.advance(); // {
        let mut methods = Vec::new();
        loop {
            self.skip_nl();
            if matches!(self.peek(), JsTok::RBrace | JsTok::Eof) { break; }
            // Skip static/async/get/set
            while let JsTok::Ident(s) = self.peek() {
                if matches!(s.as_str(), "static" | "async" | "get" | "set") { self.advance(); } else { break; }
            }
            let method_name = if let JsTok::Ident(n) = self.advance() { n } else { continue };
            if !matches!(self.peek(), JsTok::LParen) { continue; }
            self.advance();
            let mut params = Vec::new();
            while !matches!(self.peek(), JsTok::RParen | JsTok::Eof) {
                if let JsTok::Ident(p) = self.advance() { params.push(p); }
                if matches!(self.peek(), JsTok::Comma) { self.advance(); }
            }
            if matches!(self.peek(), JsTok::RParen) { self.advance(); }
            let body = self.parse_block();
            methods.push(JsStmt::FuncDecl(method_name, params, body));
        }
        if matches!(self.peek(), JsTok::RBrace) { self.advance(); }
        Some(JsStmt::ClassDecl(name, extends, methods))
    }

    fn parse_if(&mut self) -> Option<JsStmt> {
        self.advance(); // if
        if matches!(self.peek(), JsTok::LParen) { self.advance(); }
        let cond = self.parse_expr()?;
        if matches!(self.peek(), JsTok::RParen) { self.advance(); }
        let body = if matches!(self.peek(), JsTok::LBrace) { self.parse_block() } else { self.parse_stmt().into_iter().collect() };
        self.skip_nl();
        let else_body = if let JsTok::Ident(s) = self.peek() {
            if s == "else" {
                self.advance();
                if matches!(self.peek(), JsTok::LBrace) { Some(self.parse_block()) }
                else { Some(self.parse_stmt().into_iter().collect()) }
            } else { None }
        } else { None };
        Some(JsStmt::If(cond, body, else_body))
    }

    fn parse_while(&mut self) -> Option<JsStmt> {
        self.advance();
        if matches!(self.peek(), JsTok::LParen) { self.advance(); }
        let cond = self.parse_expr()?;
        if matches!(self.peek(), JsTok::RParen) { self.advance(); }
        let body = if matches!(self.peek(), JsTok::LBrace) { self.parse_block() } else { self.parse_stmt().into_iter().collect() };
        Some(JsStmt::While(cond, body))
    }

    fn parse_for(&mut self) -> Option<JsStmt> {
        self.advance(); // for
        if matches!(self.peek(), JsTok::LParen) { self.advance(); }

        // Check for for...of / for...in
        if let JsTok::Ident(s) = self.peek() {
            if s == "let" || s == "const" || s == "var" {
                let save = self.pos;
                self.advance(); // let/const/var
                if let JsTok::Ident(var_name) = self.peek() {
                    let var_name = var_name.clone();
                    self.advance();
                    if let JsTok::Ident(s) = self.peek() {
                        if s == "of" {
                            self.advance();
                            let iter = self.parse_expr()?;
                            if matches!(self.peek(), JsTok::RParen) { self.advance(); }
                            let body = if matches!(self.peek(), JsTok::LBrace) { self.parse_block() } else { self.parse_stmt().into_iter().collect() };
                            return Some(JsStmt::ForOf(var_name, iter, body));
                        } else if s == "in" {
                            self.advance();
                            let iter = self.parse_expr()?;
                            if matches!(self.peek(), JsTok::RParen) { self.advance(); }
                            let body = if matches!(self.peek(), JsTok::LBrace) { self.parse_block() } else { self.parse_stmt().into_iter().collect() };
                            return Some(JsStmt::ForIn(var_name, iter, body));
                        }
                    }
                }
                self.pos = save;
            }
        }

        // Regular for loop
        let init = if matches!(self.peek(), JsTok::Semi) { self.advance(); None }
        else {
            let s = self.parse_stmt();
            s.map(Box::new)
        };
        let cond = if matches!(self.peek(), JsTok::Semi) { None } else { self.parse_expr() };
        if matches!(self.peek(), JsTok::Semi) { self.advance(); }
        let step = if matches!(self.peek(), JsTok::RParen) { None } else { self.parse_expr() };
        if matches!(self.peek(), JsTok::RParen) { self.advance(); }
        let body = if matches!(self.peek(), JsTok::LBrace) { self.parse_block() } else { self.parse_stmt().into_iter().collect() };
        Some(JsStmt::For(init, cond, step, body))
    }

    fn parse_switch(&mut self) -> Option<JsStmt> {
        self.advance();
        if matches!(self.peek(), JsTok::LParen) { self.advance(); }
        let expr = self.parse_expr()?;
        if matches!(self.peek(), JsTok::RParen) { self.advance(); }
        if !matches!(self.peek(), JsTok::LBrace) { return None; }
        self.advance();
        let mut cases = Vec::new();
        let mut default = None;
        loop {
            self.skip_nl();
            if matches!(self.peek(), JsTok::RBrace | JsTok::Eof) { break; }
            if let JsTok::Ident(s) = self.peek() {
                if s == "case" {
                    self.advance();
                    let val = self.parse_expr().unwrap_or(JsExpr::Int(0));
                    if matches!(self.peek(), JsTok::Colon) { self.advance(); }
                    let mut body = Vec::new();
                    loop {
                        self.skip_nl();
                        match self.peek() {
                            JsTok::Ident(s) if s == "case" || s == "default" => break,
                            JsTok::RBrace | JsTok::Eof => break,
                            _ => { if let Some(s) = self.parse_stmt() { body.push(s); } }
                        }
                    }
                    cases.push((val, body));
                } else if s == "default" {
                    self.advance();
                    if matches!(self.peek(), JsTok::Colon) { self.advance(); }
                    let mut body = Vec::new();
                    loop {
                        self.skip_nl();
                        match self.peek() {
                            JsTok::RBrace | JsTok::Eof => break,
                            JsTok::Ident(s) if s == "case" => break,
                            _ => { if let Some(s) = self.parse_stmt() { body.push(s); } }
                        }
                    }
                    default = Some(body);
                } else { self.advance(); }
            } else { self.advance(); }
        }
        if matches!(self.peek(), JsTok::RBrace) { self.advance(); }
        Some(JsStmt::Switch(expr, cases, default))
    }

    fn parse_try(&mut self) -> Option<JsStmt> {
        self.advance(); // try
        let body = self.parse_block();
        self.skip_nl();
        let (catch_var, catch_body) = if let JsTok::Ident(s) = self.peek() {
            if s == "catch" {
                self.advance();
                let var = if matches!(self.peek(), JsTok::LParen) {
                    self.advance();
                    let v = if let JsTok::Ident(n) = self.advance() { Some(n) } else { None };
                    if matches!(self.peek(), JsTok::RParen) { self.advance(); }
                    v
                } else { None };
                let cb = self.parse_block();
                (var, cb)
            } else { (None, vec![]) }
        } else { (None, vec![]) };
        // Skip finally
        self.skip_nl();
        if let JsTok::Ident(s) = self.peek() {
            if s == "finally" { self.advance(); let _ = self.parse_block(); }
        }
        Some(JsStmt::Try(body, catch_var, catch_body))
    }

    fn parse_import(&mut self) -> Option<JsStmt> {
        self.advance(); // import
        let mut names = Vec::new();
        let mut module = String::new();
        // import { a, b } from 'module'
        // import name from 'module'
        // import 'module'
        match self.peek().clone() {
            JsTok::LBrace => {
                self.advance();
                while !matches!(self.peek(), JsTok::RBrace | JsTok::Eof) {
                    if let JsTok::Ident(n) = self.advance() { names.push(n); }
                    if matches!(self.peek(), JsTok::Comma) { self.advance(); }
                    // Skip 'as alias'
                    if let JsTok::Ident(s) = self.peek() { if s == "as" { self.advance(); self.advance(); } }
                }
                if matches!(self.peek(), JsTok::RBrace) { self.advance(); }
            }
            JsTok::StrLit(s) => { module = s; self.advance(); self.skip_semi(); return Some(JsStmt::Import(module, vec![])); }
            JsTok::Ident(_) => {
                if let JsTok::Ident(n) = self.advance() { names.push(n); }
            }
            _ => {}
        }
        // from 'module'
        if let JsTok::Ident(s) = self.peek() {
            if s == "from" { self.advance(); }
        }
        match self.peek().clone() {
            JsTok::StrLit(s) => { module = s; self.advance(); }
            _ => {}
        }
        self.skip_semi();
        Some(JsStmt::Import(module, names))
    }

    fn parse_expr_stmt(&mut self) -> Option<JsStmt> {
        let expr = self.parse_expr()?;
        self.skip_semi();
        Some(JsStmt::Expr(expr))
    }

    fn parse_block(&mut self) -> Vec<JsStmt> {
        let mut stmts = Vec::new();
        if !matches!(self.peek(), JsTok::LBrace) { return stmts; }
        self.advance();
        loop {
            self.skip_nl();
            if matches!(self.peek(), JsTok::RBrace | JsTok::Eof) { break; }
            if let Some(s) = self.parse_stmt() { stmts.push(s); }
            self.skip_semi();
        }
        if matches!(self.peek(), JsTok::RBrace) { self.advance(); }
        stmts
    }

    // ============ Expression Parsing ============

    fn parse_expr(&mut self) -> Option<JsExpr> { self.parse_assign() }

    fn parse_assign(&mut self) -> Option<JsExpr> {
        let left = self.parse_ternary()?;
        match self.peek() {
            JsTok::Eq => { self.advance(); let r = self.parse_assign()?; Some(JsExpr::Assign(Box::new(left), Box::new(r))) }
            JsTok::PlusEq => { self.advance(); let r = self.parse_assign()?;
                Some(JsExpr::Assign(Box::new(left.clone()), Box::new(JsExpr::BinOp(Box::new(left), "+".into(), Box::new(r))))) }
            JsTok::MinusEq => { self.advance(); let r = self.parse_assign()?;
                Some(JsExpr::Assign(Box::new(left.clone()), Box::new(JsExpr::BinOp(Box::new(left), "-".into(), Box::new(r))))) }
            _ => Some(left),
        }
    }

    fn parse_ternary(&mut self) -> Option<JsExpr> {
        let cond = self.parse_or()?;
        if matches!(self.peek(), JsTok::Question) {
            self.advance();
            let then = self.parse_expr()?;
            if matches!(self.peek(), JsTok::Colon) { self.advance(); }
            let alt = self.parse_ternary()?;
            Some(JsExpr::Ternary(Box::new(cond), Box::new(then), Box::new(alt)))
        } else { Some(cond) }
    }

    fn parse_or(&mut self) -> Option<JsExpr> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), JsTok::PipePipe) { self.advance(); let r = self.parse_and()?; left = JsExpr::BinOp(Box::new(left), "||".into(), Box::new(r)); }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<JsExpr> {
        let mut left = self.parse_comparison()?;
        while matches!(self.peek(), JsTok::AmpAmp) { self.advance(); let r = self.parse_comparison()?; left = JsExpr::BinOp(Box::new(left), "&&".into(), Box::new(r)); }
        Some(left)
    }

    fn parse_comparison(&mut self) -> Option<JsExpr> {
        let mut left = self.parse_addition()?;
        loop {
            let op = match self.peek() {
                JsTok::EqEq | JsTok::EqEqEq => "==",
                JsTok::NotEq | JsTok::NotEqEq => "!=",
                JsTok::Lt => "<", JsTok::Gt => ">",
                JsTok::LtEq => "<=", JsTok::GtEq => ">=",
                _ => break,
            };
            let op_s = op.to_string(); self.advance();
            let right = self.parse_addition()?;
            left = JsExpr::BinOp(Box::new(left), op_s, Box::new(right));
        }
        Some(left)
    }

    fn parse_addition(&mut self) -> Option<JsExpr> {
        let mut left = self.parse_multiplication()?;
        loop {
            let op = match self.peek() {
                JsTok::Plus => "+", JsTok::Minus => "-", _ => break,
            };
            let op_s = op.to_string(); self.advance();
            let right = self.parse_multiplication()?;
            left = JsExpr::BinOp(Box::new(left), op_s, Box::new(right));
        }
        Some(left)
    }

    fn parse_multiplication(&mut self) -> Option<JsExpr> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                JsTok::Star => "*", JsTok::Slash => "/", JsTok::Percent => "%",
                JsTok::StarStar => "**", _ => break,
            };
            let op_s = op.to_string(); self.advance();
            let right = self.parse_unary()?;
            left = JsExpr::BinOp(Box::new(left), op_s, Box::new(right));
        }
        Some(left)
    }

    fn parse_unary(&mut self) -> Option<JsExpr> {
        match self.peek() {
            JsTok::Minus => { self.advance(); let e = self.parse_postfix()?; Some(JsExpr::UnaryOp("-".into(), Box::new(e))) }
            JsTok::Bang => { self.advance(); let e = self.parse_postfix()?; Some(JsExpr::UnaryOp("!".into(), Box::new(e))) }
            JsTok::PlusPlus => { self.advance(); let e = self.parse_postfix()?; Some(JsExpr::UnaryOp("++".into(), Box::new(e))) }
            JsTok::MinusMinus => { self.advance(); let e = self.parse_postfix()?; Some(JsExpr::UnaryOp("--".into(), Box::new(e))) }
            JsTok::Ident(s) if s == "typeof" => { self.advance(); let _e = self.parse_unary(); Some(JsExpr::Str("object".into())) }
            JsTok::Ident(s) if s == "new" => {
                self.advance();
                let callee = self.parse_postfix()?;
                // new might already have been parsed with call
                Some(callee)
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Option<JsExpr> {
        let mut expr = self.parse_atom()?;
        loop {
            match self.peek() {
                JsTok::LParen => {
                    self.advance();
                    let mut args = Vec::new();
                    while !matches!(self.peek(), JsTok::RParen | JsTok::Eof) {
                        if let Some(a) = self.parse_expr() { args.push(a); }
                        if matches!(self.peek(), JsTok::Comma) { self.advance(); }
                    }
                    if matches!(self.peek(), JsTok::RParen) { self.advance(); }
                    expr = JsExpr::Call(Box::new(expr), args);
                }
                JsTok::LBracket => {
                    self.advance();
                    let idx = self.parse_expr()?;
                    if matches!(self.peek(), JsTok::RBracket) { self.advance(); }
                    expr = JsExpr::Index(Box::new(expr), Box::new(idx));
                }
                JsTok::Dot => {
                    self.advance();
                    if let JsTok::Ident(f) = self.advance() { expr = JsExpr::Member(Box::new(expr), f); }
                }
                JsTok::PlusPlus => { self.advance(); expr = JsExpr::UnaryOp("post++".into(), Box::new(expr)); }
                JsTok::MinusMinus => { self.advance(); expr = JsExpr::UnaryOp("post--".into(), Box::new(expr)); }
                _ => break,
            }
        }
        Some(expr)
    }

    fn parse_atom(&mut self) -> Option<JsExpr> {
        match self.peek().clone() {
            JsTok::IntLit(n) => { self.advance(); Some(JsExpr::Int(n)) }
            JsTok::FloatLit(f) => { self.advance(); Some(JsExpr::Float(f)) }
            JsTok::StrLit(s) => { self.advance(); Some(JsExpr::Str(s)) }
            JsTok::TemplateLit(s) => { self.advance(); Some(JsExpr::Template(s)) }
            JsTok::Ident(s) => match s.as_str() {
                "true" => { self.advance(); Some(JsExpr::Bool(true)) }
                "false" => { self.advance(); Some(JsExpr::Bool(false)) }
                "null" => { self.advance(); Some(JsExpr::Null) }
                "undefined" => { self.advance(); Some(JsExpr::Undefined) }
                _ => { self.advance(); Some(JsExpr::Name(s)) }
            },
            JsTok::LParen => {
                self.advance();
                // Check for arrow: (params) => body
                let save = self.pos;
                let mut params = Vec::new();
                let mut is_arrow = true;
                loop {
                    match self.peek() {
                        JsTok::RParen => break,
                        JsTok::Ident(p) => { params.push(p.clone()); self.advance(); }
                        JsTok::Comma => { self.advance(); }
                        _ => { is_arrow = false; break; }
                    }
                }
                if is_arrow && matches!(self.peek(), JsTok::RParen) {
                    self.advance();
                    if matches!(self.peek(), JsTok::Arrow) {
                        self.advance();
                        if matches!(self.peek(), JsTok::LBrace) {
                            let body_stmts = self.parse_block();
                            // Find return value
                            let ret_expr = body_stmts.iter().find_map(|s| {
                                if let JsStmt::Return(Some(e)) = s { Some(e.clone()) } else { None }
                            }).unwrap_or(JsExpr::Int(0));
                            return Some(JsExpr::Arrow(params, Box::new(ret_expr)));
                        } else {
                            let body = self.parse_expr()?;
                            return Some(JsExpr::Arrow(params, Box::new(body)));
                        }
                    }
                }
                self.pos = save;
                let e = self.parse_expr()?;
                if matches!(self.peek(), JsTok::RParen) { self.advance(); }
                Some(e)
            }
            JsTok::LBracket => {
                self.advance();
                let mut elems = Vec::new();
                while !matches!(self.peek(), JsTok::RBracket | JsTok::Eof) {
                    if let Some(e) = self.parse_expr() { elems.push(e); }
                    if matches!(self.peek(), JsTok::Comma) { self.advance(); }
                }
                if matches!(self.peek(), JsTok::RBracket) { self.advance(); }
                Some(JsExpr::Array(elems))
            }
            JsTok::LBrace => {
                self.advance();
                let mut pairs = Vec::new();
                while !matches!(self.peek(), JsTok::RBrace | JsTok::Eof) {
                    let key = match self.advance() {
                        JsTok::Ident(s) => s,
                        JsTok::StrLit(s) => s,
                        _ => continue,
                    };
                    if matches!(self.peek(), JsTok::Colon) {
                        self.advance();
                        let val = self.parse_expr().unwrap_or(JsExpr::Null);
                        pairs.push((key, val));
                    } else {
                        // Shorthand: { foo } → { foo: foo }
                        pairs.push((key.clone(), JsExpr::Name(key)));
                    }
                    if matches!(self.peek(), JsTok::Comma) { self.advance(); }
                }
                if matches!(self.peek(), JsTok::RBrace) { self.advance(); }
                Some(JsExpr::Object(pairs))
            }
            _ => { self.advance(); None }
        }
    }
}

// ============ JS → Ernos Emitter ============

pub fn emit_ernos_from_js(filename: &str, source: &str) -> String {
    let mut lexer = JsLexer::new(source);
    let tokens = lexer.tokenize();
    let mut parser = JsParser::new(tokens);
    let stmts = parser.parse();

    let mut out = String::new();
    out.push_str(&format!("# Auto-transpiled from JavaScript: {}\n", filename));
    out.push_str("# Generated by: ernos transpile\n\n");

    for stmt in &stmts {
        // Only emit function/class definitions at top level
        match stmt {
            JsStmt::FuncDecl(..) | JsStmt::ClassDecl(..) => {
                emit_js_stmt(&mut out, stmt, 0);
            }
            _ => {
                // Skip bare top-level statements
            }
        }
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

fn emit_js_stmt(out: &mut String, stmt: &JsStmt, depth: usize) {
    match stmt {
        JsStmt::FuncDecl(name, params, body) => {
            emit_indent(out, depth);
            if params.is_empty() {
                out.push_str(&format!("define {}:\n", sanitize_ernos_ident(name)));
            } else {
                let p: Vec<String> = params.iter().map(|p| format!("{} as Int", sanitize_ernos_ident(p))).collect();
                out.push_str(&format!("define {} with {}:\n", sanitize_ernos_ident(name), p.join(" and ")));
            }
            for s in body { emit_js_stmt(out, s, depth + 1); }
            if body.is_empty() { emit_indent(out, depth + 1); out.push_str("return 0\n"); }
            out.push('\n');
        }

        JsStmt::ClassDecl(name, _extends, methods) => {
            // Extract constructor fields
            let mut fields = Vec::new();
            let mut other_methods = Vec::new();
            for m in methods {
                if let JsStmt::FuncDecl(mname, _params, body) = m {
                    if mname == "constructor" {
                        for s in body {
                            if let JsStmt::Expr(JsExpr::Assign(target, _val)) = s {
                                if let JsExpr::Member(obj, field) = target.as_ref() {
                                    if let JsExpr::Name(n) = obj.as_ref() {
                                        if n == "this" { fields.push(field.clone()); }
                                    }
                                }
                            }
                        }
                    } else {
                        other_methods.push(m);
                    }
                }
            }
            emit_indent(out, depth);
            out.push_str(&format!("define structure {}:\n", name));
            for f in &fields {
                emit_indent(out, depth + 1);
                out.push_str(&format!("field {} as Int\n", f));
            }
            out.push('\n');
            for m in &other_methods {
                if let JsStmt::FuncDecl(mname, params, body) = m {
                    emit_indent(out, depth);
                    if params.is_empty() {
                        out.push_str(&format!("define method {} on {}:\n", mname, name));
                    } else {
                        let p: Vec<String> = params.iter().map(|p| format!("{} as Int", p)).collect();
                        out.push_str(&format!("define method {} on {} with {}:\n", mname, name, p.join(" and ")));
                    }
                    for s in body { emit_js_stmt(out, s, depth + 1); }
                    out.push('\n');
                }
            }
        }

        JsStmt::VarDecl(_kind, name, init) => {
            emit_indent(out, depth);
            if let Some(val) = init {
                // Arrow function → define
                if let JsExpr::Arrow(params, body) = val {
                    if params.is_empty() {
                        out.push_str(&format!("define {}:\n", name));
                    } else {
                        let p: Vec<String> = params.iter().map(|p| format!("{} as Int", p)).collect();
                        out.push_str(&format!("define {} with {}:\n", name, p.join(" and ")));
                    }
                    emit_indent(out, depth + 1);
                    out.push_str("return ");
                    emit_js_expr(out, body);
                    out.push('\n');
                    out.push('\n');
                    return;
                }
                out.push_str(&format!("set {} to ", name));
                emit_js_expr(out, val);
                out.push('\n');
            } else {
                out.push_str(&format!("set {} to 0\n", name));
            }
        }

        JsStmt::Expr(expr) => {
            // console.log → display
            if let JsExpr::Call(func, args) = expr {
                if let JsExpr::Member(obj, method) = func.as_ref() {
                    if let JsExpr::Name(n) = obj.as_ref() {
                        if n == "console" && method == "log" {
                            for a in args {
                                emit_indent(out, depth);
                                out.push_str("display ");
                                emit_js_expr(out, a);
                                out.push('\n');
                            }
                            return;
                        }
                    }
                }
            }
            emit_indent(out, depth);
            emit_js_expr(out, expr);
            out.push('\n');
        }

        JsStmt::Return(val) => {
            emit_indent(out, depth);
            if let Some(v) = val {
                out.push_str("return ");
                emit_js_expr(out, v);
                out.push('\n');
            } else {
                out.push_str("return 0\n");
            }
        }

        JsStmt::If(cond, body, else_body) => {
            emit_indent(out, depth);
            out.push_str("if ");
            emit_js_cond(out, cond);
            out.push_str(":\n");
            for s in body { emit_js_stmt(out, s, depth + 1); }
            if let Some(eb) = else_body {
                emit_indent(out, depth);
                out.push_str("else:\n");
                for s in eb { emit_js_stmt(out, s, depth + 1); }
            }
        }

        JsStmt::While(cond, body) => {
            emit_indent(out, depth);
            out.push_str("repeat while ");
            emit_js_cond(out, cond);
            out.push_str(":\n");
            for s in body { emit_js_stmt(out, s, depth + 1); }
        }

        JsStmt::For(init, cond, step, body) => {
            if let Some(i) = init { emit_js_stmt(out, i, depth); }
            emit_indent(out, depth);
            out.push_str("repeat while ");
            if let Some(c) = cond { emit_js_cond(out, c); } else { out.push_str("true"); }
            out.push_str(":\n");
            for s in body { emit_js_stmt(out, s, depth + 1); }
            if let Some(s) = step { emit_indent(out, depth + 1); emit_js_expr(out, s); out.push('\n'); }
        }

        JsStmt::ForOf(var, iter, body) => {
            emit_indent(out, depth);
            out.push_str(&format!("for each {} in ", var));
            emit_js_expr(out, iter);
            out.push_str(":\n");
            for s in body { emit_js_stmt(out, s, depth + 1); }
        }

        JsStmt::ForIn(var, iter, body) => {
            emit_indent(out, depth);
            out.push_str(&format!("for each {} in map_keys(", var));
            emit_js_expr(out, iter);
            out.push_str("):\n");
            for s in body { emit_js_stmt(out, s, depth + 1); }
        }

        JsStmt::Switch(expr, cases, default) => {
            emit_indent(out, depth);
            out.push_str("check ");
            emit_js_expr(out, expr);
            out.push_str(":\n");
            for (val, body) in cases {
                emit_indent(out, depth + 1);
                out.push_str("when ");
                emit_js_expr(out, val);
                out.push_str(":\n");
                for s in body {
                    if matches!(s, JsStmt::Break) { continue; }
                    emit_js_stmt(out, s, depth + 2);
                }
            }
            if let Some(db) = default {
                emit_indent(out, depth + 1);
                out.push_str("when _:\n");
                for s in db {
                    if matches!(s, JsStmt::Break) { continue; }
                    emit_js_stmt(out, s, depth + 2);
                }
            }
        }

        JsStmt::Break => { emit_indent(out, depth); out.push_str("break\n"); }
        JsStmt::Continue => { emit_indent(out, depth); out.push_str("continue\n"); }
        JsStmt::Throw(e) => { emit_indent(out, depth); out.push_str("# throw "); emit_js_expr(out, e); out.push('\n'); }
        JsStmt::Try(body, _catch_var, catch_body) => {
            emit_indent(out, depth);
            out.push_str("# try/catch — approximate translation\n");
            for s in body { emit_js_stmt(out, s, depth); }
            if !catch_body.is_empty() {
                emit_indent(out, depth);
                out.push_str("# catch:\n");
                for s in catch_body { emit_js_stmt(out, s, depth + 1); }
            }
        }
        JsStmt::Import(module, names) => {
            emit_indent(out, depth);
            if names.is_empty() {
                out.push_str(&format!("# import '{}'\n", module));
            } else {
                out.push_str(&format!("# import {{ {} }} from '{}'\n", names.join(", "), module));
            }
        }
        JsStmt::Export(inner) => {
            emit_js_stmt(out, inner, depth);
        }
    }
}

fn emit_js_cond(out: &mut String, expr: &JsExpr) {
    match expr {
        JsExpr::BinOp(left, op, right) => {
            let ep_op = match op.as_str() {
                "==" => " equals ",
                "!=" => " != ",
                "&&" => " and ",
                "||" => " or ",
                _ => { emit_js_expr(out, left); out.push_str(&format!(" {} ", op)); emit_js_expr(out, right); return; }
            };
            emit_js_cond(out, left);
            out.push_str(ep_op);
            emit_js_cond(out, right);
        }
        JsExpr::UnaryOp(op, e) if op == "!" => { out.push_str("not "); emit_js_cond(out, e); }
        _ => emit_js_expr(out, expr),
    }
}

fn emit_js_expr(out: &mut String, expr: &JsExpr) {
    match expr {
        JsExpr::Int(n) => out.push_str(&n.to_string()),
        JsExpr::Float(f) => out.push_str(&(*f as i64).to_string()),
        JsExpr::Str(s) => { out.push('"'); out.push_str(&s.replace('"', "\\\"").replace('\n', "\\n")); out.push('"'); }
        JsExpr::Template(s) => {
            // Convert ${expr} to {expr}
            let converted = s.replace("${", "{");
            out.push_str(&format!("f\"{}\"", converted.replace('"', "\\\"")));
        }
        JsExpr::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        JsExpr::Null | JsExpr::Undefined => out.push('0'),
        JsExpr::Name(n) => out.push_str(&sanitize_ernos_ident(n)),

        JsExpr::BinOp(left, op, right) => {
            let ep_op = match op.as_str() {
                "**" => { out.push_str("# power: "); emit_js_expr(out, left); out.push_str(" ** "); emit_js_expr(out, right); return; }
                _ => op.as_str(),
            };
            emit_js_expr(out, left);
            out.push_str(&format!(" {} ", ep_op));
            emit_js_expr(out, right);
        }

        JsExpr::UnaryOp(op, e) => {
            match op.as_str() {
                "-" => { out.push_str("0 - "); emit_js_expr(out, e); }
                "!" => { out.push_str("not "); emit_js_expr(out, e); }
                "++" | "post++" => { emit_js_expr(out, e); out.push_str(" + 1"); }
                "--" | "post--" => { emit_js_expr(out, e); out.push_str(" - 1"); }
                _ => { out.push_str(op); emit_js_expr(out, e); }
            }
        }

        JsExpr::Call(func, args) => {
            // Map JS builtins
            if let JsExpr::Member(obj, method) = func.as_ref() {
                match method.as_str() {
                    "push" => { out.push_str("append_list("); emit_js_expr(out, obj); if !args.is_empty() { out.push_str(" and "); emit_js_expr(out, &args[0]); } out.push(')'); return; }
                    "pop" => { out.push_str("pop_list("); emit_js_expr(out, obj); out.push(')'); return; }
                    "length" => { out.push_str("length_list("); emit_js_expr(out, obj); out.push(')'); return; }
                    "toUpperCase" => { out.push_str("string_upper("); emit_js_expr(out, obj); out.push(')'); return; }
                    "toLowerCase" => { out.push_str("string_lower("); emit_js_expr(out, obj); out.push(')'); return; }
                    "trim" => { out.push_str("string_trim("); emit_js_expr(out, obj); out.push(')'); return; }
                    "split" => { out.push_str("string_split("); emit_js_expr(out, obj); if !args.is_empty() { out.push_str(" and "); emit_js_expr(out, &args[0]); } else { out.push_str(" and \" \""); } out.push(')'); return; }
                    "replace" => { if args.len() >= 2 { out.push_str("string_replace("); emit_js_expr(out, obj); out.push_str(" and "); emit_js_expr(out, &args[0]); out.push_str(" and "); emit_js_expr(out, &args[1]); out.push(')'); } return; }
                    "indexOf" => { out.push_str("string_index_of("); emit_js_expr(out, obj); if !args.is_empty() { out.push_str(" and "); emit_js_expr(out, &args[0]); } out.push(')'); return; }
                    "includes" => { out.push_str("string_contains("); emit_js_expr(out, obj); if !args.is_empty() { out.push_str(" and "); emit_js_expr(out, &args[0]); } out.push(')'); return; }
                    "keys" => { out.push_str("map_keys("); emit_js_expr(out, obj); out.push(')'); return; }
                    "values" => { out.push_str("map_values("); emit_js_expr(out, obj); out.push(')'); return; }
                    "forEach" => { out.push_str("# .forEach — use for each loop\n"); return; }
                    "map" => { out.push_str("# .map — use for each loop\n"); return; }
                    "filter" => { out.push_str("# .filter — use for each + if\n"); return; }
                    "log" => {
                        // console.log handled at statement level
                        out.push_str("display ");
                        if !args.is_empty() { emit_js_expr(out, &args[0]); }
                        return;
                    }
                    _ => {}
                }
                // Math.* methods
                if let JsExpr::Name(n) = obj.as_ref() {
                    if n == "Math" {
                        match method.as_str() {
                            "abs" => { out.push_str("ep_abs("); if !args.is_empty() { emit_js_expr(out, &args[0]); } out.push(')'); return; }
                            "random" => { out.push_str("ep_random_int(0 and 100)"); return; }
                            "floor" | "round" | "ceil" => { if !args.is_empty() { emit_js_expr(out, &args[0]); } return; }
                            "max" => { out.push_str("# Math.max — "); return; }
                            "min" => { out.push_str("# Math.min — "); return; }
                            _ => {}
                        }
                    }
                    if n == "Object" && method == "keys" {
                        out.push_str("map_keys(");
                        if !args.is_empty() { emit_js_expr(out, &args[0]); }
                        out.push(')');
                        return;
                    }
                    if n == "JSON" {
                        match method.as_str() {
                            "parse" => { out.push_str("# JSON.parse — "); return; }
                            "stringify" => { out.push_str("# JSON.stringify — "); return; }
                            _ => {}
                        }
                    }
                }
            }

            // Global functions
            if let JsExpr::Name(fname) = func.as_ref() {
                match fname.as_str() {
                    "parseInt" | "Number" => {
                        out.push_str("string_to_int(");
                        if !args.is_empty() { emit_js_expr(out, &args[0]); }
                        out.push(')');
                        return;
                    }
                    "String" => {
                        out.push_str("int_to_string(");
                        if !args.is_empty() { emit_js_expr(out, &args[0]); }
                        out.push(')');
                        return;
                    }
                    "prompt" | "readline" => {
                        out.push_str("read_line()");
                        return;
                    }
                    "alert" | "console" => {
                        out.push_str("display ");
                        if !args.is_empty() { emit_js_expr(out, &args[0]); }
                        return;
                    }
                    "setTimeout" | "setInterval" => {
                        out.push_str(&format!("# {} — ", fname));
                        return;
                    }
                    _ => {}
                }
            }

            emit_js_expr(out, func);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 { out.push_str(" and "); }
                emit_js_expr(out, a);
            }
            out.push(')');
        }

        JsExpr::Member(obj, field) => {
            // .length on arrays
            if field == "length" {
                out.push_str("length_list(");
                emit_js_expr(out, obj);
                out.push(')');
                return;
            }
            out.push_str("the ");
            out.push_str(field);
            out.push_str(" of ");
            emit_js_expr(out, obj);
        }

        JsExpr::Index(obj, idx) => {
            out.push_str("get_list(");
            emit_js_expr(out, obj);
            out.push_str(" and ");
            emit_js_expr(out, idx);
            out.push(')');
        }

        JsExpr::Array(elems) => {
            if elems.is_empty() {
                out.push_str("create_list()");
            } else {
                out.push('[');
                for (i, e) in elems.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    emit_js_expr(out, e);
                }
                out.push(']');
            }
        }

        JsExpr::Object(pairs) => {
            if pairs.is_empty() {
                out.push_str("create_map()");
            } else {
                out.push_str("create_map() # { ");
                for (i, (k, _v)) in pairs.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    out.push_str(k);
                }
                out.push_str(" }");
            }
        }

        JsExpr::Arrow(params, body) => {
            out.push_str("given ");
            out.push_str(&params.join(" and "));
            out.push_str(": return ");
            emit_js_expr(out, body);
        }

        JsExpr::Ternary(cond, then, alt) => {
            out.push_str("# ternary: ");
            emit_js_expr(out, then);
            out.push_str(" if ");
            emit_js_expr(out, cond);
            out.push_str(" else ");
            emit_js_expr(out, alt);
        }

        JsExpr::Assign(target, val) => {
            out.push_str("set ");
            emit_js_expr(out, target);
            out.push_str(" to ");
            emit_js_expr(out, val);
        }

        JsExpr::New(callee, args) => {
            emit_js_expr(out, callee);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 { out.push_str(" and "); }
                emit_js_expr(out, a);
            }
            out.push(')');
        }
    }
}
