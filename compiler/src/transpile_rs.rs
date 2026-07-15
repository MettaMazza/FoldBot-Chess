/// Rust → ErnosPlain Transpiler
///
/// Parses a subset of Rust source code and emits ErnosPlain (.ep) code.

// ============ Rust Tokenizer ============

#[derive(Debug, Clone, PartialEq)]
pub enum RsTok {
    Ident(String), IntLit(i64), FloatLit(f64), StrLit(String), CharLit(char),
    Plus, Minus, Star, Slash, Percent,
    PlusEq, MinusEq, StarEq, SlashEq, PercentEq,
    AmpAmp, PipePipe, Bang,
    Eq, EqEq, NotEq,
    Lt, Gt, LtEq, GtEq,
    Arrow, FatArrow, DotDot, DotDotEq,
    Amp, Pipe, Caret, Tilde, LShift, RShift,
    LParen, RParen, LBracket, RBracket, LBrace, RBrace,
    Comma, Semi, Colon, ColonColon, Dot, Question, Hash, At,
    Newline, Eof,
}

pub struct RsLexer { chars: Vec<char>, pos: usize }

impl RsLexer {
    pub fn new(input: &str) -> Self { RsLexer { chars: input.chars().collect(), pos: 0 } }
    fn peek(&self) -> Option<char> { self.chars.get(self.pos).copied() }
    fn advance(&mut self) -> Option<char> { let c = self.chars.get(self.pos).copied(); self.pos += 1; c }
    fn peek_at(&self, off: usize) -> Option<char> { self.chars.get(self.pos + off).copied() }

    pub fn tokenize(&mut self) -> Vec<RsTok> {
        let mut tokens = Vec::new();
        loop {
            while matches!(self.peek(), Some(' ') | Some('\t') | Some('\r') | Some('\n')) { self.advance(); }
            match self.peek() {
                None => break,
                Some('/') if self.peek_at(1) == Some('/') => {
                    while self.peek() != Some('\n') && self.peek().is_some() { self.advance(); }
                }
                Some('/') if self.peek_at(1) == Some('*') => {
                    self.advance(); self.advance();
                    let mut depth = 1;
                    while depth > 0 {
                        match self.advance() {
                            Some('/') if self.peek() == Some('*') => { self.advance(); depth += 1; }
                            Some('*') if self.peek() == Some('/') => { self.advance(); depth -= 1; }
                            None => break, _ => {}
                        }
                    }
                }
                Some('#') if self.peek_at(1) == Some('[') || self.peek_at(1) == Some('!') => {
                    // Attribute: skip to end of line or ]
                    if self.peek_at(1) == Some('[') {
                        self.advance(); self.advance();
                        let mut depth = 1;
                        while depth > 0 {
                            match self.advance() {
                                Some('[') => depth += 1, Some(']') => depth -= 1,
                                None => break, _ => {}
                            }
                        }
                    } else {
                        while self.peek() != Some('\n') && self.peek().is_some() { self.advance(); }
                    }
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
                                Some('0') => s.push('\0'),
                                Some(c) => { s.push('\\'); s.push(c); }
                                None => break,
                            },
                            Some(c) => s.push(c),
                            None => break,
                        }
                    }
                    tokens.push(RsTok::StrLit(s));
                }
                Some('\'') => {
                    self.advance();
                    // Could be char literal or lifetime
                    if self.peek().map_or(false, |c| c.is_ascii_alphabetic() || c == '_') {
                        let mut name = String::new();
                        while let Some(c) = self.peek() {
                            if c.is_ascii_alphanumeric() || c == '_' { name.push(c); self.advance(); } else { break; }
                        }
                        if self.peek() == Some('\'') {
                            self.advance();
                            tokens.push(RsTok::CharLit(name.chars().next().unwrap_or(' ')));
                        }
                        // Lifetime annotation — skip it
                        continue;
                    } else if self.peek() == Some('\\') {
                        self.advance();
                        let c = match self.advance() {
                            Some('n') => '\n', Some('t') => '\t', Some('\\') => '\\', Some('\'') => '\'',
                            Some(c) => c, None => ' ',
                        };
                        if self.peek() == Some('\'') { self.advance(); }
                        tokens.push(RsTok::CharLit(c));
                    } else {
                        let c = self.advance().unwrap_or(' ');
                        if self.peek() == Some('\'') { self.advance(); }
                        tokens.push(RsTok::CharLit(c));
                    }
                }
                Some(c) if c.is_ascii_digit() => {
                    let mut n = String::new();
                    let mut is_float = false;
                    while let Some(ch) = self.peek() {
                        if ch.is_ascii_digit() || ch == '_' { n.push(ch); self.advance(); }
                        else if ch == '.' && !is_float && self.peek_at(1).map_or(false, |c| c.is_ascii_digit()) {
                            is_float = true; n.push(ch); self.advance();
                        }
                        else if (ch == 'x' || ch == 'o' || ch == 'b') && n == "0" { n.push(ch); self.advance(); }
                        else if ch.is_ascii_hexdigit() && (n.starts_with("0x")) { n.push(ch); self.advance(); }
                        else { break; }
                    }
                    // Skip type suffix like i32, u64, f64, usize, etc.
                    if self.peek().map_or(false, |c| c == 'i' || c == 'u' || c == 'f') {
                        while self.peek().map_or(false, |c| c.is_ascii_alphanumeric()) { self.advance(); }
                    }
                    let clean: String = n.replace('_', "");
                    if is_float { tokens.push(RsTok::FloatLit(clean.parse().unwrap_or(0.0))); }
                    else if clean.starts_with("0x") { tokens.push(RsTok::IntLit(i64::from_str_radix(&clean[2..], 16).unwrap_or(0))); }
                    else if clean.starts_with("0o") { tokens.push(RsTok::IntLit(i64::from_str_radix(&clean[2..], 8).unwrap_or(0))); }
                    else if clean.starts_with("0b") { tokens.push(RsTok::IntLit(i64::from_str_radix(&clean[2..], 2).unwrap_or(0))); }
                    else { tokens.push(RsTok::IntLit(clean.parse().unwrap_or(0))); }
                }
                Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                    let mut ident = String::new();
                    while let Some(ch) = self.peek() {
                        if ch.is_ascii_alphanumeric() || ch == '_' { ident.push(ch); self.advance(); } else { break; }
                    }
                    // Skip macro ! (println!, vec!, etc.)
                    if self.peek() == Some('!') && !matches!(ident.as_str(), "ne" | "eq") {
                        // Keep ident, consume !
                        ident.push('!');
                        self.advance();
                    }
                    tokens.push(RsTok::Ident(ident));
                }
                _ => { tokens.push(self.lex_punct()); }
            }
        }
        tokens.push(RsTok::Eof);
        tokens
    }

    fn lex_punct(&mut self) -> RsTok {
        let c = self.advance().unwrap();
        match c {
            '+' => if self.peek() == Some('=') { self.advance(); RsTok::PlusEq } else { RsTok::Plus },
            '-' => if self.peek() == Some('>') { self.advance(); RsTok::Arrow }
                   else if self.peek() == Some('=') { self.advance(); RsTok::MinusEq }
                   else { RsTok::Minus },
            '*' => if self.peek() == Some('=') { self.advance(); RsTok::StarEq } else { RsTok::Star },
            '/' => if self.peek() == Some('=') { self.advance(); RsTok::SlashEq } else { RsTok::Slash },
            '%' => if self.peek() == Some('=') { self.advance(); RsTok::PercentEq } else { RsTok::Percent },
            '&' => if self.peek() == Some('&') { self.advance(); RsTok::AmpAmp } else { RsTok::Amp },
            '|' => if self.peek() == Some('|') { self.advance(); RsTok::PipePipe } else { RsTok::Pipe },
            '^' => RsTok::Caret, '~' => RsTok::Tilde,
            '!' => if self.peek() == Some('=') { self.advance(); RsTok::NotEq } else { RsTok::Bang },
            '=' => if self.peek() == Some('=') { self.advance(); RsTok::EqEq }
                   else if self.peek() == Some('>') { self.advance(); RsTok::FatArrow }
                   else { RsTok::Eq },
            '<' => if self.peek() == Some('=') { self.advance(); RsTok::LtEq }
                   else if self.peek() == Some('<') { self.advance(); RsTok::LShift }
                   else { RsTok::Lt },
            '>' => if self.peek() == Some('=') { self.advance(); RsTok::GtEq }
                   else if self.peek() == Some('>') { self.advance(); RsTok::RShift }
                   else { RsTok::Gt },
            ':' => if self.peek() == Some(':') { self.advance(); RsTok::ColonColon } else { RsTok::Colon },
            '.' => if self.peek() == Some('.') {
                       self.advance();
                       if self.peek() == Some('=') { self.advance(); RsTok::DotDotEq } else { RsTok::DotDot }
                   } else { RsTok::Dot },
            '(' => RsTok::LParen, ')' => RsTok::RParen,
            '[' => RsTok::LBracket, ']' => RsTok::RBracket,
            '{' => RsTok::LBrace, '}' => RsTok::RBrace,
            ',' => RsTok::Comma, ';' => RsTok::Semi,
            '?' => RsTok::Question, '#' => RsTok::Hash, '@' => RsTok::At,
            _ => RsTok::Eof,
        }
    }
}

