/// TypeScript → ErnosPlain Transpiler
///
/// Parses a subset of TypeScript/ES6+ source code and emits ErnosPlain (.ep) code.
/// Built on top of the JS transpiler pattern but handles TypeScript-specific features:
/// - Type annotations (stripped during parsing)
/// - Interfaces, type aliases, enums
/// - Optional chaining (?.), non-null assertion (!)
/// - Decorators (@), access modifiers (public, private, protected)
/// - Generics (<T>) — skipped

// Reuse the JS token set — TypeScript tokens are a superset

#[derive(Debug, Clone, PartialEq)]
pub enum TsTok {
    Ident(String), IntLit(i64), FloatLit(f64), StrLit(String), TemplateLit(String),
    Plus, Minus, Star, Slash, Percent, StarStar,
    PlusPlus, MinusMinus,
    PlusEq, MinusEq, StarEq, SlashEq, PercentEq,
    AmpAmp, PipePipe, Bang, Question, QuestionDot,
    Eq, EqEq, EqEqEq, NotEq, NotEqEq,
    Lt, Gt, LtEq, GtEq,
    Arrow, Spread,
    Amp, Pipe, Caret, Tilde,
    LParen, RParen, LBracket, RBracket, LBrace, RBrace,
    Comma, Semi, Colon, Dot, At, Hash,
    Newline, Eof,
}

pub struct TsLexer { chars: Vec<char>, pos: usize }
impl TsLexer {
    pub fn new(input: &str) -> Self { TsLexer { chars: input.chars().collect(), pos: 0 } }
    fn peek(&self) -> Option<char> { self.chars.get(self.pos).copied() }
    fn advance(&mut self) -> Option<char> { let c = self.chars.get(self.pos).copied(); self.pos += 1; c }
    fn peek_at(&self, off: usize) -> Option<char> { self.chars.get(self.pos + off).copied() }

    pub fn tokenize(&mut self) -> Vec<TsTok> {
        let mut tokens = Vec::new();
        loop {
            while matches!(self.peek(), Some(' ') | Some('\t') | Some('\r')) { self.advance(); }
            match self.peek() {
                None => break,
                Some('\n') => { self.advance(); tokens.push(TsTok::Newline); }
                Some('/') if self.peek_at(1) == Some('/') => { while self.peek() != Some('\n') && self.peek().is_some() { self.advance(); } }
                Some('/') if self.peek_at(1) == Some('*') => { self.advance(); self.advance(); loop { match self.advance() { Some('*') if self.peek() == Some('/') => { self.advance(); break; } None => break, _ => {} } } }
                Some('"') | Some('\'') => { let q = self.peek().unwrap(); self.advance(); let mut s = String::new();
                    loop { match self.advance() { Some(c) if c == q => break, Some('\\') => { match self.advance() { Some('n') => s.push('\n'), Some('t') => s.push('\t'), Some('\\') => s.push('\\'), Some(c) => { s.push('\\'); s.push(c); } None => break } } Some(c) => s.push(c), None => break } }
                    tokens.push(TsTok::StrLit(s)); }
                Some('`') => { self.advance(); let mut s = String::new();
                    loop { match self.advance() { Some('`') => break, Some('\\') => { match self.advance() { Some('n') => s.push('\n'), Some('t') => s.push('\t'), Some(c) => { s.push('\\'); s.push(c); } None => break } } Some(c) => s.push(c), None => break } }
                    tokens.push(TsTok::TemplateLit(s)); }
                Some(c) if c.is_ascii_digit() => {
                    let mut n = String::new(); let mut is_float = false;
                    while let Some(ch) = self.peek() {
                        if ch.is_ascii_digit() || ch == '_' { n.push(ch); self.advance(); }
                        else if ch == '.' && !is_float && self.peek_at(1).map_or(false, |c| c.is_ascii_digit()) { is_float = true; n.push(ch); self.advance(); }
                        else if (ch == 'x' || ch == 'X' || ch == 'o' || ch == 'b') && n == "0" { n.push(ch); self.advance(); }
                        else if ch.is_ascii_hexdigit() && (n.starts_with("0x") || n.starts_with("0X")) { n.push(ch); self.advance(); }
                        else { break; }
                    }
                    if matches!(self.peek(), Some('n')) { self.advance(); } // BigInt suffix
                    let clean: String = n.replace('_', "");
                    if is_float { tokens.push(TsTok::FloatLit(clean.parse().unwrap_or(0.0))); }
                    else if clean.starts_with("0x") || clean.starts_with("0X") { tokens.push(TsTok::IntLit(i64::from_str_radix(&clean[2..], 16).unwrap_or(0))); }
                    else { tokens.push(TsTok::IntLit(clean.parse().unwrap_or(0))); }
                }
                Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$' => {
                    let mut ident = String::new();
                    while let Some(ch) = self.peek() { if ch.is_ascii_alphanumeric() || ch == '_' || ch == '$' { ident.push(ch); self.advance(); } else { break; } }
                    tokens.push(TsTok::Ident(ident));
                }
                Some('@') => { self.advance(); tokens.push(TsTok::At); }
                _ => { tokens.push(self.lex_punct()); }
            }
        }
        tokens.push(TsTok::Eof); tokens
    }

    fn lex_punct(&mut self) -> TsTok {
        let c = self.advance().unwrap();
        match c {
            '+' => if self.peek() == Some('+') { self.advance(); TsTok::PlusPlus } else if self.peek() == Some('=') { self.advance(); TsTok::PlusEq } else { TsTok::Plus },
            '-' => if self.peek() == Some('-') { self.advance(); TsTok::MinusMinus } else if self.peek() == Some('=') { self.advance(); TsTok::MinusEq } else { TsTok::Minus },
            '*' => if self.peek() == Some('*') { self.advance(); TsTok::StarStar } else if self.peek() == Some('=') { self.advance(); TsTok::StarEq } else { TsTok::Star },
            '/' => if self.peek() == Some('=') { self.advance(); TsTok::SlashEq } else { TsTok::Slash },
            '%' => if self.peek() == Some('=') { self.advance(); TsTok::PercentEq } else { TsTok::Percent },
            '&' => if self.peek() == Some('&') { self.advance(); TsTok::AmpAmp } else { TsTok::Amp },
            '|' => if self.peek() == Some('|') { self.advance(); TsTok::PipePipe } else { TsTok::Pipe },
            '^' => TsTok::Caret, '~' => TsTok::Tilde,
            '!' => if self.peek() == Some('=') { self.advance(); if self.peek() == Some('=') { self.advance(); TsTok::NotEqEq } else { TsTok::NotEq } } else { TsTok::Bang },
            '=' => if self.peek() == Some('=') { self.advance(); if self.peek() == Some('=') { self.advance(); TsTok::EqEqEq } else { TsTok::EqEq } } else if self.peek() == Some('>') { self.advance(); TsTok::Arrow } else { TsTok::Eq },
            '<' => if self.peek() == Some('=') { self.advance(); TsTok::LtEq } else { TsTok::Lt },
            '>' => if self.peek() == Some('=') { self.advance(); TsTok::GtEq } else { TsTok::Gt },
            '(' => TsTok::LParen, ')' => TsTok::RParen, '[' => TsTok::LBracket, ']' => TsTok::RBracket,
            '{' => TsTok::LBrace, '}' => TsTok::RBrace,
            ',' => TsTok::Comma, ';' => TsTok::Semi, ':' => TsTok::Colon,
            '.' => if self.peek() == Some('.') && self.peek_at(1) == Some('.') { self.advance(); self.advance(); TsTok::Spread } else { TsTok::Dot },
            '?' => if self.peek() == Some('.') { self.advance(); TsTok::QuestionDot } else { TsTok::Question },
            '#' => TsTok::Hash,
            _ => TsTok::Eof,
        }
    }
}

