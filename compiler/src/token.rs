#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

impl Span {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Keywords
    Define,
    With,
    And,
    Set,
    To,
    If,
    Else,
    Return,
    Display,
    Repeat,
    While,
    Import,
    Spawn,
    Channel,
    Send,
    Receive,
    From,
    External,
    Borrow,
    Structure,
    Field,
    As,
    Is,
    Create,
    Dot,
    Returning,
    For,
    Each,
    In,
    Range,
    Choice,
    Variant,
    Check,
    On,
    Trait,
    Implement,
    Not,
    Break,
    Continue,
    Of,
    Try,
    Given,
    True,
    False,
    Async,
    Await,

    // Operators
    Plus,
    Minus,
    Multiply,
    Divide,
    Modulo,

    // Comparisons
    LessThan,
    GreaterThan,
    LessEqual,
    GreaterEqual,
    Equals,
    NotEquals,
    LogicalAnd,
    LogicalOr,

    // Literals & Identifiers
    Identifier(String),
    Integer(i64),
    Float(f64),
    StringLiteral(String),
    /// Interpolated string parts: alternating (literal, expression_source) pairs + final literal
    /// e.g., "hello {name}!" → [("hello ", "name"), ("!", "")]
    InterpStringParts(Vec<(String, String)>),
    DocComment(String),

    // Punctuation
    Colon,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    Comma,

    // Indentation & Layout
    Indent,
    Dedent,
    Newline,

    // End of file
    EOF,
}
