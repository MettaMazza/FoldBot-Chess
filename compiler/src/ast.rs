use std::ops::Deref;

pub use crate::token::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompOp {
    LessThan,
    GreaterThan,
    LessEqual,
    GreaterEqual,
    Equals,
    NotEquals,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogicalOp {
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeAnnotation {
    Int,
    Float,
    Bool,
    Str,
    DynStr,
    List,
    UserDefined(String),
    Generic(String, Vec<TypeAnnotation>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    pub name: String,
    pub type_params: Vec<String>,
    pub fields: Vec<(String, TypeAnnotation, Option<Expr>)>,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    pub name: String,
    pub type_params: Vec<String>,
    pub variants: Vec<(String, Vec<(String, TypeAnnotation)>)>,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprNode {
    Integer(i64),
    FloatLiteral(f64),
    BoolLiteral(bool),
    StringLiteral(String),
    Identifier(String),
    Binary(Box<Expr>, Op, Box<Expr>),
    Comparison(Box<Expr>, CompOp, Box<Expr>),
    Logical(Box<Expr>, LogicalOp, Box<Expr>),
    Call(String, Vec<Expr>),
    Channel,
    Receive(Box<Expr>),
    Borrow(Box<Expr>),
    FieldAccess(Box<Expr>, String),
    StructCreate(String, Vec<(String, Expr)>),
    EnumCreate(String, String, Vec<Expr>),
    MethodCall(Box<Expr>, String, Vec<Expr>),
    UnaryNot(Box<Expr>),
    TryExpr(Box<Expr>),
    Closure(Vec<String>, Vec<Stmt>),
    Await(Box<Expr>),
    ListLiteral(Vec<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub node: ExprNode,
    pub span: Span,
}

impl Expr {
    pub fn new(node: ExprNode) -> Self {
        Self { node, span: Span::default() }
    }

    pub fn with_span(node: ExprNode, span: Span) -> Self {
        Self { node, span }
    }
}

impl Deref for Expr {
    type Target = ExprNode;
    fn deref(&self) -> &Self::Target {
        &self.node
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtNode {
    Set(String, Expr, Option<TypeAnnotation>),
    If(Expr, Vec<Stmt>, Option<Vec<Stmt>>),
    RepeatWhile(Expr, Vec<Stmt>),
    Return(Expr),
    Display(Expr),
    Spawn(String, Vec<Expr>),
    Send(Expr, Box<Expr>),
    FieldSet(Box<Expr>, String, Expr),
    Match(Expr, Vec<(String, Vec<String>, Vec<Stmt>)>),
    ForEach(String, Expr, Vec<Stmt>),
    Break,
    Continue,
    ExprStmt(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub node: StmtNode,
    pub span: Span,
}

impl Stmt {
    pub fn new(node: StmtNode) -> Self {
        Self { node, span: Span::default() }
    }

    pub fn with_span(node: StmtNode, span: Span) -> Self {
        Self { node, span }
    }
}

impl Deref for Stmt {
    type Target = StmtNode;
    fn deref(&self) -> &Self::Target {
        &self.node
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub params: Vec<(String, bool, Option<TypeAnnotation>)>, // (name, is_borrowed, optional_type)
    pub return_type: Option<TypeAnnotation>,
    pub body: Vec<Stmt>,
    pub is_async: bool,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExternalFunction {
    pub name: String,
    pub params: Vec<(String, bool, Option<TypeAnnotation>)>, // (name, is_borrowed, optional_type)
    pub return_type: Option<TypeAnnotation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MethodDef {
    pub name: String,
    pub struct_name: String,
    pub params: Vec<(String, bool, Option<TypeAnnotation>)>,
    pub return_type: Option<TypeAnnotation>,
    pub body: Vec<Stmt>,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraitDef {
    pub name: String,
    pub method_signatures: Vec<(String, Vec<(String, bool, Option<TypeAnnotation>)>, Option<TypeAnnotation>)>,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraitImpl {
    pub trait_name: String,
    pub for_type: String,
    pub methods: Vec<Function>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub imports: Vec<(String, Option<String>)>,  // (path, optional alias)
    pub externals: Vec<ExternalFunction>,
    pub functions: Vec<Function>,
    pub struct_defs: Vec<StructDef>,
    pub enum_defs: Vec<EnumDef>,
    pub method_defs: Vec<MethodDef>,
    pub trait_defs: Vec<TraitDef>,
    pub trait_impls: Vec<TraitImpl>,
    pub top_level_constants: Vec<Stmt>,  // Top-level set statements (constants for bridges)
}
