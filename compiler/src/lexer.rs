use crate::token::{Span, Token};

#[derive(Debug)]
pub struct LexError {
    pub message: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
enum RawToken {
    Word(String),
    Number(i64),
    FloatNum(f64),
    StringVal(String),
    InterpString(Vec<(String, String)>), // (literal_part, expression_source)
    Symbol(char),
    Spaces(usize),
    Newline,
    Comment,
    DocComment(String),
}

pub struct Lexer<'a> {
    _input: &'a str,
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            _input: input,
            chars: input.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn peek(&self) -> Option<char> {
        if self.pos < self.chars.len() {
            Some(self.chars[self.pos])
        } else {
            None
        }
    }

    fn advance(&mut self) -> Option<char> {
        if self.pos < self.chars.len() {
            let c = self.chars[self.pos];
            self.pos += 1;
            if c == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
            Some(c)
        } else {
            None
        }
    }

    // Pass 1: Lex into raw tokens
    fn lex_raw(&mut self) -> Result<Vec<(RawToken, Span)>, LexError> {
        let mut raw_tokens = Vec::new();

        while let Some(c) = self.peek() {
            let span = Span::new(self.line, self.col);
            if c == '\n' {
                self.advance();
                raw_tokens.push((RawToken::Newline, span));
            } else if c == '\r' {
                self.advance();
                if self.peek() == Some('\n') {
                    self.advance();
                }
                raw_tokens.push((RawToken::Newline, span));
            } else if c == ' ' || c == '\t' {
                let mut count = 0;
                while let Some(ch) = self.peek() {
                    if ch == ' ' {
                        count += 1;
                        self.advance();
                    } else if ch == '\t' {
                        count += 4; // Treat tab as 4 spaces
                        self.advance();
                    } else {
                        break;
                    }
                }
                raw_tokens.push((RawToken::Spaces(count), span));
            } else if c == '#' {
                // Check for triple-hash doc comment '###'
                let is_doc = self.pos + 2 < self.chars.len() 
                    && self.chars[self.pos] == '#' 
                    && self.chars[self.pos + 1] == '#' 
                    && self.chars[self.pos + 2] == '#';
                if is_doc {
                    self.advance(); // consume first '#'
                    self.advance(); // consume second '#'
                    self.advance(); // consume third '#'
                    let mut s = String::new();
                    while let Some(ch) = self.peek() {
                        if ch == '\n' || ch == '\r' {
                            break;
                        }
                        s.push(ch);
                        self.advance();
                    }
                    raw_tokens.push((RawToken::DocComment(s.trim().to_string()), span));
                } else {
                    // Regular comment
                    self.advance();
                    while let Some(ch) = self.peek() {
                        if ch == '\n' || ch == '\r' {
                            break;
                        }
                        self.advance();
                    }
                    raw_tokens.push((RawToken::Comment, span));
                }
            } else if c.is_ascii_digit() {
                let mut val = 0i64;
                while let Some(ch) = self.peek() {
                    if ch.is_ascii_digit() {
                        val = val * 10 + (ch.to_digit(10).unwrap() as i64);
                        self.advance();
                    } else {
                        break;
                    }
                }
                // Check for float: digit(s) followed by '.' followed by digit(s)
                if self.peek() == Some('.') {
                    // Lookahead: is the char after '.' a digit?
                    let next_pos = self.pos + 1;
                    if next_pos < self.chars.len() && self.chars[next_pos].is_ascii_digit() {
                        self.advance(); // consume '.'
                        let mut frac = 0.0f64;
                        let mut frac_div = 1.0f64;
                        while let Some(ch) = self.peek() {
                            if ch.is_ascii_digit() {
                                frac = frac * 10.0 + (ch.to_digit(10).unwrap() as f64);
                                frac_div *= 10.0;
                                self.advance();
                            } else {
                                break;
                            }
                        }
                        let float_val = (val as f64) + frac / frac_div;
                        raw_tokens.push((RawToken::FloatNum(float_val), span));
                    } else {
                        raw_tokens.push((RawToken::Number(val), span));
                    }
                } else {
                    raw_tokens.push((RawToken::Number(val), span));
                }
            } else if c.is_alphabetic() || c == '_' {
                let mut s = String::new();
                while let Some(ch) = self.peek() {
                    if ch.is_alphanumeric() || ch == '_' {
                        s.push(ch);
                        self.advance();
                    } else {
                        break;
                    }
                }
                raw_tokens.push((RawToken::Word(s), span));
            } else if c == '"' {
                // Check if this is an f-string (f"..." or f "...")
                // Look back past any Spaces tokens to find the preceding Word
                let is_fstring = {
                    let mut found = false;
                    for rtok in raw_tokens.iter().rev() {
                        match &rtok.0 {
                            RawToken::Spaces(_) => continue,
                            RawToken::Word(w) if w == "f" => { found = true; break; }
                            _ => break,
                        }
                    }
                    found
                };
                // If f-string, remove the 'f' word token (and any trailing Spaces)
                if is_fstring {
                    // Remove trailing Spaces first, then the 'f' Word
                    while let Some((RawToken::Spaces(_), _)) = raw_tokens.last() {
                        raw_tokens.pop();
                    }
                    raw_tokens.pop(); // remove Word("f")
                }
                
                self.advance(); // consume first quote
                
                // Check for triple-quote """..."""
                let is_triple = if self.peek() == Some('"') {
                    // Check if the next char is also a quote
                    let saved_pos = self.pos;
                    self.advance(); // consume second quote
                    if self.peek() == Some('"') {
                        self.advance(); // consume third quote
                        true
                    } else {
                        // It was just "" (empty string)
                        self.pos = saved_pos;
                        false
                    }
                } else {
                    false
                };
                
                if is_triple {
                    // Multi-line string: read until """
                    let mut s = String::new();
                    let mut closed = false;
                    while let Some(ch) = self.peek() {
                        if ch == '"' {
                            let _saved = self.pos;
                            self.advance();
                            if self.peek() == Some('"') {
                                self.advance();
                                if self.peek() == Some('"') {
                                    self.advance();
                                    closed = true;
                                    break;
                                } else {
                                    s.push('"');
                                    s.push('"');
                                }
                            } else {
                                s.push('"');
                            }
                        } else if ch == '\\' {
                            self.advance();
                            match self.peek() {
                                Some('n') => { s.push('\n'); self.advance(); }
                                Some('t') => { s.push('\t'); self.advance(); }
                                Some('"') => { s.push('"'); self.advance(); }
                                Some('\\') => { s.push('\\'); self.advance(); }
                                Some(other) => { s.push('\\'); s.push(other); self.advance(); }
                                None => break,
                            }
                        } else {
                            s.push(ch);
                            self.advance();
                        }
                    }
                    if !closed {
                        return Err(LexError {
                            message: "Unterminated triple-quoted string".to_string(),
                            span,
                        });
                    }
                    raw_tokens.push((RawToken::StringVal(s), span));
                    continue;
                }
                
                let mut s = String::new();
                let mut closed = false;
                let mut has_interp = false;
                let mut interp_parts: Vec<(String, String)> = Vec::new();
                
                while let Some(ch) = self.peek() {
                    if ch == '"' {
                        self.advance();
                        closed = true;
                        break;
                    } else if ch == '{' && is_fstring {
                        // String interpolation: {expr}
                        has_interp = true;
                        self.advance(); // consume '{'
                        let literal_part = s.clone();
                        s.clear();
                        
                        // Read expression until matching '}'
                        let mut expr_str = String::new();
                        let mut brace_depth = 1;
                        while let Some(ec) = self.peek() {
                            if ec == '"' {
                                // Nested string literal inside interpolation — scan through it
                                expr_str.push(ec);
                                self.advance();
                                while let Some(sc) = self.peek() {
                                    expr_str.push(sc);
                                    self.advance();
                                    if sc == '"' { break; }
                                    if sc == '\\' {
                                        // Handle escape sequence inside nested string
                                        if let Some(esc) = self.peek() {
                                            expr_str.push(esc);
                                            self.advance();
                                        }
                                    }
                                }
                                continue;
                            }
                            if ec == '}' {
                                brace_depth -= 1;
                                if brace_depth == 0 {
                                    self.advance(); // consume '}'
                                    break;
                                }
                            } else if ec == '{' {
                                brace_depth += 1;
                            }
                            expr_str.push(ec);
                            self.advance();
                        }
                        interp_parts.push((literal_part, expr_str));
                    } else if ch == '\\' {
                        self.advance(); // consume backslash
                        match self.peek() {
                            Some('n') => {
                                s.push('\n');
                                self.advance();
                            }
                            Some('t') => {
                                s.push('\t');
                                self.advance();
                            }
                            Some('r') => {
                                s.push('\r');
                                self.advance();
                            }
                            Some('"') => {
                                s.push('"');
                                self.advance();
                            }
                            Some('\\') => {
                                s.push('\\');
                                self.advance();
                            }
                            Some('{') => {
                                s.push('{');
                                self.advance();
                            }
                            Some('}') => {
                                s.push('}');
                                self.advance();
                            }
                            Some(other) => {
                                s.push('\\');
                                s.push(other);
                                self.advance();
                            }
                            None => {
                                return Err(LexError {
                                    message: "Unterminated string literal at escape sequence".to_string(),
                                    span,
                                });
                            }
                        }
                    } else if ch == '\n' || ch == '\r' {
                        return Err(LexError {
                            message: "Unterminated string literal".to_string(),
                            span,
                        });
                    } else {
                        s.push(ch);
                        self.advance();
                    }
                }
                if !closed {
                    return Err(LexError {
                        message: "Unterminated string literal".to_string(),
                        span,
                    });
                }
                
                if has_interp {
                    // Add the trailing literal part
                    interp_parts.push((s, String::new()));
                    raw_tokens.push((RawToken::InterpString(interp_parts), span));
                } else {
                    raw_tokens.push((RawToken::StringVal(s), span));
                }
            } else if c == ':' || c == '(' || c == ')' || c == '[' || c == ']' || c == ',' || c == '+' || c == '-' || c == '*' || c == '/'
                   || c == '<' || c == '>' || c == '&' || c == '|' || c == '=' || c == '!' || c == '.' || c == '%' {
                self.advance();
                raw_tokens.push((RawToken::Symbol(c), span));
            } else {
                return Err(LexError {
                    message: format!("Unexpected character: '{}'", c),
                    span,
                });
            }
        }

        Ok(raw_tokens)
    }