#[derive(Debug, Clone)]
pub enum TsExpr {
    Int(i64), Float(f64), Str(String), Template(String), Bool(bool), Null, Undefined,
    Name(String),
    Binary(Box<TsExpr>, String, Box<TsExpr>),
    Unary(String, Box<TsExpr>),
    Call(Box<TsExpr>, Vec<TsExpr>),
    Member(Box<TsExpr>, String),
    OptMember(Box<TsExpr>, String),
    Index(Box<TsExpr>, Box<TsExpr>),
    Array(Vec<TsExpr>),
    Object(Vec<(String, TsExpr)>),
    Arrow(Vec<String>, Vec<TsStmt>),
    New(Box<TsExpr>, Vec<TsExpr>),
    Ternary(Box<TsExpr>, Box<TsExpr>, Box<TsExpr>),
    Typeof(Box<TsExpr>),
    Spread(Box<TsExpr>),
    Await(Box<TsExpr>),
    NonNull(Box<TsExpr>),
    As(Box<TsExpr>, String),
}

#[derive(Debug, Clone)]
pub enum TsStmt {
    VarDecl(String, String, Option<TsExpr>),  // kind (let/const/var), name, init
    FnDecl(bool, String, Vec<String>, Vec<TsStmt>),  // async, name, params, body
    Class(String, Option<String>, Vec<TsStmt>),
    Interface(String, Vec<(String, String)>),
    TypeAlias(String),
    Enum(String, Vec<(String, Option<TsExpr>)>),
    Assign(TsExpr, TsExpr),
    AugAssign(TsExpr, String, TsExpr),
    IncDec(TsExpr, String),
    If(TsExpr, Vec<TsStmt>, Option<Vec<TsStmt>>),
    While(TsExpr, Vec<TsStmt>),
    DoWhile(Vec<TsStmt>, TsExpr),
    For(Option<Box<TsStmt>>, Option<TsExpr>, Option<Box<TsStmt>>, Vec<TsStmt>),
    ForOf(String, TsExpr, Vec<TsStmt>),
    ForIn(String, TsExpr, Vec<TsStmt>),
    Switch(TsExpr, Vec<(Vec<TsExpr>, Vec<TsStmt>)>, Option<Vec<TsStmt>>),
    Return(Option<TsExpr>),
    Expr(TsExpr),
    Break, Continue,
    Import(String, Vec<String>),
    Export(Box<TsStmt>),
    TryCatch(Vec<TsStmt>, Option<(String, Vec<TsStmt>)>, Option<Vec<TsStmt>>),
    Throw(TsExpr),
    Method(bool, bool, String, Vec<String>, Vec<TsStmt>),  // static, async, name, params, body
    Property(String, Option<TsExpr>),
}

pub struct TsParser { tokens: Vec<TsTok>, pos: usize }
impl TsParser {
    pub fn new(tokens: Vec<TsTok>) -> Self { TsParser { tokens, pos: 0 } }
    fn peek(&self) -> &TsTok { self.tokens.get(self.pos).unwrap_or(&TsTok::Eof) }
    fn advance(&mut self) -> TsTok { let t = self.tokens.get(self.pos).cloned().unwrap_or(TsTok::Eof); self.pos += 1; t }
    fn expect(&mut self, tok: TsTok) { if self.peek() == &tok { self.advance(); } }
    fn skip_nl(&mut self) { while self.peek() == &TsTok::Newline { self.advance(); } }
    fn at_eof(&self) -> bool { self.peek() == &TsTok::Eof }
    fn ident_str(&self) -> Option<String> { if let TsTok::Ident(s) = self.peek() { Some(s.clone()) } else { None } }

    fn skip_type_annotation(&mut self) {
        if self.peek() == &TsTok::Colon { self.advance(); self.skip_type_expr(); }
    }
    fn skip_type_expr(&mut self) {
        // Skip type expressions: number, string, Type<T>, Type[], (a) => b, etc.
        let mut depth = 0;
        loop { match self.peek() {
            TsTok::Lt => { depth += 1; self.advance(); }
            TsTok::Gt if depth > 0 => { depth -= 1; self.advance(); }
            TsTok::LBracket if depth == 0 => { if self.peek() == &TsTok::LBracket { self.advance(); self.expect(TsTok::RBracket); } else { break; } }
            TsTok::Pipe if depth == 0 => { self.advance(); }
            TsTok::Amp if depth == 0 => { self.advance(); }
            TsTok::Arrow if depth == 0 => { self.advance(); self.skip_type_expr(); return; }
            TsTok::Ident(_) => { self.advance(); }
            TsTok::LParen if depth == 0 => {
                // Could be function type
                self.advance();
                while self.peek() != &TsTok::RParen && !self.at_eof() { self.advance(); }
                self.expect(TsTok::RParen);
                if self.peek() == &TsTok::Arrow { self.advance(); self.skip_type_expr(); }
                return;
            }
            _ if depth == 0 => break,
            _ => { self.advance(); }
        }}
    }
    fn skip_generic_params(&mut self) {
        if self.peek() == &TsTok::Lt { self.advance(); let mut d = 1;
            while d > 0 && !self.at_eof() { match self.peek() { TsTok::Lt => { d += 1; self.advance(); } TsTok::Gt => { d -= 1; self.advance(); } _ => { self.advance(); } } }
        }
    }

