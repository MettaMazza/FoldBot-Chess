/// Go → ErnosPlain Transpiler
///
/// Parses a subset of Go source code and emits ErnosPlain (.ep) code.

// ============ Go Tokenizer ============

#[derive(Debug, Clone, PartialEq)]
pub enum GoTok {
    Ident(String),
    IntLit(i64),
    FloatLit(f64),
    StrLit(String),
    RuneLit(char),
    // Operators
    Plus, Minus, Star, Slash, Percent,
    PlusPlus, MinusMinus,
    PlusEq, MinusEq, StarEq, SlashEq, PercentEq,
    AmpAmp, PipePipe, Bang,
    Eq, ColonEq, EqEq, NotEq,
    Lt, Gt, LtEq, GtEq,
    Amp, Pipe, Caret, LShift, RShift,
    Arrow,  // <-
    // Delimiters
    LParen, RParen, LBracket, RBracket, LBrace, RBrace,
    Comma, Semi, Colon, Dot, Ellipsis,
    Newline, Eof,
}

pub struct GoLexer {
    chars: Vec<char>,
    pos: usize,
    prev_can_end_stmt: bool,
}

impl GoLexer {
    pub fn new(input: &str) -> Self {
        GoLexer { chars: input.chars().collect(), pos: 0, prev_can_end_stmt: false }
    }

    fn peek(&self) -> Option<char> { self.chars.get(self.pos).copied() }
    fn advance(&mut self) -> Option<char> { let c = self.chars.get(self.pos).copied(); self.pos += 1; c }
    fn peek_at(&self, off: usize) -> Option<char> { self.chars.get(self.pos + off).copied() }

    fn can_end_stmt(tok: &GoTok) -> bool {
        matches!(tok, GoTok::Ident(_) | GoTok::IntLit(_) | GoTok::FloatLit(_) |
            GoTok::StrLit(_) | GoTok::RuneLit(_) |
            GoTok::PlusPlus | GoTok::MinusMinus |
            GoTok::RParen | GoTok::RBracket | GoTok::RBrace)
    }

    pub fn tokenize(&mut self) -> Vec<GoTok> {
        let mut tokens = Vec::new();
        loop {
            while matches!(self.peek(), Some(' ') | Some('\t') | Some('\r')) { self.advance(); }
            match self.peek() {
                None => break,
                Some('\n') => {
                    self.advance();
                    if self.prev_can_end_stmt {
                        tokens.push(GoTok::Semi);
                        self.prev_can_end_stmt = false;
                    }
                    continue;
                }
                Some('/') if self.peek_at(1) == Some('/') => {
                    while self.peek() != Some('\n') && self.peek().is_some() { self.advance(); }
                    continue;
                }
                Some('/') if self.peek_at(1) == Some('*') => {
                    self.advance(); self.advance();
                    loop {
                        match self.advance() {
                            Some('*') if self.peek() == Some('/') => { self.advance(); break; }
                            None => break, _ => {}
                        }
                    }
                    continue;
                }
                Some('"') => {
                    self.advance();
                    let mut s = String::new();
                    loop {
                        match self.advance() {
                            Some('"') => break,
                            Some('\\') => match self.advance() {
                                Some('n') => s.push('\n'), Some('t') => s.push('\t'),
                                Some('\\') => s.push('\\'), Some('"') => s.push('"'),
                                Some(c) => { s.push('\\'); s.push(c); }
                                None => break,
                            },
                            Some(c) => s.push(c),
                            None => break,
                        }
                    }
                    let tok = GoTok::StrLit(s);
                    self.prev_can_end_stmt = Self::can_end_stmt(&tok);
                    tokens.push(tok);
                    continue;
                }
                Some('`') => {
                    self.advance();
                    let mut s = String::new();
                    loop {
                        match self.advance() {
                            Some('`') => break,
                            Some(c) => s.push(c),
                            None => break,
                        }
                    }
                    let tok = GoTok::StrLit(s);
                    self.prev_can_end_stmt = Self::can_end_stmt(&tok);
                    tokens.push(tok);
                    continue;
                }
                Some('\'') => {
                    self.advance();
                    let c = if self.peek() == Some('\\') {
                        self.advance();
                        match self.advance() {
                            Some('n') => '\n', Some('t') => '\t', Some('\\') => '\\',
                            Some('\'') => '\'', Some(c) => c, None => ' ',
                        }
                    } else {
                        self.advance().unwrap_or(' ')
                    };
                    if self.peek() == Some('\'') { self.advance(); }
                    let tok = GoTok::RuneLit(c);
                    self.prev_can_end_stmt = Self::can_end_stmt(&tok);
                    tokens.push(tok);
                    continue;
                }
                Some(c) if c.is_ascii_digit() => {
                    let mut num = String::new();
                    let mut is_float = false;
                    while let Some(ch) = self.peek() {
                        if ch.is_ascii_digit() || ch == '_' { num.push(ch); self.advance(); }
                        else if ch == '.' && !is_float { is_float = true; num.push(ch); self.advance(); }
                        else if (ch == 'x' || ch == 'X' || ch == 'o' || ch == 'b') && num == "0" {
                            num.push(ch); self.advance();
                        }
                        else if ch.is_ascii_hexdigit() && (num.starts_with("0x") || num.starts_with("0X")) {
                            num.push(ch); self.advance();
                        }
                        else { break; }
                    }
                    let clean: String = num.replace('_', "");
                    let tok = if is_float {
                        GoTok::FloatLit(clean.parse().unwrap_or(0.0))
                    } else if clean.starts_with("0x") || clean.starts_with("0X") {
                        GoTok::IntLit(i64::from_str_radix(&clean[2..], 16).unwrap_or(0))
                    } else if clean.starts_with("0o") {
                        GoTok::IntLit(i64::from_str_radix(&clean[2..], 8).unwrap_or(0))
                    } else if clean.starts_with("0b") {
                        GoTok::IntLit(i64::from_str_radix(&clean[2..], 2).unwrap_or(0))
                    } else {
                        GoTok::IntLit(clean.parse().unwrap_or(0))
                    };
                    self.prev_can_end_stmt = Self::can_end_stmt(&tok);
                    tokens.push(tok);
                    continue;
                }
                Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                    let mut ident = String::new();
                    while let Some(ch) = self.peek() {
                        if ch.is_ascii_alphanumeric() || ch == '_' { ident.push(ch); self.advance(); }
                        else { break; }
                    }
                    let tok = GoTok::Ident(ident);
                    self.prev_can_end_stmt = Self::can_end_stmt(&tok);
                    tokens.push(tok);
                    continue;
                }
                _ => {
                    let tok = self.lex_punct();
                    self.prev_can_end_stmt = Self::can_end_stmt(&tok);
                    tokens.push(tok);
                    continue;
                }
            }
        }
        if self.prev_can_end_stmt { tokens.push(GoTok::Semi); }
        tokens.push(GoTok::Eof);
        tokens
    }

    fn lex_punct(&mut self) -> GoTok {
        let c = self.advance().unwrap();
        match c {
            '+' => if self.peek() == Some('+') { self.advance(); GoTok::PlusPlus }
                   else if self.peek() == Some('=') { self.advance(); GoTok::PlusEq }
                   else { GoTok::Plus },
            '-' => if self.peek() == Some('-') { self.advance(); GoTok::MinusMinus }
                   else if self.peek() == Some('=') { self.advance(); GoTok::MinusEq }
                   else { GoTok::Minus },
            '*' => if self.peek() == Some('=') { self.advance(); GoTok::StarEq } else { GoTok::Star },
            '/' => if self.peek() == Some('=') { self.advance(); GoTok::SlashEq } else { GoTok::Slash },
            '%' => if self.peek() == Some('=') { self.advance(); GoTok::PercentEq } else { GoTok::Percent },
            '&' => if self.peek() == Some('&') { self.advance(); GoTok::AmpAmp } else { GoTok::Amp },
            '|' => if self.peek() == Some('|') { self.advance(); GoTok::PipePipe } else { GoTok::Pipe },
            '^' => GoTok::Caret,
            '!' => if self.peek() == Some('=') { self.advance(); GoTok::NotEq } else { GoTok::Bang },
            '=' => if self.peek() == Some('=') { self.advance(); GoTok::EqEq } else { GoTok::Eq },
            ':' => if self.peek() == Some('=') { self.advance(); GoTok::ColonEq } else { GoTok::Colon },
            '<' => if self.peek() == Some('=') { self.advance(); GoTok::LtEq }
                   else if self.peek() == Some('<') { self.advance(); GoTok::LShift }
                   else if self.peek() == Some('-') { self.advance(); GoTok::Arrow }
                   else { GoTok::Lt },
            '>' => if self.peek() == Some('=') { self.advance(); GoTok::GtEq }
                   else if self.peek() == Some('>') { self.advance(); GoTok::RShift }
                   else { GoTok::Gt },
            '(' => GoTok::LParen, ')' => GoTok::RParen,
            '[' => GoTok::LBracket, ']' => GoTok::RBracket,
            '{' => GoTok::LBrace, '}' => GoTok::RBrace,
            ',' => GoTok::Comma, ';' => GoTok::Semi,
            '.' => if self.peek() == Some('.') && self.peek_at(1) == Some('.') {
                self.advance(); self.advance(); GoTok::Ellipsis
            } else { GoTok::Dot },
            _ => GoTok::Eof,
        }
    }
}

