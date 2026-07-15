/// ErnosPlain Language Server Protocol (LSP) implementation
///
/// A minimal but functional LSP server built with ZERO external dependencies.
/// JSON parsing and serialization are hand-written using only the Rust stdlib.

use std::collections::HashMap;
use std::io::{self, BufRead, Read, Write};

use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::type_check::TypeChecker;
use crate::ast::*;

// ══════════════════════════════════════════════════════════════
// JSON Value type
// ══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        if let JsonValue::Object(fields) = self {
            for (k, v) in fields {
                if k == key {
                    return Some(v);
                }
            }
        }
        None
    }

    pub fn as_str(&self) -> Option<&str> {
        if let JsonValue::Str(s) = self {
            Some(s)
        } else {
            None
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        if let JsonValue::Number(n) = self {
            Some(*n as i64)
        } else {
            None
        }
    }

    pub fn as_array(&self) -> Option<&Vec<JsonValue>> {
        if let JsonValue::Array(arr) = self {
            Some(arr)
        } else {
            None
        }
    }

    pub fn as_object(&self) -> Option<&Vec<(String, JsonValue)>> {
        if let JsonValue::Object(obj) = self {
            Some(obj)
        } else {
            None
        }
    }
}

// ══════════════════════════════════════════════════════════════
// JSON Parser — simple recursive descent
// ══════════════════════════════════════════════════════════════

struct JsonParser {
    chars: Vec<char>,
    pos: usize,
}

impl JsonParser {
    fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.chars.len() {
            let c = self.chars[self.pos];
            if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
                self.pos += 1;
            } else {
                break;
            }
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
            Some(c)
        } else {
            None
        }
    }

    #[allow(dead_code)]
    fn expect(&mut self, expected: char) -> bool {
        self.skip_ws();
        if self.peek() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn parse_value(&mut self) -> JsonValue {
        self.skip_ws();
        match self.peek() {
            Some('"') => self.parse_string_value(),
            Some('{') => self.parse_object(),
            Some('[') => self.parse_array(),
            Some('t') => self.parse_true(),
            Some('f') => self.parse_false(),
            Some('n') => self.parse_null(),
            Some(c) if c == '-' || c.is_ascii_digit() => self.parse_number(),
            _ => JsonValue::Null,
        }
    }

    fn parse_string_value(&mut self) -> JsonValue {
        JsonValue::Str(self.parse_string())
    }

    fn parse_string(&mut self) -> String {
        self.skip_ws();
        if self.peek() != Some('"') {
            return String::new();
        }
        self.advance(); // consume opening "
        let mut s = String::new();
        loop {
            match self.advance() {
                Some('"') => break,
                Some('\\') => {
                    match self.advance() {
                        Some('"') => s.push('"'),
                        Some('\\') => s.push('\\'),
                        Some('/') => s.push('/'),
                        Some('n') => s.push('\n'),
                        Some('r') => s.push('\r'),
                        Some('t') => s.push('\t'),
                        Some('u') => {
                            // Parse 4 hex digits
                            let mut hex = String::new();
                            for _ in 0..4 {
                                if let Some(h) = self.advance() {
                                    hex.push(h);
                                }
                            }
                            if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                                if let Some(ch) = char::from_u32(cp) {
                                    s.push(ch);
                                }
                            }
                        }
                        Some(c) => { s.push('\\'); s.push(c); }
                        None => break,
                    }
                }
                Some(c) => s.push(c),
                None => break,
            }
        }
        s
    }

    fn parse_object(&mut self) -> JsonValue {
        self.skip_ws();
        self.advance(); // consume {
        let mut fields = Vec::new();
        self.skip_ws();
        if self.peek() == Some('}') {
            self.advance();
            return JsonValue::Object(fields);
        }
        loop {
            self.skip_ws();
            let key = self.parse_string();
            self.skip_ws();
            if self.peek() == Some(':') {
                self.advance();
            }
            let val = self.parse_value();
            fields.push((key, val));
            self.skip_ws();
            if self.peek() == Some(',') {
                self.advance();
            } else {
                break;
            }
        }
        self.skip_ws();
        if self.peek() == Some('}') {
            self.advance();
        }
        JsonValue::Object(fields)
    }

    fn parse_array(&mut self) -> JsonValue {
        self.skip_ws();
        self.advance(); // consume [
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(']') {
            self.advance();
            return JsonValue::Array(items);
        }
        loop {
            let val = self.parse_value();
            items.push(val);
            self.skip_ws();
            if self.peek() == Some(',') {
                self.advance();
            } else {
                break;
            }
        }
        self.skip_ws();
        if self.peek() == Some(']') {
            self.advance();
        }
        JsonValue::Array(items)
    }

    fn parse_number(&mut self) -> JsonValue {
        self.skip_ws();
        let mut s = String::new();
        if self.peek() == Some('-') {
            s.push('-');
            self.advance();
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-' {
                // Only push +/- if preceded by e/E
                if (c == '+' || c == '-') && !s.ends_with('e') && !s.ends_with('E') {
                    break;
                }
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
        if let Ok(n) = s.parse::<f64>() {
            JsonValue::Number(n)
        } else {
            JsonValue::Number(0.0)
        }
    }

    fn parse_true(&mut self) -> JsonValue {
        for _ in 0..4 { self.advance(); }
        JsonValue::Bool(true)
    }

    fn parse_false(&mut self) -> JsonValue {
        for _ in 0..5 { self.advance(); }
        JsonValue::Bool(false)
    }

    fn parse_null(&mut self) -> JsonValue {
        for _ in 0..4 { self.advance(); }
        JsonValue::Null
    }
}

