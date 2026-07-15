/// Ruby → ErnosPlain Transpiler
///
/// Parses a subset of Ruby source code and emits ErnosPlain (.ep) code.

// ============ Ruby Tokenizer ============

#[derive(Debug, Clone, PartialEq)]
pub enum RbTok {
    Ident(String), IntLit(i64), FloatLit(f64), StrLit(String), Symbol(String),
    Plus, Minus, Star, Slash, Percent, StarStar,
    PlusEq, MinusEq, StarEq, SlashEq,
    AmpAmp, PipePipe, Bang,
    Eq, EqEq, NotEq, Spaceship,
    Lt, Gt, LtEq, GtEq,
    DotDot, DotDotDot,
    Arrow, Pipe, Amp,
    LParen, RParen, LBracket, RBracket, LBrace, RBrace,
    Comma, Semi, Colon, ColonColon, Dot, At, AtAt, Hash,
    Newline, Eof,
}

pub struct RbLexer { chars: Vec<char>, pos: usize }

impl RbLexer {
    pub fn new(input: &str) -> Self { RbLexer { chars: input.chars().collect(), pos: 0 } }
    fn peek(&self) -> Option<char> { self.chars.get(self.pos).copied() }
    fn advance(&mut self) -> Option<char> { let c = self.chars.get(self.pos).copied(); self.pos += 1; c }
    fn peek_at(&self, off: usize) -> Option<char> { self.chars.get(self.pos + off).copied() }

    pub fn tokenize(&mut self) -> Vec<RbTok> {
        let mut tokens = Vec::new();
        loop {
            while matches!(self.peek(), Some(' ') | Some('\t') | Some('\r')) { self.advance(); }
            match self.peek() {
                None => break,
                Some('\n') => { self.advance(); tokens.push(RbTok::Newline); }
                Some('#') => { while self.peek() != Some('\n') && self.peek().is_some() { self.advance(); } }
                Some('"') => { tokens.push(self.lex_string('"')); }
                Some('\'') => { tokens.push(self.lex_string('\'')); }
                Some(':') if self.peek_at(1).map_or(false, |c| c.is_ascii_alphabetic() || c == '_') => {
                    self.advance();
                    let mut s = String::new();
                    while let Some(c) = self.peek() {
                        if c.is_ascii_alphanumeric() || c == '_' { s.push(c); self.advance(); } else { break; }
                    }
                    tokens.push(RbTok::Symbol(s));
                }
                Some(c) if c.is_ascii_digit() => {
                    let mut n = String::new();
                    let mut is_float = false;
                    while let Some(ch) = self.peek() {
                        if ch.is_ascii_digit() || ch == '_' { n.push(ch); self.advance(); }
                        else if ch == '.' && !is_float && self.peek_at(1).map_or(false, |c| c.is_ascii_digit()) {
                            is_float = true; n.push(ch); self.advance();
                        } else { break; }
                    }
                    let clean: String = n.replace('_', "");
                    if is_float { tokens.push(RbTok::FloatLit(clean.parse().unwrap_or(0.0))); }
                    else { tokens.push(RbTok::IntLit(clean.parse().unwrap_or(0))); }
                }
                Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                    let mut ident = String::new();
                    while let Some(ch) = self.peek() {
                        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '?' || ch == '!' { ident.push(ch); self.advance(); }
                        else { break; }
                    }
                    tokens.push(RbTok::Ident(ident));
                }
                Some('@') => {
                    self.advance();
                    if self.peek() == Some('@') { self.advance(); tokens.push(RbTok::AtAt); }
                    else { tokens.push(RbTok::At); }
                    // Read attribute name
                    let mut name = String::new();
                    while let Some(c) = self.peek() {
                        if c.is_ascii_alphanumeric() || c == '_' { name.push(c); self.advance(); } else { break; }
                    }
                    if !name.is_empty() { tokens.push(RbTok::Ident(format!("@{}", name))); }
                }
                _ => { tokens.push(self.lex_punct()); }
            }
        }
        tokens.push(RbTok::Eof);
        tokens
    }

    fn lex_string(&mut self, quote: char) -> RbTok {
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
                Some('#') if quote == '"' && self.peek() == Some('{') => {
                    self.advance();
                    let mut depth = 1;
                    s.push_str("#{");
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
        RbTok::StrLit(s)
    }

    fn lex_punct(&mut self) -> RbTok {
        let c = self.advance().unwrap();
        match c {
            '+' => if self.peek() == Some('=') { self.advance(); RbTok::PlusEq } else { RbTok::Plus },
            '-' => if self.peek() == Some('=') { self.advance(); RbTok::MinusEq }
                   else if self.peek() == Some('>') { self.advance(); RbTok::Arrow }
                   else { RbTok::Minus },
            '*' => if self.peek() == Some('*') { self.advance(); RbTok::StarStar }
                   else if self.peek() == Some('=') { self.advance(); RbTok::StarEq }
                   else { RbTok::Star },
            '/' => if self.peek() == Some('=') { self.advance(); RbTok::SlashEq } else { RbTok::Slash },
            '%' => RbTok::Percent,
            '&' => if self.peek() == Some('&') { self.advance(); RbTok::AmpAmp } else { RbTok::Amp },
            '|' => if self.peek() == Some('|') { self.advance(); RbTok::PipePipe } else { RbTok::Pipe },
            '!' => if self.peek() == Some('=') { self.advance(); RbTok::NotEq } else { RbTok::Bang },
            '=' => if self.peek() == Some('=') { self.advance(); RbTok::EqEq } else { RbTok::Eq },
            '<' => if self.peek() == Some('=') {
                       self.advance();
                       if self.peek() == Some('>') { self.advance(); RbTok::Spaceship } else { RbTok::LtEq }
                   } else { RbTok::Lt },
            '>' => if self.peek() == Some('=') { self.advance(); RbTok::GtEq } else { RbTok::Gt },
            '(' => RbTok::LParen, ')' => RbTok::RParen,
            '[' => RbTok::LBracket, ']' => RbTok::RBracket,
            '{' => RbTok::LBrace, '}' => RbTok::RBrace,
            ',' => RbTok::Comma, ';' => RbTok::Semi,
            ':' => if self.peek() == Some(':') { self.advance(); RbTok::ColonColon } else { RbTok::Colon },
            '.' => if self.peek() == Some('.') {
                       self.advance();
                       if self.peek() == Some('.') { self.advance(); RbTok::DotDotDot } else { RbTok::DotDot }
                   } else { RbTok::Dot },
            _ => RbTok::Eof,
        }
    }
}