// ============ Go AST ============

#[derive(Debug, Clone)]
pub enum GoExpr {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Nil,
    Name(String),
    Binary(Box<GoExpr>, String, Box<GoExpr>),
    Unary(String, Box<GoExpr>),
    Call(Box<GoExpr>, Vec<GoExpr>),
    Index(Box<GoExpr>, Box<GoExpr>),
    Member(Box<GoExpr>, String),
    Composite(Vec<GoExpr>),    // []int{1,2,3} or struct literal
    Receive(Box<GoExpr>),      // <-ch
    Closure(Vec<(String, String)>, Vec<GoStmt>),
    TypeAssert(Box<GoExpr>, String),
    MakeChan,
}

#[derive(Debug, Clone)]
pub enum GoStmt {
    FuncDecl(String, Vec<(String, String)>, Option<String>, Vec<GoStmt>),
    MethodDecl(String, String, String, Vec<(String, String)>, Option<String>, Vec<GoStmt>),  // recv_name, recv_type, method_name, params, ret, body
    VarDecl(String, Option<GoExpr>),
    ShortDecl(Vec<String>, Vec<GoExpr>),
    Assign(GoExpr, GoExpr),
    AugAssign(GoExpr, String, GoExpr),  // x += 1
    If(GoExpr, Vec<GoStmt>, Option<Vec<GoStmt>>),
    For(Option<Box<GoStmt>>, Option<GoExpr>, Option<Box<GoStmt>>, Vec<GoStmt>),
    ForRange(Vec<String>, GoExpr, Vec<GoStmt>),
    Return(Vec<GoExpr>),
    Expr(GoExpr),
    IncDec(GoExpr, String),   // x++ or x--
    Switch(Option<GoExpr>, Vec<(Vec<GoExpr>, Vec<GoStmt>)>, Option<Vec<GoStmt>>),
    StructType(String, Vec<(String, String)>),
    Import(Vec<(String, Option<String>)>),
    Package(String),
    Go(GoExpr),
    Send(GoExpr, GoExpr),  // ch <- val
    Break,
    Continue,
    Block(Vec<GoStmt>),
    InterfaceType(String, Vec<(String, Vec<(String, String)>, Option<String>)>),
    Const(String, Option<GoExpr>),
    Defer(GoExpr),
    Comment(String),
}

// ============ Go Parser ============

pub struct GoParser {
    tokens: Vec<GoTok>,
    pos: usize,
}

impl GoParser {
    pub fn new(tokens: Vec<GoTok>) -> Self {
        GoParser { tokens, pos: 0 }
    }

    fn peek(&self) -> &GoTok { self.tokens.get(self.pos).unwrap_or(&GoTok::Eof) }
    fn advance(&mut self) -> GoTok { let t = self.tokens.get(self.pos).cloned().unwrap_or(GoTok::Eof); self.pos += 1; t }
    fn expect(&mut self, tok: GoTok) { if self.peek() == &tok { self.advance(); } }
    fn skip_semis(&mut self) { while self.peek() == &GoTok::Semi { self.advance(); } }
    fn at_eof(&self) -> bool { self.peek() == &GoTok::Eof }

    fn ident_name(&self) -> Option<String> {
        if let GoTok::Ident(name) = self.peek() { Some(name.clone()) } else { None }
    }

    pub fn parse_file(&mut self) -> Vec<GoStmt> {
        let mut stmts = Vec::new();
        self.skip_semis();
        while !self.at_eof() {
            if let Some(s) = self.parse_top_level() { stmts.push(s); }
            self.skip_semis();
        }
        stmts
    }