pub fn parse_json(input: &str) -> JsonValue {
    let mut parser = JsonParser::new(input);
    parser.parse_value()
}

// ══════════════════════════════════════════════════════════════
// JSON Serializer
// ══════════════════════════════════════════════════════════════

pub fn json_to_string(val: &JsonValue) -> String {
    match val {
        JsonValue::Null => "null".to_string(),
        JsonValue::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
        JsonValue::Number(n) => {
            if *n == (*n as i64) as f64 && n.is_finite() {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        JsonValue::Str(s) => {
            let escaped = s
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r")
                .replace('\t', "\\t");
            format!("\"{}\"", escaped)
        }
        JsonValue::Array(items) => {
            let parts: Vec<String> = items.iter().map(|v| json_to_string(v)).collect();
            format!("[{}]", parts.join(","))
        }
        JsonValue::Object(fields) => {
            let parts: Vec<String> = fields.iter().map(|(k, v)| {
                let ek = k.replace('\\', "\\\\").replace('"', "\\\"");
                format!("\"{}\":{}", ek, json_to_string(v))
            }).collect();
            format!("{{{}}}", parts.join(","))
        }
    }
}

// Helper to build JSON objects
fn json_obj(fields: Vec<(&str, JsonValue)>) -> JsonValue {
    JsonValue::Object(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}

fn json_str(s: &str) -> JsonValue {
    JsonValue::Str(s.to_string())
}

fn json_int(n: i64) -> JsonValue {
    JsonValue::Number(n as f64)
}

fn json_bool(b: bool) -> JsonValue {
    JsonValue::Bool(b)
}

fn json_arr(items: Vec<JsonValue>) -> JsonValue {
    JsonValue::Array(items)
}

// ══════════════════════════════════════════════════════════════
// LSP Message I/O
// ══════════════════════════════════════════════════════════════

fn read_message() -> Option<String> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    
    // Read headers
    let mut content_length: Option<usize> = None;
    loop {
        let mut header_line = String::new();
        match reader.read_line(&mut header_line) {
            Ok(0) => return None, // EOF
            Ok(_) => {}
            Err(_) => return None,
        }
        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            break; // End of headers
        }
        if let Some(rest) = trimmed.strip_prefix("Content-Length:") {
            if let Ok(len) = rest.trim().parse::<usize>() {
                content_length = Some(len);
            }
        }
    }

    let len = content_length?;
    let mut body = vec![0u8; len];
    if reader.read_exact(&mut body).is_err() {
        return None;
    }
    String::from_utf8(body).ok()
}

fn send_message(json: &str) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let msg = format!("Content-Length: {}\r\n\r\n{}", json.len(), json);
    let _ = out.write_all(msg.as_bytes());
    let _ = out.flush();
}

fn send_response(id: &JsonValue, result: JsonValue) {
    let response = json_obj(vec![
        ("jsonrpc", json_str("2.0")),
        ("id", id.clone()),
        ("result", result),
    ]);
    send_message(&json_to_string(&response));
}

fn send_error_response(id: &JsonValue, code: i64, message: &str) {
    let response = json_obj(vec![
        ("jsonrpc", json_str("2.0")),
        ("id", id.clone()),
        ("error", json_obj(vec![
            ("code", json_int(code)),
            ("message", json_str(message)),
        ])),
    ]);
    send_message(&json_to_string(&response));
}

fn send_notification(method: &str, params: JsonValue) {
    let notif = json_obj(vec![
        ("jsonrpc", json_str("2.0")),
        ("method", json_str(method)),
        ("params", params),
    ]);
    send_message(&json_to_string(&notif));
}

// ══════════════════════════════════════════════════════════════
// Document Store
// ══════════════════════════════════════════════════════════════

struct DocumentStore {
    documents: HashMap<String, String>,
}

impl DocumentStore {
    fn new() -> Self {
        Self { documents: HashMap::new() }
    }

    fn open(&mut self, uri: &str, content: &str) {
        self.documents.insert(uri.to_string(), content.to_string());
    }

    fn change(&mut self, uri: &str, content: &str) {
        self.documents.insert(uri.to_string(), content.to_string());
    }

    fn get(&self, uri: &str) -> Option<&String> {
        self.documents.get(uri)
    }
}

// ══════════════════════════════════════════════════════════════
// Keywords and builtins data
// ══════════════════════════════════════════════════════════════