    pub fn parse_file(&mut self) -> Vec<TsStmt> {
        let mut stmts = Vec::new();
        self.skip_nl();
        while !self.at_eof() {
            // Skip decorators
            while self.peek() == &TsTok::At { self.advance(); if let Some(_) = self.ident_str() { self.advance(); }
                if self.peek() == &TsTok::LParen { self.advance(); let mut d = 1; while d > 0 && !self.at_eof() { match self.advance() { TsTok::LParen => d += 1, TsTok::RParen => d -= 1, _ => {} } } }
                self.skip_nl();
            }
            if let Some(s) = self.parse_stmt() { stmts.push(s); }
            self.skip_nl();
        }
        stmts
    }

    fn parse_block(&mut self) -> Vec<TsStmt> {
        let mut stmts = Vec::new();
        self.expect(TsTok::LBrace); self.skip_nl();
        while self.peek() != &TsTok::RBrace && !self.at_eof() {
            if let Some(s) = self.parse_stmt() { stmts.push(s); }
            self.skip_nl();
        }
        self.expect(TsTok::RBrace); stmts
    }

    fn parse_stmt(&mut self) -> Option<TsStmt> {
        self.skip_nl();
        // Skip access modifiers
        while matches!(self.ident_str().as_deref(), Some("public") | Some("private") | Some("protected") | Some("readonly") | Some("abstract") | Some("declare")) { self.advance(); }

        match self.peek().clone() {
            TsTok::Ident(ref name) if name == "import" => {
                self.advance();
                // import { a, b } from "module" or import "module"
                let mut names = Vec::new();
                if self.peek() == &TsTok::LBrace { self.advance();
                    while self.peek() != &TsTok::RBrace && !self.at_eof() {
                        if let Some(n) = self.ident_str() { names.push(n); self.advance(); }
                        if self.ident_str().as_deref() == Some("as") { self.advance(); self.advance(); }
                        if self.peek() == &TsTok::Comma { self.advance(); }
                    }
                    self.expect(TsTok::RBrace);
                    if self.ident_str().as_deref() == Some("from") { self.advance(); }
                } else if let Some(n) = self.ident_str() {
                    names.push(n); self.advance();
                    if self.ident_str().as_deref() == Some("from") { self.advance(); }
                } else if self.peek() == &TsTok::Star { self.advance();
                    if self.ident_str().as_deref() == Some("as") { self.advance(); if let Some(n) = self.ident_str() { names.push(n); self.advance(); } }
                    if self.ident_str().as_deref() == Some("from") { self.advance(); }
                }
                let module = if let TsTok::StrLit(s) = self.peek().clone() { self.advance(); s } else { String::new() };
                self.expect(TsTok::Semi);
                Some(TsStmt::Import(module, names))
            }
            TsTok::Ident(ref name) if name == "export" => {
                self.advance();
                if self.ident_str().as_deref() == Some("default") { self.advance(); }
                let inner = self.parse_stmt()?;
                Some(TsStmt::Export(Box::new(inner)))
            }
            TsTok::Ident(ref name) if name == "let" || name == "const" || name == "var" => {
                let kind = name.clone(); self.advance();
                let vname = self.ident_str().unwrap_or_default(); self.advance();
                self.skip_type_annotation();
                let init = if self.peek() == &TsTok::Eq { self.advance(); Some(self.parse_expr()) } else { None };
                while self.peek() == &TsTok::Semi { self.advance(); }
                Some(TsStmt::VarDecl(kind, vname, init))
            }
            TsTok::Ident(ref name) if name == "function" || name == "async" => {
                let is_async = name == "async";
                if is_async { self.advance(); }
                if self.ident_str().as_deref() == Some("function") { self.advance(); }
                let fname = self.ident_str().unwrap_or_default(); self.advance();
                self.skip_generic_params();
                let params = self.parse_params();
                self.skip_type_annotation(); // return type
                let body = self.parse_block();
                Some(TsStmt::FnDecl(is_async, fname, params, body))
            }
            TsTok::Ident(ref name) if name == "class" => {
                self.advance();
                let cname = self.ident_str().unwrap_or_default(); self.advance();
                self.skip_generic_params();
                let parent = if self.ident_str().as_deref() == Some("extends") { self.advance(); let p = self.ident_str(); self.advance(); self.skip_generic_params(); p } else { None };
                if self.ident_str().as_deref() == Some("implements") { self.advance(); while self.peek() != &TsTok::LBrace && !self.at_eof() { self.advance(); } }
                let body = self.parse_class_body();
                Some(TsStmt::Class(cname, parent, body))
            }
            TsTok::Ident(ref name) if name == "interface" => {
                self.advance();
                let iname = self.ident_str().unwrap_or_default(); self.advance();
                self.skip_generic_params();
                if self.ident_str().as_deref() == Some("extends") { self.advance(); while self.peek() != &TsTok::LBrace && !self.at_eof() { self.advance(); } }
                self.expect(TsTok::LBrace);
                let mut fields = Vec::new();
                self.skip_nl();
                while self.peek() != &TsTok::RBrace && !self.at_eof() {
                    if let Some(fname) = self.ident_str() { self.advance();
                        if self.peek() == &TsTok::Question { self.advance(); }
                        self.expect(TsTok::Colon);
                        let mut t = String::new();
                        while !matches!(self.peek(), TsTok::Semi | TsTok::Newline | TsTok::RBrace | TsTok::Comma) && !self.at_eof() {
                            if let Some(n) = self.ident_str() { t.push_str(&n); self.advance(); } else { self.advance(); }
                        }
                        fields.push((fname, t));
                    } else { self.advance(); }
                    while matches!(self.peek(), TsTok::Semi | TsTok::Newline | TsTok::Comma) { self.advance(); }
                }
                self.expect(TsTok::RBrace);
                Some(TsStmt::Interface(iname, fields))
            }
            TsTok::Ident(ref name) if name == "type" => {
                self.advance();
                let tname = self.ident_str().unwrap_or_default(); self.advance();
                // Skip the entire type alias
                while !matches!(self.peek(), TsTok::Semi | TsTok::Newline | TsTok::Eof) { self.advance(); }
                while self.peek() == &TsTok::Semi { self.advance(); }
                Some(TsStmt::TypeAlias(tname))
            }
            TsTok::Ident(ref name) if name == "enum" => {
                self.advance();
                let ename = self.ident_str().unwrap_or_default(); self.advance();
                self.expect(TsTok::LBrace); self.skip_nl();
                let mut variants = Vec::new();
                while self.peek() != &TsTok::RBrace && !self.at_eof() {
                    let vname = self.ident_str().unwrap_or_default(); self.advance();
                    let val = if self.peek() == &TsTok::Eq { self.advance(); Some(self.parse_expr()) } else { None };
                    variants.push((vname, val));
                    if self.peek() == &TsTok::Comma { self.advance(); }
                    self.skip_nl();
                }
                self.expect(TsTok::RBrace);
                Some(TsStmt::Enum(ename, variants))
            }
            TsTok::Ident(ref name) if name == "if" => {
                self.advance(); self.expect(TsTok::LParen); let cond = self.parse_expr(); self.expect(TsTok::RParen);
                let then = if self.peek() == &TsTok::LBrace { self.parse_block() } else { vec![self.parse_stmt()?] };
                self.skip_nl();
                let else_b = if self.ident_str().as_deref() == Some("else") { self.advance(); self.skip_nl();
                    if self.peek() == &TsTok::LBrace { Some(self.parse_block()) } else { Some(vec![self.parse_stmt()?]) }
                } else { None };
                Some(TsStmt::If(cond, then, else_b))
            }
            TsTok::Ident(ref name) if name == "while" => { self.advance(); self.expect(TsTok::LParen); let cond = self.parse_expr(); self.expect(TsTok::RParen);
                let body = if self.peek() == &TsTok::LBrace { self.parse_block() } else { vec![self.parse_stmt()?] };
                Some(TsStmt::While(cond, body)) }
            TsTok::Ident(ref name) if name == "for" => {
                self.advance(); self.expect(TsTok::LParen);
                // Check for for-of / for-in
                let saved = self.pos;
                if matches!(self.ident_str().as_deref(), Some("const") | Some("let") | Some("var")) {
                    self.advance();
                    let var = self.ident_str().unwrap_or_default(); self.advance();
                    if self.ident_str().as_deref() == Some("of") { self.advance(); let iter = self.parse_expr(); self.expect(TsTok::RParen);
                        let body = if self.peek() == &TsTok::LBrace { self.parse_block() } else { vec![self.parse_stmt()?] };
                        return Some(TsStmt::ForOf(var, iter, body)); }
                    if self.ident_str().as_deref() == Some("in") { self.advance(); let iter = self.parse_expr(); self.expect(TsTok::RParen);
                        let body = if self.peek() == &TsTok::LBrace { self.parse_block() } else { vec![self.parse_stmt()?] };
                        return Some(TsStmt::ForIn(var, iter, body)); }
                    self.pos = saved;
                }
                // C-style for
                let init = if self.peek() != &TsTok::Semi { self.parse_stmt().map(Box::new) } else { self.advance(); None };
                let cond = if self.peek() != &TsTok::Semi { Some(self.parse_expr()) } else { None }; self.expect(TsTok::Semi);
                let post = if self.peek() != &TsTok::RParen { self.parse_stmt().map(Box::new) } else { None };
                self.expect(TsTok::RParen);
                let body = if self.peek() == &TsTok::LBrace { self.parse_block() } else { vec![self.parse_stmt()?] };
                Some(TsStmt::For(init, cond, post, body))
            }
            TsTok::Ident(ref name) if name == "return" => { self.advance();
                let val = if !matches!(self.peek(), TsTok::Semi | TsTok::Newline | TsTok::RBrace | TsTok::Eof) { Some(self.parse_expr()) } else { None };
                while self.peek() == &TsTok::Semi { self.advance(); }
                Some(TsStmt::Return(val)) }
            TsTok::Ident(ref name) if name == "break" => { self.advance(); while self.peek() == &TsTok::Semi { self.advance(); } Some(TsStmt::Break) }
            TsTok::Ident(ref name) if name == "continue" => { self.advance(); while self.peek() == &TsTok::Semi { self.advance(); } Some(TsStmt::Continue) }
            TsTok::Ident(ref name) if name == "switch" => {
                self.advance(); self.expect(TsTok::LParen); let tag = self.parse_expr(); self.expect(TsTok::RParen);
                self.expect(TsTok::LBrace); self.skip_nl();
                let mut cases = Vec::new(); let mut default = None;
                while self.peek() != &TsTok::RBrace && !self.at_eof() {
                    if self.ident_str().as_deref() == Some("case") { self.advance();
                        let vals = vec![self.parse_expr()]; self.expect(TsTok::Colon); self.skip_nl();
                        let mut body = Vec::new();
                        while !self.at_eof() { if matches!(self.ident_str().as_deref(), Some("case") | Some("default")) || self.peek() == &TsTok::RBrace { break; }
                            if let Some(s) = self.parse_stmt() { body.push(s); } self.skip_nl(); }
                        cases.push((vals, body));
                    } else if self.ident_str().as_deref() == Some("default") { self.advance(); self.expect(TsTok::Colon); self.skip_nl();
                        let mut body = Vec::new();
                        while !self.at_eof() { if self.ident_str().as_deref() == Some("case") || self.peek() == &TsTok::RBrace { break; }
                            if let Some(s) = self.parse_stmt() { body.push(s); } self.skip_nl(); }
                        default = Some(body);
                    } else { self.advance(); } self.skip_nl();
                }
                self.expect(TsTok::RBrace); Some(TsStmt::Switch(tag, cases, default))
            }
            TsTok::Ident(ref name) if name == "try" => {
                self.advance(); let body = self.parse_block();
                let catch = if self.ident_str().as_deref() == Some("catch") { self.advance();
                    let cname = if self.peek() == &TsTok::LParen { self.advance(); let n = self.ident_str().unwrap_or("e".to_string()); self.advance(); self.skip_type_annotation(); self.expect(TsTok::RParen); n } else { "e".to_string() };
                    let cbody = self.parse_block(); Some((cname, cbody))
                } else { None };
                let finally = if self.ident_str().as_deref() == Some("finally") { self.advance(); Some(self.parse_block()) } else { None };
                Some(TsStmt::TryCatch(body, catch, finally))
            }
            TsTok::Ident(ref name) if name == "throw" => { self.advance(); let e = self.parse_expr(); while self.peek() == &TsTok::Semi { self.advance(); } Some(TsStmt::Throw(e)) }
            _ => {
                let expr = self.parse_expr();
                match self.peek().clone() {
                    TsTok::Eq => { self.advance(); let v = self.parse_expr(); while self.peek() == &TsTok::Semi { self.advance(); } Some(TsStmt::Assign(expr, v)) }
                    TsTok::PlusEq => { self.advance(); let v = self.parse_expr(); while self.peek() == &TsTok::Semi { self.advance(); } Some(TsStmt::AugAssign(expr, "+".to_string(), v)) }
                    TsTok::MinusEq => { self.advance(); let v = self.parse_expr(); while self.peek() == &TsTok::Semi { self.advance(); } Some(TsStmt::AugAssign(expr, "-".to_string(), v)) }
                    TsTok::PlusPlus => { self.advance(); while self.peek() == &TsTok::Semi { self.advance(); } Some(TsStmt::IncDec(expr, "++".to_string())) }
                    TsTok::MinusMinus => { self.advance(); while self.peek() == &TsTok::Semi { self.advance(); } Some(TsStmt::IncDec(expr, "--".to_string())) }
                    TsTok::Semi => { self.advance(); Some(TsStmt::Expr(expr)) }
                    _ => Some(TsStmt::Expr(expr)),
                }
            }
        }
    }

