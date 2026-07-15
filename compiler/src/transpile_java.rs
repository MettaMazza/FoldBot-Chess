/// Java → ErnosPlain Transpiler
///
/// Parses a subset of Java source code and emits ErnosPlain (.ep) code.

#[derive(Debug, Clone, PartialEq)]
pub enum JaTok {
    Ident(String), IntLit(i64), FloatLit(f64), StrLit(String), CharLit(char),
    Plus, Minus, Star, Slash, Percent,
    PlusPlus, MinusMinus,
    PlusEq, MinusEq, StarEq, SlashEq, PercentEq,
    AmpAmp, PipePipe, Bang,
    Eq, EqEq, NotEq, Lt, Gt, LtEq, GtEq,
    Amp, Pipe, Caret, Tilde, LShift, RShift,
    Arrow, ColonColon, Question, Colon,
    LParen, RParen, LBracket, RBracket, LBrace, RBrace,
    Comma, Semi, Dot, At, Ellipsis,
    Eof,
}

pub struct JaLexer { chars: Vec<char>, pos: usize }
impl JaLexer {
    pub fn new(input: &str) -> Self { JaLexer { chars: input.chars().collect(), pos: 0 } }
    fn peek(&self) -> Option<char> { self.chars.get(self.pos).copied() }
    fn advance(&mut self) -> Option<char> { let c = self.chars.get(self.pos).copied(); self.pos += 1; c }
    fn peek_at(&self, off: usize) -> Option<char> { self.chars.get(self.pos + off).copied() }

    pub fn tokenize(&mut self) -> Vec<JaTok> {
        let mut tokens = Vec::new();
        loop {
            while matches!(self.peek(), Some(' ') | Some('\t') | Some('\r') | Some('\n')) { self.advance(); }
            match self.peek() {
                None => break,
                Some('/') if self.peek_at(1) == Some('/') => { while self.peek() != Some('\n') && self.peek().is_some() { self.advance(); } }
                Some('/') if self.peek_at(1) == Some('*') => {
                    self.advance(); self.advance();
                    loop { match self.advance() { Some('*') if self.peek() == Some('/') => { self.advance(); break; } None => break, _ => {} } }
                }
                Some('@') => { self.advance(); tokens.push(JaTok::At);
                    // Read annotation name and skip params
                    let mut name = String::new();
                    while let Some(c) = self.peek() { if c.is_ascii_alphanumeric() || c == '_' || c == '.' { name.push(c); self.advance(); } else { break; } }
                    if self.peek() == Some('(') { self.advance(); let mut d = 1;
                        while d > 0 { match self.advance() { Some('(') => d += 1, Some(')') => d -= 1, None => break, _ => {} } }
                    }
                    continue;
                }
                Some('"') => {
                    self.advance();
                    let mut s = String::new();
                    loop { match self.advance() { Some('"') => break, Some('\\') => { match self.advance() { Some('n') => s.push('\n'), Some('t') => s.push('\t'), Some('\\') => s.push('\\'), Some('"') => s.push('"'), Some(c) => { s.push('\\'); s.push(c); } None => break } } Some(c) => s.push(c), None => break } }
                    tokens.push(JaTok::StrLit(s));
                }
                Some('\'') => {
                    self.advance();
                    let c = if self.peek() == Some('\\') { self.advance(); match self.advance() { Some('n') => '\n', Some('t') => '\t', Some('\\') => '\\', Some('\'') => '\'', Some(c) => c, None => ' ' } }
                    else { self.advance().unwrap_or(' ') };
                    if self.peek() == Some('\'') { self.advance(); }
                    tokens.push(JaTok::CharLit(c));
                }
                Some(c) if c.is_ascii_digit() => {
                    let mut n = String::new(); let mut is_float = false;
                    while let Some(ch) = self.peek() {
                        if ch.is_ascii_digit() || ch == '_' { n.push(ch); self.advance(); }
                        else if ch == '.' && !is_float { is_float = true; n.push(ch); self.advance(); }
                        else if (ch == 'x' || ch == 'X') && n == "0" { n.push(ch); self.advance(); }
                        else if ch.is_ascii_hexdigit() && (n.starts_with("0x") || n.starts_with("0X")) { n.push(ch); self.advance(); }
                        else { break; }
                    }
                    // Skip type suffix L, F, D, etc.
                    if matches!(self.peek(), Some('L') | Some('l') | Some('F') | Some('f') | Some('D') | Some('d')) { self.advance(); }
                    let clean: String = n.replace('_', "");
                    if is_float { tokens.push(JaTok::FloatLit(clean.parse().unwrap_or(0.0))); }
                    else if clean.starts_with("0x") || clean.starts_with("0X") { tokens.push(JaTok::IntLit(i64::from_str_radix(&clean[2..], 16).unwrap_or(0))); }
                    else { tokens.push(JaTok::IntLit(clean.parse().unwrap_or(0))); }
                }
                Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$' => {
                    let mut ident = String::new();
                    while let Some(ch) = self.peek() { if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' { ident.push(ch); self.advance(); } else { break; } }
                    tokens.push(JaTok::Ident(ident));
                }
                _ => { tokens.push(self.lex_punct()); }
            }
        }
        tokens.push(JaTok::Eof); tokens
    }