fn get_keywords() -> Vec<(&'static str, &'static str)> {
    vec![
        ("set", "Declare a variable: `set x to 42`"),
        ("to", "Assignment target: `set x to <expr>`"),
        ("define", "Define a function: `define name with params:`"),
        ("with", "Function parameters: `define f with x and y:`"),
        ("return", "Return a value from a function"),
        ("display", "Print a value to stdout"),
        ("if", "Conditional: `if condition:`"),
        ("else", "Else branch of a conditional"),
        ("repeat", "Loop: `repeat while condition:`"),
        ("while", "While condition for repeat loop"),
        ("for", "For loop: `for each item in collection:`"),
        ("each", "Iterator: `for each x in list:`"),
        ("in", "Membership/iteration keyword"),
        ("is", "Comparison/assignment: `field x is 5`"),
        ("equals", "Equality comparison: `x equals y`"),
        ("plus", "Addition operator"),
        ("minus", "Subtraction operator"),
        ("multiplied", "Multiplication: `multiplied by`"),
        ("divided", "Division: `divided by`"),
        ("modulo", "Modulo (remainder) operator"),
        ("greater", "Greater than comparison"),
        ("less", "Less than comparison"),
        ("than", "Used with greater/less comparisons"),
        ("not", "Logical negation"),
        ("and", "Logical AND / parameter separator"),
        ("also", "Used with `and also` for logical AND"),
        ("or", "Logical OR"),
        ("true", "Boolean true literal"),
        ("false", "Boolean false literal"),
        ("create", "Create a struct instance: `create StructName:`"),
        ("structure", "Define a struct: `define structure Name:`"),
        ("field", "Define a struct field: `field name as Type`"),
        ("choice", "Define an enum: `define choice Name:`"),
        ("variant", "Define an enum variant: `variant Name`"),
        ("check", "Pattern match: `check expr:`"),
        ("trait", "Define a trait: `define trait Name:`"),
        ("implement", "Implement a trait: `implement Trait for Type:`"),
        ("spawn", "Spawn a concurrent task: `spawn func(args)`"),
        ("channel", "Channel type for concurrency"),
        ("send", "Send a value to a channel"),
        ("receive", "Receive a value from a channel"),
        ("async", "Mark a function as asynchronous"),
        ("await", "Await an async result"),
        ("borrow", "Borrow a value (pass by reference)"),
        ("given", "Pattern guard in match arms"),
        ("break", "Break out of a loop"),
        ("continue", "Skip to next loop iteration"),
        ("import", "Import a module: `import \"module\"`"),
        ("as", "Type annotation: `x as Int` / import alias"),
        ("returning", "Specify return type: `returning Int`"),
        ("of", "Generic type parameter: `List of Int`"),
        ("try", "Try expression for error handling"),
        ("from", "Source in receive: `receive from channel`"),
        ("range", "Range expression: `range(1 and 10)`"),
    ]
}