    fn parse_params(&mut self) -> Vec<String> {
        let mut params = Vec::new();
        self.expect(TsTok::LParen);
        while self.peek() != &TsTok::RParen && !self.at_eof() {
            if self.peek() == &TsTok::Spread { self.advance(); }
            while matches!(self.ident_str().as_deref(), Some("public") | Some("private") | Some("protected") | Some("readonly")) { self.advance(); }
            if let Some(p) = self.ident_str() { params.push(p); self.advance(); }
            else { self.advance(); }
            if self.peek() == &TsTok::Question { self.advance(); }
            self.skip_type_annotation();
            if self.peek() == &TsTok::Eq { self.advance(); self.parse_expr(); }
            if self.peek() == &TsTok::Comma { self.advance(); }
        }
        self.expect(TsTok::RParen); params
    }

    fn parse_class_body(&mut self) -> Vec<TsStmt> {
        let mut stmts = Vec::new();
        self.expect(TsTok::LBrace); self.skip_nl();
        while self.peek() != &TsTok::RBrace && !self.at_eof() {
            while self.peek() == &TsTok::At { self.advance(); if let Some(_) = self.ident_str() { self.advance(); }
                if self.peek() == &TsTok::LParen { self.advance(); let mut d = 1; while d > 0 && !self.at_eof() { match self.advance() { TsTok::LParen => d += 1, TsTok::RParen => d -= 1, _ => {} } } }
                self.skip_nl();
            }
            while matches!(self.ident_str().as_deref(), Some("public") | Some("private") | Some("protected") | Some("readonly") | Some("abstract") | Some("override")) { self.advance(); }
            let is_static = self.ident_str().as_deref() == Some("static");
            if is_static { self.advance(); }
            let is_async = self.ident_str().as_deref() == Some("async");
            if is_async { self.advance(); }
            if self.ident_str().as_deref() == Some("get") || self.ident_str().as_deref() == Some("set") {
                self.advance(); // accessor keyword
            }
            let name = self.ident_str().unwrap_or_default(); self.advance();
            if self.peek() == &TsTok::Question { self.advance(); }
            if self.peek() == &TsTok::LParen {
                // Method
                self.skip_generic_params();
                let params = self.parse_params();
                self.skip_type_annotation();
                let body = if self.peek() == &TsTok::LBrace { self.parse_block() } else { while self.peek() == &TsTok::Semi { self.advance(); } Vec::new() };
                stmts.push(TsStmt::Method(is_static, is_async, name, params, body));
            } else {
                // Property
                self.skip_type_annotation();
                let init = if self.peek() == &TsTok::Eq { self.advance(); Some(self.parse_expr()) } else { None };
                while matches!(self.peek(), TsTok::Semi | TsTok::Newline) { self.advance(); }
                stmts.push(TsStmt::Property(name, init));
            }
            self.skip_nl();
        }
        self.expect(TsTok::RBrace); stmts
    }