    fn parse_top_level(&mut self) -> Option<GoStmt> {
        match self.peek().clone() {
            GoTok::Ident(ref name) if name == "package" => {
                self.advance();
                let pname = self.ident_name().unwrap_or_default(); self.advance();
                Some(GoStmt::Package(pname))
            }
            GoTok::Ident(ref name) if name == "import" => {
                self.advance();
                let mut imports = Vec::new();
                if self.peek() == &GoTok::LParen {
                    self.advance();
                    self.skip_semis();
                    while self.peek() != &GoTok::RParen && !self.at_eof() {
                        if let GoTok::StrLit(s) = self.peek().clone() { self.advance(); imports.push((s, None)); }
                        else if let GoTok::Ident(alias) = self.peek().clone() {
                            self.advance();
                            if let GoTok::StrLit(s) = self.peek().clone() { self.advance(); imports.push((s, Some(alias))); }
                        }
                        self.skip_semis();
                    }
                    self.expect(GoTok::RParen);
                } else if let GoTok::StrLit(s) = self.peek().clone() {
                    self.advance();
                    imports.push((s, None));
                }
                Some(GoStmt::Import(imports))
            }
            GoTok::Ident(ref name) if name == "func" => self.parse_func_decl(),
            GoTok::Ident(ref name) if name == "type" => self.parse_type_decl(),
            GoTok::Ident(ref name) if name == "var" => {
                self.advance();
                let vname = self.ident_name().unwrap_or_default(); self.advance();
                // skip type
                self.skip_type();
                let init = if self.peek() == &GoTok::Eq { self.advance(); Some(self.parse_expr()) } else { None };
                Some(GoStmt::VarDecl(vname, init))
            }
            GoTok::Ident(ref name) if name == "const" => {
                self.advance();
                if self.peek() == &GoTok::LParen {
                    self.advance();
                    let mut stmts = Vec::new();
                    self.skip_semis();
                    while self.peek() != &GoTok::RParen && !self.at_eof() {
                        let cname = self.ident_name().unwrap_or_default(); self.advance();
                        self.skip_type();
                        let val = if self.peek() == &GoTok::Eq { self.advance(); Some(self.parse_expr()) } else { None };
                        stmts.push(GoStmt::Const(cname, val));
                        self.skip_semis();
                    }
                    self.expect(GoTok::RParen);
                    Some(GoStmt::Block(stmts))
                } else {
                    let cname = self.ident_name().unwrap_or_default(); self.advance();
                    self.skip_type();
                    let val = if self.peek() == &GoTok::Eq { self.advance(); Some(self.parse_expr()) } else { None };
                    Some(GoStmt::Const(cname, val))
                }
            }
            _ => self.parse_stmt(),
        }
    }

    fn skip_type(&mut self) {
        // Skip type annotations: int, string, []int, *int, map[K]V, chan T, func(...), etc.
        match self.peek().clone() {
            GoTok::Ident(ref n) if matches!(n.as_str(), "int" | "int8" | "int16" | "int32" | "int64" |
                "uint" | "uint8" | "uint16" | "uint32" | "uint64" |
                "float32" | "float64" | "string" | "bool" | "byte" | "rune" | "error" |
                "interface" | "any") => { self.advance(); }
            GoTok::Star => { self.advance(); self.skip_type(); }
            GoTok::LBracket => {
                self.advance();
                if self.peek() != &GoTok::RBracket { self.parse_expr(); }
                self.expect(GoTok::RBracket);
                self.skip_type();
            }
            GoTok::Ident(ref n) if n == "map" => {
                self.advance();
                self.expect(GoTok::LBracket);
                self.skip_type();
                self.expect(GoTok::RBracket);
                self.skip_type();
            }
            GoTok::Ident(ref n) if n == "chan" => { self.advance(); self.skip_type(); }
            GoTok::Ident(ref n) if n == "func" => {
                self.advance();
                self.expect(GoTok::LParen);
                while self.peek() != &GoTok::RParen && !self.at_eof() { self.advance(); }
                self.expect(GoTok::RParen);
                // skip return type
                if self.peek() != &GoTok::LBrace && self.peek() != &GoTok::Semi && self.peek() != &GoTok::RParen && self.peek() != &GoTok::Comma {
                    self.skip_type();
                }
            }
            GoTok::Ident(_) => { self.advance(); /* user type */ }
            GoTok::LParen => {
                self.advance();
                while self.peek() != &GoTok::RParen && !self.at_eof() { self.skip_type(); if self.peek() == &GoTok::Comma { self.advance(); } }
                self.expect(GoTok::RParen);
            }
            _ => {}
        }
    }

    fn parse_func_decl(&mut self) -> Option<GoStmt> {
        self.advance(); // consume 'func'
        // Check for method: func (r *Type) Name(...)
        if self.peek() == &GoTok::LParen {
            // Could be method receiver or grouped params
            let saved = self.pos;
            self.advance(); // (
            let recv_name = self.ident_name().unwrap_or_default(); self.advance();
            // skip * for pointer receiver
            let mut recv_type = String::new();
            if self.peek() == &GoTok::Star { self.advance(); }
            if let Some(t) = self.ident_name() { recv_type = t; self.advance(); }
            if self.peek() == &GoTok::RParen {
                self.advance(); // )
                let method_name = self.ident_name().unwrap_or_default(); self.advance();
                let params = self.parse_param_list();
                let ret_type = self.parse_return_type();
                let body = self.parse_block();
                return Some(GoStmt::MethodDecl(recv_name, recv_type, method_name, params, ret_type, body));
            }
            // Not a method receiver, backtrack
            self.pos = saved;
        }

        let name = self.ident_name().unwrap_or_default(); self.advance();
        let params = self.parse_param_list();
        let ret_type = self.parse_return_type();
        let body = if self.peek() == &GoTok::LBrace { self.parse_block() } else { Vec::new() };
        Some(GoStmt::FuncDecl(name, params, ret_type, body))
    }

    fn parse_param_list(&mut self) -> Vec<(String, String)> {
        let mut params = Vec::new();
        self.expect(GoTok::LParen);
        while self.peek() != &GoTok::RParen && !self.at_eof() {
            let name = self.ident_name().unwrap_or_default(); self.advance();
            // Check if next is a type or comma/rparen (unnamed params)
            let type_name = if self.peek() != &GoTok::Comma && self.peek() != &GoTok::RParen {
                let mut t = String::new();
                if self.peek() == &GoTok::Ellipsis { self.advance(); t.push_str("..."); }
                if let Some(tn) = self.ident_name() { self.advance(); t.push_str(&tn); } else { self.skip_type(); t = "int".to_string(); }
                t
            } else { "int".to_string() };
            params.push((name, type_name));
            if self.peek() == &GoTok::Comma { self.advance(); }
        }
        self.expect(GoTok::RParen);
        params
    }

    fn parse_return_type(&mut self) -> Option<String> {
        if self.peek() == &GoTok::LBrace || self.peek() == &GoTok::Semi || self.at_eof() { return None; }
        if let GoTok::Ident(name) = self.peek().clone() {
            if matches!(name.as_str(), "int" | "int64" | "int32" | "float64" | "float32" |
                "string" | "bool" | "error" | "byte" | "rune") {
                self.advance();
                return Some(name);
            }
        }
        if self.peek() == &GoTok::LParen {
            // Multiple return values — skip
            self.advance();
            while self.peek() != &GoTok::RParen && !self.at_eof() { self.advance(); }
            self.expect(GoTok::RParen);
            return Some("int".to_string());
        }
        if self.peek() == &GoTok::Star { self.advance(); }
        if let Some(name) = self.ident_name() { self.advance(); return Some(name); }
        None
    }