// ============ Rust AST ============

#[derive(Debug, Clone)]
pub enum RsExpr {
    Int(i64), Float(f64), Str(String), Char(char), Bool(bool), None_,
    Name(String), Path(Vec<String>),
    Binary(Box<RsExpr>, String, Box<RsExpr>),
    Unary(String, Box<RsExpr>),
    Call(Box<RsExpr>, Vec<RsExpr>),
    MethodCall(Box<RsExpr>, String, Vec<RsExpr>),
    FieldAccess(Box<RsExpr>, String),
    Index(Box<RsExpr>, Box<RsExpr>),
    Array(Vec<RsExpr>),
    Tuple(Vec<RsExpr>),
    Range(Option<Box<RsExpr>>, Option<Box<RsExpr>>, bool),
    Closure(Vec<String>, Vec<RsStmt>),
    Block(Vec<RsStmt>),
    If(Box<RsExpr>, Vec<RsStmt>, Option<Vec<RsStmt>>),
    MacroCall(String, Vec<RsExpr>),
    Ref(Box<RsExpr>),
    Deref(Box<RsExpr>),
    Try(Box<RsExpr>),
    Await(Box<RsExpr>),
    StructLit(String, Vec<(String, RsExpr)>),
}

#[derive(Debug, Clone)]
pub enum RsStmt {
    Let(String, bool, Option<RsExpr>),
    Assign(RsExpr, RsExpr),
    AugAssign(RsExpr, String, RsExpr),
    Fn(String, Vec<(String, String)>, Option<String>, Vec<RsStmt>),
    Struct(String, Vec<(String, String, bool)>),  // name, type, public
    Enum(String, Vec<(String, Vec<(String, String)>)>),
    Impl(String, Option<String>, Vec<RsStmt>),  // type, trait, methods
    Trait(String, Vec<RsStmt>),
    If(RsExpr, Vec<RsStmt>, Option<Vec<RsStmt>>),
    While(RsExpr, Vec<RsStmt>),
    Loop(Vec<RsStmt>),
    For(String, RsExpr, Vec<RsStmt>),
    Match(RsExpr, Vec<(Vec<String>, Vec<String>, Vec<RsStmt>)>),
    Return(Option<RsExpr>),
    Expr(RsExpr),
    Use(String),
    Mod(String),
    Break, Continue,
    Const(String, String, RsExpr),
    Static(String, String, RsExpr),
}

// ============ Rust Parser ============

pub struct RsParser { tokens: Vec<RsTok>, pos: usize }

impl RsParser {
    pub fn new(tokens: Vec<RsTok>) -> Self { RsParser { tokens, pos: 0 } }
    fn peek(&self) -> &RsTok { self.tokens.get(self.pos).unwrap_or(&RsTok::Eof) }
    fn advance(&mut self) -> RsTok { let t = self.tokens.get(self.pos).cloned().unwrap_or(RsTok::Eof); self.pos += 1; t }
    fn expect(&mut self, tok: RsTok) { if self.peek() == &tok { self.advance(); } }
    fn at_eof(&self) -> bool { self.peek() == &RsTok::Eof }
    fn ident_str(&self) -> Option<String> { if let RsTok::Ident(s) = self.peek() { Some(s.clone()) } else { None } }

    pub fn parse_file(&mut self) -> Vec<RsStmt> {
        let mut stmts = Vec::new();
        while !self.at_eof() {
            // Skip visibility modifiers
            while self.ident_str().as_deref() == Some("pub") { self.advance();
                if self.peek() == &RsTok::LParen { self.advance(); while self.peek() != &RsTok::RParen && !self.at_eof() { self.advance(); } self.expect(RsTok::RParen); }
            }
            if let Some(s) = self.parse_item() { stmts.push(s); }
            while self.peek() == &RsTok::Semi { self.advance(); }
        }
        stmts
    }

    fn skip_generic_params(&mut self) {
        if self.peek() == &RsTok::Lt {
            self.advance();
            let mut depth = 1;
            while depth > 0 && !self.at_eof() {
                match self.peek() {
                    RsTok::Lt => { depth += 1; self.advance(); }
                    RsTok::Gt => { depth -= 1; self.advance(); }
                    _ => { self.advance(); }
                }
            }
        }
    }

    fn skip_where_clause(&mut self) {
        if self.ident_str().as_deref() == Some("where") {
            self.advance();
            while self.peek() != &RsTok::LBrace && !self.at_eof() { self.advance(); }
        }
    }

    fn parse_type(&mut self) -> String {
        let mut t = String::new();
        if self.peek() == &RsTok::Amp { t.push('&'); self.advance();
            if self.ident_str().as_deref() == Some("mut") { self.advance(); }
        }
        if self.peek() == &RsTok::Star { t.push('*'); self.advance();
            if self.ident_str().as_deref() == Some("const") || self.ident_str().as_deref() == Some("mut") { self.advance(); }
        }
        if let Some(name) = self.ident_str() { t.push_str(&name); self.advance(); }
        if self.peek() == &RsTok::ColonColon { self.advance(); if let Some(n) = self.ident_str() { t.push_str("::"); t.push_str(&n); self.advance(); } }
        self.skip_generic_params();
        t
    }