fn get_builtins() -> Vec<(&'static str, &'static str)> {
    vec![
        ("create_list", "create_list() → List — Create a new empty list"),
        ("append_list", "append_list(list, item) → Int — Append item to list"),
        ("get_list", "get_list(list, index) → Any — Get item at index"),
        ("set_list", "set_list(list, index, val) → Int — Set item at index"),
        ("length_list", "length_list(list) → Int — Get list length"),
        ("pop_list", "pop_list(list) → Any — Remove and return last element"),
        ("remove_list", "remove_list(list, index) → Int — Remove item at index"),
        ("free_list", "free_list(list) → Unit — Free list memory"),
        ("display", "display(value) — Print an integer value"),
        ("display_string", "display_string(s) — Print a string value"),
        ("concat", "concat(a: Str, b: Str) → Str — Concatenate two strings"),
        ("substring", "substring(s, start, len) → Str — Extract a substring"),
        ("string_length", "string_length(s) → Int — Get string length"),
        ("int_to_string", "int_to_string(n: Int) → Str — Convert integer to string"),
        ("string_to_int", "string_to_int(s: Str) → Int — Parse string as integer"),
        ("float_to_string", "float_to_string(f: Float) → Str — Convert float to string"),
        ("int_to_float", "int_to_float(n: Int) → Float — Convert integer to float"),
        ("float_to_int", "float_to_int(f: Float) → Int — Convert float to integer"),
        ("read_line", "read_line() → Str — Read a line from stdin"),
        ("read_int", "read_int() → Int — Read an integer from stdin"),
        ("read_float", "read_float() → Float — Read a float from stdin"),
        ("create_channel", "create_channel() → Channel — Create a message channel"),
        ("send_channel", "send_channel(ch, value) — Send value to channel"),
        ("recv_channel", "recv_channel(ch) → Int — Receive value from channel"),
        ("create_map", "create_map() → Map — Create a new hashmap"),
        ("map_insert", "map_insert(map, key, val) → Int — Insert key-value pair"),
        ("map_get_val", "map_get_val(map, key) → Any — Get value by key"),
        ("map_contains", "map_contains(map, key) → Int — Check if key exists"),
        ("map_delete", "map_delete(map, key) — Remove a key"),
        ("map_keys", "map_keys(map) → List — Get list of all keys"),
        ("map_values", "map_values(map) → List — Get list of all values"),
        ("map_size", "map_size(map) → Int — Get number of entries"),
        ("ep_dlopen", "ep_dlopen(path: Str) → Int — Open dynamic library"),
        ("ep_dlsym", "ep_dlsym(handle, name: Str) → Int — Get symbol from library"),
        ("ep_dlclose", "ep_dlclose(handle) → Int — Close dynamic library"),
        ("ep_dlcall0", "ep_dlcall0(fn_ptr) → Int — Call foreign function (0 args)"),
        ("ep_dlcall1", "ep_dlcall1(fn_ptr, a1) → Int — Call foreign function (1 arg)"),
        ("ep_dlcall2", "ep_dlcall2(fn_ptr, a1, a2) → Int — Call foreign function (2 args)"),
        ("ep_dlcall3", "ep_dlcall3(fn_ptr, a1, a2, a3) → Int — Call foreign function (3 args)"),
        ("ep_dlcall4", "ep_dlcall4(fn_ptr, a1..a4) → Int — Call foreign function (4 args)"),
        ("ep_dlcall5", "ep_dlcall5(fn_ptr, a1..a5) → Int — Call foreign function (5 args)"),
        ("ep_dlcall6", "ep_dlcall6(fn_ptr, a1..a6) → Int — Call foreign function (6 args)"),
        ("ep_dlcall7", "ep_dlcall7(fn_ptr, a1..a7) → Int — Call foreign function (7 args)"),
        ("ep_dlcall8", "ep_dlcall8(fn_ptr, a1..a8) → Int — Call foreign function (8 args)"),
        ("ep_dlcall9", "ep_dlcall9(fn_ptr, a1..a9) → Int — Call foreign function (9 args)"),
        ("ep_dlcall10", "ep_dlcall10(fn_ptr, a1..a10) → Int — Call foreign function (10 args)"),
        ("ep_dlcall_f0", "ep_dlcall_f0(fn_ptr) → Float — Call float function (0 args)"),
        ("ep_dlcall_f1", "ep_dlcall_f1(fn_ptr, f1) → Float — Call float function (1 arg)"),
        ("ep_dlcall_f2", "ep_dlcall_f2(fn_ptr, f1, f2) → Float — Call float function (2 args)"),
        ("ep_dlcall_f3", "ep_dlcall_f3(fn_ptr, f1..f3) → Float — Call float function (3 args)"),
        ("ep_dlcall_f4", "ep_dlcall_f4(fn_ptr, f1..f4) → Float — Call float function (4 args)"),
        ("ep_dlcall_f5", "ep_dlcall_f5(fn_ptr, f1..f5) → Float — Call float function (5 args)"),
        ("ep_dlcall_f6", "ep_dlcall_f6(fn_ptr, f1..f6) → Float — Call float function (6 args)"),
        ("file_read", "file_read(path: Str) → Str — Read entire file contents"),
        ("file_write", "file_write(path: Str, content: Str) → Int — Write to file"),
        ("file_append", "file_append(path: Str, content: Str) → Int — Append to file"),
        ("file_exists", "file_exists(path: Str) → Int — Check if file exists"),
        ("ep_random_int", "ep_random_int(min, max) → Int — Random integer in range"),
        ("ep_abs", "ep_abs(n: Int) → Int — Absolute value"),
        ("ep_time_ms", "ep_time_ms() → Int — Current time in milliseconds"),
        ("ep_time_now_ms", "ep_time_now_ms() → Int — Current time in milliseconds"),
        ("ep_time_now_sec", "ep_time_now_sec() → Int — Current epoch seconds"),
        ("ep_sleep_ms", "ep_sleep_ms(ms: Int) — Sleep for milliseconds"),
        ("ep_system", "ep_system(cmd: Str) → Int — Run system command"),
        ("get_character", "get_character(s, idx) → Int — Get char code at index"),
        ("char_at", "char_at(s, idx) → Int — Get char code at index"),
        ("char_from_code", "char_from_code(code) → Str — Create string from char code"),
        ("string_contains", "string_contains(s, sub) → Int — Check substring"),
        ("string_index_of", "string_index_of(s, sub) → Int — Find substring index"),
        ("string_replace", "string_replace(s, old, new) → Str — Replace occurrences"),
        ("string_from_list", "string_from_list(list) → Str — Build string from char codes"),
        ("string_upper", "string_upper(s) → Str — Convert to uppercase"),
        ("string_lower", "string_lower(s) → Str — Convert to lowercase"),
        ("string_trim", "string_trim(s) → Str — Strip whitespace"),
        ("string_split", "string_split(s, delim) → List — Split string"),
        ("ep_http_request", "ep_http_request(method, url, headers, body) → Str — HTTP request"),
        ("ep_net_connect", "ep_net_connect(host, port) → Int — TCP connect"),
        ("ep_net_listen", "ep_net_listen(port) → Int — TCP listen"),
        ("ep_net_accept", "ep_net_accept(fd) → Int — TCP accept"),
        ("ep_net_send", "ep_net_send(fd, data) → Int — Send data"),
        ("ep_net_recv", "ep_net_recv(fd, max) → Str — Receive data"),
        ("ep_net_close", "ep_net_close(fd) — Close connection"),
        ("ep_sha256", "ep_sha256(data) → Str — SHA-256 hash"),
        ("ep_md5", "ep_md5(data) → Str — MD5 hash"),
        ("ep_sha1", "ep_sha1(data) → Str — SHA-1 hash"),
        ("json_get_string", "json_get_string(json, key) → Str — Extract JSON string"),
        ("json_get_int", "json_get_int(json, key) → Int — Extract JSON integer"),
        ("json_get_bool", "json_get_bool(json, key) → Int — Extract JSON boolean"),
        ("create_deque", "create_deque() → Deque — Create double-ended queue"),
        ("deque_push_front", "deque_push_front(dq, val) — Push to front"),
        ("deque_push_back", "deque_push_back(dq, val) — Push to back"),
        ("deque_pop_front", "deque_pop_front(dq) → Int — Pop from front"),
        ("deque_pop_back", "deque_pop_back(dq) → Int — Pop from back"),
        ("deque_length", "deque_length(dq) → Int — Get deque length"),
        ("channel_has_data", "channel_has_data(ch) → Int — Check if channel has data"),
        ("channel_try_recv", "channel_try_recv(ch) → Int — Non-blocking receive"),
        ("channel_select", "channel_select(chans) → Int — Wait on multiple channels"),
        ("get_argument", "get_argument(index) → Str — Get CLI argument"),
        ("get_argument_count", "get_argument_count() → Int — Get CLI argument count"),
        ("str_to_ptr", "str_to_ptr(s) → Int — Convert string to raw pointer"),
        ("ptr_to_str", "ptr_to_str(ptr) → Str — Convert raw pointer to string"),
        ("alloc_bytes", "alloc_bytes(n) → Int — Allocate n bytes"),
        ("free_bytes", "free_bytes(ptr) → Int — Free allocated bytes"),
    ]
}