    fn parse_type_decl(&mut self) -> Option<GoStmt> {
        self.advance(); // consume 'type'
        let name = self.ident_name().unwrap_or_default(); self.advance();
        match self.peek().clone() {
            GoTok::Ident(ref kw) if kw == "struct" => {
                self.advance();
                self.expect(GoTok::LBrace);
                let mut fields = Vec::new();
                self.skip_semis();
                while self.peek() != &GoTok::RBrace && !self.at_eof() {
                    let fname = self.ident_name().unwrap_or_default(); self.advance();
                    let mut ftype = String::new();
                    if self.peek() == &GoTok::Star { ftype.push('*'); self.advance(); }
                    if let Some(t) = self.ident_name() { ftype.push_str(&t); self.advance(); }
                    else { ftype.push_str("int"); self.skip_type(); }
                    fields.push((fname, ftype));
                    self.skip_semis();
                }
                self.expect(GoTok::RBrace);
                Some(GoStmt::StructType(name, fields))
            }
            GoTok::Ident(ref kw) if kw == "interface" => {
                self.advance();
                self.expect(GoTok::LBrace);
                let mut methods = Vec::new();
                self.skip_semis();
                while self.peek() != &GoTok::RBrace && !self.at_eof() {
                    let mname = self.ident_name().unwrap_or_default(); self.advance();
                    if self.peek() == &GoTok::LParen {
                        let params = self.parse_param_list();
                        let ret = self.parse_return_type();
                        methods.push((mname, params, ret));
                    }
                    self.skip_semis();
                }
                self.expect(GoTok::RBrace);
                Some(GoStmt::InterfaceType(name, methods))
            }
            _ => {
                // Type alias — skip
                self.skip_type();
                None
            }
        }
    }

    fn parse_block(&mut self) -> Vec<GoStmt> {
        let mut stmts = Vec::new();
        self.expect(GoTok::LBrace);
        self.skip_semis();
        while self.peek() != &GoTok::RBrace && !self.at_eof() {
            if let Some(s) = self.parse_stmt() { stmts.push(s); }
            self.skip_semis();
        }
        self.expect(GoTok::RBrace);
        stmts
    }

    fn parse_stmt(&mut self) -> Option<GoStmt> {
        match self.peek().clone() {
            GoTok::Ident(ref name) if name == "var" => {
                self.advance();
                let vname = self.ident_name().unwrap_or_default(); self.advance();
                self.skip_type();
                let init = if self.peek() == &GoTok::Eq { self.advance(); Some(self.parse_expr()) } else { None };
                Some(GoStmt::VarDecl(vname, init))
            }
            GoTok::Ident(ref name) if name == "if" => {
                self.advance();
                let cond = self.parse_expr();
                let then = self.parse_block();
                let else_block = if let GoTok::Ident(ref e) = self.peek().clone() {
                    if e == "else" {
                        self.advance();
                        if let GoTok::Ident(ref kw) = self.peek().clone() {
                            if kw == "if" { Some(vec![self.parse_stmt()?]) }
                            else { Some(self.parse_block()) }
                        } else { Some(self.parse_block()) }
                    } else { None }
                } else { None };
                Some(GoStmt::If(cond, then, else_block))
            }
            GoTok::Ident(ref name) if name == "for" => {
                self.advance();
                // Check for range
                let saved = self.pos;
                // Try to parse: ident, ident := range expr
                if let GoTok::Ident(_) = self.peek().clone() {
                    let first = self.ident_name().unwrap_or_default(); self.advance();
                    if self.peek() == &GoTok::Comma {
                        self.advance();
                        let second = self.ident_name().unwrap_or_default(); self.advance();
                        if self.peek() == &GoTok::ColonEq {
                            self.advance();
                            if let GoTok::Ident(ref r) = self.peek().clone() {
                                if r == "range" {
                                    self.advance();
                                    let iterable = self.parse_expr();
                                    let body = self.parse_block();
                                    return Some(GoStmt::ForRange(vec![first, second], iterable, body));
                                }
                            }
                        }
                    } else if self.peek() == &GoTok::ColonEq {
                        self.advance();
                        if let GoTok::Ident(ref r) = self.peek().clone() {
                            if r == "range" {
                                self.advance();
                                let iterable = self.parse_expr();
                                let body = self.parse_block();
                                return Some(GoStmt::ForRange(vec![first], iterable, body));
                            }
                        }
                    }
                    self.pos = saved;
                }
                // C-style for or while-like for
                if self.peek() == &GoTok::LBrace {
                    // infinite loop: for { }
                    let body = self.parse_block();
                    return Some(GoStmt::For(None, None, None, body));
                }
                // Try while-style: for cond { }
                let cond_saved = self.pos;
                let cond = self.parse_expr();
                if self.peek() == &GoTok::LBrace {
                    let body = self.parse_block();
                    return Some(GoStmt::For(None, Some(cond), None, body));
                }
                // C-style for init; cond; post { }
                self.pos = cond_saved;
                let init = self.parse_stmt().map(Box::new);
                self.expect(GoTok::Semi);
                let cond = if self.peek() != &GoTok::Semi { Some(self.parse_expr()) } else { None };
                self.expect(GoTok::Semi);
                let post = if self.peek() != &GoTok::LBrace { self.parse_stmt().map(Box::new) } else { None };
                let body = self.parse_block();
                Some(GoStmt::For(init, cond, post, body))
            }
            GoTok::Ident(ref name) if name == "return" => {
                self.advance();
                let mut vals = Vec::new();
                if self.peek() != &GoTok::Semi && self.peek() != &GoTok::RBrace && !self.at_eof() {
                    vals.push(self.parse_expr());
                    while self.peek() == &GoTok::Comma { self.advance(); vals.push(self.parse_expr()); }
                }
                Some(GoStmt::Return(vals))
            }
            GoTok::Ident(ref name) if name == "break" => { self.advance(); Some(GoStmt::Break) }
            GoTok::Ident(ref name) if name == "continue" => { self.advance(); Some(GoStmt::Continue) }
            GoTok::Ident(ref name) if name == "switch" => {
                self.advance();
                let tag = if self.peek() != &GoTok::LBrace { Some(self.parse_expr()) } else { None };
                self.expect(GoTok::LBrace);
                let mut cases = Vec::new();
                let mut default = None;
                self.skip_semis();
                while self.peek() != &GoTok::RBrace && !self.at_eof() {
                    if let GoTok::Ident(ref kw) = self.peek().clone() {
                        if kw == "case" {
                            self.advance();
                            let mut exprs = Vec::new();
                            exprs.push(self.parse_expr());
                            while self.peek() == &GoTok::Comma { self.advance(); exprs.push(self.parse_expr()); }
                            self.expect(GoTok::Colon);
                            self.skip_semis();
                            let mut body = Vec::new();
                            while !self.at_eof() {
                                if let GoTok::Ident(ref k) = self.peek().clone() {
                                    if k == "case" || k == "default" { break; }
                                }
                                if self.peek() == &GoTok::RBrace { break; }
                                if let Some(s) = self.parse_stmt() { body.push(s); }
                                self.skip_semis();
                            }
                            cases.push((exprs, body));
                        } else if kw == "default" {
                            self.advance();
                            self.expect(GoTok::Colon);
                            self.skip_semis();
                            let mut body = Vec::new();
                            while !self.at_eof() {
                                if let GoTok::Ident(ref k) = self.peek().clone() { if k == "case" { break; } }
                                if self.peek() == &GoTok::RBrace { break; }
                                if let Some(s) = self.parse_stmt() { body.push(s); }
                                self.skip_semis();
                            }
                            default = Some(body);
                        } else { self.advance(); }
                    } else { self.advance(); }
                    self.skip_semis();
                }
                self.expect(GoTok::RBrace);
                Some(GoStmt::Switch(tag, cases, default))
            }
            GoTok::Ident(ref name) if name == "go" => {
                self.advance();
                let expr = self.parse_expr();
                Some(GoStmt::Go(expr))
            }
            GoTok::Ident(ref name) if name == "defer" => {
                self.advance();
                let expr = self.parse_expr();
                Some(GoStmt::Defer(expr))
            }
            _ => {
                // Expression statement or assignment
                let expr = self.parse_expr();
                match self.peek().clone() {
                    GoTok::ColonEq => {
                        self.advance();
                        let val = self.parse_expr();
                        // Short declaration
                        let names = match expr {
                            GoExpr::Name(n) => vec![n],
                            _ => vec!["_".to_string()],
                        };
                        Some(GoStmt::ShortDecl(names, vec![val]))
                    }
                    GoTok::Eq => {
                        self.advance();
                        let val = self.parse_expr();
                        Some(GoStmt::Assign(expr, val))
                    }
                    GoTok::PlusEq => { self.advance(); let v = self.parse_expr(); Some(GoStmt::AugAssign(expr, "+=".to_string(), v)) }
                    GoTok::MinusEq => { self.advance(); let v = self.parse_expr(); Some(GoStmt::AugAssign(expr, "-=".to_string(), v)) }
                    GoTok::StarEq => { self.advance(); let v = self.parse_expr(); Some(GoStmt::AugAssign(expr, "*=".to_string(), v)) }
                    GoTok::SlashEq => { self.advance(); let v = self.parse_expr(); Some(GoStmt::AugAssign(expr, "/=".to_string(), v)) }
                    GoTok::PlusPlus => { self.advance(); Some(GoStmt::IncDec(expr, "++".to_string())) }
                    GoTok::MinusMinus => { self.advance(); Some(GoStmt::IncDec(expr, "--".to_string())) }
                    GoTok::Arrow => {
                        self.advance();
                        let val = self.parse_expr();
                        Some(GoStmt::Send(expr, val))
                    }
                    _ => Some(GoStmt::Expr(expr)),
                }
            }
        }
    }