    // Pass 2: Combine keywords, handle indentation
    pub fn tokenize(&mut self) -> Result<Vec<(Token, Span)>, LexError> {
        let raw = self.lex_raw()?;
        let mut tokens = Vec::new();
        let mut indent_stack = vec![0];
        
        let mut i = 0;
        let mut at_line_start = true;

        // We process line-by-line, handling indentation at the start of each line
        while i < raw.len() {
            if at_line_start {
                at_line_start = false;

                // Let's look ahead to find if this line has indentation and if it's empty
                let mut spaces = 0;
                let mut temp_i = i;
                
                // Skip initial spaces to find the actual content of the line
                if temp_i < raw.len() {
                    if let (RawToken::Spaces(s), _) = &raw[temp_i] {
                        spaces = *s;
                        temp_i += 1;
                    }
                }                 // If the line is empty (contains only comments, newlines, or nothing), we ignore its indentation.
                // Note: We do NOT ignore lines containing doc comments because doc comments are passed to the parser
                // and need to be correctly placed relative to Dedent/Indent tokens.
                let is_empty_line = temp_i >= raw.len() 
                    || matches!(raw[temp_i].0, RawToken::Newline | RawToken::Comment);

                if !is_empty_line {
                    let last_indent = *indent_stack.last().unwrap();
                    if spaces > last_indent {
                        indent_stack.push(spaces);
                        let span = raw[i].1.clone();
                        tokens.push((Token::Indent, span));
                    } else if spaces < last_indent {
                        while let Some(&top) = indent_stack.last() {
                            if spaces < top {
                                indent_stack.pop();
                                let span = raw[i].1.clone();
                                tokens.push((Token::Dedent, span));
                            } else {
                                break;
                            }
                        }
                        if *indent_stack.last().unwrap() != spaces {
                            return Err(LexError {
                                message: format!(
                                    "Indentation error: expected matching level, found {} spaces",
                                    spaces
                                ),
                                span: raw[i].1.clone(),
                            });
                        }
                    }
                }
                
                // Advance past spaces if we processed them
                if i < raw.len() {
                    if let (RawToken::Spaces(_), _) = &raw[i] {
                        i += 1;
                        continue;
                    }
                }
            }

            if i >= raw.len() {
                break;
            }

            let (raw_tok, span) = &raw[i];
            
            match raw_tok {
                RawToken::Comment => {
                    i += 1;
                }
                RawToken::DocComment(s) => {
                    tokens.push((Token::DocComment(s.clone()), span.clone()));
                    i += 1;
                }
                RawToken::Newline => {
                    // Only push newline if the last token wasn't already a newline
                    // (and ignore newlines at the start of the file)
                    if let Some((Token::Newline, _)) = tokens.last() {
                        // skip duplicate
                    } else if !tokens.is_empty() {
                        tokens.push((Token::Newline, span.clone()));
                    }
                    at_line_start = true;
                    i += 1;
                }
                RawToken::Spaces(_) => {
                    // Internal spaces (not at line start) are skipped
                    i += 1;
                }
                RawToken::Number(val) => {
                    tokens.push((Token::Integer(*val), span.clone()));
                    i += 1;
                }
                RawToken::FloatNum(val) => {
                    tokens.push((Token::Float(*val), span.clone()));
                    i += 1;
                }
                RawToken::StringVal(s) => {
                    tokens.push((Token::StringLiteral(s.clone()), span.clone()));
                    i += 1;
                }
                RawToken::InterpString(parts) => {
                    tokens.push((Token::InterpStringParts(parts.clone()), span.clone()));
                    i += 1;
                }
                RawToken::Symbol(c) => {
                    let mut tok = None;
                    
                    // Lookahead helper to see if we match double-character symbols
                    if i + 1 < raw.len() {
                        if let (RawToken::Symbol(next_c), _) = &raw[i + 1] {
                            match (*c, *next_c) {
                                ('=', '=') => {
                                    tok = Some(Token::Equals);
                                    i += 2;
                                }
                                ('!', '=') => {
                                    tok = Some(Token::NotEquals);
                                    i += 2;
                                }
                                ('&', '&') => {
                                    tok = Some(Token::LogicalAnd);
                                    i += 2;
                                }
                                ('|', '|') => {
                                    tok = Some(Token::LogicalOr);
                                    i += 2;
                                }
                                ('>', '=') => {
                                    tok = Some(Token::GreaterEqual);
                                    i += 2;
                                }
                                ('<', '=') => {
                                    tok = Some(Token::LessEqual);
                                    i += 2;
                                }
                                _ => {}
                            }
                        }
                    }
                    
                    // If not combined, match single characters
                    if tok.is_none() {
                        let t = match c {
                            ':' => Token::Colon,
                            '(' => Token::LeftParen,
                            ')' => Token::RightParen,
                            '[' => Token::LeftBracket,
                            ']' => Token::RightBracket,
                            ',' => Token::Comma,
                            '+' => Token::Plus,
                            '-' => Token::Minus,
                            '*' => Token::Multiply,
                            '/' => Token::Divide,
                            '<' => Token::LessThan,
                            '>' => Token::GreaterThan,
                            '.' => Token::Dot,
                            '%' => Token::Modulo,
                            _ => {
                                return Err(LexError {
                                    message: format!("Unexpected symbol character: '{}'", c),
                                    span: span.clone(),
                                });
                            }
                        };
                        tok = Some(t);
                        i += 1;
                    }
                    
                    tokens.push((tok.unwrap(), span.clone()));
                }
                RawToken::Word(word) => {
                    // Lookahead helper to see if we match a multi-word phrase
                    let match_phrase = |mut next_idx: usize, expected: &[&str]| -> Option<usize> {
                        for &exp in expected {
                            // Skip any spaces
                            if next_idx < raw.len() {
                                if let (RawToken::Spaces(_), _) = &raw[next_idx] {
                                    next_idx += 1;
                                }
                            }
                            if next_idx >= raw.len() {
                                return None;
                            }
                            if let (RawToken::Word(w), _) = &raw[next_idx] {
                                if w.to_lowercase() == exp {
                                    next_idx += 1;
                                    continue;
                                }
                            }
                            return None;
                        }
                        Some(next_idx)
                    };

                    let w_lower = word.to_lowercase();
                    
                    if w_lower == "multiplied" {
                        if let Some(next_idx) = match_phrase(i + 1, &["by"]) {
                            tokens.push((Token::Multiply, span.clone()));
                            i = next_idx;
                            continue;
                        }
                    } else if w_lower == "divided" {
                        if let Some(next_idx) = match_phrase(i + 1, &["by"]) {
                            tokens.push((Token::Divide, span.clone()));
                            i = next_idx;
                            continue;
                        }
                    } else if w_lower == "is" {
                        // "is not equal to" must be checked before "is not" to avoid partial match
                        if let Some(next_idx) = match_phrase(i + 1, &["not", "equal", "to"]) {
                            tokens.push((Token::NotEquals, span.clone()));
                            i = next_idx;
                            continue;
                        } else if let Some(next_idx) = match_phrase(i + 1, &["less", "than"]) {
                            tokens.push((Token::LessThan, span.clone()));
                            i = next_idx;
                            continue;
                        } else if let Some(next_idx) = match_phrase(i + 1, &["greater", "than"]) {
                            tokens.push((Token::GreaterThan, span.clone()));
                            i = next_idx;
                            continue;
                        } else if let Some(next_idx) = match_phrase(i + 1, &["equal", "to"]) {
                            tokens.push((Token::Equals, span.clone()));
                            i = next_idx;
                            continue;
                        // New English aliases for comparisons
                        } else if let Some(next_idx) = match_phrase(i + 1, &["at", "least"]) {
                            tokens.push((Token::GreaterEqual, span.clone()));
                            i = next_idx;
                            continue;
                        } else if let Some(next_idx) = match_phrase(i + 1, &["at", "most"]) {
                            tokens.push((Token::LessEqual, span.clone()));
                            i = next_idx;
                            continue;
                        } else if let Some(next_idx) = match_phrase(i + 1, &["more", "than"]) {
                            tokens.push((Token::GreaterThan, span.clone()));
                            i = next_idx;
                            continue;
                        } else if let Some(next_idx) = match_phrase(i + 1, &["fewer", "than"]) {
                            tokens.push((Token::LessThan, span.clone()));
                            i = next_idx;
                            continue;
                        } else if let Some(next_idx) = match_phrase(i + 1, &["smaller", "than"]) {
                            tokens.push((Token::LessThan, span.clone()));
                            i = next_idx;
                            continue;
                        } else if let Some(next_idx) = match_phrase(i + 1, &["bigger", "than"]) {
                            tokens.push((Token::GreaterThan, span.clone()));
                            i = next_idx;
                            continue;
                        } else if let Some(next_idx) = match_phrase(i + 1, &["larger", "than"]) {
                            tokens.push((Token::GreaterThan, span.clone()));
                            i = next_idx;
                            continue;
                        } else if let Some(next_idx) = match_phrase(i + 1, &["the", "same", "as"]) {
                            tokens.push((Token::Equals, span.clone()));
                            i = next_idx;
                            continue;
                        } else if let Some(next_idx) = match_phrase(i + 1, &["different", "from"]) {
                            tokens.push((Token::NotEquals, span.clone()));
                            i = next_idx;
                            continue;
                        }
                    } else if w_lower == "does" {
                        if let Some(next_idx) = match_phrase(i + 1, &["not", "equal"]) {
                            tokens.push((Token::NotEquals, span.clone()));
                            i = next_idx;
                            continue;
                        }
                    } else if w_lower == "give" {
                        if let Some(next_idx) = match_phrase(i + 1, &["back"]) {
                            tokens.push((Token::Return, span.clone()));
                            i = next_idx;
                            continue;
                        }
                    } else if w_lower == "and" {
                        if let Some(next_idx) = match_phrase(i + 1, &["also"]) {
                            tokens.push((Token::LogicalAnd, span.clone()));
                            i = next_idx;
                            continue;
                        }
                    } else if w_lower == "or" {
                        if let Some(next_idx) = match_phrase(i + 1, &["else"]) {
                            tokens.push((Token::LogicalOr, span.clone()));
                            i = next_idx;
                            continue;
                        }
                    } else if w_lower == "for" {
                        // "for each" and "for every" both work
                        if let Some(next_idx) = match_phrase(i + 1, &["each"]) {
                            tokens.push((Token::For, span.clone()));
                            tokens.push((Token::Each, span.clone()));
                            i = next_idx;
                            continue;
                        } else if let Some(next_idx) = match_phrase(i + 1, &["every"]) {
                            tokens.push((Token::For, span.clone()));
                            tokens.push((Token::Each, span.clone()));
                            i = next_idx;
                            continue;
                        }
                    }

                    // Simple single-word keywords
                    let tok = match w_lower.as_str() {
                        "define" | "describe" => Token::Define,
                        "with" => Token::With,
                        "and" => Token::And,
                        "set" | "let" => Token::Set,
                        "to" | "be" => Token::To,
                        "if" => Token::If,
                        "else" => Token::Else,
                        "return" => Token::Return,
                        "display" | "show" | "print" => Token::Display,
                        "repeat" | "loop" => Token::Repeat,
                        "while" => Token::While,
                        "import" => Token::Import,
                        "spawn" => Token::Spawn,
                        "channel" => Token::Channel,
                        "send" => Token::Send,
                        "receive" => Token::Receive,
                        "from" => Token::From,
                        "external" => Token::External,
                        "borrow" => Token::Borrow,
                        "structure" => Token::Structure,
                        "field" => Token::Field,
                        "as" => Token::As,
                        "is" => Token::Is,
                        "create" => Token::Create,
                        "returning" | "returns" => Token::Returning,
                        "choice" => Token::Choice,
                        "variant" => Token::Variant,
                        "check" => Token::Check,
                        "for" => Token::For,
                        "each" | "every" => Token::Each,
                        "in" => Token::In,
                        "range" => Token::Range,
                        "on" => Token::On,
                        "trait" => Token::Trait,
                        "implement" => Token::Implement,
                        "not" => Token::Not,
                        "modulo" => Token::Modulo,
                        "break" | "stop" => Token::Break,
                        "continue" | "skip" => Token::Continue,
                        "of" => Token::Of,
                        "try" => Token::Try,
                        "given" => Token::Given,
                        "true" => Token::True,
                        "false" => Token::False,
                        "async" => Token::Async,
                        "await" => Token::Await,
                        "plus" => Token::Plus,
                        "minus" => Token::Minus,
                        "times" => Token::Multiply,
                        "equals" => Token::Equals,
                        _ => Token::Identifier(word.clone()),
                    };
                    
                    tokens.push((tok, span.clone()));
                    i += 1;
                }
            }
        }

        // Clean up remaining indentation stack
        let end_span = if let Some((_, last_span)) = tokens.last() {
            last_span.clone()
        } else {
            Span::new(self.line, self.col)
        };

        // Pop all remaining indentation
        while indent_stack.len() > 1 {
            indent_stack.pop();
            tokens.push((Token::Dedent, end_span.clone()));
        }

        // Always end with EOF
        tokens.push((Token::EOF, end_span));

        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_math_and_set() {
        let mut lexer = Lexer::new("set result to 10 plus 20 multiplied by 5");
        let tokens = lexer.tokenize().unwrap();
        let tok_types: Vec<Token> = tokens.into_iter().map(|(t, _)| t).collect();
        
        assert_eq!(
            tok_types,
            vec![
                Token::Set,
                Token::Identifier("result".to_string()),
                Token::To,
                Token::Integer(10),
                Token::Plus,
                Token::Integer(20),
                Token::Multiply,
                Token::Integer(5),
                Token::EOF
            ]
        );
    }

    #[test]
    fn test_lexer_comparisons() {
        let mut lexer = Lexer::new("if x is less than y:\n    return a is not equal to b");
        let tokens = lexer.tokenize().unwrap();
        let tok_types: Vec<Token> = tokens.into_iter().map(|(t, _)| t).collect();
        
        assert_eq!(
            tok_types,
            vec![
                Token::If,
                Token::Identifier("x".to_string()),
                Token::LessThan,
                Token::Identifier("y".to_string()),
                Token::Colon,
                Token::Newline,
                Token::Indent,
                Token::Return,
                Token::Identifier("a".to_string()),
                Token::NotEquals,
                Token::Identifier("b".to_string()),
                Token::Dedent,
                Token::EOF
            ]
        );
    }

    #[test]
    fn test_lexer_logical_combinators() {
        let mut lexer = Lexer::new("x and also y or else z");
        let tokens = lexer.tokenize().unwrap();
        let tok_types: Vec<Token> = tokens.into_iter().map(|(t, _)| t).collect();
        
        assert_eq!(
            tok_types,
            vec![
                Token::Identifier("x".to_string()),
                Token::LogicalAnd,
                Token::Identifier("y".to_string()),
                Token::LogicalOr,
                Token::Identifier("z".to_string()),
                Token::EOF
            ]
        );
    }

    #[test]
    fn test_lexer_shorthand_symbols() {
        let mut lexer = Lexer::new("set x to a + b * c - d / e");
        let tokens = lexer.tokenize().unwrap();
        let tok_types: Vec<Token> = tokens.into_iter().map(|(t, _)| t).collect();
        
        assert_eq!(
            tok_types,
            vec![
                Token::Set,
                Token::Identifier("x".to_string()),
                Token::To,
                Token::Identifier("a".to_string()),
                Token::Plus,
                Token::Identifier("b".to_string()),
                Token::Multiply,
                Token::Identifier("c".to_string()),
                Token::Minus,
                Token::Identifier("d".to_string()),
                Token::Divide,
                Token::Identifier("e".to_string()),
                Token::EOF
            ]
        );
    }

    #[test]
    fn test_lexer_escape_sequences() {
        let mut lexer = Lexer::new("\"hello\\nworld\\t\\\"\\\\\"");
        let tokens = lexer.tokenize().unwrap();
        let tok_types: Vec<Token> = tokens.into_iter().map(|(t, _)| t).collect();
        
        assert_eq!(
            tok_types,
            vec![
                Token::StringLiteral("hello\nworld\t\"\\".to_string()),
                Token::EOF
            ]
        );
    }
}