// ══════════════════════════════════════════════════════════════
// Diagnostics
// ══════════════════════════════════════════════════════════════

fn publish_diagnostics(uri: &str, content: &str) {
    let mut diagnostics = Vec::new();

    // Phase 1: Lex
    let mut lexer = Lexer::new(content);
    let tokens = match lexer.tokenize() {
        Ok(toks) => toks,
        Err(e) => {
            let line = if e.span.line > 0 { e.span.line - 1 } else { 0 };
            let col = if e.span.col > 0 { e.span.col - 1 } else { 0 };
            diagnostics.push(make_diagnostic(line, col, line, col + 1, 1, &e.message));
            send_diagnostics_notification(uri, diagnostics);
            return;
        }
    };

    // Phase 2: Parse
    let mut parser = Parser::new(tokens);
    let program = match parser.parse_program() {
        Ok(prog) => prog,
        Err(errors) => {
            for e in &errors {
                let line = if e.span.line > 0 { e.span.line - 1 } else { 0 };
                let col = if e.span.col > 0 { e.span.col - 1 } else { 0 };
                diagnostics.push(make_diagnostic(line, col, line, col + 1, 1, &e.message));
            }
            send_diagnostics_notification(uri, diagnostics);
            return;
        }
    };

    // Phase 3: Type check
    let (type_errors, type_warnings) = TypeChecker::check_full(&program);
    for e in &type_errors {
        let line = if e.span.line > 0 { e.span.line - 1 } else { 0 };
        let col = if e.span.col > 0 { e.span.col - 1 } else { 0 };
        let msg = if let Some(hint) = &e.hint {
            format!("{}\nhint: {}", e.message, hint)
        } else {
            e.message.clone()
        };
        diagnostics.push(make_diagnostic(line, col, line, col + 1, 1, &msg));
    }
    for w in &type_warnings {
        let line = if w.span.line > 0 { w.span.line - 1 } else { 0 };
        let col = if w.span.col > 0 { w.span.col - 1 } else { 0 };
        diagnostics.push(make_diagnostic(line, col, line, col + 1, 2, &w.message));
    }

    send_diagnostics_notification(uri, diagnostics);
}

fn make_diagnostic(start_line: usize, start_char: usize, end_line: usize, end_char: usize, severity: i64, message: &str) -> JsonValue {
    json_obj(vec![
        ("range", json_obj(vec![
            ("start", json_obj(vec![
                ("line", json_int(start_line as i64)),
                ("character", json_int(start_char as i64)),
            ])),
            ("end", json_obj(vec![
                ("line", json_int(end_line as i64)),
                ("character", json_int(end_char as i64)),
            ])),
        ])),
        ("severity", json_int(severity)),
        ("source", json_str("ernosplain")),
        ("message", json_str(message)),
    ])
}