    fn parse_expr(&mut self) -> GoExpr {
        self.parse_or_expr()
    }

    fn parse_or_expr(&mut self) -> GoExpr {
        let mut left = self.parse_and_expr();
        while self.peek() == &GoTok::PipePipe {
            self.advance();
            let right = self.parse_and_expr();
            left = GoExpr::Binary(Box::new(left), "||".to_string(), Box::new(right));
        }
        left
    }

    fn parse_and_expr(&mut self) -> GoExpr {
        let mut left = self.parse_cmp_expr();
        while self.peek() == &GoTok::AmpAmp {
            self.advance();
            let right = self.parse_cmp_expr();
            left = GoExpr::Binary(Box::new(left), "&&".to_string(), Box::new(right));
        }
        left
    }

    fn parse_cmp_expr(&mut self) -> GoExpr {
        let mut left = self.parse_add_expr();
        loop {
            let op = match self.peek() {
                GoTok::EqEq => "==", GoTok::NotEq => "!=",
                GoTok::Lt => "<", GoTok::Gt => ">",
                GoTok::LtEq => "<=", GoTok::GtEq => ">=",
                _ => break,
            };
            self.advance();
            let right = self.parse_add_expr();
            left = GoExpr::Binary(Box::new(left), op.to_string(), Box::new(right));
        }
        left
    }

    fn parse_add_expr(&mut self) -> GoExpr {
        let mut left = self.parse_mul_expr();
        loop {
            let op = match self.peek() {
                GoTok::Plus => "+", GoTok::Minus => "-",
                GoTok::Pipe => "|", GoTok::Caret => "^",
                _ => break,
            };
            self.advance();
            let right = self.parse_mul_expr();
            left = GoExpr::Binary(Box::new(left), op.to_string(), Box::new(right));
        }
        left
    }

    fn parse_mul_expr(&mut self) -> GoExpr {
        let mut left = self.parse_unary_expr();
        loop {
            let op = match self.peek() {
                GoTok::Star => "*", GoTok::Slash => "/", GoTok::Percent => "%",
                GoTok::AmpAmp => break, // don't consume &&
                GoTok::Amp => "&",
                GoTok::LShift => "<<", GoTok::RShift => ">>",
                _ => break,
            };
            self.advance();
            let right = self.parse_unary_expr();
            left = GoExpr::Binary(Box::new(left), op.to_string(), Box::new(right));
        }
        left
    }

    fn parse_unary_expr(&mut self) -> GoExpr {
        match self.peek().clone() {
            GoTok::Bang => { self.advance(); GoExpr::Unary("!".to_string(), Box::new(self.parse_unary_expr())) }
            GoTok::Minus => { self.advance(); GoExpr::Unary("-".to_string(), Box::new(self.parse_unary_expr())) }
            GoTok::Amp => { self.advance(); GoExpr::Unary("&".to_string(), Box::new(self.parse_unary_expr())) }
            GoTok::Star => { self.advance(); GoExpr::Unary("*".to_string(), Box::new(self.parse_unary_expr())) }
            GoTok::Arrow => { self.advance(); GoExpr::Receive(Box::new(self.parse_unary_expr())) }
            _ => self.parse_postfix_expr(),
        }
    }

    fn parse_postfix_expr(&mut self) -> GoExpr {
        let mut expr = self.parse_primary_expr();
        loop {
            match self.peek().clone() {
                GoTok::Dot => {
                    self.advance();
                    let field = self.ident_name().unwrap_or_default(); self.advance();
                    // Type assertion: expr.(Type)
                    if field.is_empty() && self.peek() == &GoTok::LParen {
                        self.advance();
                        let t = self.ident_name().unwrap_or_default(); self.advance();
                        self.expect(GoTok::RParen);
                        expr = GoExpr::TypeAssert(Box::new(expr), t);
                    } else {
                        expr = GoExpr::Member(Box::new(expr), field);
                    }
                }
                GoTok::LParen => {
                    self.advance();
                    let mut args = Vec::new();
                    while self.peek() != &GoTok::RParen && !self.at_eof() {
                        args.push(self.parse_expr());
                        if self.peek() == &GoTok::Comma { self.advance(); }
                        if self.peek() == &GoTok::Ellipsis { self.advance(); } // variadic spread
                    }
                    self.expect(GoTok::RParen);
                    expr = GoExpr::Call(Box::new(expr), args);
                }
                GoTok::LBracket => {
                    self.advance();
                    let idx = self.parse_expr();
                    self.expect(GoTok::RBracket);
                    expr = GoExpr::Index(Box::new(expr), Box::new(idx));
                }
                _ => break,
            }
        }
        expr
    }