    fn lex_punct(&mut self) -> JaTok {
        let c = self.advance().unwrap();
        match c {
            '+' => if self.peek() == Some('+') { self.advance(); JaTok::PlusPlus } else if self.peek() == Some('=') { self.advance(); JaTok::PlusEq } else { JaTok::Plus },
            '-' => if self.peek() == Some('-') { self.advance(); JaTok::MinusMinus } else if self.peek() == Some('=') { self.advance(); JaTok::MinusEq } else if self.peek() == Some('>') { self.advance(); JaTok::Arrow } else { JaTok::Minus },
            '*' => if self.peek() == Some('=') { self.advance(); JaTok::StarEq } else { JaTok::Star },
            '/' => if self.peek() == Some('=') { self.advance(); JaTok::SlashEq } else { JaTok::Slash },
            '%' => if self.peek() == Some('=') { self.advance(); JaTok::PercentEq } else { JaTok::Percent },
            '&' => if self.peek() == Some('&') { self.advance(); JaTok::AmpAmp } else { JaTok::Amp },
            '|' => if self.peek() == Some('|') { self.advance(); JaTok::PipePipe } else { JaTok::Pipe },
            '^' => JaTok::Caret, '~' => JaTok::Tilde,
            '!' => if self.peek() == Some('=') { self.advance(); JaTok::NotEq } else { JaTok::Bang },
            '=' => if self.peek() == Some('=') { self.advance(); JaTok::EqEq } else { JaTok::Eq },
            '<' => if self.peek() == Some('=') { self.advance(); JaTok::LtEq } else if self.peek() == Some('<') { self.advance(); JaTok::LShift } else { JaTok::Lt },
            '>' => if self.peek() == Some('=') { self.advance(); JaTok::GtEq } else if self.peek() == Some('>') { self.advance(); JaTok::RShift } else { JaTok::Gt },
            ':' => if self.peek() == Some(':') { self.advance(); JaTok::ColonColon } else { JaTok::Colon },
            '(' => JaTok::LParen, ')' => JaTok::RParen, '[' => JaTok::LBracket, ']' => JaTok::RBracket,
            '{' => JaTok::LBrace, '}' => JaTok::RBrace,
            ',' => JaTok::Comma, ';' => JaTok::Semi, '.' => {
                if self.peek() == Some('.') && self.peek_at(1) == Some('.') { self.advance(); self.advance(); JaTok::Ellipsis } else { JaTok::Dot }
            },
            '?' => JaTok::Question,
            _ => JaTok::Eof,
        }
    }
}

#[derive(Debug, Clone)]
pub enum JaExpr {
    Int(i64), Float(f64), Str(String), Char(char), Bool(bool), Null,
    Name(String),
    Binary(Box<JaExpr>, String, Box<JaExpr>),
    Unary(String, Box<JaExpr>),
    Call(Box<JaExpr>, Vec<JaExpr>),
    Member(Box<JaExpr>, String),
    Index(Box<JaExpr>, Box<JaExpr>),
    New(String, Vec<JaExpr>),
    NewArray(String, Vec<JaExpr>),
    ArrayInit(Vec<JaExpr>),
    Cast(String, Box<JaExpr>),
    Ternary(Box<JaExpr>, Box<JaExpr>, Box<JaExpr>),
    Lambda(Vec<String>, Vec<JaStmt>),
    InstanceOf(Box<JaExpr>, String),
}

#[derive(Debug, Clone)]
pub enum JaStmt {
    Class(String, Option<String>, Vec<JaStmt>),
    Method(Vec<String>, String, String, Vec<(String, String)>, Vec<JaStmt>),  // modifiers, ret_type, name, params, body
    Field(Vec<String>, String, String, Option<JaExpr>),  // modifiers, type, name, init
    VarDecl(String, String, Option<JaExpr>),
    Assign(JaExpr, JaExpr),
    AugAssign(JaExpr, String, JaExpr),
    IncDec(JaExpr, String),
    If(JaExpr, Vec<JaStmt>, Option<Vec<JaStmt>>),
    While(JaExpr, Vec<JaStmt>),
    DoWhile(Vec<JaStmt>, JaExpr),
    For(Option<Box<JaStmt>>, Option<JaExpr>, Option<Box<JaStmt>>, Vec<JaStmt>),
    ForEach(String, String, JaExpr, Vec<JaStmt>),
    Switch(JaExpr, Vec<(Vec<JaExpr>, Vec<JaStmt>)>, Option<Vec<JaStmt>>),
    Return(Option<JaExpr>),
    Expr(JaExpr),
    Break, Continue,
    Import(String),
    Package(String),
    TryCatch(Vec<JaStmt>, Vec<(String, String, Vec<JaStmt>)>, Option<Vec<JaStmt>>),
    Throw(JaExpr),
    Enum(String, Vec<String>),
    Interface(String, Vec<JaStmt>),
    Block(Vec<JaStmt>),
}

pub struct JaParser { tokens: Vec<JaTok>, pos: usize }
impl JaParser {
    pub fn new(tokens: Vec<JaTok>) -> Self { JaParser { tokens, pos: 0 } }
    fn peek(&self) -> &JaTok { self.tokens.get(self.pos).unwrap_or(&JaTok::Eof) }
    fn advance(&mut self) -> JaTok { let t = self.tokens.get(self.pos).cloned().unwrap_or(JaTok::Eof); self.pos += 1; t }
    fn expect(&mut self, tok: JaTok) { if self.peek() == &tok { self.advance(); } }
    fn at_eof(&self) -> bool { self.peek() == &JaTok::Eof }
    fn ident_str(&self) -> Option<String> { if let JaTok::Ident(s) = self.peek() { Some(s.clone()) } else { None } }

    fn is_modifier(name: &str) -> bool {
        matches!(name, "public" | "private" | "protected" | "static" | "final" | "abstract" |
            "synchronized" | "volatile" | "transient" | "native" | "strictfp" | "default" | "sealed" | "record")
    }

    fn is_type(name: &str) -> bool {
        matches!(name, "void" | "int" | "long" | "short" | "byte" | "char" | "float" | "double" | "boolean" | "String" | "Object" |
            "Integer" | "Long" | "Float" | "Double" | "Boolean" | "Byte" | "Short" | "Character" | "Void" |
            "List" | "ArrayList" | "Map" | "HashMap" | "Set" | "HashSet")
    }

    fn skip_generic_params(&mut self) {
        if self.peek() == &JaTok::Lt { self.advance(); let mut d = 1;
            while d > 0 && !self.at_eof() { match self.peek() { JaTok::Lt => { d += 1; self.advance(); } JaTok::Gt => { d -= 1; self.advance(); } _ => { self.advance(); } } }
        }
    }

    fn consume_modifiers(&mut self) -> Vec<String> {
        let mut mods = Vec::new();
        while let Some(name) = self.ident_str() { if Self::is_modifier(&name) { mods.push(name); self.advance(); } else { break; } }
        mods
    }

