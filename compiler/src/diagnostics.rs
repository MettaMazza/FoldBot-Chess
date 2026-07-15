/// ErnosPlain Diagnostic Infrastructure
/// 
/// Provides rich, Rust-style error/warning messages with source context,
/// colored output, error codes, and suggestions.

use std::fmt;

// ──────────────────────────────────────────────
// Severity & Error Codes
// ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

impl Severity {
    fn color_code(&self) -> &'static str {
        match self {
            Severity::Error   => "\x1b[1;31m", // bold red
            Severity::Warning => "\x1b[1;33m", // bold yellow
            Severity::Info    => "\x1b[1;36m", // bold cyan
            Severity::Hint    => "\x1b[1;32m", // bold green
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Severity::Error   => "error",
            Severity::Warning => "warning",
            Severity::Info    => "info",
            Severity::Hint    => "hint",
        }
    }
}

/// Standard error codes for ErnosPlain
pub struct ErrorCode;

impl ErrorCode {
    // Syntax errors (E0001-E0009)
    pub const SYNTAX_ERROR: &'static str = "E0001";
    pub const UNEXPECTED_TOKEN: &'static str = "E0002";
    pub const UNEXPECTED_INDENT: &'static str = "E0003";
    pub const UNTERMINATED_STRING: &'static str = "E0004";

    // Name resolution (E0010-E0019)
    pub const UNDEFINED_VARIABLE: &'static str = "E0010";
    pub const UNDEFINED_FUNCTION: &'static str = "E0011";
    pub const UNDEFINED_TYPE: &'static str = "E0012";
    pub const UNDEFINED_FIELD: &'static str = "E0013";
    pub const UNDEFINED_METHOD: &'static str = "E0014";
    pub const UNDEFINED_VARIANT: &'static str = "E0015";

    // Type errors (E0020-E0029)
    pub const TYPE_MISMATCH: &'static str = "E0020";
    pub const ARG_COUNT_MISMATCH: &'static str = "E0021";
    pub const NO_SUCH_FIELD: &'static str = "E0022";
    pub const INVALID_RETURN_TYPE: &'static str = "E0023";
    pub const INVALID_CONDITION: &'static str = "E0024";

    // Ownership/borrowing errors (E0030-E0039)
    pub const USE_AFTER_MOVE: &'static str = "E0030";
    pub const BORROW_WHILE_MOVED: &'static str = "E0031";
    pub const MOVE_WHILE_BORROWED: &'static str = "E0032";
    pub const DOUBLE_MOVE: &'static str = "E0033";
    pub const MUTABLE_BORROW_CONFLICT: &'static str = "E0034";
    pub const SEND_BORROW: &'static str = "E0035";
    pub const NON_SEND: &'static str = "E0036";

    // Warnings (W0040-W0049)
    pub const UNREACHABLE_CODE: &'static str = "W0040";
    pub const UNUSED_VARIABLE: &'static str = "W0041";
    pub const SHADOWED_VARIABLE: &'static str = "W0042";
    pub const IMPLICIT_COERCION: &'static str = "W0043";
    pub const DEPRECATED: &'static str = "W0044";
}

// ──────────────────────────────────────────────
// Diagnostic struct
// ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Label {
    pub line: usize,
    pub col: usize,
    pub len: usize,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: Option<String>,
    pub message: String,
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub span_len: usize,
    pub source_line: Option<String>,
    pub suggestion: Option<String>,
    pub labels: Vec<Label>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code: None,
            message: message.into(),
            file: String::new(),
            line: 0,
            col: 0,
            span_len: 1,
            source_line: None,
            suggestion: None,
            labels: vec![],
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code: None,
            message: message.into(),
            file: String::new(),
            line: 0,
            col: 0,
            span_len: 1,
            source_line: None,
            suggestion: None,
            labels: vec![],
        }
    }

    pub fn with_code(mut self, code: &str) -> Self {
        self.code = Some(code.to_string());
        self
    }

    pub fn at(mut self, file: &str, line: usize, col: usize) -> Self {
        self.file = file.to_string();
        self.line = line;
        self.col = col;
        self
    }

    pub fn with_span(mut self, len: usize) -> Self {
        self.span_len = len;
        self
    }

    pub fn with_source(mut self, source: &str) -> Self {
        self.source_line = Some(source.to_string());
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    pub fn with_label(mut self, line: usize, col: usize, len: usize, message: impl Into<String>) -> Self {
        self.labels.push(Label { line, col, len, message: message.into() });
        self
    }

    /// Format the diagnostic for terminal output with ANSI colors
    pub fn render(&self) -> String {
        let mut out = String::new();
        let reset = "\x1b[0m";
        let bold = "\x1b[1m";
        let _dim = "\x1b[2m";
        let blue = "\x1b[1;34m";
        let sev_color = self.severity.color_code();

        // Header: error[E0042]: message
        out.push_str(&format!("{}{}", sev_color, self.severity.label()));
        if let Some(code) = &self.code {
            out.push_str(&format!("[{}]", code));
        }
        out.push_str(&format!("{}: {}{}{}\n", reset, bold, self.message, reset));

        // Location: --> file:line:col
        if !self.file.is_empty() && self.line > 0 {
            let line_num_width = format!("{}", self.line).len();
            let padding = " ".repeat(line_num_width);
            
            out.push_str(&format!(" {}{} -->{}  {}:{}:{}\n", 
                padding, blue, reset, self.file, self.line, self.col));

            // Source line with underline
            if let Some(source) = &self.source_line {
                out.push_str(&format!(" {} {}|{}\n", padding, blue, reset));
                out.push_str(&format!(" {}{} |{}  {}\n", 
                    blue, self.line, reset, source));
                
                // Underline carets
                let col_offset = if self.col > 0 { self.col - 1 } else { 0 };
                let spaces = " ".repeat(col_offset);
                let carets = "^".repeat(self.span_len.max(1));
                out.push_str(&format!(" {} {}|{}  {}{}{}{}\n",
                    padding, blue, reset, spaces, sev_color, carets, reset));

                // Primary label (if any)
                if !self.labels.is_empty() {
                    let label = &self.labels[0];
                    let label_spaces = " ".repeat(col_offset);
                    out.push_str(&format!(" {} {}|{}  {}{}{}{}\n",
                        padding, blue, reset, label_spaces, sev_color, label.message, reset));
                }
            }

            // Suggestion
            if let Some(suggestion) = &self.suggestion {
                out.push_str(&format!(" {} {}|{}\n", padding, blue, reset));
                out.push_str(&format!(" {} {}={} {}help{}: {}\n",
                    padding, blue, reset, bold, reset, suggestion));
            }
        }

        out
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.render())
    }
}