fn send_diagnostics_notification(uri: &str, diagnostics: Vec<JsonValue>) {
    send_notification("textDocument/publishDiagnostics", json_obj(vec![
        ("uri", json_str(uri)),
        ("diagnostics", json_arr(diagnostics)),
    ]));
}

// ══════════════════════════════════════════════════════════════
// Completion
// ══════════════════════════════════════════════════════════════

fn handle_completion(_params: &JsonValue) -> JsonValue {
    let mut items = Vec::new();

    // Add keywords (kind = 14 = Keyword)
    for (kw, detail) in get_keywords() {
        items.push(json_obj(vec![
            ("label", json_str(kw)),
            ("kind", json_int(14)),
            ("detail", json_str(detail)),
            ("insertText", json_str(kw)),
        ]));
    }

    // Add builtin functions (kind = 3 = Function)
    for (name, detail) in get_builtins() {
        items.push(json_obj(vec![
            ("label", json_str(name)),
            ("kind", json_int(3)),
            ("detail", json_str(detail)),
            ("insertText", json_str(name)),
        ]));
    }

    json_obj(vec![
        ("isIncomplete", json_bool(false)),
        ("items", json_arr(items)),
    ])
}

// ══════════════════════════════════════════════════════════════
// Hover
// ══════════════════════════════════════════════════════════════

fn handle_hover(params: &JsonValue, store: &DocumentStore) -> JsonValue {
    let uri = params.get("textDocument")
        .and_then(|td| td.get("uri"))
        .and_then(|u| u.as_str())
        .unwrap_or("");
    let line = params.get("position")
        .and_then(|p| p.get("line"))
        .and_then(|l| l.as_i64())
        .unwrap_or(0) as usize;
    let col = params.get("position")
        .and_then(|p| p.get("character"))
        .and_then(|c| c.as_i64())
        .unwrap_or(0) as usize;

    let content = match store.get(uri) {
        Some(c) => c,
        None => return JsonValue::Null,
    };

    let word = get_word_at(content, line, col);
    if word.is_empty() {
        return JsonValue::Null;
    }

    // Check keywords
    for (kw, desc) in get_keywords() {
        if kw == word {
            return json_obj(vec![
                ("contents", json_obj(vec![
                    ("kind", json_str("markdown")),
                    ("value", json_str(&format!("**Keyword: `{}`**\n\n{}", kw, desc))),
                ])),
            ]);
        }
    }

    // Check builtins
    for (name, desc) in get_builtins() {
        if name == word {
            return json_obj(vec![
                ("contents", json_obj(vec![
                    ("kind", json_str("markdown")),
                    ("value", json_str(&format!("**Built-in Function**\n\n```\n{}\n```", desc))),
                ])),
            ]);
        }
    }

    // Check user-defined functions/structs/enums in the document
    if let Some(hover_text) = get_definition_hover(content, &word) {
        return json_obj(vec![
            ("contents", json_obj(vec![
                ("kind", json_str("markdown")),
                ("value", json_str(&hover_text)),
            ])),
        ]);
    }

    JsonValue::Null
}

fn get_word_at(content: &str, line: usize, col: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if line >= lines.len() {
        return String::new();
    }
    let line_str = lines[line];
    let chars: Vec<char> = line_str.chars().collect();
    if col >= chars.len() {
        return String::new();
    }

    // Find word boundaries
    let mut start = col;
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }
    let mut end = col;
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
        end += 1;
    }

    chars[start..end].iter().collect()
}

fn get_definition_hover(content: &str, word: &str) -> Option<String> {
    // Try to parse and look for matching definitions
    let mut lexer = Lexer::new(content);
    let tokens = lexer.tokenize().ok()?;
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program().ok()?;

    // Check functions
    for func in &program.functions {
        if func.name == word {
            let params: Vec<String> = func.params.iter().map(|(name, borrowed, ty)| {
                let mut s = String::new();
                if *borrowed { s.push_str("borrow "); }
                s.push_str(name);
                if let Some(t) = ty {
                    s.push_str(&format!(" as {}", format_type_annotation(t)));
                }
                s
            }).collect();
            let ret = match &func.return_type {
                Some(t) => format!(" returning {}", format_type_annotation(t)),
                None => String::new(),
            };
            let async_prefix = if func.is_async { "async " } else { "" };
            return Some(format!("**Function**\n\n```\n{}define {} with {}{}\n```",
                async_prefix, word, params.join(" and "), ret));
        }
    }

    // Check structs
    for sd in &program.struct_defs {
        if sd.name == word {
            let fields: Vec<String> = sd.fields.iter().map(|(name, ty, _)| {
                format!("  field {} as {}", name, format_type_annotation(ty))
            }).collect();
            return Some(format!("**Structure: `{}`**\n\n```\n{}\n```", word, fields.join("\n")));
        }
    }

    // Check enums
    for ed in &program.enum_defs {
        if ed.name == word {
            let variants: Vec<String> = ed.variants.iter().map(|(name, fields)| {
                if fields.is_empty() {
                    format!("  variant {}", name)
                } else {
                    let fs: Vec<String> = fields.iter().map(|(n, t)| {
                        format!("{} as {}", n, format_type_annotation(t))
                    }).collect();
                    format!("  variant {} with {}", name, fs.join(" and "))
                }
            }).collect();
            return Some(format!("**Choice: `{}`**\n\n```\n{}\n```", word, variants.join("\n")));
        }
    }

    None
}