// ============ Ruby AST ============

#[derive(Debug, Clone)]
pub enum RbExpr {
    Int(i64), Float(f64), Str(String), Sym(String), Bool(bool), Nil,
    Name(String), InstanceVar(String),
    Binary(Box<RbExpr>, String, Box<RbExpr>),
    Unary(String, Box<RbExpr>),
    Call(Box<RbExpr>, Vec<RbExpr>),
    MethodCall(Box<RbExpr>, String, Vec<RbExpr>),
    Index(Box<RbExpr>, Box<RbExpr>),
    Array(Vec<RbExpr>),
    Hash(Vec<(RbExpr, RbExpr)>),
    Range(Box<RbExpr>, Box<RbExpr>, bool),  // start, end, exclusive
    Block(Vec<String>, Vec<RbStmt>),  // |params| body (do...end or {})
    Self_,
    Yield(Vec<RbExpr>),
    Interpolated(String),
}

#[derive(Debug, Clone)]
pub enum RbStmt {
    Def(String, Vec<String>, Vec<RbStmt>),
    Class(String, Option<String>, Vec<RbStmt>),
    Module(String, Vec<RbStmt>),
    Assign(String, RbExpr),
    InstanceAssign(String, RbExpr),
    AugAssign(String, String, RbExpr),
    If(RbExpr, Vec<RbStmt>, Vec<(RbExpr, Vec<RbStmt>)>, Option<Vec<RbStmt>>),
    Unless(RbExpr, Vec<RbStmt>, Option<Vec<RbStmt>>),
    While(RbExpr, Vec<RbStmt>),
    Until(RbExpr, Vec<RbStmt>),
    For(String, RbExpr, Vec<RbStmt>),
    Return(Option<RbExpr>),
    Expr(RbExpr),
    Break, Next,
    Case(Option<RbExpr>, Vec<(Vec<RbExpr>, Vec<RbStmt>)>, Option<Vec<RbStmt>>),
    AttrAccessor(Vec<String>),
    Require(String),
    Begin(Vec<RbStmt>, Vec<(Option<String>, Vec<RbStmt>)>, Option<Vec<RbStmt>>),
}

// ============ Ruby Parser ============

pub struct RbParser { tokens: Vec<RbTok>, pos: usize }

impl RbParser {
    pub fn new(tokens: Vec<RbTok>) -> Self { RbParser { tokens, pos: 0 } }
    fn peek(&self) -> &RbTok { self.tokens.get(self.pos).unwrap_or(&RbTok::Eof) }
    fn advance(&mut self) -> RbTok { let t = self.tokens.get(self.pos).cloned().unwrap_or(RbTok::Eof); self.pos += 1; t }
    fn expect(&mut self, tok: RbTok) { if self.peek() == &tok { self.advance(); } }
    fn skip_newlines(&mut self) { while matches!(self.peek(), RbTok::Newline | RbTok::Semi) { self.advance(); } }
    fn at_eof(&self) -> bool { self.peek() == &RbTok::Eof }
    fn ident_str(&self) -> Option<String> { if let RbTok::Ident(s) = self.peek() { Some(s.clone()) } else { None } }

    pub fn parse_file(&mut self) -> Vec<RbStmt> {
        let mut stmts = Vec::new();
        self.skip_newlines();
        while !self.at_eof() {
            if let Some(s) = self.parse_stmt() { stmts.push(s); }
            self.skip_newlines();
        }
        stmts
    }

    fn parse_body_until_end(&mut self) -> Vec<RbStmt> {
        let mut stmts = Vec::new();
        self.skip_newlines();
        while !self.at_eof() {
            if let Some(ref name) = self.ident_str() {
                if matches!(name.as_str(), "end" | "else" | "elsif" | "when" | "rescue" | "ensure") { break; }
            }
            if let Some(s) = self.parse_stmt() { stmts.push(s); }
            self.skip_newlines();
        }
        stmts
    }