    fn consume_type(&mut self) -> String {
        let mut t = self.ident_str().unwrap_or("void".to_string()); self.advance();
        self.skip_generic_params();
        while self.peek() == &JaTok::LBracket { self.advance(); self.expect(JaTok::RBracket); t.push_str("[]"); }
        while self.peek() == &JaTok::Dot { self.advance(); if let Some(n) = self.ident_str() { t.push('.'); t.push_str(&n); self.advance(); self.skip_generic_params(); } }
        t
    }

    pub fn parse_file(&mut self) -> Vec<JaStmt> {
        let mut stmts = Vec::new();
        while !self.at_eof() {
            if self.peek() == &JaTok::At { self.advance(); while self.peek() != &JaTok::Eof && self.peek() != &JaTok::Eof { break; } continue; }
            if let Some(s) = self.parse_top_level() { stmts.push(s); }
        }
        stmts
    }

    fn parse_top_level(&mut self) -> Option<JaStmt> {
        match self.peek().clone() {
            JaTok::Ident(ref name) if name == "package" => {
                self.advance(); let mut pkg = String::new();
                while self.peek() != &JaTok::Semi && !self.at_eof() { if let Some(n) = self.ident_str() { pkg.push_str(&n); self.advance(); } else if self.peek() == &JaTok::Dot { pkg.push('.'); self.advance(); } else { self.advance(); } }
                self.expect(JaTok::Semi);
                Some(JaStmt::Package(pkg))
            }
            JaTok::Ident(ref name) if name == "import" => {
                self.advance(); let mut path = String::new();
                if self.ident_str().as_deref() == Some("static") { self.advance(); path.push_str("static "); }
                while self.peek() != &JaTok::Semi && !self.at_eof() { if let Some(n) = self.ident_str() { path.push_str(&n); self.advance(); } else if self.peek() == &JaTok::Dot { path.push('.'); self.advance(); } else if self.peek() == &JaTok::Star { path.push('*'); self.advance(); } else { self.advance(); } }
                self.expect(JaTok::Semi);
                Some(JaStmt::Import(path))
            }
            _ => {
                let mods = self.consume_modifiers();
                match self.peek().clone() {
                    JaTok::Ident(ref name) if name == "class" => {
                        self.advance();
                        let cname = self.ident_str().unwrap_or_default(); self.advance();
                        self.skip_generic_params();
                        let parent = if self.ident_str().as_deref() == Some("extends") { self.advance(); let p = self.ident_str(); self.advance(); self.skip_generic_params(); p } else { None };
                        if self.ident_str().as_deref() == Some("implements") { self.advance(); while self.peek() != &JaTok::LBrace && !self.at_eof() { self.advance(); } }
                        let body = self.parse_class_body();
                        Some(JaStmt::Class(cname, parent, body))
                    }
                    JaTok::Ident(ref name) if name == "interface" => {
                        self.advance();
                        let iname = self.ident_str().unwrap_or_default(); self.advance();
                        self.skip_generic_params();
                        if self.ident_str().as_deref() == Some("extends") { self.advance(); while self.peek() != &JaTok::LBrace && !self.at_eof() { self.advance(); } }
                        let body = self.parse_class_body();
                        Some(JaStmt::Interface(iname, body))
                    }
                    JaTok::Ident(ref name) if name == "enum" => {
                        self.advance();
                        let ename = self.ident_str().unwrap_or_default(); self.advance();
                        self.expect(JaTok::LBrace);
                        let mut variants = Vec::new();
                        while self.peek() != &JaTok::RBrace && self.peek() != &JaTok::Semi && !self.at_eof() {
                            if let Some(v) = self.ident_str() { variants.push(v); self.advance(); }
                            if self.peek() == &JaTok::LParen { self.advance(); while self.peek() != &JaTok::RParen && !self.at_eof() { self.advance(); } self.expect(JaTok::RParen); }
                            if self.peek() == &JaTok::Comma { self.advance(); }
                        }
                        while self.peek() != &JaTok::RBrace && !self.at_eof() { self.advance(); }
                        self.expect(JaTok::RBrace);
                        Some(JaStmt::Enum(ename, variants))
                    }
                    _ => {
                        // Could be field or method
                        if self.peek() == &JaTok::LBrace { let body = self.parse_block(); return Some(JaStmt::Block(body)); }
                        let ret_type = self.consume_type();
                        let name = self.ident_str().unwrap_or_default(); self.advance();
                        if self.peek() == &JaTok::LParen {
                            // Method
                            let params = self.parse_params();
                            if self.ident_str().as_deref() == Some("throws") { self.advance(); while self.peek() != &JaTok::LBrace && self.peek() != &JaTok::Semi && !self.at_eof() { self.advance(); } }
                            let body = if self.peek() == &JaTok::LBrace { self.parse_block() } else { self.expect(JaTok::Semi); Vec::new() };
                            Some(JaStmt::Method(mods, ret_type, name, params, body))
                        } else {
                            // Field
                            let init = if self.peek() == &JaTok::Eq { self.advance(); Some(self.parse_expr()) } else { None };
                            self.expect(JaTok::Semi);
                            Some(JaStmt::Field(mods, ret_type, name, init))
                        }
                    }
                }
            }
        }
    }

    fn parse_class_body(&mut self) -> Vec<JaStmt> {
        let mut stmts = Vec::new();
        self.expect(JaTok::LBrace);
        while self.peek() != &JaTok::RBrace && !self.at_eof() {
            if self.peek() == &JaTok::At { self.advance();
                if let Some(_) = self.ident_str() { self.advance(); }
                if self.peek() == &JaTok::LParen { self.advance(); let mut d = 1; while d > 0 { match self.advance() { JaTok::LParen => d += 1, JaTok::RParen => d -= 1, _ => {} } } }
                continue;
            }
            if let Some(s) = self.parse_top_level() { stmts.push(s); }
        }
        self.expect(JaTok::RBrace); stmts
    }

    fn parse_params(&mut self) -> Vec<(String, String)> {
        let mut params = Vec::new();
        self.expect(JaTok::LParen);
        while self.peek() != &JaTok::RParen && !self.at_eof() {
            let _ = self.consume_modifiers();
            let ptype = self.consume_type();
            if self.peek() == &JaTok::Ellipsis { self.advance(); }
            let pname = self.ident_str().unwrap_or_default(); self.advance();
            params.push((pname, ptype));
            if self.peek() == &JaTok::Comma { self.advance(); }
        }
        self.expect(JaTok::RParen); params
    }