    fn parse_expr(&mut self) -> TsExpr { self.parse_ternary() }
    fn parse_ternary(&mut self) -> TsExpr { let e = self.parse_or(); if self.peek() == &TsTok::Question { self.advance(); let then = self.parse_expr(); self.expect(TsTok::Colon); let else_e = self.parse_ternary(); TsExpr::Ternary(Box::new(e), Box::new(then), Box::new(else_e)) } else { e } }
    fn parse_or(&mut self) -> TsExpr { let mut l = self.parse_and(); while self.peek() == &TsTok::PipePipe { self.advance(); let r = self.parse_and(); l = TsExpr::Binary(Box::new(l), "||".to_string(), Box::new(r)); } l }
    fn parse_and(&mut self) -> TsExpr { let mut l = self.parse_cmp(); while self.peek() == &TsTok::AmpAmp { self.advance(); let r = self.parse_cmp(); l = TsExpr::Binary(Box::new(l), "&&".to_string(), Box::new(r)); } l }
    fn parse_cmp(&mut self) -> TsExpr { let mut l = self.parse_add();
        loop { let op = match self.peek() { TsTok::EqEq | TsTok::EqEqEq => "==", TsTok::NotEq | TsTok::NotEqEq => "!=", TsTok::Lt => "<", TsTok::Gt => ">", TsTok::LtEq => "<=", TsTok::GtEq => ">=", _ => break }; self.advance(); let r = self.parse_add(); l = TsExpr::Binary(Box::new(l), op.to_string(), Box::new(r)); } l }
    fn parse_add(&mut self) -> TsExpr { let mut l = self.parse_mul(); loop { let op = match self.peek() { TsTok::Plus => "+", TsTok::Minus => "-", _ => break }; self.advance(); let r = self.parse_mul(); l = TsExpr::Binary(Box::new(l), op.to_string(), Box::new(r)); } l }
    fn parse_mul(&mut self) -> TsExpr { let mut l = self.parse_unary(); loop { let op = match self.peek() { TsTok::Star => "*", TsTok::Slash => "/", TsTok::Percent => "%", TsTok::StarStar => "**", _ => break }; self.advance(); let r = self.parse_unary(); l = TsExpr::Binary(Box::new(l), op.to_string(), Box::new(r)); } l }
    fn parse_unary(&mut self) -> TsExpr {
        match self.peek().clone() {
            TsTok::Bang => { self.advance(); TsExpr::Unary("!".to_string(), Box::new(self.parse_unary())) }
            TsTok::Minus => { self.advance(); TsExpr::Unary("-".to_string(), Box::new(self.parse_unary())) }
            TsTok::PlusPlus => { self.advance(); let e = self.parse_unary(); TsExpr::Unary("++".to_string(), Box::new(e)) }
            TsTok::MinusMinus => { self.advance(); let e = self.parse_unary(); TsExpr::Unary("--".to_string(), Box::new(e)) }
            TsTok::Ident(ref name) if name == "typeof" => { self.advance(); TsExpr::Typeof(Box::new(self.parse_unary())) }
            TsTok::Ident(ref name) if name == "await" => { self.advance(); TsExpr::Await(Box::new(self.parse_unary())) }
            TsTok::Spread => { self.advance(); TsExpr::Spread(Box::new(self.parse_unary())) }
            _ => self.parse_postfix(),
        }
    }
    fn parse_postfix(&mut self) -> TsExpr {
        let mut expr = self.parse_primary();
        loop { match self.peek().clone() {
            TsTok::Dot => { self.advance(); let f = self.ident_str().unwrap_or_default(); self.advance();
                if self.peek() == &TsTok::LParen { self.advance(); let mut args = Vec::new();
                    while self.peek() != &TsTok::RParen && !self.at_eof() { args.push(self.parse_expr()); if self.peek() == &TsTok::Comma { self.advance(); } }
                    self.expect(TsTok::RParen); expr = TsExpr::Call(Box::new(TsExpr::Member(Box::new(expr), f)), args);
                } else { expr = TsExpr::Member(Box::new(expr), f); } }
            TsTok::QuestionDot => { self.advance(); let f = self.ident_str().unwrap_or_default(); self.advance(); expr = TsExpr::OptMember(Box::new(expr), f); }
            TsTok::LParen => { self.advance(); let mut args = Vec::new();
                while self.peek() != &TsTok::RParen && !self.at_eof() { args.push(self.parse_expr()); if self.peek() == &TsTok::Comma { self.advance(); } }
                self.expect(TsTok::RParen); expr = TsExpr::Call(Box::new(expr), args); }
            TsTok::LBracket => { self.advance(); let idx = self.parse_expr(); self.expect(TsTok::RBracket); expr = TsExpr::Index(Box::new(expr), Box::new(idx)); }
            TsTok::Bang => { self.advance(); expr = TsExpr::NonNull(Box::new(expr)); }
            TsTok::Ident(ref name) if name == "as" => { self.advance(); let t = self.ident_str().unwrap_or_default(); self.advance(); self.skip_generic_params(); expr = TsExpr::As(Box::new(expr), t); }
            _ => break,
        }} expr
    }
    fn parse_primary(&mut self) -> TsExpr {
        match self.peek().clone() {
            TsTok::IntLit(n) => { self.advance(); TsExpr::Int(n) }
            TsTok::FloatLit(f) => { self.advance(); TsExpr::Float(f) }
            TsTok::StrLit(s) => { self.advance(); TsExpr::Str(s) }
            TsTok::TemplateLit(s) => { self.advance(); TsExpr::Template(s) }
            TsTok::Ident(ref name) if name == "true" => { self.advance(); TsExpr::Bool(true) }
            TsTok::Ident(ref name) if name == "false" => { self.advance(); TsExpr::Bool(false) }
            TsTok::Ident(ref name) if name == "null" => { self.advance(); TsExpr::Null }
            TsTok::Ident(ref name) if name == "undefined" => { self.advance(); TsExpr::Undefined }
            TsTok::Ident(ref name) if name == "new" => { self.advance(); let callee = self.parse_postfix();
                if self.peek() == &TsTok::LParen { self.advance(); let mut args = Vec::new();
                    while self.peek() != &TsTok::RParen && !self.at_eof() { args.push(self.parse_expr()); if self.peek() == &TsTok::Comma { self.advance(); } }
                    self.expect(TsTok::RParen); TsExpr::New(Box::new(callee), args) }
                else { TsExpr::New(Box::new(callee), Vec::new()) } }
            TsTok::Ident(name) => { self.advance(); TsExpr::Name(name) }
            TsTok::LParen => {
                let saved = self.pos; self.advance();
                // Try arrow function
                let mut params = Vec::new(); let mut is_arrow = true;
                while self.peek() != &TsTok::RParen && !self.at_eof() {
                    if let Some(p) = self.ident_str() { params.push(p); self.advance(); }
                    else { is_arrow = false; break; }
                    if self.peek() == &TsTok::Question { self.advance(); }
                    self.skip_type_annotation();
                    if self.peek() == &TsTok::Eq { self.advance(); self.parse_expr(); }
                    if self.peek() == &TsTok::Comma { self.advance(); }
                }
                if is_arrow && self.peek() == &TsTok::RParen { self.advance(); self.skip_type_annotation();
                    if self.peek() == &TsTok::Arrow { self.advance();
                        let body = if self.peek() == &TsTok::LBrace { self.parse_block() } else { vec![TsStmt::Return(Some(self.parse_expr()))] };
                        return TsExpr::Arrow(params, body); } }
                self.pos = saved; self.advance();
                let expr = self.parse_expr(); self.expect(TsTok::RParen); expr
            }
            TsTok::LBracket => { self.advance(); let mut elems = Vec::new();
                while self.peek() != &TsTok::RBracket && !self.at_eof() { self.skip_nl(); elems.push(self.parse_expr()); self.skip_nl(); if self.peek() == &TsTok::Comma { self.advance(); } }
                self.expect(TsTok::RBracket); TsExpr::Array(elems) }
            TsTok::LBrace => { self.advance(); self.skip_nl(); let mut pairs = Vec::new();
                while self.peek() != &TsTok::RBrace && !self.at_eof() {
                    let key = if let TsTok::StrLit(s) = self.peek().clone() { self.advance(); s }
                    else if let Some(k) = self.ident_str() { self.advance(); k }
                    else { self.advance(); String::new() };
                    let val = if self.peek() == &TsTok::Colon { self.advance(); self.parse_expr() } else { TsExpr::Name(key.clone()) };
                    pairs.push((key, val));
                    if self.peek() == &TsTok::Comma { self.advance(); } self.skip_nl();
                }
                self.expect(TsTok::RBrace); TsExpr::Object(pairs) }
            _ => { self.advance(); TsExpr::Int(0) }
        }
    }
}