    fn parse_stmt(&mut self) -> Option<RbStmt> {
        self.skip_newlines();
        match self.peek().clone() {
            RbTok::Ident(ref name) if name == "def" => self.parse_def(),
            RbTok::Ident(ref name) if name == "class" => self.parse_class(),
            RbTok::Ident(ref name) if name == "module" => {
                self.advance();
                let mname = self.ident_str().unwrap_or_default(); self.advance();
                let body = self.parse_body_until_end();
                self.expect_end();
                Some(RbStmt::Module(mname, body))
            }
            RbTok::Ident(ref name) if name == "if" => self.parse_if(),
            RbTok::Ident(ref name) if name == "unless" => {
                self.advance();
                let cond = self.parse_expr();
                self.skip_newlines();
                let body = self.parse_body_until_end();
                let else_b = if self.ident_str().as_deref() == Some("else") { self.advance(); Some(self.parse_body_until_end()) } else { None };
                self.expect_end();
                Some(RbStmt::Unless(cond, body, else_b))
            }
            RbTok::Ident(ref name) if name == "while" => {
                self.advance();
                let cond = self.parse_expr();
                self.skip_newlines();
                if self.ident_str().as_deref() == Some("do") { self.advance(); }
                let body = self.parse_body_until_end();
                self.expect_end();
                Some(RbStmt::While(cond, body))
            }
            RbTok::Ident(ref name) if name == "until" => {
                self.advance();
                let cond = self.parse_expr();
                self.skip_newlines();
                if self.ident_str().as_deref() == Some("do") { self.advance(); }
                let body = self.parse_body_until_end();
                self.expect_end();
                Some(RbStmt::Until(cond, body))
            }
            RbTok::Ident(ref name) if name == "for" => {
                self.advance();
                let var = self.ident_str().unwrap_or_default(); self.advance();
                if self.ident_str().as_deref() == Some("in") { self.advance(); }
                let iterable = self.parse_expr();
                self.skip_newlines();
                if self.ident_str().as_deref() == Some("do") { self.advance(); }
                let body = self.parse_body_until_end();
                self.expect_end();
                Some(RbStmt::For(var, iterable, body))
            }
            RbTok::Ident(ref name) if name == "case" => self.parse_case(),
            RbTok::Ident(ref name) if name == "return" => {
                self.advance();
                let val = if !matches!(self.peek(), RbTok::Newline | RbTok::Semi | RbTok::Eof) {
                    Some(self.parse_expr())
                } else { None };
                Some(RbStmt::Return(val))
            }
            RbTok::Ident(ref name) if name == "break" => { self.advance(); Some(RbStmt::Break) }
            RbTok::Ident(ref name) if name == "next" => { self.advance(); Some(RbStmt::Next) }
            RbTok::Ident(ref name) if name == "require" || name == "require_relative" => {
                self.advance();
                if let RbTok::StrLit(s) = self.peek().clone() { self.advance(); Some(RbStmt::Require(s)) }
                else { let s = self.ident_str().unwrap_or_default(); self.advance(); Some(RbStmt::Require(s)) }
            }
            RbTok::Ident(ref name) if name == "attr_accessor" || name == "attr_reader" || name == "attr_writer" => {
                self.advance();
                let mut attrs = Vec::new();
                while let RbTok::Symbol(s) = self.peek().clone() { attrs.push(s); self.advance(); if self.peek() == &RbTok::Comma { self.advance(); } }
                Some(RbStmt::AttrAccessor(attrs))
            }
            RbTok::Ident(ref name) if name == "begin" => {
                self.advance();
                let body = self.parse_body_until_end();
                let mut rescues = Vec::new();
                while self.ident_str().as_deref() == Some("rescue") {
                    self.advance();
                    let exc = if !matches!(self.peek(), RbTok::Newline | RbTok::Semi) { let n = self.ident_str(); self.advance(); n } else { None };
                    let rescue_body = self.parse_body_until_end();
                    rescues.push((exc, rescue_body));
                }
                let ensure = if self.ident_str().as_deref() == Some("ensure") { self.advance(); Some(self.parse_body_until_end()) } else { None };
                self.expect_end();
                Some(RbStmt::Begin(body, rescues, ensure))
            }
            _ => {
                let expr = self.parse_expr();
                // Check for assignment
                match self.peek().clone() {
                    RbTok::Eq => {
                        self.advance();
                        let val = self.parse_expr();
                        if let RbExpr::Name(n) = expr { return Some(RbStmt::Assign(n, val)); }
                        if let RbExpr::InstanceVar(n) = expr { return Some(RbStmt::InstanceAssign(n, val)); }
                        Some(RbStmt::Expr(val))
                    }
                    RbTok::PlusEq => { self.advance(); let v = self.parse_expr();
                        if let RbExpr::Name(n) = expr { Some(RbStmt::AugAssign(n, "+".to_string(), v)) } else { Some(RbStmt::Expr(v)) } }
                    RbTok::MinusEq => { self.advance(); let v = self.parse_expr();
                        if let RbExpr::Name(n) = expr { Some(RbStmt::AugAssign(n, "-".to_string(), v)) } else { Some(RbStmt::Expr(v)) } }
                    _ => {
                        // Check for trailing if/unless/while/until
                        if let Some(ref kw) = self.ident_str() {
                            match kw.as_str() {
                                "if" => { self.advance(); let cond = self.parse_expr();
                                    return Some(RbStmt::If(cond, vec![RbStmt::Expr(expr)], Vec::new(), None)); }
                                "unless" => { self.advance(); let cond = self.parse_expr();
                                    return Some(RbStmt::Unless(cond, vec![RbStmt::Expr(expr)], None)); }
                                "while" => { self.advance(); let cond = self.parse_expr();
                                    return Some(RbStmt::While(cond, vec![RbStmt::Expr(expr)])); }
                                _ => {}
                            }
                        }
                        Some(RbStmt::Expr(expr))
                    }
                }
            }
        }
    }

    fn expect_end(&mut self) { if self.ident_str().as_deref() == Some("end") { self.advance(); } }

    fn parse_def(&mut self) -> Option<RbStmt> {
        self.advance(); // def
        let name = self.ident_str().unwrap_or_default(); self.advance();
        let mut params = Vec::new();
        if self.peek() == &RbTok::LParen {
            self.advance();
            while self.peek() != &RbTok::RParen && !self.at_eof() {
                if let Some(p) = self.ident_str() { params.push(p); self.advance(); }
                else { self.advance(); }
                if self.peek() == &RbTok::Comma { self.advance(); }
                // Skip default values
                if self.peek() == &RbTok::Eq { self.advance(); self.parse_expr(); }
            }
            self.expect(RbTok::RParen);
        }
        let body = self.parse_body_until_end();
        self.expect_end();
        Some(RbStmt::Def(name, params, body))
    }