fn format_type_annotation(ty: &TypeAnnotation) -> String {
    match ty {
        TypeAnnotation::Int => "Int".to_string(),
        TypeAnnotation::Float => "Float".to_string(),
        TypeAnnotation::Bool => "Bool".to_string(),
        TypeAnnotation::Str => "Str".to_string(),
        TypeAnnotation::DynStr => "DynStr".to_string(),
        TypeAnnotation::List => "List".to_string(),
        TypeAnnotation::UserDefined(name) => name.clone(),
        TypeAnnotation::Generic(name, args) => {
            let args_str: Vec<String> = args.iter().map(|a| format_type_annotation(a)).collect();
            format!("{} of {}", name, args_str.join(" and "))
        }
    }
}

// ══════════════════════════════════════════════════════════════
// Go-to-Definition
// ══════════════════════════════════════════════════════════════

fn handle_definition(params: &JsonValue, store: &DocumentStore) -> JsonValue {
    let uri = params.get("textDocument")
        .and_then(|td| td.get("uri"))
        .and_then(|u| u.as_str())
        .unwrap_or("");
    let line = params.get("position")
        .and_then(|p| p.get("line"))
        .and_then(|l| l.as_i64())
        .unwrap_or(0) as usize;
    let col = params.get("position")
        .and_then(|p| p.get("character"))
        .and_then(|c| c.as_i64())
        .unwrap_or(0) as usize;

    let content = match store.get(uri) {
        Some(c) => c,
        None => return JsonValue::Null,
    };

    let word = get_word_at(content, line, col);
    if word.is_empty() {
        return JsonValue::Null;
    }

    // Search for definition in source text (line-based search for reliability)
    // This is more robust than AST-based since spans may not always cover the name
    let lines: Vec<&str> = content.lines().collect();

    // Look for function definitions: "define <name>"
    for (i, text_line) in lines.iter().enumerate() {
        let trimmed = text_line.trim();
        // "define <word> " or "define <word>:" or "async define <word>"
        let check = if let Some(rest) = trimmed.strip_prefix("async ") {
            rest.trim()
        } else {
            trimmed
        };
        if let Some(rest) = check.strip_prefix("define ") {
            let rest = rest.trim();
            // Check for "define structure <word>" and "define choice <word>" etc.
            let name_part = if let Some(after) = rest.strip_prefix("structure ") {
                after.trim()
            } else if let Some(after) = rest.strip_prefix("choice ") {
                after.trim()
            } else if let Some(after) = rest.strip_prefix("trait ") {
                after.trim()
            } else {
                rest
            };
            // Extract the first word from name_part
            let def_name: String = name_part.chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if def_name == word {
                let col_offset = text_line.find(&word).unwrap_or(0);
                return json_obj(vec![
                    ("uri", json_str(uri)),
                    ("range", json_obj(vec![
                        ("start", json_obj(vec![
                            ("line", json_int(i as i64)),
                            ("character", json_int(col_offset as i64)),
                        ])),
                        ("end", json_obj(vec![
                            ("line", json_int(i as i64)),
                            ("character", json_int((col_offset + word.len()) as i64)),
                        ])),
                    ])),
                ]);
            }
        }
    }

    JsonValue::Null
}

// ══════════════════════════════════════════════════════════════
// Initialize handler
// ══════════════════════════════════════════════════════════════

fn handle_initialize(id: &JsonValue) {
    let result = json_obj(vec![
        ("capabilities", json_obj(vec![
            ("textDocumentSync", json_int(1)), // Full sync
            ("completionProvider", json_obj(vec![
                ("triggerCharacters", json_arr(vec![])),
                ("resolveProvider", json_bool(false)),
            ])),
            ("hoverProvider", json_bool(true)),
            ("definitionProvider", json_bool(true)),
        ])),
        ("serverInfo", json_obj(vec![
            ("name", json_str("ernosplain-lsp")),
            ("version", json_str("1.0.0")),
        ])),
    ]);
    send_response(id, result);
}

// ══════════════════════════════════════════════════════════════
// Main LSP loop
// ══════════════════════════════════════════════════════════════