// ============ Emitter ============
fn san(name: &str) -> String { match name { "set" | "to" | "display" | "return" | "if" | "else" | "repeat" | "while" | "define" | "with" | "and" | "for" | "each" | "in" | "break" | "continue" | "check" | "structure" | "field" | "choice" | "variant" | "create" | "is" | "not" | "true" | "false" | "import" | "borrow" | "spawn" | "send" | "receive" | "from" | "channel" | "given" | "returning" | "as" | "trait" | "implement" | "equals" | "async" | "await" | "try" | "external" => format!("{}_", name), _ => name.to_string() } }
fn ind(out: &mut String, d: usize) { for _ in 0..d { out.push_str("    "); } }

pub fn emit_ernos_from_typescript(_filename: &str, source: &str) -> String {
    let mut lexer = TsLexer::new(source);
    let tokens = lexer.tokenize();
    let mut parser = TsParser::new(tokens);
    let stmts = parser.parse_file();
    let mut out = String::new();
    out.push_str("# Transpiled from TypeScript\n\n");
    for s in &stmts {
        // Drop bare top-level main/main() calls — ErnosPlain runs main automatically
        match s {
            TsStmt::Expr(TsExpr::Name(n)) if n == "main" => continue,
            TsStmt::Expr(TsExpr::Call(func, args)) => {
                if let TsExpr::Name(n) = func.as_ref() {
                    if n == "main" && args.is_empty() { continue; }
                }
                emit_ts_stmt(&mut out, s, 0, None);
            }
            _ => emit_ts_stmt(&mut out, s, 0, None),
        }
    }
    out
}

