/// C Header Binding Generator for ErnosPlain
///
/// Parses C header files (.h) and emits ErnosPlain binding modules (.ep)
/// with `external define` declarations, struct definitions, enum constants,
/// and type mappings.
///
/// Architecture:
///   C header text → C tokenizer → C declaration parser → Ernos emitter → .ep file
///
/// Handles: function declarations, struct/typedef, enum, #define constants,
/// function pointers (as opaque Int), unions, variadic functions, and type
/// qualifiers (const, volatile, restrict, unsigned, signed, static, inline).
///
/// Does NOT require libclang — zero external dependencies.

use std::collections::HashMap;

// ============ C Tokenizer ============

#[derive(Debug, Clone, PartialEq)]
pub enum CToken {
    Ident(String),
    IntLit(i64),
    StrLit(String),
    CharLit(char),
    // Punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semi,
    Comma,
    Star,
    Ampersand,
    Equals,
    Colon,
    Dot,
    Arrow,     // ->
    Ellipsis,  // ...
    Hash,      // #
    DoubleHash, // ##
    Lt,
    Gt,
    Plus,
    Minus,
    Slash,
    Percent,
    Pipe,
    Caret,
    Tilde,
    Bang,
    Question,
    LShift,    // <<
    RShift,    // >>
    // Preprocessor (we handle #define, #include, #ifdef etc.)
    Newline,
    Eof,
}

pub struct CLexer {
    chars: Vec<char>,
    pos: usize,
}