    fn parse_class(&mut self) -> Option<RbStmt> {
        self.advance(); // class
        let name = self.ident_str().unwrap_or_default(); self.advance();
        let parent = if self.peek() == &RbTok::Lt { self.advance(); let p = self.ident_str(); self.advance(); p } else { None };
        let body = self.parse_body_until_end();
        self.expect_end();
        Some(RbStmt::Class(name, parent, body))
    }

    fn parse_if(&mut self) -> Option<RbStmt> {
        self.advance(); // if
        let cond = self.parse_expr();
        self.skip_newlines();
        if self.ident_str().as_deref() == Some("then") { self.advance(); }
        let body = self.parse_body_until_end();
        let mut elsifs = Vec::new();
        while self.ident_str().as_deref() == Some("elsif") {
            self.advance();
            let c = self.parse_expr();
            self.skip_newlines();
            if self.ident_str().as_deref() == Some("then") { self.advance(); }
            let b = self.parse_body_until_end();
            elsifs.push((c, b));
        }
        let else_b = if self.ident_str().as_deref() == Some("else") { self.advance(); Some(self.parse_body_until_end()) } else { None };
        self.expect_end();
        Some(RbStmt::If(cond, body, elsifs, else_b))
    }

    fn parse_case(&mut self) -> Option<RbStmt> {
        self.advance(); // case
        let tag = if !matches!(self.peek(), RbTok::Newline | RbTok::Semi) { Some(self.parse_expr()) } else { None };
        self.skip_newlines();
        let mut whens = Vec::new();
        while self.ident_str().as_deref() == Some("when") {
            self.advance();
            let mut vals = Vec::new();
            vals.push(self.parse_expr());
            while self.peek() == &RbTok::Comma { self.advance(); vals.push(self.parse_expr()); }
            self.skip_newlines();
            if self.ident_str().as_deref() == Some("then") { self.advance(); }
            let body = self.parse_body_until_end();
            whens.push((vals, body));
        }
        let else_b = if self.ident_str().as_deref() == Some("else") { self.advance(); Some(self.parse_body_until_end()) } else { None };
        self.expect_end();
        Some(RbStmt::Case(tag, whens, else_b))
    }

    fn parse_expr(&mut self) -> RbExpr { self.parse_or() }

    fn parse_or(&mut self) -> RbExpr {
        let mut left = self.parse_and();
        while matches!(self.peek(), RbTok::PipePipe) || self.ident_str().as_deref() == Some("or") {
            self.advance(); let right = self.parse_and();
            left = RbExpr::Binary(Box::new(left), "||".to_string(), Box::new(right));
        }
        left
    }

    fn parse_and(&mut self) -> RbExpr {
        let mut left = self.parse_not();
        while matches!(self.peek(), RbTok::AmpAmp) || self.ident_str().as_deref() == Some("and") {
            self.advance(); let right = self.parse_not();
            left = RbExpr::Binary(Box::new(left), "&&".to_string(), Box::new(right));
        }
        left
    }

    fn parse_not(&mut self) -> RbExpr {
        if matches!(self.peek(), RbTok::Bang) || self.ident_str().as_deref() == Some("not") {
            self.advance();
            RbExpr::Unary("!".to_string(), Box::new(self.parse_not()))
        } else { self.parse_comparison() }
    }

    fn parse_comparison(&mut self) -> RbExpr {
        let mut left = self.parse_add();
        loop {
            let op = match self.peek() {
                RbTok::EqEq => "==", RbTok::NotEq => "!=",
                RbTok::Lt => "<", RbTok::Gt => ">",
                RbTok::LtEq => "<=", RbTok::GtEq => ">=",
                RbTok::Spaceship => "<=>",
                _ => break,
            };
            self.advance();
            let right = self.parse_add();
            left = RbExpr::Binary(Box::new(left), op.to_string(), Box::new(right));
        }
        left
    }

    fn parse_add(&mut self) -> RbExpr {
        let mut left = self.parse_mul();
        loop {
            let op = match self.peek() { RbTok::Plus => "+", RbTok::Minus => "-", _ => break };
            self.advance(); let right = self.parse_mul();
            left = RbExpr::Binary(Box::new(left), op.to_string(), Box::new(right));
        }
        left
    }

    fn parse_mul(&mut self) -> RbExpr {
        let mut left = self.parse_unary();
        loop {
            let op = match self.peek() { RbTok::Star => "*", RbTok::Slash => "/", RbTok::Percent => "%", RbTok::StarStar => "**", _ => break };
            self.advance(); let right = self.parse_unary();
            left = RbExpr::Binary(Box::new(left), op.to_string(), Box::new(right));
        }
        left
    }