    fn parse_block(&mut self) -> Vec<JaStmt> {
        let mut stmts = Vec::new();
        self.expect(JaTok::LBrace);
        while self.peek() != &JaTok::RBrace && !self.at_eof() {
            if let Some(s) = self.parse_stmt() { stmts.push(s); }
        }
        self.expect(JaTok::RBrace); stmts
    }

    fn parse_stmt(&mut self) -> Option<JaStmt> {
        match self.peek().clone() {
            JaTok::Ident(ref name) if name == "if" => {
                self.advance(); self.expect(JaTok::LParen);
                let cond = self.parse_expr(); self.expect(JaTok::RParen);
                let then = if self.peek() == &JaTok::LBrace { self.parse_block() } else { let s = self.parse_stmt()?; vec![s] };
                let else_b = if self.ident_str().as_deref() == Some("else") { self.advance();
                    if self.peek() == &JaTok::LBrace { Some(self.parse_block()) } else { Some(vec![self.parse_stmt()?]) }
                } else { None };
                Some(JaStmt::If(cond, then, else_b))
            }
            JaTok::Ident(ref name) if name == "while" => {
                self.advance(); self.expect(JaTok::LParen); let cond = self.parse_expr(); self.expect(JaTok::RParen);
                let body = if self.peek() == &JaTok::LBrace { self.parse_block() } else { vec![self.parse_stmt()?] };
                Some(JaStmt::While(cond, body))
            }
            JaTok::Ident(ref name) if name == "for" => {
                self.advance(); self.expect(JaTok::LParen);
                // Check for enhanced for
                let saved = self.pos;
                let _ = self.consume_modifiers();
                if let Some(type_name) = self.ident_str() {
                    self.advance(); self.skip_generic_params();
                    while self.peek() == &JaTok::LBracket { self.advance(); self.expect(JaTok::RBracket); }
                    if let Some(var_name) = self.ident_str() {
                        self.advance();
                        if self.peek() == &JaTok::Colon {
                            self.advance();
                            let iter = self.parse_expr(); self.expect(JaTok::RParen);
                            let body = if self.peek() == &JaTok::LBrace { self.parse_block() } else { vec![self.parse_stmt()?] };
                            return Some(JaStmt::ForEach(type_name, var_name, iter, body));
                        }
                    }
                }
                self.pos = saved;
                // C-style for
                let init = if self.peek() != &JaTok::Semi { self.parse_stmt().map(Box::new) } else { self.advance(); None };
                let cond = if self.peek() != &JaTok::Semi { Some(self.parse_expr()) } else { None }; self.expect(JaTok::Semi);
                let post = if self.peek() != &JaTok::RParen { self.parse_stmt().map(Box::new) } else { None };
                self.expect(JaTok::RParen);
                let body = if self.peek() == &JaTok::LBrace { self.parse_block() } else { vec![self.parse_stmt()?] };
                Some(JaStmt::For(init, cond, post, body))
            }
            JaTok::Ident(ref name) if name == "return" => {
                self.advance();
                let val = if self.peek() != &JaTok::Semi { Some(self.parse_expr()) } else { None };
                self.expect(JaTok::Semi); Some(JaStmt::Return(val))
            }
            JaTok::Ident(ref name) if name == "break" => { self.advance(); self.expect(JaTok::Semi); Some(JaStmt::Break) }
            JaTok::Ident(ref name) if name == "continue" => { self.advance(); self.expect(JaTok::Semi); Some(JaStmt::Continue) }
            JaTok::Ident(ref name) if name == "switch" => {
                self.advance(); self.expect(JaTok::LParen); let tag = self.parse_expr(); self.expect(JaTok::RParen);
                self.expect(JaTok::LBrace);
                let mut cases = Vec::new(); let mut default = None;
                while self.peek() != &JaTok::RBrace && !self.at_eof() {
                    if self.ident_str().as_deref() == Some("case") {
                        self.advance(); let mut vals = Vec::new(); vals.push(self.parse_expr()); self.expect(JaTok::Colon);
                        let mut body = Vec::new();
                        while !self.at_eof() { if self.ident_str().as_deref() == Some("case") || self.ident_str().as_deref() == Some("default") || self.peek() == &JaTok::RBrace { break; }
                            if let Some(s) = self.parse_stmt() { body.push(s); } }
                        cases.push((vals, body));
                    } else if self.ident_str().as_deref() == Some("default") {
                        self.advance(); self.expect(JaTok::Colon);
                        let mut body = Vec::new();
                        while !self.at_eof() { if self.ident_str().as_deref() == Some("case") || self.peek() == &JaTok::RBrace { break; }
                            if let Some(s) = self.parse_stmt() { body.push(s); } }
                        default = Some(body);
                    } else { self.advance(); }
                }
                self.expect(JaTok::RBrace); Some(JaStmt::Switch(tag, cases, default))
            }
            JaTok::Ident(ref name) if name == "try" => {
                self.advance(); let body = self.parse_block();
                let mut catches = Vec::new();
                while self.ident_str().as_deref() == Some("catch") {
                    self.advance(); self.expect(JaTok::LParen);
                    let etype = self.ident_str().unwrap_or_default(); self.advance();
                    let ename = self.ident_str().unwrap_or_default(); self.advance();
                    self.expect(JaTok::RParen);
                    let cbody = self.parse_block();
                    catches.push((etype, ename, cbody));
                }
                let finally = if self.ident_str().as_deref() == Some("finally") { self.advance(); Some(self.parse_block()) } else { None };
                Some(JaStmt::TryCatch(body, catches, finally))
            }
            JaTok::Ident(ref name) if name == "throw" => {
                self.advance(); let e = self.parse_expr(); self.expect(JaTok::Semi); Some(JaStmt::Throw(e))
            }
            _ => {
                // Check for variable declaration: Type name = expr;
                let saved = self.pos;
                let _ = self.consume_modifiers();
                if let Some(ref name) = self.ident_str() {
                    if Self::is_type(name) || name.chars().next().map_or(false, |c| c.is_ascii_uppercase()) {
                        let type_name = self.consume_type();
                        if let Some(var_name) = self.ident_str() {
                            self.advance();
                            if self.peek() == &JaTok::Eq { self.advance(); let v = self.parse_expr(); self.expect(JaTok::Semi); return Some(JaStmt::VarDecl(type_name, var_name, Some(v))); }
                            if self.peek() == &JaTok::Semi { self.advance(); return Some(JaStmt::VarDecl(type_name, var_name, None)); }
                        }
                    }
                }
                self.pos = saved;
                let expr = self.parse_expr();
                match self.peek().clone() {
                    JaTok::Eq => { self.advance(); let v = self.parse_expr(); self.expect(JaTok::Semi); Some(JaStmt::Assign(expr, v)) }
                    JaTok::PlusEq => { self.advance(); let v = self.parse_expr(); self.expect(JaTok::Semi); Some(JaStmt::AugAssign(expr, "+".to_string(), v)) }
                    JaTok::MinusEq => { self.advance(); let v = self.parse_expr(); self.expect(JaTok::Semi); Some(JaStmt::AugAssign(expr, "-".to_string(), v)) }
                    JaTok::PlusPlus => { self.advance(); self.expect(JaTok::Semi); Some(JaStmt::IncDec(expr, "++".to_string())) }
                    JaTok::MinusMinus => { self.advance(); self.expect(JaTok::Semi); Some(JaStmt::IncDec(expr, "--".to_string())) }
                    JaTok::Semi => { self.advance(); Some(JaStmt::Expr(expr)) }
                    _ => Some(JaStmt::Expr(expr)),
                }
            }
        }
    }