impl CLexer {
    pub fn new(input: &str) -> Self {
        CLexer {
            chars: input.chars().collect(),
            pos: 0,
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

    fn skip_whitespace_no_newline(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' || c == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn skip_block_comment(&mut self) {
        // Already consumed /*
        loop {
            match self.advance() {
                Some('*') => {
                    if self.peek() == Some('/') {
                        self.advance();
                        return;
                    }
                }
                None => return,
                _ => {}
            }
        }
    }

    pub fn tokenize(&mut self) -> Vec<CToken> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_no_newline();
            match self.peek() {
                None => {
                    tokens.push(CToken::Eof);
                    break;
                }
                Some('\n') => {
                    self.advance();
                    tokens.push(CToken::Newline);
                }
                Some('/') => {
                    self.advance();
                    if self.peek() == Some('/') {
                        self.advance();
                        self.skip_line_comment();
                    } else if self.peek() == Some('*') {
                        self.advance();
                        self.skip_block_comment();
                    } else {
                        tokens.push(CToken::Slash);
                    }
                }
                Some('#') => {
                    self.advance();
                    if self.peek() == Some('#') {
                        self.advance();
                        tokens.push(CToken::DoubleHash);
                    } else {
                        tokens.push(CToken::Hash);
                    }
                }
                Some('"') => {
                    self.advance();
                    let mut s = String::new();
                    loop {
                        match self.advance() {
                            Some('"') => break,
                            Some('\\') => {
                                if let Some(esc) = self.advance() {
                                    match esc {
                                        'n' => s.push('\n'),
                                        't' => s.push('\t'),
                                        '\\' => s.push('\\'),
                                        '"' => s.push('"'),
                                        '0' => s.push('\0'),
                                        _ => { s.push('\\'); s.push(esc); }
                                    }
                                }
                            }
                            Some(c) => s.push(c),
                            None => break,
                        }
                    }
                    tokens.push(CToken::StrLit(s));
                }
                Some('\'') => {
                    self.advance();
                    let c = if self.peek() == Some('\\') {
                        self.advance();
                        match self.advance() {
                            Some('n') => '\n',
                            Some('t') => '\t',
                            Some('\\') => '\\',
                            Some('0') => '\0',
                            Some(c) => c,
                            None => '\0',
                        }
                    } else {
                        self.advance().unwrap_or('\0')
                    };
                    if self.peek() == Some('\'') { self.advance(); }
                    tokens.push(CToken::CharLit(c));
                }
                Some(c) if c.is_ascii_digit() => {
                    let mut num = String::new();
                    let is_hex = c == '0' && (self.chars.get(self.pos + 1) == Some(&'x') || self.chars.get(self.pos + 1) == Some(&'X'));
                    if is_hex {
                        num.push(self.advance().unwrap()); // 0
                        num.push(self.advance().unwrap()); // x
                        while let Some(c) = self.peek() {
                            if c.is_ascii_hexdigit() || c == '_' {
                                if c != '_' { num.push(c); }
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    } else {
                        while let Some(c) = self.peek() {
                            if c.is_ascii_digit() || c == '_' {
                                if c != '_' { num.push(c); }
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    }
                    // Skip type suffixes like UL, ULL, L, LL, etc.
                    while let Some(c) = self.peek() {
                        if c == 'u' || c == 'U' || c == 'l' || c == 'L' {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    let val = if is_hex {
                        i64::from_str_radix(&num[2..], 16).unwrap_or(0)
                    } else {
                        num.parse::<i64>().unwrap_or(0)
                    };
                    tokens.push(CToken::IntLit(val));
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
                    tokens.push(CToken::Ident(ident));
                }
                Some('(') => { self.advance(); tokens.push(CToken::LParen); }
                Some(')') => { self.advance(); tokens.push(CToken::RParen); }
                Some('{') => { self.advance(); tokens.push(CToken::LBrace); }
                Some('}') => { self.advance(); tokens.push(CToken::RBrace); }
                Some('[') => { self.advance(); tokens.push(CToken::LBracket); }
                Some(']') => { self.advance(); tokens.push(CToken::RBracket); }
                Some(';') => { self.advance(); tokens.push(CToken::Semi); }
                Some(',') => { self.advance(); tokens.push(CToken::Comma); }
                Some('*') => { self.advance(); tokens.push(CToken::Star); }
                Some('&') => { self.advance(); tokens.push(CToken::Ampersand); }
                Some('=') => { self.advance(); tokens.push(CToken::Equals); }
                Some(':') => { self.advance(); tokens.push(CToken::Colon); }
                Some('.') => {
                    self.advance();
                    if self.peek() == Some('.') {
                        self.advance();
                        if self.peek() == Some('.') {
                            self.advance();
                        }
                        tokens.push(CToken::Ellipsis);
                    } else {
                        tokens.push(CToken::Dot);
                    }
                }
                Some('-') => {
                    self.advance();
                    if self.peek() == Some('>') {
                        self.advance();
                        tokens.push(CToken::Arrow);
                    } else {
                        tokens.push(CToken::Minus);
                    }
                }
                Some('<') => {
                    self.advance();
                    if self.peek() == Some('<') {
                        self.advance();
                        tokens.push(CToken::LShift);
                    } else {
                        tokens.push(CToken::Lt);
                    }
                }
                Some('>') => {
                    self.advance();
                    if self.peek() == Some('>') {
                        self.advance();
                        tokens.push(CToken::RShift);
                    } else {
                        tokens.push(CToken::Gt);
                    }
                }
                Some('+') => { self.advance(); tokens.push(CToken::Plus); }
                Some('|') => { self.advance(); tokens.push(CToken::Pipe); }
                Some('^') => { self.advance(); tokens.push(CToken::Caret); }
                Some('~') => { self.advance(); tokens.push(CToken::Tilde); }
                Some('!') => { self.advance(); tokens.push(CToken::Bang); }
                Some('?') => { self.advance(); tokens.push(CToken::Question); }
                Some('%') => { self.advance(); tokens.push(CToken::Percent); }
                Some(_) => { self.advance(); } // skip unknown chars
            }
        }
        tokens
    }
}

// ============ C Declaration Types ============

#[derive(Debug, Clone)]
pub enum CType {
    Void,
    Int,       // int, short, long, long long, int8_t...int64_t, size_t, etc.
    Str,       // char*, const char*
    Bool,      // bool, _Bool
    Float,     // float, double (mapped to Int with warning)
    Pointer,   // T* (opaque pointer → Int)
    FuncPtr(Vec<CType>, Box<CType>),  // function pointer: args → return
    Struct(String),
    Enum(String),
    Named(String),  // unresolved typedef name
}

#[derive(Debug, Clone)]
pub struct CParam {
    pub name: String,
    pub ctype: CType,
}

#[derive(Debug, Clone)]
pub enum CDecl {
    Function {
        name: String,
        params: Vec<CParam>,
        return_type: CType,
        is_variadic: bool,
        is_static: bool,
        is_inline: bool,
    },
    Struct {
        name: String,
        fields: Vec<(String, CType)>,
    },
    Enum {
        name: String,
        variants: Vec<(String, Option<i64>)>,
    },
    Typedef {
        name: String,
        target: CType,
    },
    Define {
        name: String,
        value: DefineValue,
    },
    Include {
        path: String,
        is_system: bool,
    },
}

#[derive(Debug, Clone)]
pub enum DefineValue {
    IntVal(i64),
    StrVal(String),
    FuncMacro(Vec<String>, String), // params, expansion text
    Other(String),                   // unparseable expansion
}

// ============ C Header Parser ============

pub struct CHeaderParser {
    tokens: Vec<CToken>,
    pos: usize,
    typedefs: HashMap<String, CType>,
}

impl CHeaderParser {
    pub fn new(tokens: Vec<CToken>) -> Self {
        let mut typedefs = HashMap::new();
        // Pre-register standard type names
        for name in &["size_t", "ssize_t", "ptrdiff_t", "intptr_t", "uintptr_t",
                       "int8_t", "int16_t", "int32_t", "int64_t",
                       "uint8_t", "uint16_t", "uint32_t", "uint64_t",
                       "time_t", "off_t", "pid_t", "uid_t", "gid_t",
                       "socklen_t", "mode_t", "dev_t", "ino_t", "nlink_t",
                       "FILE", "DIR", "va_list"] {
            typedefs.insert(name.to_string(), CType::Int);
        }
        CHeaderParser { tokens, pos: 0, typedefs }
    }

    fn peek(&self) -> &CToken {
        self.tokens.get(self.pos).unwrap_or(&CToken::Eof)
    }

    fn advance(&mut self) -> CToken {
        let tok = self.tokens.get(self.pos).cloned().unwrap_or(CToken::Eof);
        self.pos += 1;
        tok
    }

    fn expect_ident(&mut self) -> Option<String> {
        if let CToken::Ident(name) = self.peek().clone() {
            self.advance();
            Some(name)
        } else {
            None
        }
    }

    fn skip_newlines(&mut self) {
        while *self.peek() == CToken::Newline {
            self.advance();
        }
    }

    fn skip_to_newline(&mut self) {
        while *self.peek() != CToken::Newline && *self.peek() != CToken::Eof {
            self.advance();
        }
    }

    fn skip_braced_body(&mut self) {
        // Skip past { ... } including nested braces
        let mut depth = 0;
        if *self.peek() == CToken::LBrace {
            self.advance();
            depth = 1;
        }
        while depth > 0 {
            match self.advance() {
                CToken::LBrace => depth += 1,
                CToken::RBrace => depth -= 1,
                CToken::Eof => return,
                _ => {}
            }
        }
    }

    /// Parse a C type from the token stream
    fn parse_type(&mut self) -> CType {
        // Skip qualifiers
        loop {
            match self.peek() {
                CToken::Ident(s) if s == "const" || s == "volatile" || s == "restrict"
                    || s == "__restrict" || s == "__const" || s == "register"
                    || s == "_Atomic" || s == "__attribute__" => {
                    self.advance();
                    // Skip __attribute__((...))
                    if self.peek() == &CToken::LParen {
                        self.advance();
                        let mut depth = 1;
                        while depth > 0 {
                            match self.advance() {
                                CToken::LParen => depth += 1,
                                CToken::RParen => depth -= 1,
                                CToken::Eof => break,
                                _ => {}
                            }
                        }
                    }
                }
                _ => break,
            }
        }

        let mut is_unsigned = false;
        let mut is_signed = false;
        let mut is_long = 0u32;
        let mut is_short = false;
        let mut was_char = false;
        let mut base_type: Option<CType> = None;

        // Consume type specifiers
        loop {
            match self.peek() {
                CToken::Ident(s) => {
                    let s = s.clone();
                    match s.as_str() {
                        "unsigned" => { self.advance(); is_unsigned = true; }
                        "signed" => { self.advance(); is_signed = true; }
                        "long" => { self.advance(); is_long += 1; }
                        "short" => { self.advance(); is_short = true; }
                        "int" => { self.advance(); base_type = Some(CType::Int); break; }
                        "char" => {
                            self.advance();
                            was_char = true;
                            base_type = Some(CType::Int); // plain char = Int
                            break;
                        }
                        "void" => { self.advance(); base_type = Some(CType::Void); break; }
                        "float" | "double" => { self.advance(); base_type = Some(CType::Float); break; }
                        "bool" | "_Bool" => { self.advance(); base_type = Some(CType::Bool); break; }
                        "struct" => {
                            self.advance();
                            let name = self.expect_ident().unwrap_or_else(|| "anon".into());
                            base_type = Some(CType::Struct(name));
                            break;
                        }
                        "union" => {
                            self.advance();
                            let name = self.expect_ident().unwrap_or_else(|| "anon".into());
                            base_type = Some(CType::Struct(name)); // unions → struct
                            break;
                        }
                        "enum" => {
                            self.advance();
                            let name = self.expect_ident().unwrap_or_else(|| "anon".into());
                            base_type = Some(CType::Enum(name));
                            break;
                        }
                        _ if self.typedefs.contains_key(&s) => {
                            self.advance();
                            base_type = Some(self.typedefs.get(&s).cloned().unwrap_or(CType::Int));
                            break;
                        }
                        _ if is_unsigned || is_signed || is_long > 0 || is_short => {
                            // We've accumulated modifiers (unsigned, long, etc.) and hit
                            // a non-type identifier — that's the variable/param name, stop
                            break;
                        }
                        _ if base_type.is_some() => {
                            // We already have a base type — this identifier is NOT a type,
                            // it's the variable/parameter name. Stop without consuming it.
                            break;
                        }
                        _ => {
                            // No base type yet, assume this is a typedef or user type name
                            self.advance();
                            base_type = Some(CType::Named(s));
                            break;
                        }
                    }
                }
                _ => break,
            }
        }

        // Resolve base type from modifiers
        let result = if let Some(bt) = base_type {
            bt
        } else if is_unsigned || is_signed || is_long > 0 || is_short {
            CType::Int
        } else {
            CType::Int // default
        };

        // Skip qualifiers after base type
        loop {
            match self.peek() {
                CToken::Ident(s) if s == "const" || s == "volatile" || s == "restrict"
                    || s == "__restrict" => {
                    self.advance();
                }
                _ => break,
            }
        }

        // Handle pointer modifiers: *, **, etc.
        let mut ptr_count = 0;
        while *self.peek() == CToken::Star {
            self.advance();
            ptr_count += 1;
            // Skip qualifiers after *
            loop {
                match self.peek() {
                    CToken::Ident(s) if s == "const" || s == "volatile" || s == "restrict"
                        || s == "__restrict" => {
                        self.advance();
                    }
                    _ => break,
                }
            }
        }

        if ptr_count > 0 {
            // char* → Str, void* → Pointer, other* → Pointer
            if was_char {
                return CType::Str;
            }
            match result {
                CType::Void => return CType::Pointer,
                _ => return CType::Pointer,
            }
        }

        result
    }

    /// Parse a function parameter list
    fn parse_params(&mut self) -> (Vec<CParam>, bool) {
        let mut params = Vec::new();
        let mut is_variadic = false;

        if *self.peek() != CToken::LParen {
            return (params, false);
        }
        self.advance(); // (

        // Check for void params
        if let CToken::Ident(s) = self.peek() {
            if s == "void" {
                let save = self.pos;
                self.advance();
                if *self.peek() == CToken::RParen {
                    self.advance();
                    return (params, false);
                }
                self.pos = save;
            }
        }

        if *self.peek() == CToken::RParen {
            self.advance();
            return (params, false);
        }

        loop {
            if *self.peek() == CToken::Ellipsis {
                self.advance();
                is_variadic = true;
                break;
            }

            let param_type = self.parse_type();

            // Handle function pointer parameters: type (*name)(args)
            if *self.peek() == CToken::LParen {
                // Could be function pointer: ret_type (*name)(params)
                self.advance(); // (
                let mut _ptr = false;
                if *self.peek() == CToken::Star {
                    self.advance();
                    _ptr = true;
                }
                let name = self.expect_ident().unwrap_or_else(|| format!("arg{}", params.len()));
                if *self.peek() == CToken::RParen { self.advance(); }
                // Skip the param list of the function pointer
                if *self.peek() == CToken::LParen {
                    let mut depth = 1;
                    self.advance();
                    while depth > 0 {
                        match self.advance() {
                            CToken::LParen => depth += 1,
                            CToken::RParen => depth -= 1,
                            CToken::Eof => break,
                            _ => {}
                        }
                    }
                }
                params.push(CParam { name, ctype: CType::Pointer });
            } else {
                let name = if let CToken::Ident(_) = self.peek() {
                    self.expect_ident().unwrap_or_else(|| format!("arg{}", params.len()))
                } else {
                    format!("arg{}", params.len())
                };

                // Skip array declarators: name[]
                if *self.peek() == CToken::LBracket {
                    while *self.peek() == CToken::LBracket {
                        self.advance();
                        while *self.peek() != CToken::RBracket && *self.peek() != CToken::Eof {
                            self.advance();
                        }
                        if *self.peek() == CToken::RBracket { self.advance(); }
                    }
                }

                params.push(CParam { name, ctype: param_type });
            }

            match self.peek() {
                CToken::Comma => { self.advance(); }
                _ => break,
            }
        }

        if *self.peek() == CToken::RParen {
            self.advance();
        }

        (params, is_variadic)
    }

    /// Parse all declarations from the token stream
    pub fn parse(&mut self) -> Vec<CDecl> {
        let mut decls = Vec::new();

        loop {
            self.skip_newlines();
            match self.peek().clone() {
                CToken::Eof => break,

                // Preprocessor directive
                CToken::Hash => {
                    self.advance(); // #
                    if let Some(directive) = self.expect_ident() {
                        match directive.as_str() {
                            "define" => {
                                if let Some(d) = self.parse_define() {
                                    decls.push(d);
                                }
                            }
                            "include" => {
                                if let Some(d) = self.parse_include() {
                                    decls.push(d);
                                }
                            }
                            "pragma" | "ifdef" | "ifndef" | "endif" | "else" | "elif"
                            | "if" | "undef" | "error" | "warning" | "line" => {
                                self.skip_to_newline();
                            }
                            _ => {
                                self.skip_to_newline();
                            }
                        }
                    }
                }

                // typedef
                CToken::Ident(ref s) if s == "typedef" => {
                    self.advance();
                    if let Some(d) = self.parse_typedef() {
                        if let CDecl::Typedef { ref name, ref target } = d {
                            self.typedefs.insert(name.clone(), target.clone());
                        }
                        decls.push(d);
                    }
                }

                // extern "C" { ... }
                CToken::Ident(ref s) if s == "extern" => {
                    self.advance();
                    if let CToken::StrLit(s) = self.peek().clone() {
                        if s == "C" {
                            self.advance();
                            if *self.peek() == CToken::LBrace {
                                self.advance(); // Skip { — contents parsed normally
                            }
                            continue;
                        }
                    }
                    // extern declaration — parse as function/var
                    if let Some(d) = self.parse_declaration(false, false) {
                        decls.push(d);
                    }
                }

                // static, inline
                CToken::Ident(ref s) if s == "static" || s == "inline"
                    || s == "__inline" || s == "__inline__"
                    || s == "__attribute__" || s == "__extension__" => {
                    let mut is_static = false;
                    let mut is_inline = false;
                    loop {
                        match self.peek() {
                            CToken::Ident(s) => {
                                match s.as_str() {
                                    "static" => { is_static = true; self.advance(); }
                                    "inline" | "__inline" | "__inline__" => { is_inline = true; self.advance(); }
                                    "__extension__" => { self.advance(); }
                                    "__attribute__" => {
                                        self.advance();
                                        if *self.peek() == CToken::LParen {
                                            let mut depth = 1;
                                            self.advance();
                                            while depth > 0 {
                                                match self.advance() {
                                                    CToken::LParen => depth += 1,
                                                    CToken::RParen => depth -= 1,
                                                    CToken::Eof => break,
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                    _ => break,
                                }
                            }
                            _ => break,
                        }
                    }
                    if let Some(d) = self.parse_declaration(is_static, is_inline) {
                        decls.push(d);
                    }
                }

                // struct/union definition
                CToken::Ident(ref s) if s == "struct" || s == "union" => {
                    if let Some(d) = self.parse_struct_or_union() {
                        decls.push(d);
                    }
                }

                // enum definition
                CToken::Ident(ref s) if s == "enum" => {
                    if let Some(d) = self.parse_enum() {
                        decls.push(d);
                    }
                }

                // Closing brace from extern "C"
                CToken::RBrace => {
                    self.advance();
                }

                // Regular declaration (function or variable)
                CToken::Ident(_) => {
                    if let Some(d) = self.parse_declaration(false, false) {
                        decls.push(d);
                    }
                }

                _ => {
                    self.advance(); // skip unexpected token
                }
            }
        }

        decls
    }

    fn parse_define(&mut self) -> Option<CDecl> {
        let name = self.expect_ident()?;

        // Check for function-like macro: #define NAME(params) expansion
        if *self.peek() == CToken::LParen {
            self.advance();
            let mut params = Vec::new();
            loop {
                match self.peek() {
                    CToken::RParen => { self.advance(); break; }
                    CToken::Ident(s) => { params.push(s.clone()); self.advance(); }
                    CToken::Comma => { self.advance(); }
                    CToken::Ellipsis => { params.push("...".to_string()); self.advance(); }
                    _ => { self.advance(); }
                }
            }
            // Collect expansion text
            let mut expansion = String::new();
            while *self.peek() != CToken::Newline && *self.peek() != CToken::Eof {
                match self.advance() {
                    CToken::Ident(s) => { if !expansion.is_empty() { expansion.push(' '); } expansion.push_str(&s); }
                    CToken::IntLit(n) => { if !expansion.is_empty() { expansion.push(' '); } expansion.push_str(&n.to_string()); }
                    CToken::StrLit(s) => { if !expansion.is_empty() { expansion.push(' '); } expansion.push('"'); expansion.push_str(&s); expansion.push('"'); }
                    CToken::Star => expansion.push('*'),
                    CToken::Plus => expansion.push('+'),
                    CToken::Minus => expansion.push('-'),
                    CToken::LParen => expansion.push('('),
                    CToken::RParen => expansion.push(')'),
                    _ => {}
                }
            }
            return Some(CDecl::Define {
                name,
                value: DefineValue::FuncMacro(params, expansion),
            });
        }

        // Simple value macro
        match self.peek() {
            CToken::IntLit(n) => {
                let n = *n;
                self.advance();
                self.skip_to_newline();
                Some(CDecl::Define { name, value: DefineValue::IntVal(n) })
            }
            CToken::Minus => {
                self.advance();
                if let CToken::IntLit(n) = self.peek() {
                    let n = -*n;
                    self.advance();
                    self.skip_to_newline();
                    Some(CDecl::Define { name, value: DefineValue::IntVal(n) })
                } else {
                    self.skip_to_newline();
                    Some(CDecl::Define { name, value: DefineValue::Other("-...".into()) })
                }
            }
            CToken::StrLit(s) => {
                let s = s.clone();
                self.advance();
                self.skip_to_newline();
                Some(CDecl::Define { name, value: DefineValue::StrVal(s) })
            }
            CToken::LParen => {
                // Parenthesized expression — try to extract
                let mut text = String::from("(");
                self.advance();
                let mut depth = 1;
                while depth > 0 {
                    match self.advance() {
                        CToken::LParen => { depth += 1; text.push('('); }
                        CToken::RParen => { depth -= 1; if depth > 0 { text.push(')'); } }
                        CToken::Ident(s) => { text.push_str(&s); text.push(' '); }
                        CToken::IntLit(n) => { text.push_str(&n.to_string()); }
                        CToken::Star => text.push('*'),
                        CToken::Plus => text.push('+'),
                        CToken::Minus => text.push('-'),
                        CToken::Newline | CToken::Eof => break,
                        _ => {}
                    }
                }
                text.push(')');
                self.skip_to_newline();
                Some(CDecl::Define { name, value: DefineValue::Other(text) })
            }
            CToken::Newline | CToken::Eof => {
                // Empty define — header guard or flag
                None
            }
            _ => {
                self.skip_to_newline();
                Some(CDecl::Define { name, value: DefineValue::Other("...".into()) })
            }
        }
    }

    fn parse_include(&mut self) -> Option<CDecl> {
        match self.peek() {
            CToken::StrLit(path) => {
                let path = path.clone();
                self.advance();
                self.skip_to_newline();
                Some(CDecl::Include { path, is_system: false })
            }
            CToken::Lt => {
                self.advance();
                let mut path = String::new();
                loop {
                    match self.advance() {
                        CToken::Gt => break,
                        CToken::Ident(s) => path.push_str(&s),
                        CToken::Dot => path.push('.'),
                        CToken::Slash => path.push('/'),
                        CToken::Newline | CToken::Eof => break,
                        _ => {}
                    }
                }
                self.skip_to_newline();
                Some(CDecl::Include { path, is_system: true })
            }
            _ => {
                self.skip_to_newline();
                None
            }
        }
    }

    fn parse_typedef(&mut self) -> Option<CDecl> {
        // typedef struct { ... } Name;
        // typedef int Name;
        // typedef ret_type (*Name)(params);

        if let CToken::Ident(s) = self.peek() {
            if s == "struct" || s == "union" {
                let is_struct = s == "struct";
                self.advance();
                let tag_name = if let CToken::Ident(_) = self.peek() {
                    self.expect_ident()
                } else {
                    None
                };

                if *self.peek() == CToken::LBrace {
                    // typedef struct { fields } Name;
                    let fields = self.parse_struct_fields();
                    let name = self.expect_ident().unwrap_or_else(|| tag_name.clone().unwrap_or_else(|| "anon".into()));
                    if *self.peek() == CToken::Semi { self.advance(); }
                    return Some(CDecl::Struct { name, fields });
                } else {
                    // typedef struct Foo Name;
                    let name = self.expect_ident().unwrap_or_else(|| "anon".into());
                    if *self.peek() == CToken::Semi { self.advance(); }
                    let target = if is_struct {
                        CType::Struct(tag_name.unwrap_or_else(|| name.clone()))
                    } else {
                        CType::Struct(tag_name.unwrap_or_else(|| name.clone()))
                    };
                    return Some(CDecl::Typedef { name, target });
                }
            }

            if s == "enum" {
                self.advance();
                let tag_name = if let CToken::Ident(_) = self.peek() {
                    self.expect_ident()
                } else {
                    None
                };

                if *self.peek() == CToken::LBrace {
                    let variants = self.parse_enum_body();
                    let name = self.expect_ident().unwrap_or_else(|| tag_name.clone().unwrap_or_else(|| "anon".into()));
                    if *self.peek() == CToken::Semi { self.advance(); }
                    return Some(CDecl::Enum { name, variants });
                } else {
                    let name = self.expect_ident().unwrap_or_else(|| "anon".into());
                    if *self.peek() == CToken::Semi { self.advance(); }
                    return Some(CDecl::Typedef { name, target: CType::Int });
                }
            }
        }

        let base_type = self.parse_type();

        // Check for function pointer typedef: typedef ret (*name)(params);
        if *self.peek() == CToken::LParen {
            let save = self.pos;
            self.advance();
            if *self.peek() == CToken::Star {
                self.advance();
                let name = self.expect_ident().unwrap_or_else(|| "anon".into());
                if *self.peek() == CToken::RParen { self.advance(); }
                // Parse the parameter types
                let (params, _) = self.parse_params();
                let param_types: Vec<CType> = params.iter().map(|p| p.ctype.clone()).collect();
                if *self.peek() == CToken::Semi { self.advance(); }
                return Some(CDecl::Typedef {
                    name,
                    target: CType::FuncPtr(param_types, Box::new(base_type)),
                });
            }
            self.pos = save;
        }

        let name = self.expect_ident().unwrap_or_else(|| "anon".into());

        // Skip array declarators
        while *self.peek() == CToken::LBracket {
            self.advance();
            while *self.peek() != CToken::RBracket && *self.peek() != CToken::Eof {
                self.advance();
            }
            if *self.peek() == CToken::RBracket { self.advance(); }
        }

        if *self.peek() == CToken::Semi { self.advance(); }

        Some(CDecl::Typedef { name, target: base_type })
    }

    fn parse_struct_or_union(&mut self) -> Option<CDecl> {
        self.advance(); // struct or union
        let name = self.expect_ident().unwrap_or_else(|| "anon".into());

        if *self.peek() == CToken::LBrace {
            let fields = self.parse_struct_fields();
            if *self.peek() == CToken::Semi { self.advance(); }
            Some(CDecl::Struct { name, fields })
        } else {
            // Forward declaration: struct Foo;
            if *self.peek() == CToken::Semi { self.advance(); }
            None
        }
    }

    fn parse_struct_fields(&mut self) -> Vec<(String, CType)> {
        let mut fields = Vec::new();
        if *self.peek() != CToken::LBrace { return fields; }
        self.advance(); // {

        while *self.peek() != CToken::RBrace && *self.peek() != CToken::Eof {
            self.skip_newlines();
            if *self.peek() == CToken::RBrace { break; }

            let field_type = self.parse_type();
            let name = self.expect_ident().unwrap_or_else(|| format!("field{}", fields.len()));

            // Handle bitfields: int field : 4;
            if *self.peek() == CToken::Colon {
                self.advance();
                if let CToken::IntLit(_) = self.peek() { self.advance(); }
            }

            // Handle array fields: int field[N];
            while *self.peek() == CToken::LBracket {
                self.advance();
                while *self.peek() != CToken::RBracket && *self.peek() != CToken::Eof {
                    self.advance();
                }
                if *self.peek() == CToken::RBracket { self.advance(); }
            }

            fields.push((name, field_type));

            if *self.peek() == CToken::Semi { self.advance(); }
            self.skip_newlines();
        }

        if *self.peek() == CToken::RBrace { self.advance(); }
        fields
    }

    fn parse_enum(&mut self) -> Option<CDecl> {
        self.advance(); // enum
        let name = if let CToken::Ident(_) = self.peek() {
            self.expect_ident().unwrap_or_else(|| "anon".into())
        } else {
            "anon".into()
        };

        if *self.peek() == CToken::LBrace {
            let variants = self.parse_enum_body();
            if *self.peek() == CToken::Semi { self.advance(); }
            Some(CDecl::Enum { name, variants })
        } else {
            if *self.peek() == CToken::Semi { self.advance(); }
            None
        }
    }

    fn parse_enum_body(&mut self) -> Vec<(String, Option<i64>)> {
        let mut variants = Vec::new();
        if *self.peek() != CToken::LBrace { return variants; }
        self.advance(); // {

        loop {
            self.skip_newlines();
            if *self.peek() == CToken::RBrace || *self.peek() == CToken::Eof { break; }

            if let Some(name) = self.expect_ident() {
                let value = if *self.peek() == CToken::Equals {
                    self.advance();
                    let neg = if *self.peek() == CToken::Minus {
                        self.advance();
                        true
                    } else {
                        false
                    };
                    if let CToken::IntLit(n) = self.peek() {
                        let n = *n;
                        self.advance();
                        Some(if neg { -n } else { n })
                    } else {
                        // Complex expression — skip to comma/brace
                        while *self.peek() != CToken::Comma && *self.peek() != CToken::RBrace && *self.peek() != CToken::Eof {
                            self.advance();
                        }
                        None
                    }
                } else {
                    None
                };
                variants.push((name, value));
            }

            if *self.peek() == CToken::Comma { self.advance(); }
        }

        if *self.peek() == CToken::RBrace { self.advance(); }
        variants
    }

    fn parse_declaration(&mut self, is_static: bool, is_inline: bool) -> Option<CDecl> {
        let return_type = self.parse_type();
        let name = self.expect_ident()?;

        // Function declaration or definition
        if *self.peek() == CToken::LParen {
            let (params, is_variadic) = self.parse_params();

            // Skip __attribute__ after params
            while let CToken::Ident(s) = self.peek() {
                if s == "__attribute__" || s == "__asm__" || s == "asm" {
                    self.advance();
                    if *self.peek() == CToken::LParen {
                        let mut depth = 1;
                        self.advance();
                        while depth > 0 {
                            match self.advance() {
                                CToken::LParen => depth += 1,
                                CToken::RParen => depth -= 1,
                                CToken::Eof => break,
                                _ => {}
                            }
                        }
                    }
                } else {
                    break;
                }
            }

            // If followed by {, skip the function body (definition, not just declaration)
            if *self.peek() == CToken::LBrace {
                self.skip_braced_body();
            } else if *self.peek() == CToken::Semi {
                self.advance();
            }

            Some(CDecl::Function {
                name,
                params,
                return_type,
                is_variadic,
                is_static,
                is_inline,
            })
        } else {
            // Variable declaration — skip it
            while *self.peek() != CToken::Semi && *self.peek() != CToken::Eof {
                self.advance();
            }
            if *self.peek() == CToken::Semi { self.advance(); }
            None
        }
    }
}

// ============ Ernos Emitter ============

fn ctype_to_ernos(ctype: &CType) -> &'static str {
    match ctype {
        CType::Int | CType::Bool | CType::Pointer | CType::Enum(_)
        | CType::Struct(_) | CType::Named(_) | CType::FuncPtr(_, _) => "Int",
        CType::Str => "Str",
        CType::Float => "Int",  // Float mapped to Int (warning added at call site)
        CType::Void => "Int",   // void returns mapped to implicit Int
    }
}

fn sanitize_param_name(name: &str, index: usize) -> String {
    if name.is_empty() || name == "arg" {
        format!("arg{}", index)
    } else if name == "self" || name == "set" || name == "to" || name == "and"
        || name == "with" || name == "as" || name == "if" || name == "else"
        || name == "for" || name == "each" || name == "in" || name == "while"
        || name == "return" || name == "define" || name == "display"
        || name == "true" || name == "false" || name == "not"
        || name == "import" || name == "create" || name == "is"
    {
        format!("{}_param", name)
    } else {
        name.to_string()
    }
}

pub fn emit_ernos_bindings(header_path: &str, source: &str) -> String {
    let mut lexer = CLexer::new(source);
    let tokens = lexer.tokenize();
    let mut parser = CHeaderParser::new(tokens);
    let decls = parser.parse();

    let mut out = String::new();

    out.push_str(&format!("# Auto-generated ErnosPlain bindings for {}\n", header_path));
    out.push_str(&format!("# Generated by: ernos bind {}\n", header_path));

    // Detect link flags from header path
    let link_flags = detect_link_flags(header_path);
    if !link_flags.is_empty() {
        out.push_str(&format!("# Link flags: {}\n", link_flags));
    }
    out.push_str("#\n# DO NOT EDIT — regenerate with: ernos bind\n\n");

    for decl in &decls {
        match decl {
            CDecl::Function { name, params, return_type, is_variadic, is_static, is_inline } => {
                if *is_static && !*is_inline {
                    out.push_str(&format!("# static function {} — not exported, skipped\n", name));
                    continue;
                }

                let mut param_parts = Vec::new();
                for (i, p) in params.iter().enumerate() {
                    let pname = sanitize_param_name(&p.name, i);
                    let ptype = ctype_to_ernos(&p.ctype);
                    param_parts.push(format!("{} as {}", pname, ptype));
                }

                let has_float = params.iter().any(|p| matches!(p.ctype, CType::Float))
                    || matches!(return_type, CType::Float);

                let ret = ctype_to_ernos(return_type);

                if param_parts.is_empty() {
                    if matches!(return_type, CType::Void) {
                        out.push_str(&format!("external define {}:\n", name));
                    } else {
                        out.push_str(&format!("external define {} returning {}:\n", name, ret));
                    }
                } else {
                    let params_str = param_parts.join(" and ");
                    if matches!(return_type, CType::Void) {
                        out.push_str(&format!("external define {} with {}:\n", name, params_str));
                    } else {
                        out.push_str(&format!("external define {} with {} returning {}:\n", name, params_str, ret));
                    }
                }

                if *is_variadic {
                    out.push_str(&format!("# NOTE: {} is variadic — extra args need ep_dlcall\n", name));
                }
                if has_float {
                    out.push_str(&format!("# WARNING: {} uses float/double — values truncated to Int\n", name));
                }
                out.push_str("\n");
            }

            CDecl::Struct { name, fields } => {
                if fields.is_empty() {
                    out.push_str(&format!("# Forward declaration: struct {} (opaque — use as Int)\n\n", name));
                    continue;
                }
                out.push_str(&format!("define structure {}:\n", name));
                for (fname, ftype) in fields {
                    let etype = ctype_to_ernos(ftype);
                    out.push_str(&format!("    field {} as {}\n", fname, etype));
                    if matches!(ftype, CType::Float) {
                        out.push_str(&format!("    # WARNING: field {} is float/double — truncated to Int\n", fname));
                    }
                }
                out.push_str("\n");
            }

            CDecl::Enum { name, variants } => {
                out.push_str(&format!("# Enum {} — integer constants\n", name));
                // Wrap in a function so we don't emit bare top-level statements
                out.push_str(&format!("define get_{}_constants returning Int:\n", name.to_lowercase()));
                let mut auto_val = 0i64;
                for (vname, explicit_val) in variants {
                    let val = explicit_val.unwrap_or(auto_val);
                    out.push_str(&format!("    # {}_{} = {}\n", name.to_uppercase(), vname.to_uppercase(), val));
                    auto_val = val + 1;
                }
                out.push_str("    return 0\n");
                out.push_str("\n");
                // Also emit individual accessor functions for each variant
                auto_val = 0;
                for (vname, explicit_val) in variants {
                    let val = explicit_val.unwrap_or(auto_val);
                    out.push_str(&format!("define BIND_{}_{} returning Int:\n", name.to_uppercase(), vname.to_uppercase()));
                    out.push_str(&format!("    return {}\n", val));
                    out.push_str("\n");
                    auto_val = val + 1;
                }
                out.push_str("\n");
            }

            CDecl::Typedef { name, target } => {
                match target {
                    CType::FuncPtr(args, ret) => {
                        let arg_types: Vec<&str> = args.iter().map(|a| ctype_to_ernos(a)).collect();
                        let ret_type = ctype_to_ernos(ret);
                        out.push_str(&format!("# {} is function pointer: ({}) -> {}\n\n",
                            name, arg_types.join(", "), ret_type));
                    }
                    _ => {
                        let etype = ctype_to_ernos(target);
                        out.push_str(&format!("# typedef {} = {} ({})\n", name, etype,
                            match target {
                                CType::Struct(s) => format!("struct {}", s),
                                CType::Enum(s) => format!("enum {}", s),
                                CType::Named(s) => s.clone(),
                                _ => etype.to_string(),
                            }));
                    }
                }
            }

            CDecl::Define { name, value } => {
                // Skip header guards and internal macros
                if name.starts_with("_") && name.ends_with("_H") { continue; }
                if name.starts_with("__") { continue; }

                match value {
                    DefineValue::IntVal(n) => {
                        // Emit as a function that returns the value (top-level set is illegal)
                        out.push_str(&format!("define BIND_{} returning Int:\n", name));
                        out.push_str(&format!("    return {}\n", n));
                        out.push_str("\n");
                    }
                    DefineValue::StrVal(s) => {
                        out.push_str(&format!("define BIND_{} returning Str:\n", name));
                        out.push_str(&format!("    return \"{}\"\n", s));
                        out.push_str("\n");
                    }
                    DefineValue::FuncMacro(params, expansion) => {
                        out.push_str(&format!("# Function-like macro {}({}) — inline: {}\n",
                            name, params.join(", "), expansion));
                    }
                    DefineValue::Other(text) => {
                        out.push_str(&format!("# #define {} {}\n", name, text));
                    }
                }
            }

            CDecl::Include { path, is_system } => {
                if *is_system {
                    out.push_str(&format!("# #include <{}>\n", path));
                } else {
                    out.push_str(&format!("# #include \"{}\"\n", path));
                }
            }
        }
    }

    out
}

fn detect_link_flags(header_path: &str) -> String {
    let path_lower = header_path.to_lowercase();
    let mut flags = Vec::new();

    if path_lower.contains("openssl") || path_lower.contains("ssl") {
        flags.push("-lssl");
        flags.push("-lcrypto");
    }
    if path_lower.contains("sqlite") {
        flags.push("-lsqlite3");
    }
    if path_lower.contains("curl") {
        flags.push("-lcurl");
    }
    if path_lower.contains("zlib") || path_lower.contains("zconf") {
        flags.push("-lz");
    }
    if path_lower.contains("pcre") {
        flags.push("-lpcre");
    }
    if path_lower.contains("jansson") {
        flags.push("-ljansson");
    }
    if path_lower.contains("math.h") {
        flags.push("-lm");
    }
    if path_lower.contains("pthread") {
        flags.push("-lpthread");
    }
    if path_lower.contains("raylib") {
        flags.push("-lraylib");
    }

    flags.join(" ")
}