fn emit_ts_stmt(out: &mut String, stmt: &TsStmt, depth: usize, class_name: Option<&str>) {
    match stmt {
        TsStmt::Import(module, _) => { ind(out, depth); out.push_str(&format!("# import \"{}\"\n", module)); }
        TsStmt::Export(inner) => emit_ts_stmt(out, inner, depth, class_name),
        TsStmt::TypeAlias(_) => {}
        TsStmt::VarDecl(_, name, init) => { ind(out, depth); out.push_str(&format!("set {} to ", san(name)));
            if let Some(v) = init { emit_ts_expr(out, v); } else { out.push('0'); } out.push('\n'); }
        TsStmt::FnDecl(_, name, params, body) => {
            ind(out, depth); let sname = san(name);
            if params.is_empty() { out.push_str(&format!("define {}:\n", sname)); }
            else { let p: Vec<String> = params.iter().map(|p| format!("{} as Int", san(p))).collect(); out.push_str(&format!("define {} with {}:\n", sname, p.join(" and "))); }
            for s in body { emit_ts_stmt(out, s, depth + 1, None); }
            if body.is_empty() { ind(out, depth + 1); out.push_str("return 0\n"); } out.push('\n');
        }
        TsStmt::Class(name, _, body) => {
            let fields: Vec<_> = body.iter().filter_map(|s| if let TsStmt::Property(n, _) = s { Some(n.clone()) } else { None }).collect();
            if !fields.is_empty() { ind(out, depth); out.push_str(&format!("structure {}:\n", name));
                for f in &fields { ind(out, depth + 1); out.push_str(&format!("field {} as Int\n", san(f))); } out.push('\n'); }
            for s in body { match s { TsStmt::Property(..) => {} _ => emit_ts_stmt(out, s, depth, Some(name)) } }
        }
        TsStmt::Interface(name, fields) => {
            ind(out, depth); out.push_str(&format!("structure {}:\n", name));
            for (fname, _) in fields { ind(out, depth + 1); out.push_str(&format!("field {} as Int\n", san(fname))); }
            out.push('\n');
        }
        TsStmt::Enum(name, variants) => {
            ind(out, depth); out.push_str(&format!("choice {}:\n", name));
            for (vname, _) in variants { ind(out, depth + 1); out.push_str(&format!("variant {}\n", vname)); }
            out.push('\n');
        }
        TsStmt::Method(_, _, name, params, body) => {
            ind(out, depth);
            let sname = san(name);
            if let Some(cn) = class_name {
                let filtered: Vec<_> = params.iter().filter(|p| *p != "this").collect();
                if filtered.is_empty() { out.push_str(&format!("define method {} on {}:\n", sname, cn)); }
                else { let p: Vec<String> = filtered.iter().map(|p| format!("{} as Int", san(p))).collect(); out.push_str(&format!("define method {} on {} with {}:\n", sname, cn, p.join(" and "))); }
            } else {
                if params.is_empty() { out.push_str(&format!("define {}:\n", sname)); }
                else { let p: Vec<String> = params.iter().map(|p| format!("{} as Int", san(p))).collect(); out.push_str(&format!("define {} with {}:\n", sname, p.join(" and "))); }
            }
            for s in body { emit_ts_stmt(out, s, depth + 1, None); }
            if body.is_empty() { ind(out, depth + 1); out.push_str("return 0\n"); } out.push('\n');
        }
        TsStmt::Property(..) => {}
        TsStmt::Assign(target, val) => { ind(out, depth);
            if let TsExpr::Name(n) = target { out.push_str(&format!("set {} to ", san(n))); }
            else { out.push_str("set _x to "); }
            emit_ts_expr(out, val); out.push('\n'); }
        TsStmt::AugAssign(target, op, val) => { ind(out, depth);
            if let TsExpr::Name(n) = target { let sn = san(n); out.push_str(&format!("set {0} to {0} {1} ", sn, op)); } else { out.push_str("set _x to _x + "); }
            emit_ts_expr(out, val); out.push('\n'); }
        TsStmt::IncDec(target, op) => { ind(out, depth);
            if let TsExpr::Name(n) = target { let sn = san(n); let o = if op == "++" { "+" } else { "-" }; out.push_str(&format!("set {0} to {0} {1} 1\n", sn, o)); } }
        TsStmt::If(cond, then, else_b) => { ind(out, depth); out.push_str("if "); emit_ts_expr(out, cond); out.push_str(":\n");
            for s in then { emit_ts_stmt(out, s, depth + 1, None); }
            if let Some(eb) = else_b { if eb.len() == 1 { if let TsStmt::If(..) = &eb[0] { ind(out, depth); out.push_str("else "); emit_ts_stmt(out, &eb[0], depth, None); return; } }
                ind(out, depth); out.push_str("else:\n"); for s in eb { emit_ts_stmt(out, s, depth + 1, None); } } }
        TsStmt::While(cond, body) => { ind(out, depth); out.push_str("repeat while "); emit_ts_expr(out, cond); out.push_str(":\n"); for s in body { emit_ts_stmt(out, s, depth + 1, None); } }
        TsStmt::DoWhile(body, cond) => { ind(out, depth); out.push_str("repeat while true:\n"); for s in body { emit_ts_stmt(out, s, depth + 1, None); } ind(out, depth + 1); out.push_str("if not "); emit_ts_expr(out, cond); out.push_str(":\n"); ind(out, depth + 2); out.push_str("break\n"); }
        TsStmt::For(init, cond, post, body) => { if let Some(i) = init { emit_ts_stmt(out, i, depth, None); }
            ind(out, depth); out.push_str("repeat while "); if let Some(c) = cond { emit_ts_expr(out, c); } else { out.push_str("true"); } out.push_str(":\n");
            for s in body { emit_ts_stmt(out, s, depth + 1, None); }
            if let Some(p) = post { emit_ts_stmt(out, p, depth + 1, None); } }
        TsStmt::ForOf(var, iter, body) | TsStmt::ForIn(var, iter, body) => {
            ind(out, depth); out.push_str(&format!("for each {} in ", san(var))); emit_ts_expr(out, iter); out.push_str(":\n");
            for s in body { emit_ts_stmt(out, s, depth + 1, None); } }
        TsStmt::Return(val) => { ind(out, depth); out.push_str("return "); if let Some(v) = val { emit_ts_expr(out, v); } else { out.push('0'); } out.push('\n'); }
        TsStmt::Break => { ind(out, depth); out.push_str("break\n"); }
        TsStmt::Continue => { ind(out, depth); out.push_str("continue\n"); }
        TsStmt::Expr(expr) => {
            if let TsExpr::Call(func, args) = expr {
                if let TsExpr::Member(obj, method) = func.as_ref() {
                    if let TsExpr::Name(n) = obj.as_ref() { if n == "console" && (method == "log" || method == "error" || method == "warn") {
                        for a in args { ind(out, depth); out.push_str("display "); emit_ts_expr(out, a); out.push('\n'); } if args.is_empty() { ind(out, depth); out.push_str("display \"\"\n"); } return;
                    }}
                }
            }
            ind(out, depth); emit_ts_expr(out, expr); out.push('\n'); }
        TsStmt::Switch(tag, cases, default) => {
            ind(out, depth); out.push_str("check "); emit_ts_expr(out, tag); out.push_str(":\n");
            for (vals, body) in cases { for v in vals { ind(out, depth + 1); out.push_str("if "); emit_ts_expr(out, v); out.push_str(":\n"); }
                for s in body { if let TsStmt::Break = s { continue; } emit_ts_stmt(out, s, depth + 2, None); } }
            if let Some(def) = default { ind(out, depth + 1); out.push_str("if default:\n"); for s in def { if let TsStmt::Break = s { continue; } emit_ts_stmt(out, s, depth + 2, None); } } }
        TsStmt::TryCatch(body, catch, _) => { ind(out, depth); out.push_str("# try\n");
            for s in body { emit_ts_stmt(out, s, depth, None); }
            if let Some((_, cbody)) = catch { ind(out, depth); out.push_str("# catch\n"); for s in cbody { emit_ts_stmt(out, s, depth, None); } } }
        TsStmt::Throw(e) => { ind(out, depth); out.push_str("# throw "); emit_ts_expr(out, e); out.push('\n'); }
    }
}