    fn parse_unary(&mut self) -> RbExpr {
        match self.peek().clone() {
            RbTok::Minus => { self.advance(); RbExpr::Unary("-".to_string(), Box::new(self.parse_unary())) }
            RbTok::Bang => { self.advance(); RbExpr::Unary("!".to_string(), Box::new(self.parse_unary())) }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> RbExpr {
        let mut expr = self.parse_primary();
        loop {
            match self.peek().clone() {
                RbTok::Dot => {
                    self.advance();
                    let method = self.ident_str().unwrap_or_default(); self.advance();
                    // Check for block: .each do |x| or .each { |x| }
                    let mut args = Vec::new();
                    if self.peek() == &RbTok::LParen {
                        self.advance();
                        while self.peek() != &RbTok::RParen && !self.at_eof() {
                            args.push(self.parse_expr());
                            if self.peek() == &RbTok::Comma { self.advance(); }
                        }
                        self.expect(RbTok::RParen);
                    }
                    // Check for block
                    if self.ident_str().as_deref() == Some("do") {
                        self.advance();
                        let params = self.parse_block_params();
                        let body = self.parse_body_until_end();
                        self.expect_end();
                        args.push(RbExpr::Block(params, body));
                    } else if self.peek() == &RbTok::LBrace {
                        self.advance();
                        let params = self.parse_block_params();
                        let mut body = Vec::new();
                        while self.peek() != &RbTok::RBrace && !self.at_eof() {
                            if let Some(s) = self.parse_stmt() { body.push(s); }
                            self.skip_newlines();
                        }
                        self.expect(RbTok::RBrace);
                        args.push(RbExpr::Block(params, body));
                    }
                    expr = RbExpr::MethodCall(Box::new(expr), method, args);
                }
                RbTok::LBracket => {
                    self.advance();
                    let idx = self.parse_expr();
                    self.expect(RbTok::RBracket);
                    expr = RbExpr::Index(Box::new(expr), Box::new(idx));
                }
                RbTok::ColonColon => {
                    self.advance();
                    let member = self.ident_str().unwrap_or_default(); self.advance();
                    expr = RbExpr::MethodCall(Box::new(expr), member, Vec::new());
                }
                _ => break,
            }
        }
        expr
    }

    fn parse_block_params(&mut self) -> Vec<String> {
        let mut params = Vec::new();
        if self.peek() == &RbTok::Pipe {
            self.advance();
            while self.peek() != &RbTok::Pipe && !self.at_eof() {
                if let Some(p) = self.ident_str() { params.push(p); self.advance(); }
                else { self.advance(); }
                if self.peek() == &RbTok::Comma { self.advance(); }
            }
            self.expect(RbTok::Pipe);
        }
        params
    }

    fn parse_primary(&mut self) -> RbExpr {
        match self.peek().clone() {
            RbTok::IntLit(n) => { self.advance(); RbExpr::Int(n) }
            RbTok::FloatLit(f) => { self.advance(); RbExpr::Float(f) }
            RbTok::StrLit(s) => {
                self.advance();
                if s.contains("#{") { RbExpr::Interpolated(s) }
                else { RbExpr::Str(s) }
            }
            RbTok::Symbol(s) => { self.advance(); RbExpr::Sym(s) }
            RbTok::Ident(ref name) if name == "true" => { self.advance(); RbExpr::Bool(true) }
            RbTok::Ident(ref name) if name == "false" => { self.advance(); RbExpr::Bool(false) }
            RbTok::Ident(ref name) if name == "nil" => { self.advance(); RbExpr::Nil }
            RbTok::Ident(ref name) if name == "self" => { self.advance(); RbExpr::Self_ }
            RbTok::Ident(ref name) if name == "yield" => {
                self.advance();
                let mut args = Vec::new();
                if !matches!(self.peek(), RbTok::Newline | RbTok::Semi | RbTok::Eof) {
                    args.push(self.parse_expr());
                    while self.peek() == &RbTok::Comma { self.advance(); args.push(self.parse_expr()); }
                }
                RbExpr::Yield(args)
            }
            RbTok::Ident(name) => {
                self.advance();
                // Check if it's a function call without parens (e.g., puts "hello")
                if matches!(name.as_str(), "puts" | "print" | "p" | "raise" | "include" | "extend") {
                    if !matches!(self.peek(), RbTok::Newline | RbTok::Semi | RbTok::Eof | RbTok::Dot) {
                        let mut args = Vec::new();
                        args.push(self.parse_expr());
                        while self.peek() == &RbTok::Comma { self.advance(); args.push(self.parse_expr()); }
                        return RbExpr::Call(Box::new(RbExpr::Name(name)), args);
                    }
                }
                if self.peek() == &RbTok::LParen {
                    self.advance();
                    let mut args = Vec::new();
                    while self.peek() != &RbTok::RParen && !self.at_eof() {
                        args.push(self.parse_expr());
                        if self.peek() == &RbTok::Comma { self.advance(); }
                    }
                    self.expect(RbTok::RParen);
                    RbExpr::Call(Box::new(RbExpr::Name(name)), args)
                } else {
                    RbExpr::Name(name)
                }
            }
            RbTok::At => {
                self.advance();
                if let RbTok::Ident(name) = self.peek().clone() {
                    self.advance();
                    RbExpr::InstanceVar(name)
                } else { RbExpr::Nil }
            }
            RbTok::LBracket => {
                self.advance();
                let mut elems = Vec::new();
                while self.peek() != &RbTok::RBracket && !self.at_eof() {
                    self.skip_newlines();
                    elems.push(self.parse_expr());
                    self.skip_newlines();
                    if self.peek() == &RbTok::Comma { self.advance(); }
                }
                self.expect(RbTok::RBracket);
                RbExpr::Array(elems)
            }
            RbTok::LBrace => {
                self.advance();
                let mut pairs = Vec::new();
                while self.peek() != &RbTok::RBrace && !self.at_eof() {
                    let key = self.parse_expr();
                    if self.peek() == &RbTok::Arrow || self.peek() == &RbTok::Colon { self.advance(); }
                    let val = self.parse_expr();
                    pairs.push((key, val));
                    if self.peek() == &RbTok::Comma { self.advance(); }
                }
                self.expect(RbTok::RBrace);
                RbExpr::Hash(pairs)
            }
            RbTok::LParen => {
                self.advance();
                let expr = self.parse_expr();
                self.expect(RbTok::RParen);
                expr
            }
            _ => { self.advance(); RbExpr::Int(0) }
        }
    }
}

// ============ Emitter ============

fn sanitize_ernos_ident(name: &str) -> String {
    let clean = name.replace('@', "").replace('?', "_q").replace('!', "_bang");
    match clean.as_str() {
        "set" | "to" | "display" | "return" | "if" | "else" | "repeat" | "while" |
        "define" | "with" | "and" | "for" | "each" | "in" | "break" | "continue" |
        "check" | "structure" | "field" | "choice" | "variant" | "create" | "is" |
        "not" | "true" | "false" | "import" | "borrow" | "spawn" | "send" | "receive" |
        "from" | "channel" | "given" | "returning" | "as" | "trait" | "implement" |
        "equals" | "async" | "await" | "try" | "external" => format!("{}_", clean),
        _ => clean,
    }
}

fn emit_indent(out: &mut String, depth: usize) { for _ in 0..depth { out.push_str("    "); } }

pub fn emit_ernos_from_ruby(_filename: &str, source: &str) -> String {
    let mut lexer = RbLexer::new(source);
    let tokens = lexer.tokenize();
    let mut parser = RbParser::new(tokens);
    let stmts = parser.parse_file();
    let mut out = String::new();
    out.push_str("# Transpiled from Ruby\n\n");
    for s in &stmts {
        // Drop bare top-level main/main() calls — ErnosPlain runs main automatically
        match s {
            RbStmt::Expr(RbExpr::Name(n)) if n == "main" => continue,
            RbStmt::Expr(RbExpr::Call(func, args)) => {
                if let RbExpr::Name(n) = func.as_ref() {
                    if n == "main" && args.is_empty() { continue; }
                }
                emit_rb_stmt(&mut out, s, 0);
            }
            _ => emit_rb_stmt(&mut out, s, 0),
        }
    }
    out
}

fn emit_rb_stmt(out: &mut String, stmt: &RbStmt, depth: usize) {
    match stmt {
        RbStmt::Def(name, params, body) => {
            emit_indent(out, depth);
            let sname = sanitize_ernos_ident(name);
            if params.is_empty() { out.push_str(&format!("define {}:\n", sname)); }
            else {
                let p: Vec<String> = params.iter().map(|p| format!("{} as Int", sanitize_ernos_ident(p))).collect();
                out.push_str(&format!("define {} with {}:\n", sname, p.join(" and ")));
            }
            for s in body { emit_rb_stmt(out, s, depth + 1); }
            if body.is_empty() { emit_indent(out, depth + 1); out.push_str("return 0\n"); }
            out.push('\n');
        }
        RbStmt::Class(name, _parent, body) => {
            // Collect attr_accessors for fields
            let mut fields: Vec<String> = Vec::new();
            let mut methods = Vec::new();
            for s in body {
                match s {
                    RbStmt::AttrAccessor(attrs) => fields.extend(attrs.clone()),
                    RbStmt::Def(dname, _, _) if dname == "initialize" => {
                        // Extract instance var assignments
                        if let RbStmt::Def(_, params, init_body) = s {
                            for p in params { if !fields.contains(p) { fields.push(p.clone()); } }
                            for stmt in init_body {
                                if let RbStmt::InstanceAssign(n, _) = stmt {
                                    if !fields.contains(n) { fields.push(n.clone()); }
                                }
                            }
                        }
                    }
                    RbStmt::Def(..) => methods.push(s),
                    _ => {}
                }
            }
            emit_indent(out, depth);
            out.push_str(&format!("structure {}:\n", name));
            for f in &fields {
                emit_indent(out, depth + 1);
                out.push_str(&format!("field {} as Int\n", sanitize_ernos_ident(f)));
            }
            out.push('\n');
            for m in methods {
                if let RbStmt::Def(mname, params, mbody) = m {
                    emit_indent(out, depth);
                    let sname = sanitize_ernos_ident(mname);
                    if params.is_empty() {
                        out.push_str(&format!("define method {} on {}:\n", sname, name));
                    } else {
                        let p: Vec<String> = params.iter().map(|p| format!("{} as Int", sanitize_ernos_ident(p))).collect();
                        out.push_str(&format!("define method {} on {} with {}:\n", sname, name, p.join(" and ")));
                    }
                    for s in mbody { emit_rb_stmt(out, s, depth + 1); }
                    if mbody.is_empty() { emit_indent(out, depth + 1); out.push_str("return 0\n"); }
                    out.push('\n');
                }
            }
        }
        RbStmt::Module(_, body) => { for s in body { emit_rb_stmt(out, s, depth); } }
        RbStmt::Assign(name, val) => {
            emit_indent(out, depth);
            out.push_str(&format!("set {} to ", sanitize_ernos_ident(name)));
            emit_rb_expr(out, val);
            out.push('\n');
        }
        RbStmt::InstanceAssign(name, val) => {
            emit_indent(out, depth);
            out.push_str(&format!("set self.{} to ", sanitize_ernos_ident(name)));
            emit_rb_expr(out, val);
            out.push('\n');
        }
        RbStmt::AugAssign(name, op, val) => {
            emit_indent(out, depth);
            let sname = sanitize_ernos_ident(name);
            out.push_str(&format!("set {0} to {0} {1} ", sname, op));
            emit_rb_expr(out, val);
            out.push('\n');
        }
        RbStmt::If(cond, body, elsifs, else_b) => {
            emit_indent(out, depth);
            out.push_str("if ");
            emit_rb_expr(out, cond);
            out.push_str(":\n");
            for s in body { emit_rb_stmt(out, s, depth + 1); }
            for (c, b) in elsifs {
                emit_indent(out, depth);
                out.push_str("else if ");
                emit_rb_expr(out, c);
                out.push_str(":\n");
                for s in b { emit_rb_stmt(out, s, depth + 1); }
            }
            if let Some(eb) = else_b {
                emit_indent(out, depth);
                out.push_str("else:\n");
                for s in eb { emit_rb_stmt(out, s, depth + 1); }
            }
        }
        RbStmt::Unless(cond, body, else_b) => {
            emit_indent(out, depth);
            out.push_str("if not ");
            emit_rb_expr(out, cond);
            out.push_str(":\n");
            for s in body { emit_rb_stmt(out, s, depth + 1); }
            if let Some(eb) = else_b {
                emit_indent(out, depth);
                out.push_str("else:\n");
                for s in eb { emit_rb_stmt(out, s, depth + 1); }
            }
        }
        RbStmt::While(cond, body) => {
            emit_indent(out, depth);
            out.push_str("repeat while ");
            emit_rb_expr(out, cond);
            out.push_str(":\n");
            for s in body { emit_rb_stmt(out, s, depth + 1); }
        }
        RbStmt::Until(cond, body) => {
            emit_indent(out, depth);
            out.push_str("repeat while not ");
            emit_rb_expr(out, cond);
            out.push_str(":\n");
            for s in body { emit_rb_stmt(out, s, depth + 1); }
        }
        RbStmt::For(var, iterable, body) => {
            emit_indent(out, depth);
            out.push_str(&format!("for each {} in ", sanitize_ernos_ident(var)));
            emit_rb_expr(out, iterable);
            out.push_str(":\n");
            for s in body { emit_rb_stmt(out, s, depth + 1); }
        }
        RbStmt::Return(val) => {
            emit_indent(out, depth);
            out.push_str("return ");
            if let Some(v) = val { emit_rb_expr(out, v); } else { out.push('0'); }
            out.push('\n');
        }
        RbStmt::Break => { emit_indent(out, depth); out.push_str("break\n"); }
        RbStmt::Next => { emit_indent(out, depth); out.push_str("continue\n"); }
        RbStmt::Expr(expr) => {
            if let RbExpr::Call(func, args) = expr {
                if let RbExpr::Name(name) = func.as_ref() {
                    if matches!(name.as_str(), "puts" | "print" | "p") {
                        for a in args {
                            emit_indent(out, depth);
                            out.push_str("display ");
                            emit_rb_expr(out, a);
                            out.push('\n');
                        }
                        if args.is_empty() { emit_indent(out, depth); out.push_str("display \"\"\n"); }
                        return;
                    }
                }
            }
            if let RbExpr::MethodCall(obj, method, args) = expr {
                if (method == "each" || method == "each_with_index") && !args.is_empty() {
                    if let Some(RbExpr::Block(params, body)) = args.last() {
                        let var = if !params.is_empty() { &params[params.len() - 1] } else { "_item" };
                        emit_indent(out, depth);
                        out.push_str(&format!("for each {} in ", sanitize_ernos_ident(var)));
                        emit_rb_expr(out, obj);
                        out.push_str(":\n");
                        for s in body { emit_rb_stmt(out, s, depth + 1); }
                        return;
                    }
                }
                if method == "times" && !args.is_empty() {
                    if let Some(RbExpr::Block(params, body)) = args.last() {
                        let var = if !params.is_empty() { &params[0] } else { "_i" };
                        emit_indent(out, depth);
                        out.push_str(&format!("for each {} in range(0 and ", sanitize_ernos_ident(var)));
                        emit_rb_expr(out, obj);
                        out.push_str("):\n");
                        for s in body { emit_rb_stmt(out, s, depth + 1); }
                        return;
                    }
                }
            }
            emit_indent(out, depth);
            emit_rb_expr(out, expr);
            out.push('\n');
        }
        RbStmt::Case(tag, whens, else_b) => {
            if let Some(t) = tag {
                emit_indent(out, depth);
                out.push_str("check ");
                emit_rb_expr(out, t);
                out.push_str(":\n");
                for (vals, body) in whens {
                    for v in vals {
                        emit_indent(out, depth + 1);
                        out.push_str("if ");
                        emit_rb_expr(out, v);
                        out.push_str(":\n");
                    }
                    for s in body { emit_rb_stmt(out, s, depth + 2); }
                }
                if let Some(def) = else_b {
                    emit_indent(out, depth + 1);
                    out.push_str("if default:\n");
                    for s in def { emit_rb_stmt(out, s, depth + 2); }
                }
            } else {
                let mut first = true;
                for (vals, body) in whens {
                    emit_indent(out, depth);
                    if first { out.push_str("if "); first = false; } else { out.push_str("else if "); }
                    if !vals.is_empty() { emit_rb_expr(out, &vals[0]); }
                    out.push_str(":\n");
                    for s in body { emit_rb_stmt(out, s, depth + 1); }
                }
                if let Some(def) = else_b {
                    emit_indent(out, depth);
                    out.push_str("else:\n");
                    for s in def { emit_rb_stmt(out, s, depth + 1); }
                }
            }
        }
        RbStmt::Require(path) => { emit_indent(out, depth); out.push_str(&format!("# require \"{}\"\n", path)); }
        RbStmt::AttrAccessor(_) => {}  // Handled in class
        RbStmt::Begin(body, rescues, ensure) => {
            emit_indent(out, depth); out.push_str("# begin\n");
            for s in body { emit_rb_stmt(out, s, depth); }
            for (exc, rescue_body) in rescues {
                emit_indent(out, depth);
                if let Some(e) = exc { out.push_str(&format!("# rescue {}\n", e)); }
                else { out.push_str("# rescue\n"); }
                for s in rescue_body { emit_rb_stmt(out, s, depth); }
            }
            if let Some(ens) = ensure {
                emit_indent(out, depth); out.push_str("# ensure\n");
                for s in ens { emit_rb_stmt(out, s, depth); }
            }
        }
    }
}

fn emit_rb_expr(out: &mut String, expr: &RbExpr) {
    match expr {
        RbExpr::Int(n) => out.push_str(&n.to_string()),
        RbExpr::Float(f) => out.push_str(&format!("{}", f)),
        RbExpr::Str(s) => {
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
            out.push_str(&format!("\"{}\"", escaped));
        }
        RbExpr::Interpolated(s) => {
            let converted = s.replace("#{", "{");
            out.push_str(&format!("f\"{}\"", converted));
        }
        RbExpr::Sym(s) => out.push_str(&format!("\"{}\"", s)),
        RbExpr::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        RbExpr::Nil => out.push('0'),
        RbExpr::Name(n) => out.push_str(&sanitize_ernos_ident(n)),
        RbExpr::InstanceVar(n) => out.push_str(&format!("self.{}", sanitize_ernos_ident(n))),
        RbExpr::Self_ => out.push_str("self"),
        RbExpr::Binary(left, op, right) => {
            emit_rb_expr(out, left);
            let ep_op = match op.as_str() {
                "+" => " + ", "-" => " - ", "*" => " * ", "/" => " / ", "%" => " modulo ", "**" => " * ",
                "==" => " equals ", "!=" => " != ", "<" => " < ", ">" => " > ",
                "<=" => " <= ", ">=" => " >= ",
                "&&" => " and also ", "||" => " or else ",
                _ => { emit_rb_expr(out, left); out.push_str(&format!(" {} ", op)); emit_rb_expr(out, right); return; }
            };
            out.push_str(ep_op);
            emit_rb_expr(out, right);
        }
        RbExpr::Unary(op, inner) => {
            if op == "!" { out.push_str("not "); emit_rb_expr(out, inner); }
            else if op == "-" { out.push_str("(0 - "); emit_rb_expr(out, inner); out.push(')'); }
            else { emit_rb_expr(out, inner); }
        }
        RbExpr::Call(func, args) => {
            if let RbExpr::Name(name) = func.as_ref() {
                match name.as_str() {
                    "puts" | "print" | "p" => {
                        out.push_str("display ");
                        if !args.is_empty() { emit_rb_expr(out, &args[0]); }
                        return;
                    }
                    _ => {}
                }
            }
            emit_rb_expr(out, func);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 { out.push_str(" and "); }
                emit_rb_expr(out, a);
            }
            out.push(')');
        }
        RbExpr::MethodCall(obj, method, args) => {
            // Map common Ruby methods
            match method.as_str() {
                "length" | "size" | "count" => {
                    out.push_str("length(");
                    emit_rb_expr(out, obj);
                    out.push(')');
                    return;
                }
                "to_s" => {
                    out.push_str("int_to_string(");
                    emit_rb_expr(out, obj);
                    out.push(')');
                    return;
                }
                "to_i" => {
                    out.push_str("string_to_int(");
                    emit_rb_expr(out, obj);
                    out.push(')');
                    return;
                }
                "push" | "append" | "<<" => {
                    out.push_str("append(");
                    emit_rb_expr(out, obj);
                    if !args.is_empty() { out.push_str(" and "); emit_rb_expr(out, &args[0]); }
                    out.push(')');
                    return;
                }
                "include?" | "include" => {
                    out.push_str("string_contains(");
                    emit_rb_expr(out, obj);
                    if !args.is_empty() { out.push_str(" and "); emit_rb_expr(out, &args[0]); }
                    out.push(')');
                    return;
                }
                "new" => {
                    out.push_str("create ");
                    emit_rb_expr(out, obj);
                    return;
                }
                _ => {}
            }
            emit_rb_expr(out, obj);
            out.push('.');
            out.push_str(&sanitize_ernos_ident(method));
            if !args.is_empty() {
                out.push('(');
                let non_block_args: Vec<&RbExpr> = args.iter().filter(|a| !matches!(a, RbExpr::Block(..))).collect();
                for (i, a) in non_block_args.iter().enumerate() {
                    if i > 0 { out.push_str(" and "); }
                    emit_rb_expr(out, a);
                }
                out.push(')');
            }
        }
        RbExpr::Index(arr, idx) => {
            out.push_str("get_list(");
            emit_rb_expr(out, arr);
            out.push_str(" and ");
            emit_rb_expr(out, idx);
            out.push(')');
        }
        RbExpr::Array(elems) => {
            out.push('[');
            for (i, e) in elems.iter().enumerate() { if i > 0 { out.push_str(", "); } emit_rb_expr(out, e); }
            out.push(']');
        }
        RbExpr::Hash(pairs) => {
            out.push_str("# hash: {");
            for (i, (k, v)) in pairs.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                emit_rb_expr(out, k); out.push_str(": "); emit_rb_expr(out, v);
            }
            out.push('}');
        }
        RbExpr::Range(start, end, _exclusive) => {
            out.push_str("range(");
            emit_rb_expr(out, start);
            out.push_str(" and ");
            emit_rb_expr(out, end);
            out.push(')');
        }
        RbExpr::Block(params, _body) => {
            out.push_str("given ");
            if params.is_empty() { out.push_str("_"); }
            else { out.push_str(&params.iter().map(|p| sanitize_ernos_ident(p)).collect::<Vec<_>>().join(" and ")); }
            out.push(':');
        }
        RbExpr::Yield(args) => {
            out.push_str("# yield");
            if !args.is_empty() {
                out.push(' ');
                for (i, a) in args.iter().enumerate() { if i > 0 { out.push_str(", "); } emit_rb_expr(out, a); }
            }
        }
    }
}