    fn parse_primary_expr(&mut self) -> GoExpr {
        match self.peek().clone() {
            GoTok::IntLit(n) => { self.advance(); GoExpr::Int(n) }
            GoTok::FloatLit(f) => { self.advance(); GoExpr::Float(f) }
            GoTok::StrLit(s) => { self.advance(); GoExpr::Str(s) }
            GoTok::RuneLit(c) => { self.advance(); GoExpr::Int(c as i64) }
            GoTok::Ident(ref name) if name == "true" => { self.advance(); GoExpr::Bool(true) }
            GoTok::Ident(ref name) if name == "false" => { self.advance(); GoExpr::Bool(false) }
            GoTok::Ident(ref name) if name == "nil" => { self.advance(); GoExpr::Nil }
            GoTok::Ident(ref name) if name == "make" => {
                self.advance();
                self.expect(GoTok::LParen);
                if let GoTok::Ident(ref t) = self.peek().clone() {
                    if t == "chan" { self.advance(); self.skip_type(); self.expect(GoTok::RParen); return GoExpr::MakeChan; }
                }
                // Skip remaining args
                while self.peek() != &GoTok::RParen && !self.at_eof() { self.advance(); }
                self.expect(GoTok::RParen);
                GoExpr::Call(Box::new(GoExpr::Name("make".to_string())), Vec::new())
            }
            GoTok::Ident(ref name) if name == "func" => {
                self.advance();
                let params = self.parse_param_list();
                let _ret = self.parse_return_type();
                let body = self.parse_block();
                let stmts: Vec<GoStmt> = body;
                GoExpr::Closure(params, stmts)
            }
            GoTok::Ident(name) => {
                self.advance();
                // Check for composite literal: TypeName{...}
                if self.peek() == &GoTok::LBrace {
                    // Could be a composite literal
                    self.advance();
                    let mut elems = Vec::new();
                    while self.peek() != &GoTok::RBrace && !self.at_eof() {
                        elems.push(self.parse_expr());
                        if self.peek() == &GoTok::Comma { self.advance(); }
                    }
                    self.expect(GoTok::RBrace);
                    return GoExpr::Composite(elems);
                }
                GoExpr::Name(name)
            }
            GoTok::LParen => {
                self.advance();
                let expr = self.parse_expr();
                self.expect(GoTok::RParen);
                expr
            }
            GoTok::LBracket => {
                // Slice/array literal: []int{1,2,3} or just []
                self.advance();
                if self.peek() == &GoTok::RBracket {
                    self.advance();
                    self.skip_type(); // skip element type
                    if self.peek() == &GoTok::LBrace {
                        self.advance();
                        let mut elems = Vec::new();
                        while self.peek() != &GoTok::RBrace && !self.at_eof() {
                            elems.push(self.parse_expr());
                            if self.peek() == &GoTok::Comma { self.advance(); }
                        }
                        self.expect(GoTok::RBrace);
                        return GoExpr::Composite(elems);
                    }
                }
                GoExpr::Composite(Vec::new())
            }
            _ => { self.advance(); GoExpr::Int(0) }
        }
    }
}

// ============ Emitter ============

fn sanitize_ernos_ident(name: &str) -> String {
    match name {
        "set" | "to" | "display" | "return" | "if" | "else" | "repeat" | "while" |
        "define" | "with" | "and" | "for" | "each" | "in" | "break" | "continue" |
        "check" | "structure" | "field" | "choice" | "variant" | "create" | "is" |
        "not" | "true" | "false" | "import" | "borrow" | "spawn" | "send" | "receive" |
        "from" | "channel" | "given" | "returning" | "as" | "trait" | "implement" |
        "equals" | "async" | "await" | "try" | "external" => format!("{}_", name),
        _ => name.to_string(),
    }
}

fn emit_indent(out: &mut String, depth: usize) {
    for _ in 0..depth { out.push_str("    "); }
}

pub fn emit_ernos_from_go(_filename: &str, source: &str) -> String {
    let mut lexer = GoLexer::new(source);
    let tokens = lexer.tokenize();
    let mut parser = GoParser::new(tokens);
    let stmts = parser.parse_file();
    let mut out = String::new();
    out.push_str("# Transpiled from Go\n\n");
    for s in &stmts { emit_go_stmt(&mut out, s, 0); }
    out
}