fn emit_ts_expr(out: &mut String, expr: &TsExpr) {
    match expr {
        TsExpr::Int(n) => out.push_str(&n.to_string()),
        TsExpr::Float(f) => out.push_str(&format!("{}", f)),
        TsExpr::Str(s) => out.push_str(&format!("\"{}\"", s.replace('"', "\\\"").replace('\n', "\\n"))),
        TsExpr::Template(s) => out.push_str(&format!("f\"{}\"", s.replace('"', "\\\""))),
        TsExpr::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        TsExpr::Null | TsExpr::Undefined => out.push('0'),
        TsExpr::Name(n) => out.push_str(&san(n)),
        TsExpr::Binary(l, op, r) => {
            emit_ts_expr(out, l);
            let ep = match op.as_str() { "+" => " + ", "-" => " - ", "*" => " * ", "/" => " / ", "%" => " modulo ", "**" => " * ",
                "==" => " equals ", "!=" => " != ", "<" => " < ", ">" => " > ", "<=" => " <= ", ">=" => " >= ",
                "&&" => " and also ", "||" => " or else ", _ => " + " };
            out.push_str(ep); emit_ts_expr(out, r);
        }
        TsExpr::Unary(op, inner) => { if op == "!" { out.push_str("not "); emit_ts_expr(out, inner); } else if op == "-" { out.push_str("(0 - "); emit_ts_expr(out, inner); out.push(')'); } else { emit_ts_expr(out, inner); } }
        TsExpr::Call(func, args) => {
            if let TsExpr::Member(obj, method) = func.as_ref() {
                match method.as_str() {
                    "length" => { out.push_str("length("); emit_ts_expr(out, obj); out.push(')'); return; }
                    "push" if args.len() == 1 => { out.push_str("append("); emit_ts_expr(out, obj); out.push_str(" and "); emit_ts_expr(out, &args[0]); out.push(')'); return; }
                    "toString" => { out.push_str("int_to_string("); emit_ts_expr(out, obj); out.push(')'); return; }
                    "parseInt" if args.len() == 1 => { out.push_str("string_to_int("); emit_ts_expr(out, &args[0]); out.push(')'); return; }
                    "includes" if args.len() == 1 => { out.push_str("string_contains("); emit_ts_expr(out, obj); out.push_str(" and "); emit_ts_expr(out, &args[0]); out.push(')'); return; }
                    _ => {}
                }
            }
            emit_ts_expr(out, func); out.push('('); for (i, a) in args.iter().enumerate() { if i > 0 { out.push_str(" and "); } emit_ts_expr(out, a); } out.push(')');
        }
        TsExpr::Member(obj, field) | TsExpr::OptMember(obj, field) => { emit_ts_expr(out, obj); out.push('.'); out.push_str(field); }
        TsExpr::Index(arr, idx) => { out.push_str("get_list("); emit_ts_expr(out, arr); out.push_str(" and "); emit_ts_expr(out, idx); out.push(')'); }
        TsExpr::Array(elems) => { out.push('['); for (i, e) in elems.iter().enumerate() { if i > 0 { out.push_str(", "); } emit_ts_expr(out, e); } out.push(']'); }
        TsExpr::Object(pairs) => { out.push_str("# object: {"); for (i, (k, v)) in pairs.iter().enumerate() { if i > 0 { out.push_str(", "); } out.push_str(k); out.push_str(": "); emit_ts_expr(out, v); } out.push('}'); }
        TsExpr::Arrow(params, body) => {
            if params.is_empty() { out.push_str("given _:"); } else { out.push_str(&format!("given {}:", params.iter().map(|p| san(p)).collect::<Vec<_>>().join(" and "))); }
            if body.len() == 1 { if let TsStmt::Return(Some(e)) = &body[0] { out.push(' '); emit_ts_expr(out, e); return; } }
            out.push('\n'); for s in body { emit_ts_stmt(out, s, 1, None); }
        }
        TsExpr::New(callee, args) => { out.push_str("create "); emit_ts_expr(out, callee); if !args.is_empty() { out.push(':'); for (i, a) in args.iter().enumerate() { out.push_str(&format!("\n    field{} is ", i)); emit_ts_expr(out, a); } } }
        TsExpr::Ternary(cond, then, else_e) => { out.push_str("(if "); emit_ts_expr(out, cond); out.push_str(": "); emit_ts_expr(out, then); out.push_str(" else "); emit_ts_expr(out, else_e); out.push(')'); }
        TsExpr::Typeof(inner) => { out.push_str("# typeof "); emit_ts_expr(out, inner); }
        TsExpr::Spread(inner) => emit_ts_expr(out, inner),
        TsExpr::Await(inner) => { out.push_str("await "); emit_ts_expr(out, inner); }
        TsExpr::NonNull(inner) | TsExpr::As(inner, _) => emit_ts_expr(out, inner),
    }
}