    fn parse_item(&mut self) -> Option<RsStmt> {
        match self.peek().clone() {
            RsTok::Ident(ref name) if name == "fn" || name == "async" => self.parse_fn_item(),
            RsTok::Ident(ref name) if name == "struct" => self.parse_struct_item(),
            RsTok::Ident(ref name) if name == "enum" => self.parse_enum_item(),
            RsTok::Ident(ref name) if name == "impl" => self.parse_impl_item(),
            RsTok::Ident(ref name) if name == "trait" => self.parse_trait_item(),
            RsTok::Ident(ref name) if name == "use" => {
                self.advance();
                let mut path = String::new();
                while self.peek() != &RsTok::Semi && !self.at_eof() {
                    if let Some(id) = self.ident_str() { path.push_str(&id); self.advance(); }
                    else if self.peek() == &RsTok::ColonColon { path.push_str("::"); self.advance(); }
                    else if self.peek() == &RsTok::LBrace {
                        self.advance();
                        while self.peek() != &RsTok::RBrace && !self.at_eof() { self.advance(); }
                        self.expect(RsTok::RBrace); break;
                    }
                    else if self.peek() == &RsTok::Star { path.push('*'); self.advance(); break; }
                    else { self.advance(); }
                }
                self.expect(RsTok::Semi);
                Some(RsStmt::Use(path))
            }
            RsTok::Ident(ref name) if name == "mod" => {
                self.advance();
                let mname = self.ident_str().unwrap_or_default(); self.advance();
                if self.peek() == &RsTok::LBrace {
                    let body = self.parse_block();
                    for _s in body { /* flatten */ }
                }
                self.expect(RsTok::Semi);
                Some(RsStmt::Mod(mname))
            }
            RsTok::Ident(ref name) if name == "const" => {
                self.advance();
                let cname = self.ident_str().unwrap_or_default(); self.advance();
                self.expect(RsTok::Colon);
                let ctype = self.parse_type();
                self.expect(RsTok::Eq);
                let val = self.parse_expr();
                self.expect(RsTok::Semi);
                Some(RsStmt::Const(cname, ctype, val))
            }
            RsTok::Ident(ref name) if name == "static" => {
                self.advance();
                if self.ident_str().as_deref() == Some("mut") { self.advance(); }
                let sname = self.ident_str().unwrap_or_default(); self.advance();
                self.expect(RsTok::Colon);
                let stype = self.parse_type();
                self.expect(RsTok::Eq);
                let val = self.parse_expr();
                self.expect(RsTok::Semi);
                Some(RsStmt::Static(sname, stype, val))
            }
            RsTok::Ident(ref name) if name == "type" => {
                self.advance();
                while self.peek() != &RsTok::Semi && !self.at_eof() { self.advance(); }
                self.expect(RsTok::Semi);
                None
            }
            RsTok::Ident(ref name) if name == "extern" => {
                self.advance();
                if let RsTok::StrLit(_) = self.peek().clone() { self.advance(); }
                if self.peek() == &RsTok::LBrace {
                    self.advance();
                    while self.peek() != &RsTok::RBrace && !self.at_eof() { self.advance(); }
                    self.expect(RsTok::RBrace);
                } else {
                    while self.peek() != &RsTok::Semi && !self.at_eof() { self.advance(); }
                    self.expect(RsTok::Semi);
                }
                None
            }
            _ => self.parse_stmt(),
        }
    }

    fn parse_fn_item(&mut self) -> Option<RsStmt> {
        let is_async = self.ident_str().as_deref() == Some("async");
        if is_async { self.advance(); }
        self.advance(); // fn
        let name = self.ident_str().unwrap_or_default(); self.advance();
        self.skip_generic_params();
        let params = self.parse_fn_params();
        let ret = if self.peek() == &RsTok::Arrow { self.advance(); Some(self.parse_type()) } else { None };
        self.skip_where_clause();
        let body = if self.peek() == &RsTok::LBrace { self.parse_block() } else { self.expect(RsTok::Semi); Vec::new() };
        Some(RsStmt::Fn(name, params, ret, body))
    }

    fn parse_fn_params(&mut self) -> Vec<(String, String)> {
        let mut params = Vec::new();
        self.expect(RsTok::LParen);
        while self.peek() != &RsTok::RParen && !self.at_eof() {
            // Skip &self, &mut self, self
            if self.peek() == &RsTok::Amp { self.advance();
                if self.ident_str().as_deref() == Some("mut") { self.advance(); }
                if self.ident_str().as_deref() == Some("self") { self.advance(); if self.peek() == &RsTok::Comma { self.advance(); } continue; }
            }
            if self.ident_str().as_deref() == Some("self") || self.ident_str().as_deref() == Some("mut") {
                self.advance();
                if self.ident_str().as_deref() == Some("self") { self.advance(); }
                if self.peek() == &RsTok::Comma { self.advance(); }
                continue;
            }
            let pname = self.ident_str().unwrap_or_default(); self.advance();
            self.expect(RsTok::Colon);
            let ptype = self.parse_type();
            params.push((pname, ptype));
            if self.peek() == &RsTok::Comma { self.advance(); }
        }
        self.expect(RsTok::RParen);
        params
    }

    fn parse_struct_item(&mut self) -> Option<RsStmt> {
        self.advance(); // struct
        let name = self.ident_str().unwrap_or_default(); self.advance();
        self.skip_generic_params();
        self.skip_where_clause();
        if self.peek() == &RsTok::Semi { self.advance(); return Some(RsStmt::Struct(name, Vec::new())); }
        // Tuple struct
        if self.peek() == &RsTok::LParen {
            self.advance();
            let mut fields = Vec::new();
            let mut idx = 0;
            while self.peek() != &RsTok::RParen && !self.at_eof() {
                let is_pub = self.ident_str().as_deref() == Some("pub");
                if is_pub { self.advance(); }
                let t = self.parse_type();
                fields.push((format!("field{}", idx), t, is_pub));
                idx += 1;
                if self.peek() == &RsTok::Comma { self.advance(); }
            }
            self.expect(RsTok::RParen);
            self.expect(RsTok::Semi);
            return Some(RsStmt::Struct(name, fields));
        }
        self.expect(RsTok::LBrace);
        let mut fields = Vec::new();
        while self.peek() != &RsTok::RBrace && !self.at_eof() {
            let is_pub = self.ident_str().as_deref() == Some("pub");
            if is_pub { self.advance(); }
            let fname = self.ident_str().unwrap_or_default(); self.advance();
            self.expect(RsTok::Colon);
            let ftype = self.parse_type();
            fields.push((fname, ftype, is_pub));
            if self.peek() == &RsTok::Comma { self.advance(); }
        }
        self.expect(RsTok::RBrace);
        Some(RsStmt::Struct(name, fields))
    }