    fn parse_expr(&mut self) -> JaExpr { self.parse_ternary() }
    fn parse_ternary(&mut self) -> JaExpr {
        let expr = self.parse_or();
        if self.peek() == &JaTok::Question { self.advance(); let then = self.parse_expr(); self.expect(JaTok::Colon); let else_e = self.parse_ternary(); JaExpr::Ternary(Box::new(expr), Box::new(then), Box::new(else_e)) }
        else { expr }
    }
    fn parse_or(&mut self) -> JaExpr { let mut l = self.parse_and_expr(); while self.peek() == &JaTok::PipePipe { self.advance(); let r = self.parse_and_expr(); l = JaExpr::Binary(Box::new(l), "||".to_string(), Box::new(r)); } l }
    fn parse_and_expr(&mut self) -> JaExpr { let mut l = self.parse_cmp(); while self.peek() == &JaTok::AmpAmp { self.advance(); let r = self.parse_cmp(); l = JaExpr::Binary(Box::new(l), "&&".to_string(), Box::new(r)); } l }
    fn parse_cmp(&mut self) -> JaExpr { let mut l = self.parse_add();
        loop { let op = match self.peek() { JaTok::EqEq => "==", JaTok::NotEq => "!=", JaTok::Lt => "<", JaTok::Gt => ">", JaTok::LtEq => "<=", JaTok::GtEq => ">=", _ => break }; self.advance(); let r = self.parse_add(); l = JaExpr::Binary(Box::new(l), op.to_string(), Box::new(r)); }
        if self.ident_str().as_deref() == Some("instanceof") { self.advance(); let t = self.ident_str().unwrap_or_default(); self.advance(); l = JaExpr::InstanceOf(Box::new(l), t); }
        l
    }
    fn parse_add(&mut self) -> JaExpr { let mut l = self.parse_mul(); loop { let op = match self.peek() { JaTok::Plus => "+", JaTok::Minus => "-", _ => break }; self.advance(); let r = self.parse_mul(); l = JaExpr::Binary(Box::new(l), op.to_string(), Box::new(r)); } l }
    fn parse_mul(&mut self) -> JaExpr { let mut l = self.parse_unary(); loop { let op = match self.peek() { JaTok::Star => "*", JaTok::Slash => "/", JaTok::Percent => "%", _ => break }; self.advance(); let r = self.parse_unary(); l = JaExpr::Binary(Box::new(l), op.to_string(), Box::new(r)); } l }
    fn parse_unary(&mut self) -> JaExpr {
        match self.peek().clone() {
            JaTok::Bang => { self.advance(); JaExpr::Unary("!".to_string(), Box::new(self.parse_unary())) }
            JaTok::Minus => { self.advance(); JaExpr::Unary("-".to_string(), Box::new(self.parse_unary())) }
            JaTok::PlusPlus => { self.advance(); let e = self.parse_unary(); JaExpr::Unary("++".to_string(), Box::new(e)) }
            JaTok::MinusMinus => { self.advance(); let e = self.parse_unary(); JaExpr::Unary("--".to_string(), Box::new(e)) }
            JaTok::LParen => {
                let saved = self.pos; self.advance();
                if let Some(t) = self.ident_str() { self.advance();
                    if self.peek() == &JaTok::RParen { self.advance(); let e = self.parse_unary(); return JaExpr::Cast(t, Box::new(e)); }
                }
                self.pos = saved; self.parse_postfix()
            }
            _ => self.parse_postfix(),
        }
    }
    fn parse_postfix(&mut self) -> JaExpr {
        let mut expr = self.parse_primary();
        loop { match self.peek().clone() {
            JaTok::Dot => { self.advance(); let f = self.ident_str().unwrap_or_default(); self.advance();
                if self.peek() == &JaTok::LParen { self.advance(); let mut args = Vec::new();
                    while self.peek() != &JaTok::RParen && !self.at_eof() { args.push(self.parse_expr()); if self.peek() == &JaTok::Comma { self.advance(); } }
                    self.expect(JaTok::RParen); expr = JaExpr::Call(Box::new(JaExpr::Member(Box::new(expr), f)), args);
                } else { expr = JaExpr::Member(Box::new(expr), f); } }
            JaTok::LBracket => { self.advance(); let idx = self.parse_expr(); self.expect(JaTok::RBracket); expr = JaExpr::Index(Box::new(expr), Box::new(idx)); }
            JaTok::LParen => { self.advance(); let mut args = Vec::new();
                while self.peek() != &JaTok::RParen && !self.at_eof() { args.push(self.parse_expr()); if self.peek() == &JaTok::Comma { self.advance(); } }
                self.expect(JaTok::RParen); expr = JaExpr::Call(Box::new(expr), args); }
            _ => break,
        }} expr
    }
    fn parse_primary(&mut self) -> JaExpr {
        match self.peek().clone() {
            JaTok::IntLit(n) => { self.advance(); JaExpr::Int(n) }
            JaTok::FloatLit(f) => { self.advance(); JaExpr::Float(f) }
            JaTok::StrLit(s) => { self.advance(); JaExpr::Str(s) }
            JaTok::CharLit(c) => { self.advance(); JaExpr::Char(c) }
            JaTok::Ident(ref name) if name == "true" => { self.advance(); JaExpr::Bool(true) }
            JaTok::Ident(ref name) if name == "false" => { self.advance(); JaExpr::Bool(false) }
            JaTok::Ident(ref name) if name == "null" => { self.advance(); JaExpr::Null }
            JaTok::Ident(ref name) if name == "new" => {
                self.advance(); let t = self.ident_str().unwrap_or_default(); self.advance();
                self.skip_generic_params();
                if self.peek() == &JaTok::LBracket { self.advance(); let sz = self.parse_expr(); self.expect(JaTok::RBracket); JaExpr::NewArray(t, vec![sz]) }
                else { self.expect(JaTok::LParen); let mut args = Vec::new();
                    while self.peek() != &JaTok::RParen && !self.at_eof() { args.push(self.parse_expr()); if self.peek() == &JaTok::Comma { self.advance(); } }
                    self.expect(JaTok::RParen);
                    // Skip anonymous class body
                    if self.peek() == &JaTok::LBrace { self.advance(); let mut d = 1; while d > 0 && !self.at_eof() { match self.advance() { JaTok::LBrace => d += 1, JaTok::RBrace => d -= 1, _ => {} } } }
                    JaExpr::New(t, args)
                }
            }
            JaTok::Ident(name) => { self.advance(); JaExpr::Name(name) }
            JaTok::LParen => {
                self.advance();
                // Check for lambda: (params) -> { body }
                let saved = self.pos;
                let mut might_be_lambda = true;
                let mut params = Vec::new();
                while self.peek() != &JaTok::RParen && !self.at_eof() {
                    if let Some(p) = self.ident_str() { params.push(p); self.advance(); }
                    else { might_be_lambda = false; break; }
                    // Skip type annotations
                    if let Some(_) = self.ident_str() { self.advance(); }
                    if self.peek() == &JaTok::Comma { self.advance(); }
                }
                if might_be_lambda && self.peek() == &JaTok::RParen {
                    self.advance();
                    if self.peek() == &JaTok::Arrow {
                        self.advance();
                        let body = if self.peek() == &JaTok::LBrace { self.parse_block() } else { vec![JaStmt::Return(Some(self.parse_expr()))] };
                        return JaExpr::Lambda(params, body);
                    }
                }
                self.pos = saved;
                let expr = self.parse_expr();
                self.expect(JaTok::RParen); expr
            }
            JaTok::LBrace => {
                self.advance(); let mut elems = Vec::new();
                while self.peek() != &JaTok::RBrace && !self.at_eof() { elems.push(self.parse_expr()); if self.peek() == &JaTok::Comma { self.advance(); } }
                self.expect(JaTok::RBrace); JaExpr::ArrayInit(elems)
            }
            _ => { self.advance(); JaExpr::Int(0) }
        }
    }
}