// ──────────────────────────────────────────────
// Diagnostic Emitter
// ──────────────────────────────────────────────

pub struct DiagnosticEmitter {
    diagnostics: Vec<Diagnostic>,
    error_count: usize,
    warning_count: usize,
}

impl DiagnosticEmitter {
    pub fn new() -> Self {
        Self {
            diagnostics: vec![],
            error_count: 0,
            warning_count: 0,
        }
    }

    pub fn emit(&mut self, diag: Diagnostic) {
        match diag.severity {
            Severity::Error => self.error_count += 1,
            Severity::Warning => self.warning_count += 1,
            _ => {}
        }
        self.diagnostics.push(diag);
    }

    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }

    pub fn error_count(&self) -> usize {
        self.error_count
    }

    pub fn warning_count(&self) -> usize {
        self.warning_count
    }

    /// Print all diagnostics to stderr
    pub fn print_all(&self) {
        for diag in &self.diagnostics {
            eprint!("{}", diag.render());
        }

        if self.error_count > 0 || self.warning_count > 0 {
            let reset = "\x1b[0m";
            let bold = "\x1b[1m";
            let mut parts = vec![];
            if self.error_count > 0 {
                parts.push(format!("\x1b[1;31m{} error{}\x1b[0m", 
                    self.error_count, if self.error_count == 1 { "" } else { "s" }));
            }
            if self.warning_count > 0 {
                parts.push(format!("\x1b[1;33m{} warning{}\x1b[0m",
                    self.warning_count, if self.warning_count == 1 { "" } else { "s" }));
            }
            eprintln!("\n{bold}aborting due to {}{reset}", parts.join(", "));
        }
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

// ──────────────────────────────────────────────
// Source file cache (for looking up source lines)
// ──────────────────────────────────────────────

pub struct SourceCache {
    files: std::collections::HashMap<String, Vec<String>>,
}

impl SourceCache {
    pub fn new() -> Self {
        Self { files: std::collections::HashMap::new() }
    }

    pub fn load_file(&mut self, path: &str) -> std::io::Result<()> {
        let content = std::fs::read_to_string(path)?;
        let lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        self.files.insert(path.to_string(), lines);
        Ok(())
    }

    pub fn load_source(&mut self, name: &str, source: &str) {
        let lines: Vec<String> = source.lines().map(|l| l.to_string()).collect();
        self.files.insert(name.to_string(), lines);
    }

    pub fn get_line(&self, file: &str, line: usize) -> Option<&str> {
        self.files.get(file)
            .and_then(|lines| lines.get(line.saturating_sub(1)))
            .map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_diagnostic_render() {
        let diag = Diagnostic::error("type mismatch in function call")
            .with_code(ErrorCode::TYPE_MISMATCH)
            .at("program.ep", 12, 25)
            .with_span(7)
            .with_source("    set result to add(\"hello\" and 42)")
            .with_label(12, 25, 7, "expected Int, found Str")
            .with_suggestion("did you mean to convert the string to an integer?");

        let rendered = diag.render();
        assert!(rendered.contains("error[E0020]"));
        assert!(rendered.contains("type mismatch"));
        assert!(rendered.contains("program.ep:12:25"));
        assert!(rendered.contains("^^^^^^^"));
        assert!(rendered.contains("did you mean"), "Missing help text in rendered output");
    }

    #[test]
    fn test_warning_diagnostic() {
        let diag = Diagnostic::warning("unused variable 'x'")
            .with_code(ErrorCode::UNUSED_VARIABLE)
            .at("test.ep", 5, 9)
            .with_span(1)
            .with_source("    set x to 42");

        let rendered = diag.render();
        assert!(rendered.contains("warning[W0041]"));
        assert!(rendered.contains("unused variable"));
    }

    #[test]
    fn test_emitter_counts() {
        let mut emitter = DiagnosticEmitter::new();
        emitter.emit(Diagnostic::error("err1"));
        emitter.emit(Diagnostic::warning("warn1"));
        emitter.emit(Diagnostic::error("err2"));
        emitter.emit(Diagnostic::warning("warn2"));
        emitter.emit(Diagnostic::warning("warn3"));
        
        assert_eq!(emitter.error_count(), 2);
        assert_eq!(emitter.warning_count(), 3);
        assert!(emitter.has_errors());
    }
}