fn emit_go_stmt(out: &mut String, stmt: &GoStmt, depth: usize) {
    match stmt {
        GoStmt::Package(_) => {}  // Skip package declaration
        GoStmt::Import(imports) => {
            for (path, _alias) in imports {
                emit_indent(out, depth);
                out.push_str(&format!("# import \"{}\"\n", path));
            }
            out.push('\n');
        }
        GoStmt::FuncDecl(name, params, _ret, body) => {
            emit_indent(out, depth);
            let sname = sanitize_ernos_ident(name);
            if params.is_empty() {
                out.push_str(&format!("define {}:\n", sname));
            } else {
                let p: Vec<String> = params.iter()
                    .map(|(n, t)| format!("{} as {}", sanitize_ernos_ident(n), go_type_to_ep(t)))
                    .collect();
                out.push_str(&format!("define {} with {}:\n", sname, p.join(" and ")));
            }
            for s in body { emit_go_stmt(out, s, depth + 1); }
            if body.is_empty() { emit_indent(out, depth + 1); out.push_str("return 0\n"); }
            out.push('\n');
        }
        GoStmt::MethodDecl(_recv, recv_type, method, params, _ret, body) => {
            emit_indent(out, depth);
            let sname = sanitize_ernos_ident(method);
            if params.is_empty() {
                out.push_str(&format!("define method {} on {}:\n", sname, recv_type));
            } else {
                let p: Vec<String> = params.iter()
                    .map(|(n, t)| format!("{} as {}", sanitize_ernos_ident(n), go_type_to_ep(t)))
                    .collect();
                out.push_str(&format!("define method {} on {} with {}:\n", sname, recv_type, p.join(" and ")));
            }
            for s in body { emit_go_stmt(out, s, depth + 1); }
            if body.is_empty() { emit_indent(out, depth + 1); out.push_str("return 0\n"); }
            out.push('\n');
        }
        GoStmt::StructType(name, fields) => {
            emit_indent(out, depth);
            out.push_str(&format!("structure {}:\n", name));
            for (fname, ftype) in fields {
                emit_indent(out, depth + 1);
                out.push_str(&format!("field {} as {}\n", sanitize_ernos_ident(fname), go_type_to_ep(ftype)));
            }
            out.push('\n');
        }
        GoStmt::InterfaceType(name, methods) => {
            emit_indent(out, depth);
            out.push_str(&format!("trait {}:\n", name));
            for (mname, params, _ret) in methods {
                emit_indent(out, depth + 1);
                if params.is_empty() {
                    out.push_str(&format!("define {}\n", sanitize_ernos_ident(mname)));
                } else {
                    let p: Vec<String> = params.iter().map(|(n, t)| format!("{} as {}", sanitize_ernos_ident(n), go_type_to_ep(t))).collect();
                    out.push_str(&format!("define {} with {}\n", sanitize_ernos_ident(mname), p.join(" and ")));
                }
            }
            out.push('\n');
        }
        GoStmt::VarDecl(name, init) | GoStmt::Const(name, init) => {
            emit_indent(out, depth);
            out.push_str(&format!("set {} to ", sanitize_ernos_ident(name)));
            if let Some(val) = init { emit_go_expr(out, val); } else { out.push('0'); }
            out.push('\n');
        }
        GoStmt::ShortDecl(names, vals) => {
            for (i, name) in names.iter().enumerate() {
                if name == "_" { continue; }
                emit_indent(out, depth);
                out.push_str(&format!("set {} to ", sanitize_ernos_ident(name)));
                if i < vals.len() { emit_go_expr(out, &vals[i]); } else { out.push('0'); }
                out.push('\n');
            }
        }
        GoStmt::Assign(target, val) => {
            emit_indent(out, depth);
            if let GoExpr::Name(name) = target {
                out.push_str(&format!("set {} to ", sanitize_ernos_ident(name)));
            } else if let GoExpr::Member(obj, field) = target {
                out.push_str("set ");
                emit_go_expr(out, obj);
                out.push_str(&format!(".{} to ", field));
            } else if let GoExpr::Index(arr, idx) = target {
                out.push_str("set_list(");
                emit_go_expr(out, arr);
                out.push_str(" and ");
                emit_go_expr(out, idx);
                out.push_str(" and ");
                emit_go_expr(out, val);
                out.push_str(")\n");
                return;
            } else {
                out.push_str("set _x to ");
            }
            emit_go_expr(out, val);
            out.push('\n');
        }
        GoStmt::AugAssign(target, op, val) => {
            emit_indent(out, depth);
            if let GoExpr::Name(name) = target {
                let sname = sanitize_ernos_ident(name);
                let ep_op = match op.as_str() { "+=" => "+", "-=" => "-", "*=" => "*", "/=" => "/", _ => "+" };
                out.push_str(&format!("set {0} to {0} {1} ", sname, ep_op));
                emit_go_expr(out, val);
                out.push('\n');
            }
        }
        GoStmt::IncDec(target, op) => {
            emit_indent(out, depth);
            if let GoExpr::Name(name) = target {
                let sname = sanitize_ernos_ident(name);
                let ep_op = if op == "++" { "+" } else { "-" };
                out.push_str(&format!("set {0} to {0} {1} 1\n", sname, ep_op));
            }
        }
        GoStmt::If(cond, then, else_block) => {
            emit_indent(out, depth);
            out.push_str("if ");
            emit_go_expr(out, cond);
            out.push_str(":\n");
            for s in then { emit_go_stmt(out, s, depth + 1); }
            if let Some(eb) = else_block {
                if eb.len() == 1 { if let GoStmt::If(..) = &eb[0] {
                    emit_indent(out, depth); out.push_str("else ");
                    emit_go_stmt_inline_if(out, &eb[0], depth);
                    return;
                }}
                emit_indent(out, depth);
                out.push_str("else:\n");
                for s in eb { emit_go_stmt(out, s, depth + 1); }
            }
        }
        GoStmt::For(init, cond, post, body) => {
            if let Some(init) = init { emit_go_stmt(out, init, depth); }
            emit_indent(out, depth);
            out.push_str("repeat while ");
            if let Some(c) = cond { emit_go_expr(out, c); } else { out.push_str("true"); }
            out.push_str(":\n");
            for s in body { emit_go_stmt(out, s, depth + 1); }
            if let Some(post) = post { emit_go_stmt(out, post, depth + 1); }
        }
        GoStmt::ForRange(vars, iterable, body) => {
            let item_var = if vars.len() >= 2 { &vars[1] } else if !vars.is_empty() && vars[0] != "_" { &vars[0] } else { "_item" };
            emit_indent(out, depth);
            out.push_str(&format!("for each {} in ", sanitize_ernos_ident(item_var)));
            emit_go_expr(out, iterable);
            out.push_str(":\n");
            for s in body { emit_go_stmt(out, s, depth + 1); }
        }
        GoStmt::Return(vals) => {
            emit_indent(out, depth);
            out.push_str("return ");
            if vals.is_empty() { out.push('0'); }
            else { emit_go_expr(out, &vals[0]); }
            out.push('\n');
        }
        GoStmt::Break => { emit_indent(out, depth); out.push_str("break\n"); }
        GoStmt::Continue => { emit_indent(out, depth); out.push_str("continue\n"); }
        GoStmt::Expr(expr) => {
            // Check for fmt.Println/Printf
            if let GoExpr::Call(func, args) = expr {
                if let GoExpr::Member(obj, method) = func.as_ref() {
                    if let GoExpr::Name(pkg) = obj.as_ref() {
                        if (pkg == "fmt" && (method == "Println" || method == "Printf" || method == "Print")) ||
                           (pkg == "log" && (method == "Println" || method == "Printf")) {
                            for a in args {
                                emit_indent(out, depth);
                                out.push_str("display ");
                                emit_go_expr(out, a);
                                out.push('\n');
                            }
                            return;
                        }
                    }
                }
                // Bare println/print (Go builtins)
                if let GoExpr::Name(name) = func.as_ref() {
                    if name == "println" || name == "print" {
                        for a in args {
                            emit_indent(out, depth);
                            out.push_str("display ");
                            emit_go_expr(out, a);
                            out.push('\n');
                        }
                        if args.is_empty() {
                            emit_indent(out, depth);
                            out.push_str("display \"\"\n");
                        }
                        return;
                    }
                }
            }
            emit_indent(out, depth);
            emit_go_expr(out, expr);
            out.push('\n');
        }
        GoStmt::Go(expr) => {
            emit_indent(out, depth);
            if let GoExpr::Call(func, args) = expr {
                out.push_str("spawn ");
                emit_go_expr(out, func);
                out.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 { out.push_str(" and "); }
                    emit_go_expr(out, a);
                }
                out.push_str(")\n");
            } else {
                out.push_str("# go ");
                emit_go_expr(out, expr);
                out.push('\n');
            }
        }
        GoStmt::Send(chan, val) => {
            emit_indent(out, depth);
            out.push_str("send ");
            emit_go_expr(out, val);
            out.push_str(" to ");
            emit_go_expr(out, chan);
            out.push('\n');
        }
        GoStmt::Defer(expr) => {
            emit_indent(out, depth);
            out.push_str("# defer ");
            emit_go_expr(out, expr);
            out.push('\n');
        }
        GoStmt::Switch(tag, cases, default) => {
            if let Some(tag) = tag {
                emit_indent(out, depth);
                out.push_str("check ");
                emit_go_expr(out, tag);
                out.push_str(":\n");
                for (exprs, body) in cases {
                    for e in exprs {
                        emit_indent(out, depth + 1);
                        out.push_str("if ");
                        emit_go_expr(out, e);
                        out.push_str(":\n");
                    }
                    for s in body { emit_go_stmt(out, s, depth + 2); }
                }
                if let Some(def) = default {
                    emit_indent(out, depth + 1);
                    out.push_str("if default:\n");
                    for s in def { emit_go_stmt(out, s, depth + 2); }
                }
            } else {
                // Expression-less switch → if/else chain
                let mut first = true;
                for (exprs, body) in cases {
                    emit_indent(out, depth);
                    if first { out.push_str("if "); first = false; } else { out.push_str("else if "); }
                    if !exprs.is_empty() { emit_go_expr(out, &exprs[0]); }
                    out.push_str(":\n");
                    for s in body { emit_go_stmt(out, s, depth + 1); }
                }
                if let Some(def) = default {
                    emit_indent(out, depth);
                    out.push_str("else:\n");
                    for s in def { emit_go_stmt(out, s, depth + 1); }
                }
            }
        }
        GoStmt::Block(stmts) => {
            for s in stmts { emit_go_stmt(out, s, depth); }
        }
        GoStmt::Comment(text) => {
            emit_indent(out, depth);
            out.push_str(&format!("# {}\n", text));
        }
    }
}