// ============ Emitter ============
fn san(name: &str) -> String {
    match name { "set" | "to" | "display" | "return" | "if" | "else" | "repeat" | "while" | "define" | "with" | "and" | "for" | "each" | "in" | "break" | "continue" | "check" | "structure" | "field" | "choice" | "variant" | "create" | "is" | "not" | "true" | "false" | "import" | "borrow" | "spawn" | "send" | "receive" | "from" | "channel" | "given" | "returning" | "as" | "trait" | "implement" | "equals" | "async" | "await" | "try" | "external" => format!("{}_", name), _ => name.to_string() }
}
fn ind(out: &mut String, d: usize) { for _ in 0..d { out.push_str("    "); } }
fn ja_type(t: &str) -> &str { match t { "int" | "long" | "short" | "byte" | "char" | "Integer" | "Long" | "Short" | "Byte" | "Character" => "Int", "float" | "double" | "Float" | "Double" => "Float", "boolean" | "Boolean" => "Bool", "String" => "Str", _ => "Int" } }

pub fn emit_ernos_from_java(_filename: &str, source: &str) -> String {
    let mut lexer = JaLexer::new(source);
    let tokens = lexer.tokenize();
    let mut parser = JaParser::new(tokens);
    let stmts = parser.parse_file();
    let mut out = String::new();
    out.push_str("# Transpiled from Java\n\n");
    for s in &stmts { emit_ja_stmt(&mut out, s, 0, None); }
    out
}