    fn parse_enum_item(&mut self) -> Option<RsStmt> {
        self.advance(); // enum
        let name = self.ident_str().unwrap_or_default(); self.advance();
        self.skip_generic_params();
        self.expect(RsTok::LBrace);
        let mut variants = Vec::new();
        while self.peek() != &RsTok::RBrace && !self.at_eof() {
            let vname = self.ident_str().unwrap_or_default(); self.advance();
            let mut fields = Vec::new();
            if self.peek() == &RsTok::LParen {
                self.advance();
                let mut idx = 0;
                while self.peek() != &RsTok::RParen && !self.at_eof() {
                    let t = self.parse_type();
                    fields.push((format!("val{}", idx), t));
                    idx += 1;
                    if self.peek() == &RsTok::Comma { self.advance(); }
                }
                self.expect(RsTok::RParen);
            } else if self.peek() == &RsTok::LBrace {
                self.advance();
                while self.peek() != &RsTok::RBrace && !self.at_eof() {
                    let fname = self.ident_str().unwrap_or_default(); self.advance();
                    self.expect(RsTok::Colon);
                    let ftype = self.parse_type();
                    fields.push((fname, ftype));
                    if self.peek() == &RsTok::Comma { self.advance(); }
                }
                self.expect(RsTok::RBrace);
            }
            // Skip discriminant
            if self.peek() == &RsTok::Eq { self.advance(); self.parse_expr(); }
            variants.push((vname, fields));
            if self.peek() == &RsTok::Comma { self.advance(); }
        }
        self.expect(RsTok::RBrace);
        Some(RsStmt::Enum(name, variants))
    }

    fn parse_impl_item(&mut self) -> Option<RsStmt> {
        self.advance(); // impl
        self.skip_generic_params();
        let first = self.ident_str().unwrap_or_default(); self.advance();
        self.skip_generic_params();
        let (type_name, trait_name) = if self.ident_str().as_deref() == Some("for") {
            self.advance();
            let tn = self.ident_str().unwrap_or_default(); self.advance();
            self.skip_generic_params();
            (tn, Some(first))
        } else { (first, None) };
        self.skip_where_clause();
        self.expect(RsTok::LBrace);
        let mut methods = Vec::new();
        while self.peek() != &RsTok::RBrace && !self.at_eof() {
            while self.ident_str().as_deref() == Some("pub") { self.advance(); }
            if let Some(s) = self.parse_item() { methods.push(s); }
        }
        self.expect(RsTok::RBrace);
        Some(RsStmt::Impl(type_name, trait_name, methods))
    }

    fn parse_trait_item(&mut self) -> Option<RsStmt> {
        self.advance(); // trait
        let name = self.ident_str().unwrap_or_default(); self.advance();
        self.skip_generic_params();
        // Skip supertraits
        if self.peek() == &RsTok::Colon {
            self.advance();
            while self.peek() != &RsTok::LBrace && !self.at_eof() { self.advance(); }
        }
        self.skip_where_clause();
        self.expect(RsTok::LBrace);
        let mut methods = Vec::new();
        while self.peek() != &RsTok::RBrace && !self.at_eof() {
            if let Some(s) = self.parse_item() { methods.push(s); }
        }
        self.expect(RsTok::RBrace);
        Some(RsStmt::Trait(name, methods))
    }

    fn parse_block(&mut self) -> Vec<RsStmt> {
        let mut stmts = Vec::new();
        self.expect(RsTok::LBrace);
        while self.peek() != &RsTok::RBrace && !self.at_eof() {
            while self.ident_str().as_deref() == Some("pub") { self.advance(); }
            if let Some(s) = self.parse_stmt() { stmts.push(s); }
            while self.peek() == &RsTok::Semi { self.advance(); }
        }
        self.expect(RsTok::RBrace);
        stmts
    }

    fn parse_stmt(&mut self) -> Option<RsStmt> {
        match self.peek().clone() {
            RsTok::Ident(ref name) if name == "let" => {
                self.advance();
                let is_mut = self.ident_str().as_deref() == Some("mut");
                if is_mut { self.advance(); }
                let vname = self.ident_str().unwrap_or("_".to_string()); self.advance();
                // Skip type annotation
                if self.peek() == &RsTok::Colon { self.advance(); self.parse_type(); }
                let init = if self.peek() == &RsTok::Eq { self.advance(); Some(self.parse_expr()) } else { None };
                self.expect(RsTok::Semi);
                Some(RsStmt::Let(vname, is_mut, init))
            }
            RsTok::Ident(ref name) if name == "if" => {
                self.advance();
                let cond = self.parse_expr();
                let then = self.parse_block();
                let else_b = if self.ident_str().as_deref() == Some("else") {
                    self.advance();
                    if self.ident_str().as_deref() == Some("if") {
                        Some(vec![self.parse_stmt()?])
                    } else { Some(self.parse_block()) }
                } else { None };
                Some(RsStmt::If(cond, then, else_b))
            }
            RsTok::Ident(ref name) if name == "while" => {
                self.advance();
                let cond = self.parse_expr();
                let body = self.parse_block();
                Some(RsStmt::While(cond, body))
            }
            RsTok::Ident(ref name) if name == "loop" => {
                self.advance();
                let body = self.parse_block();
                Some(RsStmt::Loop(body))
            }
            RsTok::Ident(ref name) if name == "for" => {
                self.advance();
                let var = self.ident_str().unwrap_or("_".to_string()); self.advance();
                if self.ident_str().as_deref() == Some("in") { self.advance(); }
                let iter = self.parse_expr();
                let body = self.parse_block();
                Some(RsStmt::For(var, iter, body))
            }
            RsTok::Ident(ref name) if name == "match" => {
                self.advance();
                let expr = self.parse_expr();
                self.expect(RsTok::LBrace);
                let mut arms = Vec::new();
                while self.peek() != &RsTok::RBrace && !self.at_eof() {
                    let mut patterns = Vec::new();
                    let mut bindings = Vec::new();
                    // Parse pattern
                    while self.peek() != &RsTok::FatArrow && !self.at_eof() {
                        if let Some(p) = self.ident_str() { patterns.push(p); self.advance(); }
                        else if self.peek() == &RsTok::ColonColon { self.advance(); }
                        else if self.peek() == &RsTok::LParen {
                            self.advance();
                            while self.peek() != &RsTok::RParen && !self.at_eof() {
                                if let Some(b) = self.ident_str() { bindings.push(b); self.advance(); }
                                if self.peek() == &RsTok::Comma { self.advance(); }
                                else if self.peek() != &RsTok::RParen { self.advance(); }
                            }
                            self.expect(RsTok::RParen);
                        }
                        else if self.peek() == &RsTok::Pipe { self.advance(); }
                        else { self.advance(); }
                    }
                    self.expect(RsTok::FatArrow);
                    let body = if self.peek() == &RsTok::LBrace {
                        self.parse_block()
                    } else {
                        let e = self.parse_expr();
                        vec![RsStmt::Expr(e)]
                    };
                    arms.push((patterns, bindings, body));
                    if self.peek() == &RsTok::Comma { self.advance(); }
                }
                self.expect(RsTok::RBrace);
                Some(RsStmt::Match(expr, arms))
            }
            RsTok::Ident(ref name) if name == "return" => {
                self.advance();
                let val = if self.peek() != &RsTok::Semi && self.peek() != &RsTok::RBrace && !self.at_eof() {
                    Some(self.parse_expr())
                } else { None };
                self.expect(RsTok::Semi);
                Some(RsStmt::Return(val))
            }
            RsTok::Ident(ref name) if name == "break" => { self.advance(); self.expect(RsTok::Semi); Some(RsStmt::Break) }
            RsTok::Ident(ref name) if name == "continue" => { self.advance(); self.expect(RsTok::Semi); Some(RsStmt::Continue) }
            RsTok::Ident(ref name) if name == "fn" || name == "async" => self.parse_fn_item(),
            RsTok::Ident(ref name) if name == "struct" => self.parse_struct_item(),
            RsTok::Ident(ref name) if name == "enum" => self.parse_enum_item(),
            RsTok::Ident(ref name) if name == "impl" => self.parse_impl_item(),
            _ => {
                let expr = self.parse_expr();
                match self.peek().clone() {
                    RsTok::Eq => { self.advance(); let v = self.parse_expr(); self.expect(RsTok::Semi); Some(RsStmt::Assign(expr, v)) }
                    RsTok::PlusEq => { self.advance(); let v = self.parse_expr(); self.expect(RsTok::Semi); Some(RsStmt::AugAssign(expr, "+".to_string(), v)) }
                    RsTok::MinusEq => { self.advance(); let v = self.parse_expr(); self.expect(RsTok::Semi); Some(RsStmt::AugAssign(expr, "-".to_string(), v)) }
                    RsTok::StarEq => { self.advance(); let v = self.parse_expr(); self.expect(RsTok::Semi); Some(RsStmt::AugAssign(expr, "*".to_string(), v)) }
                    RsTok::SlashEq => { self.advance(); let v = self.parse_expr(); self.expect(RsTok::Semi); Some(RsStmt::AugAssign(expr, "/".to_string(), v)) }
                    RsTok::Semi => { self.advance(); Some(RsStmt::Expr(expr)) }
                    _ => Some(RsStmt::Expr(expr)),
                }
            }
        }
    }