pub fn run_lsp() {
    eprintln!("[ernosplain-lsp] Language server starting...");

    let mut store = DocumentStore::new();
    let mut initialized = false;
    let mut shutdown_requested = false;

    loop {
        let msg = match read_message() {
            Some(m) => m,
            None => {
                eprintln!("[ernosplain-lsp] stdin closed, exiting.");
                break;
            }
        };

        let json = parse_json(&msg);
        let method = json.get("method").and_then(|m| m.as_str()).unwrap_or("").to_string();
        let id = json.get("id").cloned();
        let params = json.get("params").cloned().unwrap_or(JsonValue::Null);

        eprintln!("[ernosplain-lsp] Received: {}", method);

        match method.as_str() {
            "initialize" => {
                if let Some(ref id) = id {
                    handle_initialize(id);
                    initialized = true;
                    eprintln!("[ernosplain-lsp] Initialized.");
                }
            }

            "initialized" => {
                // Client acknowledges initialization, nothing to do
                eprintln!("[ernosplain-lsp] Client initialized notification received.");
            }

            "shutdown" => {
                if let Some(ref id) = id {
                    shutdown_requested = true;
                    send_response(id, JsonValue::Null);
                    eprintln!("[ernosplain-lsp] Shutdown requested.");
                }
            }

            "exit" => {
                let code = if shutdown_requested { 0 } else { 1 };
                eprintln!("[ernosplain-lsp] Exiting with code {}.", code);
                std::process::exit(code);
            }

            "textDocument/didOpen" => {
                if !initialized { continue; }
                if let Some(td) = params.get("textDocument") {
                    let uri = td.get("uri").and_then(|u| u.as_str()).unwrap_or("");
                    let text = td.get("text").and_then(|t| t.as_str()).unwrap_or("");
                    store.open(uri, text);
                    publish_diagnostics(uri, text);
                }
            }

            "textDocument/didChange" => {
                if !initialized { continue; }
                let uri = params.get("textDocument")
                    .and_then(|td| td.get("uri"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("");
                // Full sync: take the last content change
                if let Some(changes) = params.get("contentChanges").and_then(|c| c.as_array()) {
                    if let Some(last) = changes.last() {
                        let text = last.get("text").and_then(|t| t.as_str()).unwrap_or("");
                        store.change(uri, text);
                        publish_diagnostics(uri, text);
                    }
                }
            }

            "textDocument/didClose" => {
                if !initialized { continue; }
                let uri = params.get("textDocument")
                    .and_then(|td| td.get("uri"))
                    .and_then(|u| u.as_str())
                    .unwrap_or("");
                // Clear diagnostics on close
                send_diagnostics_notification(uri, vec![]);
            }

            "textDocument/completion" => {
                if !initialized { continue; }
                if let Some(ref id) = id {
                    let result = handle_completion(&params);
                    send_response(id, result);
                }
            }

            "textDocument/hover" => {
                if !initialized { continue; }
                if let Some(ref id) = id {
                    let result = handle_hover(&params, &store);
                    send_response(id, result);
                }
            }

            "textDocument/definition" => {
                if !initialized { continue; }
                if let Some(ref id) = id {
                    let result = handle_definition(&params, &store);
                    send_response(id, result);
                }
            }

            _ => {
                // Unknown request — return method not found if it has an ID
                if let Some(ref id) = id {
                    if !method.starts_with("$/") {
                        send_error_response(id, -32601, &format!("Method not found: {}", method));
                    }
                }
                // Notifications without IDs are silently ignored
            }
        }
    }
}

// ══════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_parse_object() {
        let input = r#"{"method":"initialize","id":1,"params":{}}"#;
        let val = parse_json(input);
        assert_eq!(val.get("method").and_then(|v| v.as_str()), Some("initialize"));
        assert_eq!(val.get("id").and_then(|v| v.as_i64()), Some(1));
    }

    #[test]
    fn test_json_parse_string_escapes() {
        let input = r#"{"msg":"hello\nworld"}"#;
        let val = parse_json(input);
        let msg = val.get("msg").and_then(|v| v.as_str()).unwrap();
        assert!(msg.contains('\n'));
    }

    #[test]
    fn test_json_serialize() {
        let val = json_obj(vec![
            ("name", json_str("test")),
            ("count", json_int(42)),
            ("active", json_bool(true)),
        ]);
        let s = json_to_string(&val);
        assert!(s.contains("\"name\":\"test\""));
        assert!(s.contains("\"count\":42"));
        assert!(s.contains("\"active\":true"));
    }

    #[test]
    fn test_json_roundtrip() {
        let original = json_obj(vec![
            ("array", json_arr(vec![json_int(1), json_int(2), json_int(3)])),
            ("nested", json_obj(vec![
                ("key", json_str("value")),
            ])),
        ]);
        let serialized = json_to_string(&original);
        let parsed = parse_json(&serialized);
        assert_eq!(
            parsed.get("array").and_then(|a| a.as_array()).map(|a| a.len()),
            Some(3)
        );
        assert_eq!(
            parsed.get("nested").and_then(|n| n.get("key")).and_then(|k| k.as_str()),
            Some("value")
        );
    }

    #[test]
    fn test_get_word_at_basic() {
        let content = "set x to 42\ndefine main:\n    display x";
        assert_eq!(get_word_at(content, 0, 0), "set");
        assert_eq!(get_word_at(content, 0, 4), "x");
        assert_eq!(get_word_at(content, 1, 7), "main");
    }
}