fn emit_ja_stmt(out: &mut String, stmt: &JaStmt, depth: usize, class_name: Option<&str>) {
    match stmt {
        JaStmt::Package(_) => {}
        JaStmt::Import(path) => { ind(out, depth); out.push_str(&format!("# import {}\n", path)); }
        JaStmt::Class(name, _, body) => {
            // Extract fields for structure
            let fields: Vec<_> = body.iter().filter_map(|s| if let JaStmt::Field(_, t, n, _) = s { Some((n.clone(), t.clone())) } else { None }).collect();
            if !fields.is_empty() {
                ind(out, depth); out.push_str(&format!("structure {}:\n", name));
                for (fn_, ft) in &fields { ind(out, depth + 1); out.push_str(&format!("field {} as {}\n", san(fn_), ja_type(ft))); }
                out.push('\n');
            }
            for s in body { match s { JaStmt::Field(..) => {} _ => emit_ja_stmt(out, s, depth, Some(name)) } }
        }
        JaStmt::Interface(name, body) => {
            ind(out, depth); out.push_str(&format!("trait {}:\n", name));
            for s in body { if let JaStmt::Method(_, _, mname, params, _) = s {
                ind(out, depth + 1);
                if params.is_empty() { out.push_str(&format!("define {}\n", san(mname))); }
                else { let p: Vec<String> = params.iter().map(|(n, t)| format!("{} as {}", san(n), ja_type(t))).collect(); out.push_str(&format!("define {} with {}\n", san(mname), p.join(" and "))); }
            }}
            out.push('\n');
        }
        JaStmt::Enum(name, variants) => {
            ind(out, depth); out.push_str(&format!("choice {}:\n", name));
            for v in variants { ind(out, depth + 1); out.push_str(&format!("variant {}\n", v)); }
            out.push('\n');
        }
        JaStmt::Method(mods, _, name, params, body) => {
            // main must be parameterless in ErnosPlain — drop all its params
            let filtered_params: Vec<_> = if name == "main" {
                Vec::new()
            } else {
                params.iter().collect()
            };
            ind(out, depth);
            let sname = san(name);
            let is_static = mods.contains(&"static".to_string());
            if let Some(cn) = class_name {
                if !is_static && name != cn {
                    if filtered_params.is_empty() { out.push_str(&format!("define method {} on {}:\n", sname, cn)); }
                    else { let p: Vec<String> = filtered_params.iter().map(|(n, t)| format!("{} as {}", san(n), ja_type(t))).collect(); out.push_str(&format!("define method {} on {} with {}:\n", sname, cn, p.join(" and "))); }
                } else {
                    if filtered_params.is_empty() { out.push_str(&format!("define {}:\n", sname)); }
                    else { let p: Vec<String> = filtered_params.iter().map(|(n, t)| format!("{} as {}", san(n), ja_type(t))).collect(); out.push_str(&format!("define {} with {}:\n", sname, p.join(" and "))); }
                }
            } else {
                if filtered_params.is_empty() { out.push_str(&format!("define {}:\n", sname)); }
                else { let p: Vec<String> = filtered_params.iter().map(|(n, t)| format!("{} as {}", san(n), ja_type(t))).collect(); out.push_str(&format!("define {} with {}:\n", sname, p.join(" and "))); }
            }
            for s in body { emit_ja_stmt(out, s, depth + 1, None); }
            if body.is_empty() { ind(out, depth + 1); out.push_str("return 0\n"); }
            out.push('\n');
        }
        JaStmt::VarDecl(_, name, init) => {
            ind(out, depth); out.push_str(&format!("set {} to ", san(name)));
            if let Some(v) = init { emit_ja_expr(out, v); } else { out.push('0'); } out.push('\n');
        }
        JaStmt::Assign(target, val) => {
            ind(out, depth);
            if let JaExpr::Name(n) = target { out.push_str(&format!("set {} to ", san(n))); }
            else { out.push_str("set _x to "); }
            emit_ja_expr(out, val); out.push('\n');
        }
        JaStmt::AugAssign(target, op, val) => {
            ind(out, depth);
            if let JaExpr::Name(n) = target { let sn = san(n); out.push_str(&format!("set {0} to {0} {1} ", sn, op)); }
            else { out.push_str("set _x to _x + "); }
            emit_ja_expr(out, val); out.push('\n');
        }
        JaStmt::IncDec(target, op) => {
            ind(out, depth);
            if let JaExpr::Name(n) = target { let sn = san(n); let o = if op == "++" { "+" } else { "-" }; out.push_str(&format!("set {0} to {0} {1} 1\n", sn, o)); }
        }
        JaStmt::If(cond, then, else_b) => {
            ind(out, depth); out.push_str("if "); emit_ja_expr(out, cond); out.push_str(":\n");
            for s in then { emit_ja_stmt(out, s, depth + 1, None); }
            if let Some(eb) = else_b {
                if eb.len() == 1 { if let JaStmt::If(..) = &eb[0] { ind(out, depth); out.push_str("else "); emit_ja_stmt(out, &eb[0], depth, None); return; } }
                ind(out, depth); out.push_str("else:\n");
                for s in eb { emit_ja_stmt(out, s, depth + 1, None); }
            }
        }
        JaStmt::While(cond, body) => { ind(out, depth); out.push_str("repeat while "); emit_ja_expr(out, cond); out.push_str(":\n"); for s in body { emit_ja_stmt(out, s, depth + 1, None); } }
        JaStmt::DoWhile(body, cond) => { ind(out, depth); out.push_str("repeat while true:\n"); for s in body { emit_ja_stmt(out, s, depth + 1, None); } ind(out, depth + 1); out.push_str("if not "); emit_ja_expr(out, cond); out.push_str(":\n"); ind(out, depth + 2); out.push_str("break\n"); }
        JaStmt::For(init, cond, post, body) => {
            if let Some(i) = init { emit_ja_stmt(out, i, depth, None); }
            ind(out, depth); out.push_str("repeat while ");
            if let Some(c) = cond { emit_ja_expr(out, c); } else { out.push_str("true"); }
            out.push_str(":\n");
            for s in body { emit_ja_stmt(out, s, depth + 1, None); }
            if let Some(p) = post { emit_ja_stmt(out, p, depth + 1, None); }
        }
        JaStmt::ForEach(_, var, iter, body) => {
            ind(out, depth); out.push_str(&format!("for each {} in ", san(var))); emit_ja_expr(out, iter); out.push_str(":\n");
            for s in body { emit_ja_stmt(out, s, depth + 1, None); }
        }
        JaStmt::Return(val) => { ind(out, depth); out.push_str("return "); if let Some(v) = val { emit_ja_expr(out, v); } else { out.push('0'); } out.push('\n'); }
        JaStmt::Break => { ind(out, depth); out.push_str("break\n"); }
        JaStmt::Continue => { ind(out, depth); out.push_str("continue\n"); }
        JaStmt::Expr(expr) => {
            if let JaExpr::Call(func, args) = expr {
                if let JaExpr::Member(obj, method) = func.as_ref() {
                    if let JaExpr::Member(outer, inner_f) = obj.as_ref() {
                        if let JaExpr::Name(n) = outer.as_ref() {
                            if n == "System" && (inner_f == "out" || inner_f == "err") && (method == "println" || method == "print") {
                                for a in args { ind(out, depth); out.push_str("display "); emit_ja_expr(out, a); out.push('\n'); }
                                if args.is_empty() { ind(out, depth); out.push_str("display \"\"\n"); }
                                return;
                            }
                        }
                    }
                }
            }
            ind(out, depth); emit_ja_expr(out, expr); out.push('\n');
        }
        JaStmt::Switch(tag, cases, default) => {
            ind(out, depth); out.push_str("check "); emit_ja_expr(out, tag); out.push_str(":\n");
            for (vals, body) in cases { for v in vals { ind(out, depth + 1); out.push_str("if "); emit_ja_expr(out, v); out.push_str(":\n"); }
                for s in body { if let JaStmt::Break = s { continue; } emit_ja_stmt(out, s, depth + 2, None); } }
            if let Some(def) = default { ind(out, depth + 1); out.push_str("if default:\n"); for s in def { if let JaStmt::Break = s { continue; } emit_ja_stmt(out, s, depth + 2, None); } }
        }
        JaStmt::TryCatch(body, catches, _) => {
            ind(out, depth); out.push_str("# try\n");
            for s in body { emit_ja_stmt(out, s, depth, None); }
            for (etype, _, cbody) in catches { ind(out, depth); out.push_str(&format!("# catch {}\n", etype)); for s in cbody { emit_ja_stmt(out, s, depth, None); } }
        }
        JaStmt::Throw(e) => { ind(out, depth); out.push_str("# throw "); emit_ja_expr(out, e); out.push('\n'); }
        JaStmt::Field(..) | JaStmt::Block(_) => {}
    }
}