    fn parse_expr(&mut self) -> RsExpr { self.parse_or() }

    fn parse_or(&mut self) -> RsExpr {
        let mut left = self.parse_and();
        while self.peek() == &RsTok::PipePipe { self.advance(); let right = self.parse_and();
            left = RsExpr::Binary(Box::new(left), "||".to_string(), Box::new(right)); }
        left
    }
    fn parse_and(&mut self) -> RsExpr {
        let mut left = self.parse_cmp();
        while self.peek() == &RsTok::AmpAmp { self.advance(); let right = self.parse_cmp();
            left = RsExpr::Binary(Box::new(left), "&&".to_string(), Box::new(right)); }
        left
    }
    fn parse_cmp(&mut self) -> RsExpr {
        let mut left = self.parse_add();
        loop {
            let op = match self.peek() {
                RsTok::EqEq => "==", RsTok::NotEq => "!=",
                RsTok::Lt => "<", RsTok::Gt => ">",
                RsTok::LtEq => "<=", RsTok::GtEq => ">=",
                _ => break,
            }; self.advance(); let right = self.parse_add();
            left = RsExpr::Binary(Box::new(left), op.to_string(), Box::new(right));
        }
        left
    }
    fn parse_add(&mut self) -> RsExpr {
        let mut left = self.parse_mul();
        loop {
            let op = match self.peek() { RsTok::Plus => "+", RsTok::Minus => "-", _ => break };
            self.advance(); let right = self.parse_mul();
            left = RsExpr::Binary(Box::new(left), op.to_string(), Box::new(right));
        }
        left
    }
    fn parse_mul(&mut self) -> RsExpr {
        let mut left = self.parse_unary();
        loop {
            let op = match self.peek() { RsTok::Star => "*", RsTok::Slash => "/", RsTok::Percent => "%", _ => break };
            self.advance(); let right = self.parse_unary();
            left = RsExpr::Binary(Box::new(left), op.to_string(), Box::new(right));
        }
        left
    }
    fn parse_unary(&mut self) -> RsExpr {
        match self.peek().clone() {
            RsTok::Bang => { self.advance(); RsExpr::Unary("!".to_string(), Box::new(self.parse_unary())) }
            RsTok::Minus => { self.advance(); RsExpr::Unary("-".to_string(), Box::new(self.parse_unary())) }
            RsTok::Amp => {
                self.advance();
                if self.ident_str().as_deref() == Some("mut") { self.advance(); }
                RsExpr::Ref(Box::new(self.parse_unary()))
            }
            RsTok::Star => { self.advance(); RsExpr::Deref(Box::new(self.parse_unary())) }
            _ => self.parse_postfix(),
        }
    }
    fn parse_postfix(&mut self) -> RsExpr {
        let mut expr = self.parse_primary();
        loop {
            match self.peek().clone() {
                RsTok::Dot => {
                    self.advance();
                    if self.ident_str().as_deref() == Some("await") { self.advance(); expr = RsExpr::Await(Box::new(expr)); continue; }
                    let field = self.ident_str().unwrap_or_default(); self.advance();
                    if self.peek() == &RsTok::LParen {
                        self.advance();
                        let mut args = Vec::new();
                        while self.peek() != &RsTok::RParen && !self.at_eof() {
                            args.push(self.parse_expr());
                            if self.peek() == &RsTok::Comma { self.advance(); }
                        }
                        self.expect(RsTok::RParen);
                        expr = RsExpr::MethodCall(Box::new(expr), field, args);
                    } else { expr = RsExpr::FieldAccess(Box::new(expr), field); }
                }
                RsTok::LParen => {
                    self.advance();
                    let mut args = Vec::new();
                    while self.peek() != &RsTok::RParen && !self.at_eof() {
                        args.push(self.parse_expr());
                        if self.peek() == &RsTok::Comma { self.advance(); }
                    }
                    self.expect(RsTok::RParen);
                    expr = RsExpr::Call(Box::new(expr), args);
                }
                RsTok::LBracket => {
                    self.advance();
                    let idx = self.parse_expr();
                    self.expect(RsTok::RBracket);
                    expr = RsExpr::Index(Box::new(expr), Box::new(idx));
                }
                RsTok::Question => { self.advance(); expr = RsExpr::Try(Box::new(expr)); }
                RsTok::ColonColon => {
                    self.advance();
                    if let Some(member) = self.ident_str() {
                        self.advance();
                        if self.peek() == &RsTok::LParen {
                            self.advance();
                            let mut args = Vec::new();
                            while self.peek() != &RsTok::RParen && !self.at_eof() {
                                args.push(self.parse_expr());
                                if self.peek() == &RsTok::Comma { self.advance(); }
                            }
                            self.expect(RsTok::RParen);
                            expr = RsExpr::Call(Box::new(expr), args);
                        } else {
                            expr = RsExpr::FieldAccess(Box::new(expr), member);
                        }
                    }
                }
                _ => break,
            }
        }
        expr
    }
    fn parse_primary(&mut self) -> RsExpr {
        match self.peek().clone() {
            RsTok::IntLit(n) => { self.advance(); RsExpr::Int(n) }
            RsTok::FloatLit(f) => { self.advance(); RsExpr::Float(f) }
            RsTok::StrLit(s) => { self.advance(); RsExpr::Str(s) }
            RsTok::CharLit(c) => { self.advance(); RsExpr::Char(c) }
            RsTok::Ident(ref name) if name == "true" => { self.advance(); RsExpr::Bool(true) }
            RsTok::Ident(ref name) if name == "false" => { self.advance(); RsExpr::Bool(false) }
            RsTok::Ident(ref name) if name == "None" || name == "none" => { self.advance(); RsExpr::None_ }
            RsTok::Ident(ref name) if name == "Some" => {
                self.advance();
                if self.peek() == &RsTok::LParen {
                    self.advance(); let v = self.parse_expr(); self.expect(RsTok::RParen); v
                } else { RsExpr::Name("Some".to_string()) }
            }
            RsTok::Ident(ref name) if name.ends_with('!') => {
                let macro_name = name.clone(); self.advance();
                if self.peek() == &RsTok::LParen || self.peek() == &RsTok::LBracket {
                    let close = if self.peek() == &RsTok::LParen { RsTok::RParen } else { RsTok::RBracket };
                    self.advance();
                    let mut args = Vec::new();
                    while self.peek() != &close && !self.at_eof() {
                        args.push(self.parse_expr());
                        if self.peek() == &RsTok::Comma { self.advance(); }
                    }
                    self.expect(close);
                    RsExpr::MacroCall(macro_name, args)
                } else { RsExpr::Name(macro_name) }
            }
            RsTok::Ident(ref name) if name == "if" => {
                self.advance();
                let cond = self.parse_expr();
                let then = self.parse_block();
                let else_b = if self.ident_str().as_deref() == Some("else") {
                    self.advance();
                    Some(self.parse_block())
                } else { None };
                RsExpr::If(Box::new(cond), then, else_b)
            }
            RsTok::Ident(name) => {
                self.advance();
                // Check for struct literal: Name { field: val }
                if self.peek() == &RsTok::LBrace {
                    let saved = self.pos;
                    self.advance();
                    if let Some(_fname) = self.ident_str() {
                        let after_ident = self.pos + 1;
                        if self.tokens.get(after_ident).map_or(false, |t| t == &RsTok::Colon) {
                            // Struct literal
                            let mut fields = Vec::new();
                            while self.peek() != &RsTok::RBrace && !self.at_eof() {
                                let f = self.ident_str().unwrap_or_default(); self.advance();
                                self.expect(RsTok::Colon);
                                let v = self.parse_expr();
                                fields.push((f, v));
                                if self.peek() == &RsTok::Comma { self.advance(); }
                                if self.peek() == &RsTok::DotDot { self.advance(); self.parse_expr(); break; }
                            }
                            self.expect(RsTok::RBrace);
                            return RsExpr::StructLit(name, fields);
                        }
                    }
                    self.pos = saved;
                }
                RsExpr::Name(name)
            }
            RsTok::LParen => {
                self.advance();
                if self.peek() == &RsTok::RParen { self.advance(); return RsExpr::Tuple(Vec::new()); }
                let expr = self.parse_expr();
                if self.peek() == &RsTok::Comma {
                    let mut elems = vec![expr];
                    while self.peek() == &RsTok::Comma { self.advance(); if self.peek() != &RsTok::RParen { elems.push(self.parse_expr()); } }
                    self.expect(RsTok::RParen);
                    return RsExpr::Tuple(elems);
                }
                self.expect(RsTok::RParen);
                expr
            }
            RsTok::LBracket => {
                self.advance();
                let mut elems = Vec::new();
                while self.peek() != &RsTok::RBracket && !self.at_eof() {
                    elems.push(self.parse_expr());
                    if self.peek() == &RsTok::Comma { self.advance(); }
                    if self.peek() == &RsTok::Semi { self.advance(); self.parse_expr(); break; }
                }
                self.expect(RsTok::RBracket);
                RsExpr::Array(elems)
            }
            RsTok::Pipe => {
                self.advance();
                let mut params = Vec::new();
                while self.peek() != &RsTok::Pipe && !self.at_eof() {
                    if let Some(p) = self.ident_str() { params.push(p); self.advance(); }
                    if self.peek() == &RsTok::Colon { self.advance(); self.parse_type(); }
                    if self.peek() == &RsTok::Comma { self.advance(); }
                }
                self.expect(RsTok::Pipe);
                let body = if self.peek() == &RsTok::LBrace { self.parse_block() } else { vec![RsStmt::Expr(self.parse_expr())] };
                RsExpr::Closure(params, body)
            }
            RsTok::PipePipe => {
                self.advance(); // empty closure ||
                let body = if self.peek() == &RsTok::LBrace { self.parse_block() } else { vec![RsStmt::Expr(self.parse_expr())] };
                RsExpr::Closure(Vec::new(), body)
            }
            _ => { self.advance(); RsExpr::Int(0) }
        }
    }
}