fn emit_go_stmt_inline_if(out: &mut String, stmt: &GoStmt, depth: usize) {
    if let GoStmt::If(cond, then, else_block) = stmt {
        out.push_str("if ");
        emit_go_expr(out, cond);
        out.push_str(":\n");
        for s in then { emit_go_stmt(out, s, depth + 1); }
        if let Some(eb) = else_block {
            if eb.len() == 1 { if let GoStmt::If(..) = &eb[0] {
                emit_indent(out, depth); out.push_str("else ");
                emit_go_stmt_inline_if(out, &eb[0], depth);
                return;
            }}
            emit_indent(out, depth);
            out.push_str("else:\n");
            for s in eb { emit_go_stmt(out, s, depth + 1); }
        }
    }
}

fn emit_go_expr(out: &mut String, expr: &GoExpr) {
    match expr {
        GoExpr::Int(n) => out.push_str(&n.to_string()),
        GoExpr::Float(f) => out.push_str(&format!("{}", f)),
        GoExpr::Str(s) => {
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
            out.push_str(&format!("\"{}\"", escaped));
        }
        GoExpr::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        GoExpr::Nil => out.push('0'),
        GoExpr::Name(name) => out.push_str(&sanitize_ernos_ident(name)),
        GoExpr::Binary(left, op, right) => {
            emit_go_expr(out, left);
            let ep_op = match op.as_str() {
                "+" => " + ", "-" => " - ", "*" => " * ", "/" => " / ", "%" => " modulo ",
                "==" => " equals ", "!=" => " != ", "<" => " < ", ">" => " > ",
                "<=" => " <= ", ">=" => " >= ",
                "&&" => " and also ", "||" => " or else ",
                "&" => " + ", "|" => " + ", "^" => " + ", "<<" => " * ", ">>" => " / ",
                _ => { emit_go_expr(out, left); out.push_str(&format!(" {} ", op)); emit_go_expr(out, right); return; }
            };
            out.push_str(ep_op);
            emit_go_expr(out, right);
        }
        GoExpr::Unary(op, inner) => {
            match op.as_str() {
                "!" => { out.push_str("not "); emit_go_expr(out, inner); }
                "-" => { out.push_str("(0 - "); emit_go_expr(out, inner); out.push(')'); }
                "&" => { out.push_str("borrow "); emit_go_expr(out, inner); }
                "*" => emit_go_expr(out, inner),  // deref → just use the value
                _ => emit_go_expr(out, inner),
            }
        }
        GoExpr::Call(func, args) => {
            // Map common Go builtins
            if let GoExpr::Name(name) = func.as_ref() {
                match name.as_str() {
                    "len" => {
                        out.push_str("length(");
                        if !args.is_empty() { emit_go_expr(out, &args[0]); }
                        out.push(')');
                        return;
                    }
                    "append" => {
                        out.push_str("append(");
                        for (i, a) in args.iter().enumerate() {
                            if i > 0 { out.push_str(" and "); }
                            emit_go_expr(out, a);
                        }
                        out.push(')');
                        return;
                    }
                    "string" | "int" | "int64" | "float64" => {
                        // Type conversion → just emit the value
                        if !args.is_empty() { emit_go_expr(out, &args[0]); }
                        return;
                    }
                    _ => {}
                }
            }
            emit_go_expr(out, func);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 { out.push_str(" and "); }
                emit_go_expr(out, a);
            }
            out.push(')');
        }
        GoExpr::Member(obj, field) => {
            emit_go_expr(out, obj);
            out.push('.');
            out.push_str(field);
        }
        GoExpr::Index(arr, idx) => {
            out.push_str("get_list(");
            emit_go_expr(out, arr);
            out.push_str(" and ");
            emit_go_expr(out, idx);
            out.push(')');
        }
        GoExpr::Composite(elems) => {
            out.push('[');
            for (i, e) in elems.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                emit_go_expr(out, e);
            }
            out.push(']');
        }
        GoExpr::Receive(chan) => {
            out.push_str("receive from ");
            emit_go_expr(out, chan);
        }
        GoExpr::MakeChan => { out.push_str("channel"); }
        GoExpr::Closure(params, body) => {
            if params.is_empty() {
                out.push_str("given _:\n");
            } else {
                let p: Vec<String> = params.iter().map(|(n, _)| sanitize_ernos_ident(n)).collect();
                out.push_str(&format!("given {}:\n", p.join(" and ")));
            }
            for s in body { emit_go_stmt(out, s, 1); }
        }
        GoExpr::TypeAssert(inner, _) => emit_go_expr(out, inner),
    }
}

fn go_type_to_ep(t: &str) -> &str {
    match t {
        "int" | "int8" | "int16" | "int32" | "int64" |
        "uint" | "uint8" | "uint16" | "uint32" | "uint64" | "byte" | "rune" => "Int",
        "float32" | "float64" => "Float",
        "bool" => "Bool",
        "string" => "Str",
        _ => "Int",
    }
}