fn emit_ja_expr(out: &mut String, expr: &JaExpr) {
    match expr {
        JaExpr::Int(n) => out.push_str(&n.to_string()),
        JaExpr::Float(f) => out.push_str(&format!("{}", f)),
        JaExpr::Str(s) => out.push_str(&format!("\"{}\"", s.replace('"', "\\\"").replace('\n', "\\n"))),
        JaExpr::Char(c) => out.push_str(&(*c as i64).to_string()),
        JaExpr::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        JaExpr::Null => out.push('0'),
        JaExpr::Name(n) => out.push_str(&san(n)),
        JaExpr::Binary(l, op, r) => {
            emit_ja_expr(out, l);
            let ep = match op.as_str() { "+" => " + ", "-" => " - ", "*" => " * ", "/" => " / ", "%" => " modulo ", "==" => " equals ", "!=" => " != ", "<" => " < ", ">" => " > ", "<=" => " <= ", ">=" => " >= ", "&&" => " and also ", "||" => " or else ", _ => " + " };
            out.push_str(ep); emit_ja_expr(out, r);
        }
        JaExpr::Unary(op, inner) => { if op == "!" { out.push_str("not "); emit_ja_expr(out, inner); } else if op == "-" { out.push_str("(0 - "); emit_ja_expr(out, inner); out.push(')'); } else { emit_ja_expr(out, inner); } }
        JaExpr::Call(func, args) => {
            if let JaExpr::Member(obj, method) = func.as_ref() {
                match method.as_str() {
                    "length" => { out.push_str("length("); emit_ja_expr(out, obj); out.push(')'); return; }
                    "size" => { out.push_str("length("); emit_ja_expr(out, obj); out.push(')'); return; }
                    "toString" => { out.push_str("int_to_string("); emit_ja_expr(out, obj); out.push(')'); return; }
                    "parseInt" | "valueOf" if args.len() == 1 => { out.push_str("string_to_int("); emit_ja_expr(out, &args[0]); out.push(')'); return; }
                    "add" if args.len() == 1 => { out.push_str("append("); emit_ja_expr(out, obj); out.push_str(" and "); emit_ja_expr(out, &args[0]); out.push(')'); return; }
                    "get" if args.len() == 1 => { out.push_str("get_list("); emit_ja_expr(out, obj); out.push_str(" and "); emit_ja_expr(out, &args[0]); out.push(')'); return; }
                    "contains" if args.len() == 1 => { out.push_str("string_contains("); emit_ja_expr(out, obj); out.push_str(" and "); emit_ja_expr(out, &args[0]); out.push(')'); return; }
                    "equals" if args.len() == 1 => { emit_ja_expr(out, obj); out.push_str(" equals "); emit_ja_expr(out, &args[0]); return; }
                    _ => {}
                }
            }
            emit_ja_expr(out, func); out.push('('); for (i, a) in args.iter().enumerate() { if i > 0 { out.push_str(" and "); } emit_ja_expr(out, a); } out.push(')');
        }
        JaExpr::Member(obj, field) => { emit_ja_expr(out, obj); out.push('.'); out.push_str(field); }
        JaExpr::Index(arr, idx) => { out.push_str("get_list("); emit_ja_expr(out, arr); out.push_str(" and "); emit_ja_expr(out, idx); out.push(')'); }
        JaExpr::New(name, args) => { out.push_str(&format!("create {}:", name)); if !args.is_empty() { out.push('\n'); for (i, a) in args.iter().enumerate() { out.push_str(&format!("    field{} is ", i)); emit_ja_expr(out, a); if i < args.len() - 1 { out.push('\n'); } } } }
        JaExpr::NewArray(_, sizes) => { out.push_str("["); for s in sizes { emit_ja_expr(out, s); } out.push(']'); }
        JaExpr::ArrayInit(elems) => { out.push('['); for (i, e) in elems.iter().enumerate() { if i > 0 { out.push_str(", "); } emit_ja_expr(out, e); } out.push(']'); }
        JaExpr::Cast(_, inner) => emit_ja_expr(out, inner),
        JaExpr::Ternary(cond, then, else_e) => { out.push_str("(if "); emit_ja_expr(out, cond); out.push_str(": "); emit_ja_expr(out, then); out.push_str(" else "); emit_ja_expr(out, else_e); out.push(')'); }
        JaExpr::Lambda(params, body) => { if params.is_empty() { out.push_str("given _:"); } else { out.push_str(&format!("given {}:", params.iter().map(|p| san(p)).collect::<Vec<_>>().join(" and "))); }
            if body.len() == 1 { if let JaStmt::Return(Some(e)) = &body[0] { out.push(' '); emit_ja_expr(out, e); return; } }
            out.push('\n'); for s in body { emit_ja_stmt(out, s, 1, None); }
        }
        JaExpr::InstanceOf(obj, _t) => { emit_ja_expr(out, obj); }
    }
}