// ============ Emitter ============

fn sanitize(name: &str) -> String {
    let clean = name.replace("::", "_");
    match clean.as_str() {
        "set" | "to" | "display" | "return" | "if" | "else" | "repeat" | "while" |
        "define" | "with" | "and" | "for" | "each" | "in" | "break" | "continue" |
        "check" | "structure" | "field" | "choice" | "variant" | "create" | "is" |
        "not" | "true" | "false" | "import" | "borrow" | "spawn" | "send" | "receive" |
        "from" | "channel" | "given" | "returning" | "as" | "trait" | "implement" |
        "equals" | "async" | "await" | "try" | "external" | "type" | "match" => format!("{}_", clean),
        _ => clean,
    }
}

fn ind(out: &mut String, depth: usize) { for _ in 0..depth { out.push_str("    "); } }

fn rs_type_to_ep(t: &str) -> &str {
    match t.trim_start_matches('&').trim_start_matches("mut ").trim_start_matches('*') {
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" |
        "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => "Int",
        "f32" | "f64" => "Float",
        "bool" => "Bool",
        "String" | "str" | "&str" => "Str",
        "Vec" => "List",
        _ => "Int",
    }
}

pub fn emit_ernos_from_rust(_filename: &str, source: &str) -> String {
    let mut lexer = RsLexer::new(source);
    let tokens = lexer.tokenize();
    let mut parser = RsParser::new(tokens);
    let stmts = parser.parse_file();
    let mut out = String::new();
    out.push_str("# Transpiled from Rust\n\n");
    for s in &stmts { emit_rs_stmt(&mut out, s, 0, None); }
    out
}

fn emit_rs_stmt(out: &mut String, stmt: &RsStmt, depth: usize, impl_type: Option<&str>) {
    match stmt {
        RsStmt::Use(path) => { ind(out, depth); out.push_str(&format!("# use {}\n", path)); }
        RsStmt::Mod(name) => { ind(out, depth); out.push_str(&format!("# mod {}\n", name)); }
        RsStmt::Fn(name, params, _ret, body) => {
            ind(out, depth);
            let sname = sanitize(name);
            if let Some(tname) = impl_type {
                if params.is_empty() { out.push_str(&format!("define method {} on {}:\n", sname, tname)); }
                else {
                    let p: Vec<String> = params.iter().map(|(n, t)| format!("{} as {}", sanitize(n), rs_type_to_ep(t))).collect();
                    out.push_str(&format!("define method {} on {} with {}:\n", sname, tname, p.join(" and ")));
                }
            } else if params.is_empty() { out.push_str(&format!("define {}:\n", sname)); }
            else {
                let p: Vec<String> = params.iter().map(|(n, t)| format!("{} as {}", sanitize(n), rs_type_to_ep(t))).collect();
                out.push_str(&format!("define {} with {}:\n", sname, p.join(" and ")));
            }
            for s in body { emit_rs_stmt(out, s, depth + 1, None); }
            if body.is_empty() { ind(out, depth + 1); out.push_str("return 0\n"); }
            out.push('\n');
        }
        RsStmt::Struct(name, fields) => {
            ind(out, depth); out.push_str(&format!("structure {}:\n", name));
            for (fname, ftype, _) in fields {
                ind(out, depth + 1); out.push_str(&format!("field {} as {}\n", sanitize(fname), rs_type_to_ep(ftype)));
            }
            out.push('\n');
        }
        RsStmt::Enum(name, variants) => {
            ind(out, depth); out.push_str(&format!("choice {}:\n", name));
            for (vname, fields) in variants {
                ind(out, depth + 1);
                if fields.is_empty() { out.push_str(&format!("variant {}\n", vname)); }
                else {
                    let p: Vec<String> = fields.iter().map(|(n, t)| format!("{} as {}", sanitize(n), rs_type_to_ep(t))).collect();
                    out.push_str(&format!("variant {} with {}\n", vname, p.join(" and ")));
                }
            }
            out.push('\n');
        }
        RsStmt::Impl(type_name, trait_name, methods) => {
            if let Some(tname) = trait_name {
                ind(out, depth); out.push_str(&format!("implement {} for {}:\n\n", tname, type_name));
            }
            for m in methods { emit_rs_stmt(out, m, depth, Some(type_name)); }
        }
        RsStmt::Trait(name, methods) => {
            ind(out, depth); out.push_str(&format!("trait {}:\n", name));
            for m in methods {
                if let RsStmt::Fn(mname, params, _, _) = m {
                    ind(out, depth + 1);
                    if params.is_empty() { out.push_str(&format!("define {}\n", sanitize(mname))); }
                    else {
                        let p: Vec<String> = params.iter().map(|(n, t)| format!("{} as {}", sanitize(n), rs_type_to_ep(t))).collect();
                        out.push_str(&format!("define {} with {}\n", sanitize(mname), p.join(" and ")));
                    }
                }
            }
            out.push('\n');
        }
        RsStmt::Let(name, _, init) => {
            ind(out, depth); out.push_str(&format!("set {} to ", sanitize(name)));
            if let Some(v) = init { emit_rs_expr(out, v); } else { out.push('0'); }
            out.push('\n');
        }
        RsStmt::Assign(target, val) => {
            ind(out, depth);
            if let RsExpr::Name(n) = target { out.push_str(&format!("set {} to ", sanitize(n))); }
            else if let RsExpr::FieldAccess(obj, f) = target {
                out.push_str("set "); emit_rs_expr(out, obj); out.push_str(&format!(".{} to ", f));
            }
            else { out.push_str("set _x to "); }
            emit_rs_expr(out, val);
            out.push('\n');
        }
        RsStmt::AugAssign(target, op, val) => {
            ind(out, depth);
            if let RsExpr::Name(n) = target {
                let sn = sanitize(n);
                out.push_str(&format!("set {0} to {0} {1} ", sn, op));
            } else { out.push_str("set _x to _x + "); }
            emit_rs_expr(out, val); out.push('\n');
        }
        RsStmt::If(cond, then, else_b) => {
            ind(out, depth); out.push_str("if "); emit_rs_expr(out, cond); out.push_str(":\n");
            for s in then { emit_rs_stmt(out, s, depth + 1, None); }
            if let Some(eb) = else_b {
                if eb.len() == 1 { if let RsStmt::If(..) = &eb[0] {
                    ind(out, depth); out.push_str("else ");
                    emit_rs_stmt_inline_if(out, &eb[0], depth);
                    return;
                }}
                ind(out, depth); out.push_str("else:\n");
                for s in eb { emit_rs_stmt(out, s, depth + 1, None); }
            }
        }
        RsStmt::While(cond, body) => {
            ind(out, depth); out.push_str("repeat while "); emit_rs_expr(out, cond); out.push_str(":\n");
            for s in body { emit_rs_stmt(out, s, depth + 1, None); }
        }
        RsStmt::Loop(body) => {
            ind(out, depth); out.push_str("repeat while true:\n");
            for s in body { emit_rs_stmt(out, s, depth + 1, None); }
        }
        RsStmt::For(var, iter, body) => {
            ind(out, depth); out.push_str(&format!("for each {} in ", sanitize(var)));
            emit_rs_expr(out, iter); out.push_str(":\n");
            for s in body { emit_rs_stmt(out, s, depth + 1, None); }
        }
        RsStmt::Match(expr, arms) => {
            ind(out, depth); out.push_str("check "); emit_rs_expr(out, expr); out.push_str(":\n");
            for (patterns, _bindings, body) in arms {
                let pat = if !patterns.is_empty() { patterns.join("_") } else { "default".to_string() };
                ind(out, depth + 1);
                if pat == "_" { out.push_str("if default:\n"); }
                else { out.push_str(&format!("if {}:\n", pat)); }
                for s in body { emit_rs_stmt(out, s, depth + 2, None); }
            }
        }
        RsStmt::Return(val) => {
            ind(out, depth); out.push_str("return ");
            if let Some(v) = val { emit_rs_expr(out, v); } else { out.push('0'); }
            out.push('\n');
        }
        RsStmt::Break => { ind(out, depth); out.push_str("break\n"); }
        RsStmt::Continue => { ind(out, depth); out.push_str("continue\n"); }
        RsStmt::Expr(expr) => {
            // Check for println!/print!
            if let RsExpr::MacroCall(name, args) = expr {
                if matches!(name.as_str(), "println!" | "print!" | "eprintln!") {
                    if args.is_empty() { ind(out, depth); out.push_str("display \"\"\n"); return; }
                    // First arg is format string
                    if let RsExpr::Str(fmt) = &args[0] {
                        if args.len() == 1 {
                            ind(out, depth);
                            out.push_str(&format!("display \"{}\"\n", fmt.replace('\n', "\\n")));
                        } else {
                            for a in &args[1..] {
                                ind(out, depth);
                                out.push_str("display ");
                                emit_rs_expr(out, a);
                                out.push('\n');
                            }
                        }
                    } else {
                        for a in args {
                            ind(out, depth); out.push_str("display "); emit_rs_expr(out, a); out.push('\n');
                        }
                    }
                    return;
                }
            }
            ind(out, depth); emit_rs_expr(out, expr); out.push('\n');
        }
        RsStmt::Const(name, _, val) | RsStmt::Static(name, _, val) => {
            ind(out, depth); out.push_str(&format!("set {} to ", sanitize(name)));
            emit_rs_expr(out, val); out.push('\n');
        }
    }
}

fn emit_rs_stmt_inline_if(out: &mut String, stmt: &RsStmt, depth: usize) {
    if let RsStmt::If(cond, then, else_b) = stmt {
        out.push_str("if "); emit_rs_expr(out, cond); out.push_str(":\n");
        for s in then { emit_rs_stmt(out, s, depth + 1, None); }
        if let Some(eb) = else_b {
            if eb.len() == 1 { if let RsStmt::If(..) = &eb[0] {
                ind(out, depth); out.push_str("else ");
                emit_rs_stmt_inline_if(out, &eb[0], depth);
                return;
            }}
            ind(out, depth); out.push_str("else:\n");
            for s in eb { emit_rs_stmt(out, s, depth + 1, None); }
        }
    }
}

fn emit_rs_expr(out: &mut String, expr: &RsExpr) {
    match expr {
        RsExpr::Int(n) => out.push_str(&n.to_string()),
        RsExpr::Float(f) => out.push_str(&format!("{}", f)),
        RsExpr::Str(s) => { out.push_str(&format!("\"{}\"", s.replace('"', "\\\"").replace('\n', "\\n"))); }
        RsExpr::Char(c) => out.push_str(&(*c as i64).to_string()),
        RsExpr::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        RsExpr::None_ => out.push('0'),
        RsExpr::Name(n) => out.push_str(&sanitize(n)),
        RsExpr::Path(parts) => out.push_str(&parts.join("_")),
        RsExpr::Binary(l, op, r) => {
            emit_rs_expr(out, l);
            let ep_op = match op.as_str() {
                "+" => " + ", "-" => " - ", "*" => " * ", "/" => " / ", "%" => " modulo ",
                "==" => " equals ", "!=" => " != ", "<" => " < ", ">" => " > ",
                "<=" => " <= ", ">=" => " >= ",
                "&&" => " and also ", "||" => " or else ",
                _ => { emit_rs_expr(out, l); out.push_str(&format!(" {} ", op)); emit_rs_expr(out, r); return; }
            };
            out.push_str(ep_op); emit_rs_expr(out, r);
        }
        RsExpr::Unary(op, inner) => {
            if op == "!" { out.push_str("not "); emit_rs_expr(out, inner); }
            else if op == "-" { out.push_str("(0 - "); emit_rs_expr(out, inner); out.push(')'); }
            else { emit_rs_expr(out, inner); }
        }
        RsExpr::Call(func, args) => {
            emit_rs_expr(out, func);
            out.push('(');
            for (i, a) in args.iter().enumerate() { if i > 0 { out.push_str(" and "); } emit_rs_expr(out, a); }
            out.push(')');
        }
        RsExpr::MethodCall(obj, method, args) => {
            match method.as_str() {
                "len" => { out.push_str("length("); emit_rs_expr(out, obj); out.push(')'); return; }
                "push" => { out.push_str("append("); emit_rs_expr(out, obj);
                    if !args.is_empty() { out.push_str(" and "); emit_rs_expr(out, &args[0]); }
                    out.push(')'); return; }
                "to_string" => { out.push_str("int_to_string("); emit_rs_expr(out, obj); out.push(')'); return; }
                "parse" => { out.push_str("string_to_int("); emit_rs_expr(out, obj); out.push(')'); return; }
                "contains" => { out.push_str("string_contains("); emit_rs_expr(out, obj);
                    if !args.is_empty() { out.push_str(" and "); emit_rs_expr(out, &args[0]); }
                    out.push(')'); return; }
                "unwrap" | "unwrap_or" | "expect" => { emit_rs_expr(out, obj); return; }
                _ => {}
            }
            emit_rs_expr(out, obj); out.push('.'); out.push_str(&sanitize(method));
            if !args.is_empty() {
                out.push('(');
                for (i, a) in args.iter().enumerate() { if i > 0 { out.push_str(" and "); } emit_rs_expr(out, a); }
                out.push(')');
            }
        }
        RsExpr::FieldAccess(obj, field) => { emit_rs_expr(out, obj); out.push('.'); out.push_str(field); }
        RsExpr::Index(arr, idx) => { out.push_str("get_list("); emit_rs_expr(out, arr); out.push_str(" and "); emit_rs_expr(out, idx); out.push(')'); }
        RsExpr::Array(elems) => { out.push('['); for (i, e) in elems.iter().enumerate() { if i > 0 { out.push_str(", "); } emit_rs_expr(out, e); } out.push(']'); }
        RsExpr::Tuple(elems) => { out.push('['); for (i, e) in elems.iter().enumerate() { if i > 0 { out.push_str(", "); } emit_rs_expr(out, e); } out.push(']'); }
        RsExpr::Range(start, end, _) => {
            out.push_str("range(");
            if let Some(s) = start { emit_rs_expr(out, s); } else { out.push('0'); }
            out.push_str(" and ");
            if let Some(e) = end { emit_rs_expr(out, e); } else { out.push_str("999999"); }
            out.push(')');
        }
        RsExpr::Closure(params, body) => {
            if params.is_empty() { out.push_str("given _:"); }
            else { out.push_str(&format!("given {}:", params.iter().map(|p| sanitize(p)).collect::<Vec<_>>().join(" and "))); }
            if body.len() == 1 { if let RsStmt::Expr(e) = &body[0] { out.push(' '); emit_rs_expr(out, e); return; } }
            out.push('\n');
            for s in body { emit_rs_stmt(out, s, 1, None); }
        }
        RsExpr::MacroCall(name, args) => {
            match name.as_str() {
                "vec!" => { out.push('['); for (i, a) in args.iter().enumerate() { if i > 0 { out.push_str(", "); } emit_rs_expr(out, a); } out.push(']'); }
                "format!" => {
                    if !args.is_empty() {
                        if let RsExpr::Str(fmt) = &args[0] { out.push_str(&format!("f\"{}\"", fmt.replace('\n', "\\n"))); }
                        else { out.push_str("\"\""); }
                    } else { out.push_str("\"\""); }
                }
                "panic!" | "todo!" | "unimplemented!" => {
                    out.push_str("# "); out.push_str(name);
                    if !args.is_empty() { out.push(' '); emit_rs_expr(out, &args[0]); }
                }
                _ => {
                    out.push_str(&sanitize(&name.replace('!', ""))); out.push('(');
                    for (i, a) in args.iter().enumerate() { if i > 0 { out.push_str(" and "); } emit_rs_expr(out, a); }
                    out.push(')');
                }
            }
        }
        RsExpr::Ref(inner) => { out.push_str("borrow "); emit_rs_expr(out, inner); }
        RsExpr::Deref(inner) => emit_rs_expr(out, inner),
        RsExpr::Try(inner) => { out.push_str("try "); emit_rs_expr(out, inner); }
        RsExpr::Await(inner) => { out.push_str("await "); emit_rs_expr(out, inner); }
        RsExpr::StructLit(name, fields) => {
            out.push_str(&format!("create {}:", name));
            for (fname, fval) in fields { out.push_str(&format!("\n    {} is ", fname)); emit_rs_expr(out, fval); }
        }
        RsExpr::Block(stmts) => { for s in stmts { emit_rs_stmt(out, s, 0, None); } }
        RsExpr::If(cond, then, _else_b) => {
            out.push_str("if "); emit_rs_expr(out, cond); out.push_str(": ");
            if !then.is_empty() { if let RsStmt::Expr(e) = &then[0] { emit_rs_expr(out, e); } }
        }
    }
}
