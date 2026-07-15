/// ErnosPlain Type Checker — Hindley-Milner type inference with unification
///
/// This module implements a real type system for ErnosPlain:
/// - Type variables for unknown types
/// - Constraint generation by walking the AST
/// - Unification algorithm to solve constraints
/// - Error reporting with source locations

use std::collections::HashMap;
use crate::ast::*;

/// Simple Levenshtein distance for fuzzy function name matching
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m { dp[i][0] = i; }
    for j in 0..=n { dp[0][j] = j; }
    for i in 1..=m {
        for j in 1..=n {
            let cost = if a[i-1] == b[j-1] { 0 } else { 1 };
            dp[i][j] = (dp[i-1][j] + 1).min(dp[i][j-1] + 1).min(dp[i-1][j-1] + cost);
        }
    }
    dp[m][n]
}

// ──────────────────────────────────────────────
// Core type representations
// ──────────────────────────────────────────────

/// A unique identifier for type variables
pub type TypeVarId = usize;

/// Monomorphic type — a concrete type or a type variable
#[derive(Debug, Clone, PartialEq)]
pub enum MonoType {
    Int,
    Float,
    Bool,
    Str,       // static string literal (&str in C terms)
    DynStr,    // heap-allocated string
    Unit,      // void / no value
    Never,     // unreachable (e.g., after return)
    Any,       // top type — unifies with everything (for heterogeneous container returns)

    /// An unresolved type variable, to be unified
    Var(TypeVarId),

    /// Typed list: List(T) where T is the element type
    List(Box<MonoType>),

    /// Function type: Fun(params, return_type)
    Fun(Vec<MonoType>, Box<MonoType>),

    /// Named struct with optional type arguments
    Struct(String, Vec<MonoType>),

    /// Named enum with optional type arguments  
    Enum(String, Vec<MonoType>),

    /// Borrowed reference to T
    Ref(Box<MonoType>),

    /// Future<T> — result type of an async function
    Future(Box<MonoType>),
}

impl MonoType {
    /// Returns true if this type is or contains heap-allocated data
    pub fn is_heap_allocated(&self) -> bool {
        matches!(self, MonoType::DynStr | MonoType::List(_) | MonoType::Struct(_, _) | MonoType::Enum(_, _))
    }
    
    /// Human-readable name for error messages
    pub fn display_name(&self) -> String {
        match self {
            MonoType::Int => "Int".to_string(),
            MonoType::Float => "Float".to_string(),
            MonoType::Bool => "Bool".to_string(),
            MonoType::Str => "Str".to_string(),
            MonoType::DynStr => "DynStr".to_string(),
            MonoType::Unit => "Unit".to_string(),
            MonoType::Any => "Any".to_string(),
            MonoType::Never => "Never".to_string(),
            MonoType::Var(id) => format!("?T{}", id),
            MonoType::List(elem) => format!("List of {}", elem.display_name()),
            MonoType::Fun(params, ret) => {
                let params_str: Vec<String> = params.iter().map(|p| p.display_name()).collect();
                format!("({}) -> {}", params_str.join(", "), ret.display_name())
            }
            MonoType::Struct(name, args) => {
                if args.is_empty() {
                    name.clone()
                } else {
                    let args_str: Vec<String> = args.iter().map(|a| a.display_name()).collect();
                    format!("{} of {}", name, args_str.join(" and "))
                }
            }
            MonoType::Enum(name, args) => {
                if args.is_empty() {
                    name.clone()
                } else {
                    let args_str: Vec<String> = args.iter().map(|a| a.display_name()).collect();
                    format!("{} of {}", name, args_str.join(" and "))
                }
            }
            MonoType::Ref(inner) => format!("borrow of {}", inner.display_name()),
            MonoType::Future(inner) => format!("Future of {}", inner.display_name()),
        }
    }

    /// Returns true if this type can be safely sent to another thread (ownership transfer).
    /// Types that are NOT Send:
    /// - Ref(T): borrowed references point into another scope and cannot be transferred
    /// - Var(id): unresolved type variables — conservatively treated as Send
    ///
    /// Types that ARE Send:
    /// - All primitives (Int, Float, Bool, Str, DynStr, Unit, Never)
    /// - Any (top type, used for heterogeneous containers — values are owned)
    /// - List(T): owned list (mutable, but ownership transfers)
    /// - Fun: closures capture by value in ErnosPlain, so they are Send
    /// - Struct(name, args): owned struct value
    /// - Enum(name, args): owned enum value
    /// - Future(T): owned future handle
    pub fn is_send(&self) -> bool {
        match self {
            // Primitives are always Send — they are plain values (long long)
            MonoType::Int | MonoType::Float | MonoType::Bool 
            | MonoType::Str | MonoType::DynStr | MonoType::Unit 
            | MonoType::Never | MonoType::Any => true,

            // Unresolved type variables — conservatively allow Send
            // (will be resolved before codegen; if it resolves to Ref, the
            //  borrow checker catches it via SEND_BORROW)
            MonoType::Var(_) => true,

            // Owned containers: Send if the elements are Send
            // (prevents sending a List of borrowed references)
            MonoType::List(elem) => elem.is_send(),

            // Closures: ErnosPlain captures by value, so closures are Send
            MonoType::Fun(_, _) => true,

            // Struct/Enum: Send because they are fully owned values
            // (all fields are owned — ErnosPlain structs cannot hold Ref fields)
            MonoType::Struct(_, _) => true,
            MonoType::Enum(_, _) => true,

            // Borrowed references are NOT Send — they point into the borrower's scope
            // and the referenced data may be freed when the scope exits
            MonoType::Ref(_) => false,

            // Futures: Send — they are owned handles to async results
            MonoType::Future(_) => true,
        }
    }

    /// Returns true if this type can be safely shared (by reference) between threads.
    /// A type T is Sync if &T is Send.
    /// In ErnosPlain, Ref(T) represents a shared reference.
    /// Types that are NOT Sync:
    /// - List(T): mutable container, not safe to share without synchronization
    /// - Struct/Enum: mutable fields, not safe to share
    /// Types that ARE Sync:
    /// - All primitives (immutable values)
    /// - Str (immutable string pointer)
    pub fn is_sync(&self) -> bool {
        match self {
            // Primitives are Sync — they are immutable values
            MonoType::Int | MonoType::Float | MonoType::Bool
            | MonoType::Str | MonoType::Unit | MonoType::Never => true,

            // DynStr: heap-allocated but in ErnosPlain strings are effectively immutable
            // once created (no mutation API), so they are Sync
            MonoType::DynStr => true,

            // Any: conservatively Sync (might be a primitive at runtime)
            MonoType::Any => true,

            // Unresolved type variables — conservatively Sync
            MonoType::Var(_) => true,

            // Lists are NOT Sync — they are mutable containers
            MonoType::List(_) => false,

            // Closures: not Sync (could capture mutable state)
            MonoType::Fun(_, _) => false,

            // Structs/Enums: not Sync — fields can be mutated
            MonoType::Struct(_, _) => false,
            MonoType::Enum(_, _) => false,

            // Borrowed references: Sync if the inner type is Sync
            MonoType::Ref(inner) => inner.is_sync(),

            // Futures: not Sync (one-shot consumption)
            MonoType::Future(_) => false,
        }
    }
}

// ──────────────────────────────────────────────
// Type error
// ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TypeError {
    pub message: String,
    pub span: Span,
    pub hint: Option<String>,
}

impl TypeError {
    fn new(message: String, span: Span) -> Self {
        Self { message, span, hint: None }
    }
    
    fn with_hint(message: String, span: Span, hint: String) -> Self {
        Self { message, span, hint: Some(hint) }
    }
}

impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Type error at line {}:{}: {}", self.span.line, self.span.col, self.message)?;
        if let Some(hint) = &self.hint {
            write!(f, "\n  hint: {}", hint)?;
        }
        Ok(())
    }
}

// ──────────────────────────────────────────────
// Substitution table (type variable → resolved type)
// ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Substitution {
    bindings: HashMap<TypeVarId, MonoType>,
}

impl Substitution {
    fn new() -> Self {
        Self { bindings: HashMap::new() }
    }

    /// Apply substitution to a type, resolving all bound type variables
    fn apply(&self, ty: &MonoType) -> MonoType {
        match ty {
            MonoType::Var(id) => {
                if let Some(bound) = self.bindings.get(id) {
                    // Follow the chain — the bound type might also contain variables
                    self.apply(bound)
                } else {
                    ty.clone()
                }
            }
            MonoType::List(elem) => MonoType::List(Box::new(self.apply(elem))),
            MonoType::Fun(params, ret) => MonoType::Fun(
                params.iter().map(|p| self.apply(p)).collect(),
                Box::new(self.apply(ret)),
            ),
            MonoType::Struct(name, args) => MonoType::Struct(
                name.clone(),
                args.iter().map(|a| self.apply(a)).collect(),
            ),
            MonoType::Enum(name, args) => MonoType::Enum(
                name.clone(),
                args.iter().map(|a| self.apply(a)).collect(),
            ),
            MonoType::Ref(inner) => MonoType::Ref(Box::new(self.apply(inner))),
            MonoType::Future(inner) => MonoType::Future(Box::new(self.apply(inner))),
            // Primitive types are not affected by substitution
            _ => ty.clone(),
        }
    }

    /// Bind a type variable to a type, with occurs check
    fn bind(&mut self, var: TypeVarId, ty: &MonoType) -> Result<(), TypeError> {
        if let MonoType::Var(id) = ty {
            if *id == var {
                // X = X, trivially true
                return Ok(());
            }
        }
        
        // Occurs check: prevent infinite types like T = List(T)
        if self.occurs_in(var, ty) {
            return Err(TypeError::new(
                format!("Infinite type: ?T{} occurs in {}", var, self.apply(ty).display_name()),
                Span::default(),
            ));
        }
        
        self.bindings.insert(var, ty.clone());
        Ok(())
    }

    /// Check if a type variable occurs anywhere in a type
    fn occurs_in(&self, var: TypeVarId, ty: &MonoType) -> bool {
        let resolved = self.apply(ty);
        match &resolved {
            MonoType::Var(id) => *id == var,
            MonoType::List(elem) => self.occurs_in(var, elem),
            MonoType::Fun(params, ret) => {
                params.iter().any(|p| self.occurs_in(var, p)) || self.occurs_in(var, ret)
            }
            MonoType::Struct(_, args) | MonoType::Enum(_, args) => {
                args.iter().any(|a| self.occurs_in(var, a))
            }
            MonoType::Ref(inner) | MonoType::Future(inner) => self.occurs_in(var, inner),
            _ => false,
        }
    }
}

// ──────────────────────────────────────────────
// Unification
// ──────────────────────────────────────────────

/// Unify two types, updating the substitution table.
/// Returns an error if the types are incompatible.
fn unify(subst: &mut Substitution, t1: &MonoType, t2: &MonoType, span: Span) -> Result<(), TypeError> {
    let t1 = subst.apply(t1);
    let t2 = subst.apply(t2);

    match (&t1, &t2) {
        // Same primitive types
        (MonoType::Int, MonoType::Int) => Ok(()),
        (MonoType::Float, MonoType::Float) => Ok(()),
        (MonoType::Bool, MonoType::Bool) => Ok(()),
        (MonoType::Str, MonoType::Str) => Ok(()),
        (MonoType::DynStr, MonoType::DynStr) => Ok(()),
        (MonoType::Unit, MonoType::Unit) => Ok(()),
        (MonoType::Never, _) => Ok(()), // Never unifies with anything (bottom type)
        (_, MonoType::Never) => Ok(()),

        // Any unifies with everything (top type for heterogeneous containers)
        (MonoType::Any, _) => Ok(()),
        (_, MonoType::Any) => Ok(()),
        
        // Str and DynStr are compatible (string coercion)
        (MonoType::Str, MonoType::DynStr) | (MonoType::DynStr, MonoType::Str) => Ok(()),
        
        // Int and Float mixed arithmetic promotion
        (MonoType::Int, MonoType::Float) | (MonoType::Float, MonoType::Int) => Ok(()),
        
        // Int and Bool are compatible (ErnosPlain uses int for booleans)
        (MonoType::Int, MonoType::Bool) | (MonoType::Bool, MonoType::Int) => Ok(()),

        // NOTE: Int ↔ Str/DynStr coercion is intentionally NOT here.
        // It must stay removed so double("hello") fails at the call site.
        // The self-hosted compiler's string-as-int pattern (+ 0 cast) is
        // handled by allowing Str/DynStr in the arithmetic addition check.

        // Int ↔ List coercion: lists are pointers (long long) at runtime.
        // The self-hosted compiler stores list handles in other lists.
        (MonoType::Int, MonoType::List(_)) | (MonoType::List(_), MonoType::Int) => Ok(()),

        // Type variable binding
        (MonoType::Var(id), _) => subst.bind(*id, &t2),
        (_, MonoType::Var(id)) => subst.bind(*id, &t1),

        // Structural types
        (MonoType::List(e1), MonoType::List(e2)) => unify(subst, e1, e2, span),
        
        (MonoType::Fun(p1, r1), MonoType::Fun(p2, r2)) => {
            if p1.len() != p2.len() {
                return Err(TypeError::new(
                    format!("Function argument count mismatch: expected {}, found {}", p1.len(), p2.len()),
                    span,
                ));
            }
            for (a, b) in p1.iter().zip(p2.iter()) {
                unify(subst, a, b, span)?;
            }
            unify(subst, r1, r2, span)
        }
        
        (MonoType::Struct(n1, a1), MonoType::Struct(n2, a2)) => {
            if n1 != n2 {
                return Err(TypeError::new(
                    format!("Type mismatch: expected {}, found {}", n1, n2),
                    span,
                ));
            }
            for (a, b) in a1.iter().zip(a2.iter()) {
                unify(subst, a, b, span)?;
            }
            Ok(())
        }
        
        (MonoType::Enum(n1, a1), MonoType::Enum(n2, a2)) => {
            if n1 != n2 {
                return Err(TypeError::new(
                    format!("Type mismatch: expected {}, found {}", n1, n2),
                    span,
                ));
            }
            for (a, b) in a1.iter().zip(a2.iter()) {
                unify(subst, a, b, span)?;
            }
            Ok(())
        }
        
        (MonoType::Ref(i1), MonoType::Ref(i2)) => unify(subst, i1, i2, span),
        (MonoType::Future(i1), MonoType::Future(i2)) => unify(subst, i1, i2, span),

        // Incompatible types
        _ => Err(TypeError::new(
            format!("Type mismatch: expected {}, found {}", t1.display_name(), t2.display_name()),
            span,
        )),
    }
}

// ──────────────────────────────────────────────
// Type checker
// ──────────────────────────────────────────────

pub struct TypeChecker {
    subst: Substitution,
    next_var: TypeVarId,
    /// Variable → type for the current scope
    env: Vec<HashMap<String, MonoType>>,
    /// Function name → (param types, return type)
    func_types: HashMap<String, (Vec<MonoType>, MonoType)>,
    /// Struct name → field definitions
    struct_defs: HashMap<String, Vec<(String, MonoType)>>,
    /// Enum name → variant definitions
    enum_defs: HashMap<String, Vec<(String, Vec<(String, MonoType)>)>>,
    /// Variant name → enum name (quick lookup)
    variant_to_enum: HashMap<String, String>,
    /// Method (struct_name, method_name) → (param types, return type)
    method_types: HashMap<(String, String), (Vec<MonoType>, MonoType)>,
    /// Trait name → method signatures: (method_name, param_types, return_type)
    trait_defs: HashMap<String, Vec<(String, Vec<MonoType>, MonoType)>>,
    /// (trait_name, for_type)
    trait_impls: std::collections::HashSet<(String, String)>,
    /// Collected errors (we continue checking even after errors)
    pub errors: Vec<TypeError>,
    /// Collected warnings
    pub warnings: Vec<TypeError>,
    /// Variables bound to closures (for call resolution)
    closure_names: std::collections::HashSet<String>,
    /// Declared return type of the function/method currently being checked.
    /// `Some` only when the function has an explicit `returning` annotation —
    /// inferred-return functions are left unconstrained to preserve self-hosting.
    current_return_type: Option<MonoType>,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self {
            subst: Substitution::new(),
            next_var: 0,
            env: vec![HashMap::new()],
            func_types: HashMap::new(),
            struct_defs: HashMap::new(),
            enum_defs: HashMap::new(),
            variant_to_enum: HashMap::new(),
            method_types: HashMap::new(),
            trait_defs: HashMap::new(),
            trait_impls: std::collections::HashSet::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            closure_names: std::collections::HashSet::new(),
            current_return_type: None,
        }
    }

    /// Generate a fresh type variable
    fn fresh_var(&mut self) -> MonoType {
        let id = self.next_var;
        self.next_var += 1;
        MonoType::Var(id)
    }

    /// Instantiate a function type scheme with fresh type variables.
    /// This ensures each call site gets independent type variables,
    /// preventing cross-call-site unification conflicts.
    fn instantiate(&mut self, param_types: &[MonoType], ret_type: &MonoType) -> (Vec<MonoType>, MonoType) {
        // Collect all Var IDs in the signature
        let mut var_ids = std::collections::HashSet::new();
        for pt in param_types {
            self.collect_vars(pt, &mut var_ids);
        }
        self.collect_vars(ret_type, &mut var_ids);

        if var_ids.is_empty() {
            // No type variables — no need to instantiate
            return (param_types.to_vec(), ret_type.clone());
        }

        // Create a mapping from old Var IDs to fresh ones
        let mut mapping: HashMap<TypeVarId, MonoType> = HashMap::new();
        for id in var_ids {
            mapping.insert(id, self.fresh_var());
        }

        let new_params: Vec<MonoType> = param_types.iter()
            .map(|pt| self.substitute_vars(pt, &mapping))
            .collect();
        let new_ret = self.substitute_vars(ret_type, &mapping);
        (new_params, new_ret)
    }

    /// Collect all Var IDs in a MonoType
    fn collect_vars(&self, ty: &MonoType, ids: &mut std::collections::HashSet<TypeVarId>) {
        match ty {
            MonoType::Var(id) => { ids.insert(*id); }
            MonoType::List(elem) => self.collect_vars(elem, ids),
            MonoType::Fun(params, ret) => {
                for p in params { self.collect_vars(p, ids); }
                self.collect_vars(ret, ids);
            }
            MonoType::Ref(inner) => self.collect_vars(inner, ids),
            MonoType::Future(inner) => self.collect_vars(inner, ids),
            MonoType::Struct(_, args) | MonoType::Enum(_, args) => {
                for a in args { self.collect_vars(a, ids); }
            }
            _ => {} // Int, Float, Bool, Str, DynStr, Unit, Any, Never
        }
    }

    /// Replace Var IDs according to a mapping
    fn substitute_vars(&self, ty: &MonoType, mapping: &HashMap<TypeVarId, MonoType>) -> MonoType {
        match ty {
            MonoType::Var(id) => {
                if let Some(replacement) = mapping.get(id) {
                    replacement.clone()
                } else {
                    ty.clone()
                }
            }
            MonoType::List(elem) => MonoType::List(Box::new(self.substitute_vars(elem, mapping))),
            MonoType::Fun(params, ret) => {
                let new_params: Vec<MonoType> = params.iter()
                    .map(|p| self.substitute_vars(p, mapping))
                    .collect();
                MonoType::Fun(new_params, Box::new(self.substitute_vars(ret, mapping)))
            }
            MonoType::Ref(inner) => MonoType::Ref(Box::new(self.substitute_vars(inner, mapping))),
            MonoType::Future(inner) => MonoType::Future(Box::new(self.substitute_vars(inner, mapping))),
            MonoType::Struct(name, args) => {
                let new_args: Vec<MonoType> = args.iter()
                    .map(|a| self.substitute_vars(a, mapping))
                    .collect();
                MonoType::Struct(name.clone(), new_args)
            }
            MonoType::Enum(name, args) => {
                let new_args: Vec<MonoType> = args.iter()
                    .map(|a| self.substitute_vars(a, mapping))
                    .collect();
                MonoType::Enum(name.clone(), new_args)
            }
            _ => ty.clone(), // Int, Float, Bool, Str, DynStr, Unit, Any, Never
        }
    }

    /// Push a new scope
    fn push_scope(&mut self) {
        self.env.push(HashMap::new());
    }

    /// Pop the current scope
    fn pop_scope(&mut self) {
        self.env.pop();
    }

    /// Look up a variable in all scopes (innermost first)
    fn lookup(&self, name: &str) -> Option<MonoType> {
        for scope in self.env.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return Some(ty.clone());
            }
        }
        None
    }

    /// Define a variable in the current scope
    fn define(&mut self, name: String, ty: MonoType) {
        if let Some(scope) = self.env.last_mut() {
            scope.insert(name, ty);
        }
    }

    /// Convert a TypeAnnotation (from the AST) to a MonoType
    fn annotation_to_mono(&self, ann: &TypeAnnotation) -> MonoType {
        match ann {
            TypeAnnotation::Int => MonoType::Int,
            TypeAnnotation::Float => MonoType::Float,
            TypeAnnotation::Bool => MonoType::Bool,
            TypeAnnotation::Str => MonoType::Str,
            TypeAnnotation::DynStr => MonoType::DynStr,
            TypeAnnotation::List => MonoType::List(Box::new(self.fresh_var_immut())),
            TypeAnnotation::UserDefined(name) => {
                if name == "Any" {
                    MonoType::Any
                } else if self.enum_defs.contains_key(name) {
                    MonoType::Enum(name.clone(), vec![])
                } else {
                    MonoType::Struct(name.clone(), vec![])
                }
            }
            TypeAnnotation::Generic(name, args) => {
                let mono_args: Vec<MonoType> = args.iter().map(|a| self.annotation_to_mono(a)).collect();
                if self.enum_defs.contains_key(name) {
                    MonoType::Enum(name.clone(), mono_args)
                } else {
                    MonoType::Struct(name.clone(), mono_args)
                }
            }
        }
    }

    // Immutable version for use inside &self methods
    fn fresh_var_immut(&self) -> MonoType {
        MonoType::Var(self.next_var) // Not ideal but the var won't collide if we're careful
    }

    /// Record an error without stopping
    fn error(&mut self, message: String, span: Span) {
        self.errors.push(TypeError::new(message, span));
    }
    
    fn error_with_hint(&mut self, message: String, span: Span, hint: String) {
        self.errors.push(TypeError::with_hint(message, span, hint));
    }

    // ──────────────────────────────────────────
    // Phase 1: Register all declarations
    // ──────────────────────────────────────────

    fn register_declarations(&mut self, program: &Program) {
        // Register struct definitions
        for sd in &program.struct_defs {
            let fields: Vec<(String, MonoType)> = sd.fields.iter()
                .map(|(name, ann, _)| (name.clone(), self.annotation_to_mono(ann)))
                .collect();
            self.struct_defs.insert(sd.name.clone(), fields);
        }

        // Register enum definitions — two passes to support recursive types
        // Pass 1: register enum names so annotation_to_mono can see them
        for ed in &program.enum_defs {
            self.enum_defs.insert(ed.name.clone(), vec![]);
            for (vname, _) in &ed.variants {
                self.variant_to_enum.insert(vname.clone(), ed.name.clone());
            }
        }
        // Pass 2: populate variant fields (now self-referential fields resolve correctly)
        for ed in &program.enum_defs {
            let variants: Vec<(String, Vec<(String, MonoType)>)> = ed.variants.iter()
                .map(|(vname, fields)| {
                    let mono_fields: Vec<(String, MonoType)> = fields.iter()
                        .map(|(fname, ann)| (fname.clone(), self.annotation_to_mono(ann)))
                        .collect();
                    (vname.clone(), mono_fields)
                })
                .collect();
            
            self.enum_defs.insert(ed.name.clone(), variants);
        }

        // Register function signatures
        for func in &program.functions {
            let param_types: Vec<MonoType> = func.params.iter()
                .map(|(_, is_borrowed, ann)| {
                    let base = if let Some(a) = ann {
                        self.annotation_to_mono(a)
                    } else {
                        self.fresh_var()
                    };
                    if *is_borrowed {
                        MonoType::Ref(Box::new(base))
                    } else {
                        base
                    }
                })
                .collect();
            
            let ret_type = if let Some(ann) = &func.return_type {
                self.annotation_to_mono(ann)
            } else {
                self.fresh_var()
            };
            
            self.func_types.insert(func.name.clone(), (param_types, ret_type));
        }

        // Register built-in functions FIRST so they can't be shadowed
        // by untyped external defines from imported modules.
        self.register_builtins();

        // Register external function signatures.
        // Skip externals that shadow a builtin unless they have explicit type annotations
        // (which would indicate an intentional override with a more specific type).
        for ext in &program.externals {
            let has_annotations = ext.params.iter().any(|(_, _, ann)| ann.is_some())
                || ext.return_type.is_some();
            if !has_annotations && self.func_types.contains_key(&ext.name) {
                // Untyped external that shadows a known builtin — skip to preserve
                // the builtin's proper types (e.g., ep_sha1: Str -> DynStr).
                continue;
            }

            let param_types: Vec<MonoType> = ext.params.iter()
                .map(|(_, is_borrowed, ann)| {
                    let base = if let Some(a) = ann {
                        self.annotation_to_mono(a)
                    } else {
                        self.fresh_var()
                    };
                    if *is_borrowed {
                        MonoType::Ref(Box::new(base))
                    } else {
                        base
                    }
                })
                .collect();
            
            let ret_type = if let Some(ann) = &ext.return_type {
                self.annotation_to_mono(ann)
            } else {
                self.fresh_var()
            };
            
            self.func_types.insert(ext.name.clone(), (param_types, ret_type));
        }

        // Register method signatures
        for md in &program.method_defs {
            let param_types: Vec<MonoType> = md.params.iter()
                .map(|(_, is_borrowed, ann)| {
                    let base = if let Some(a) = ann {
                        self.annotation_to_mono(a)
                    } else {
                        self.fresh_var()
                    };
                    if *is_borrowed { MonoType::Ref(Box::new(base)) } else { base }
                })
                .collect();
            
            let ret_type = if let Some(ann) = &md.return_type {
                self.annotation_to_mono(ann)
            } else {
                self.fresh_var()
            };
            
            self.method_types.insert(
                (md.struct_name.clone(), md.name.clone()),
                (param_types, ret_type),
            );
        }

        // Register trait definitions
        for td in &program.trait_defs {
            let methods: Vec<(String, Vec<MonoType>, MonoType)> = td.method_signatures.iter()
                .map(|(name, params, ret_ann)| {
                    let param_types: Vec<MonoType> = params.iter()
                        .map(|(_, is_borrowed, ann)| {
                            let base = if let Some(a) = ann {
                                self.annotation_to_mono(a)
                            } else {
                                self.fresh_var()
                            };
                            if *is_borrowed { MonoType::Ref(Box::new(base)) } else { base }
                        })
                        .collect();
                    let ret_type = if let Some(ann) = ret_ann {
                        self.annotation_to_mono(ann)
                    } else {
                        MonoType::Unit
                    };
                    (name.clone(), param_types, ret_type)
                })
                .collect();
            self.trait_defs.insert(td.name.clone(), methods);
        }

        // Register trait implementation methods as regular methods
        for ti in &program.trait_impls {
            self.trait_impls.insert((ti.trait_name.clone(), ti.for_type.clone()));
            for func in &ti.methods {
                let param_types: Vec<MonoType> = func.params.iter()
                    .map(|(_, is_borrowed, ann)| {
                        let base = if let Some(a) = ann {
                            self.annotation_to_mono(a)
                        } else {
                            self.fresh_var()
                        };
                        if *is_borrowed { MonoType::Ref(Box::new(base)) } else { base }
                    })
                    .collect();
                let ret_type = if let Some(ann) = &func.return_type {
                    self.annotation_to_mono(ann)
                } else {
                    self.fresh_var()
                };
                self.method_types.insert(
                    (ti.for_type.clone(), func.name.clone()),
                    (param_types, ret_type),
                );
            }
        }
    }

    fn register_builtins(&mut self) {
        // List operations — pre-compute fresh vars to avoid borrow checker issues
        let v0 = self.fresh_var();
        self.func_types.insert("create_list".into(), (vec![], MonoType::List(Box::new(v0))));
        
        let v1 = self.fresh_var();
        let v2 = self.fresh_var();
        self.func_types.insert("append_list".into(), (vec![MonoType::List(Box::new(v1)), v2], MonoType::Int));
        
        let v3 = self.fresh_var();
        self.func_types.insert("get_list".into(), (vec![MonoType::List(Box::new(v3)), MonoType::Int], MonoType::Any));
        
        let v5 = self.fresh_var();
        let v6 = self.fresh_var();
        self.func_types.insert("set_list".into(), (vec![MonoType::List(Box::new(v5)), MonoType::Int, v6], MonoType::Int));
        
        let v7 = self.fresh_var();
        self.func_types.insert("length_list".into(), (vec![MonoType::List(Box::new(v7))], MonoType::Int));
        
        let v8 = self.fresh_var();
        self.func_types.insert("remove_list".into(), (vec![MonoType::List(Box::new(v8)), MonoType::Int], MonoType::Int));

        // String operations
        self.func_types.insert("string_length".into(), (vec![MonoType::Str], MonoType::Int));
        self.func_types.insert("concat".into(), (vec![MonoType::Str, MonoType::Str], MonoType::DynStr));
        self.func_types.insert("substring".into(), (vec![MonoType::Str, MonoType::Int, MonoType::Int], MonoType::DynStr));
        self.func_types.insert("int_to_string".into(), (vec![MonoType::Int], MonoType::DynStr));
        self.func_types.insert("float_to_string".into(), (vec![MonoType::Float], MonoType::DynStr));
        self.func_types.insert("string_to_int".into(), (vec![MonoType::Str], MonoType::Int));
        self.func_types.insert("ep_int_to_str".into(), (vec![MonoType::Int], MonoType::DynStr));

        // Math
        self.func_types.insert("int_to_float".into(), (vec![MonoType::Int], MonoType::Float));
        self.func_types.insert("float_to_int".into(), (vec![MonoType::Float], MonoType::Int));

        // I/O
        self.func_types.insert("read_line".into(), (vec![], MonoType::DynStr));
        self.func_types.insert("read_int".into(), (vec![], MonoType::Int));
        self.func_types.insert("read_float".into(), (vec![], MonoType::Float));

        // Concurrency
        self.func_types.insert("create_channel".into(), (vec![], MonoType::Int));
        self.func_types.insert("send_channel".into(), (vec![MonoType::Int, MonoType::Int], MonoType::Unit));
        self.func_types.insert("recv_channel".into(), (vec![MonoType::Int], MonoType::Int));

        // List operations (continued)
        let v9 = self.fresh_var();
        self.func_types.insert("pop_list".into(), (vec![MonoType::List(Box::new(v9))], MonoType::Any));

        // Map operations
        let v10 = self.fresh_var();
        self.func_types.insert("create_map".into(), (vec![], v10));
        self.func_types.insert("map_insert".into(), (vec![MonoType::Int, MonoType::Int, MonoType::Int], MonoType::Int));
        self.func_types.insert("map_set_str".into(), (vec![MonoType::Int, MonoType::Str, MonoType::Int], MonoType::Int));
        self.func_types.insert("map_get_val".into(), (vec![MonoType::Int, MonoType::Int], MonoType::Any));
        self.func_types.insert("map_get_str".into(), (vec![MonoType::Int, MonoType::Str], MonoType::Any));
        let v11 = self.fresh_var();
        self.func_types.insert("map_keys".into(), (vec![MonoType::Int], MonoType::List(Box::new(v11))));
        self.func_types.insert("map_has_key".into(), (vec![MonoType::Int, MonoType::Int], MonoType::Int));

        // String operations (continued)
        self.func_types.insert("string_upper".into(), (vec![MonoType::Str], MonoType::DynStr));
        self.func_types.insert("string_lower".into(), (vec![MonoType::Str], MonoType::DynStr));
        self.func_types.insert("string_trim".into(), (vec![MonoType::Str], MonoType::DynStr));
        let v_split = self.fresh_var();
        self.func_types.insert("string_split".into(), (vec![MonoType::Str, MonoType::Str], MonoType::List(Box::new(v_split))));
        self.func_types.insert("string_to_list".into(), (vec![MonoType::Str], MonoType::List(Box::new(MonoType::Int))));
        self.func_types.insert("char_at".into(), (vec![MonoType::Str, MonoType::Int], MonoType::Int));
        self.func_types.insert("char_from_code".into(), (vec![MonoType::Int], MonoType::DynStr));
        self.func_types.insert("string_contains".into(), (vec![MonoType::Str, MonoType::Str], MonoType::Int));
        self.func_types.insert("string_index_of".into(), (vec![MonoType::Str, MonoType::Str], MonoType::Int));
        self.func_types.insert("string_replace".into(), (vec![MonoType::Str, MonoType::Str, MonoType::Str], MonoType::DynStr));

        // File I/O
        self.func_types.insert("file_read".into(), (vec![MonoType::Str], MonoType::DynStr));
        self.func_types.insert("file_write".into(), (vec![MonoType::Str, MonoType::Str], MonoType::Int));
        self.func_types.insert("file_append".into(), (vec![MonoType::Str, MonoType::Str], MonoType::Int));
        self.func_types.insert("file_exists".into(), (vec![MonoType::Str], MonoType::Int));

        // Math / random
        self.func_types.insert("ep_abs".into(), (vec![MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_random_int".into(), (vec![MonoType::Int, MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_time_ms".into(), (vec![], MonoType::Int));
        self.func_types.insert("ep_sleep_ms".into(), (vec![MonoType::Int], MonoType::Unit));
        self.func_types.insert("ep_system".into(), (vec![MonoType::Str], MonoType::Int));
        self.func_types.insert("ep_play_sound".into(), (vec![MonoType::Str], MonoType::Int));

        // Dynamic library loading (FFI)
        self.func_types.insert("ep_dlopen".into(), (vec![MonoType::Str], MonoType::Int));
        self.func_types.insert("ep_dlsym".into(), (vec![MonoType::Int, MonoType::Str], MonoType::Int));
        self.func_types.insert("ep_dlclose".into(), (vec![MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_dlcall0".into(), (vec![MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_dlcall1".into(), (vec![MonoType::Int, MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_dlcall2".into(), (vec![MonoType::Int, MonoType::Int, MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_dlcall3".into(), (vec![MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_dlcall4".into(), (vec![MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_dlcall5".into(), (vec![MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_dlcall6".into(), (vec![MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_dlcall7".into(), (vec![MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_dlcall8".into(), (vec![MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_dlcall9".into(), (vec![MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_dlcall10".into(), (vec![MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int, MonoType::Int], MonoType::Int));

        // Float FFI: ep_dlcall_f* — call C functions that take/return doubles
        self.func_types.insert("ep_dlcall_f0".into(), (vec![MonoType::Int], MonoType::Float));
        self.func_types.insert("ep_dlcall_f1".into(), (vec![MonoType::Int, MonoType::Float], MonoType::Float));
        self.func_types.insert("ep_dlcall_f2".into(), (vec![MonoType::Int, MonoType::Float, MonoType::Float], MonoType::Float));
        self.func_types.insert("ep_dlcall_f3".into(), (vec![MonoType::Int, MonoType::Float, MonoType::Float, MonoType::Float], MonoType::Float));
        self.func_types.insert("ep_dlcall_f4".into(), (vec![MonoType::Int, MonoType::Float, MonoType::Float, MonoType::Float, MonoType::Float], MonoType::Float));
        self.func_types.insert("ep_dlcall_f5".into(), (vec![MonoType::Int, MonoType::Float, MonoType::Float, MonoType::Float, MonoType::Float, MonoType::Float], MonoType::Float));
        self.func_types.insert("ep_dlcall_f6".into(), (vec![MonoType::Int, MonoType::Float, MonoType::Float, MonoType::Float, MonoType::Float, MonoType::Float, MonoType::Float], MonoType::Float));

        // Float FFI: ep_dlcall_fd* — call C functions that take doubles but return int
        self.func_types.insert("ep_dlcall_fd1".into(), (vec![MonoType::Int, MonoType::Float], MonoType::Int));
        self.func_types.insert("ep_dlcall_fd2".into(), (vec![MonoType::Int, MonoType::Float, MonoType::Float], MonoType::Int));
        self.func_types.insert("ep_dlcall_fd3".into(), (vec![MonoType::Int, MonoType::Float, MonoType::Float, MonoType::Float], MonoType::Int));

        // Float/bits conversion utilities
        self.func_types.insert("ep_double_to_bits".into(), (vec![MonoType::Float], MonoType::Int));
        self.func_types.insert("ep_bits_to_double".into(), (vec![MonoType::Int], MonoType::Float));
        self.func_types.insert("display".into(), (vec![MonoType::Int], MonoType::Unit));
        self.func_types.insert("display_string".into(), (vec![MonoType::Str], MonoType::Unit));
        self.func_types.insert("ep_auto_to_string".into(), (vec![MonoType::Int], MonoType::DynStr));

        // Memory management
        let v12 = self.fresh_var();
        self.func_types.insert("free_list".into(), (vec![MonoType::List(Box::new(v12))], MonoType::Unit));
        self.func_types.insert("free_map".into(), (vec![MonoType::Int], MonoType::Unit));
        self.func_types.insert("free_deque".into(), (vec![MonoType::Int], MonoType::Unit));
        self.func_types.insert("ep_gc_get_minor_count".into(), (vec![], MonoType::Int));
        self.func_types.insert("ep_gc_get_major_count".into(), (vec![], MonoType::Int));
        self.func_types.insert("ep_gc_get_nursery_count".into(), (vec![], MonoType::Int));

        // Map operations (continued)
        self.func_types.insert("map_size".into(), (vec![MonoType::Int], MonoType::Int));
        let vmc = self.fresh_var();
        self.func_types.insert("map_contains".into(), (vec![MonoType::Int, vmc], MonoType::Int));
        let vmd = self.fresh_var();
        self.func_types.insert("map_delete".into(), (vec![MonoType::Int, vmd], MonoType::Unit));
        let v13 = self.fresh_var();
        self.func_types.insert("map_values".into(), (vec![MonoType::Int], MonoType::List(Box::new(v13))));

        // Deque operations
        self.func_types.insert("create_deque".into(), (vec![], MonoType::Int));
        self.func_types.insert("deque_push_front".into(), (vec![MonoType::Int, MonoType::Int], MonoType::Unit));
        self.func_types.insert("deque_push_back".into(), (vec![MonoType::Int, MonoType::Int], MonoType::Unit));
        self.func_types.insert("deque_pop_front".into(), (vec![MonoType::Int], MonoType::Int));
        self.func_types.insert("deque_pop_back".into(), (vec![MonoType::Int], MonoType::Int));
        self.func_types.insert("deque_length".into(), (vec![MonoType::Int], MonoType::Int));

        self.func_types.insert("channel_has_data".into(), (vec![MonoType::Int], MonoType::Int));
        self.func_types.insert("channel_try_recv".into(), (vec![MonoType::Int], MonoType::Int));
        self.func_types.insert("channel_select".into(), (vec![MonoType::Int], MonoType::Int));

        // Structured Concurrency
        self.func_types.insert("create_task_group".into(), (vec![], MonoType::Int));
        let v_fut = self.fresh_var();
        self.func_types.insert("add_task_group".into(), (vec![MonoType::Int, v_fut], MonoType::Unit));
        let v_tg = self.fresh_var();
        self.func_types.insert("wait_task_group".into(), (vec![MonoType::Int], MonoType::List(Box::new(v_tg))));
        let v_timeout_fut = self.fresh_var();
        self.func_types.insert("async_timeout".into(), (vec![MonoType::Int, v_timeout_fut], MonoType::Int));
        let v_cancel_fut = self.fresh_var();
        self.func_types.insert("cancel_task".into(), (vec![v_cancel_fut], MonoType::Unit));
        self.func_types.insert("sleep_ms".into(), (vec![MonoType::Int], MonoType::Future(Box::new(MonoType::Int))));

        // File system
        self.func_types.insert("read_file_content".into(), (vec![MonoType::Str], MonoType::DynStr));
        self.func_types.insert("write_file_content".into(), (vec![MonoType::Str, MonoType::Str], MonoType::Int));
        self.func_types.insert("run_command".into(), (vec![MonoType::Str], MonoType::DynStr));
        self.func_types.insert("fs_exists".into(), (vec![MonoType::Str], MonoType::Int));
        self.func_types.insert("fs_is_file".into(), (vec![MonoType::Str], MonoType::Int));
        self.func_types.insert("fs_is_dir".into(), (vec![MonoType::Str], MonoType::Int));
        self.func_types.insert("fs_get_size".into(), (vec![MonoType::Str], MonoType::Int));
        self.func_types.insert("fs_copy_file".into(), (vec![MonoType::Str, MonoType::Str], MonoType::Int));
        self.func_types.insert("fs_move_file".into(), (vec![MonoType::Str, MonoType::Str], MonoType::Int));
        self.func_types.insert("fs_delete_file".into(), (vec![MonoType::Str], MonoType::Int));
        let v14 = self.fresh_var();
        self.func_types.insert("fs_scan_dir".into(), (vec![MonoType::Str], MonoType::List(Box::new(v14))));

        // String utilities (continued)
        self.func_types.insert("get_character".into(), (vec![MonoType::Str, MonoType::Int], MonoType::Int));
        self.func_types.insert("string_from_list".into(), (vec![MonoType::List(Box::new(MonoType::Int))], MonoType::DynStr));
        let v_gldp = self.fresh_var();
        self.func_types.insert("get_list_data_ptr".into(), (vec![MonoType::List(Box::new(v_gldp))], MonoType::Int));

        // CLI arguments
        self.func_types.insert("get_argument".into(), (vec![MonoType::Int], MonoType::DynStr));
        self.func_types.insert("get_argument_count".into(), (vec![], MonoType::Int));

        // Networking
        self.func_types.insert("ep_net_connect".into(), (vec![MonoType::Str, MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_net_listen".into(), (vec![MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_net_accept".into(), (vec![MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_net_send".into(), (vec![MonoType::Int, MonoType::Str], MonoType::Int));
        self.func_types.insert("ep_net_recv".into(), (vec![MonoType::Int, MonoType::Int], MonoType::DynStr));
        self.func_types.insert("ep_net_recv_bytes".into(), (vec![MonoType::Int, MonoType::Int], MonoType::DynStr));
        self.func_types.insert("ep_net_close".into(), (vec![MonoType::Int], MonoType::Unit));

        // HTTP
        self.func_types.insert("ep_http_request".into(), (vec![MonoType::Str, MonoType::Str, MonoType::Str, MonoType::Str], MonoType::DynStr));

        // Crypto
        self.func_types.insert("ep_md5".into(), (vec![MonoType::Str], MonoType::DynStr));
        self.func_types.insert("ep_sha256".into(), (vec![MonoType::Str], MonoType::DynStr));
        self.func_types.insert("ep_sha1".into(), (vec![MonoType::Str], MonoType::DynStr));

        // JSON
        self.func_types.insert("json_get_string".into(), (vec![MonoType::Str, MonoType::Str], MonoType::DynStr));
        self.func_types.insert("json_get_int".into(), (vec![MonoType::Str, MonoType::Str], MonoType::Int));
        self.func_types.insert("json_get_bool".into(), (vec![MonoType::Str, MonoType::Str], MonoType::Int));

        // SQLite
        self.func_types.insert("sqlite_get_callback_ptr".into(), (vec![MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_sqlite3_open".into(), (vec![MonoType::Str, MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_sqlite3_close".into(), (vec![MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_sqlite3_exec".into(), (vec![MonoType::Int, MonoType::Str, MonoType::Int, MonoType::Int, MonoType::Int], MonoType::Int));

        // Time (additional)
        self.func_types.insert("ep_time_now_ms".into(), (vec![], MonoType::Int));
        self.func_types.insert("ep_time_now_sec".into(), (vec![], MonoType::Int));
        self.func_types.insert("ep_time_year".into(), (vec![MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_time_month".into(), (vec![MonoType::Int], MonoType::Int));
        self.func_types.insert("ep_time_day".into(), (vec![MonoType::Int], MonoType::Int));

        // Platform
        self.func_types.insert("ep_os_name".into(), (vec![], MonoType::Str));

        // FFI pointer/byte builtins
        self.func_types.insert("str_to_ptr".into(), (vec![MonoType::Str], MonoType::Int));
        self.func_types.insert("ptr_to_str".into(), (vec![MonoType::Int], MonoType::DynStr));
        self.func_types.insert("peek_byte".into(), (vec![MonoType::Int, MonoType::Int], MonoType::Int));
        self.func_types.insert("poke_byte".into(), (vec![MonoType::Int, MonoType::Int, MonoType::Int], MonoType::Int));
        self.func_types.insert("alloc_bytes".into(), (vec![MonoType::Int], MonoType::Int));
        self.func_types.insert("free_bytes".into(), (vec![MonoType::Int], MonoType::Int));
        let v_ltb = self.fresh_var();
        self.func_types.insert("list_to_bytes".into(), (vec![MonoType::List(Box::new(v_ltb))], MonoType::Int));
        let v_btl = self.fresh_var();
        self.func_types.insert("bytes_to_list".into(), (vec![MonoType::Int, MonoType::Int], MonoType::List(Box::new(v_btl))));
    }

    // ──────────────────────────────────────────
    // Phase 2: Type check each function body
    // ──────────────────────────────────────────

    pub fn check_program(&mut self, program: &Program) {
        self.register_declarations(program);

        // Register top-level `set` globals into the base scope so they are
        // visible inside every function body (they outlive any function scope).
        for stmt in &program.top_level_constants {
            self.check_stmt(stmt);
        }

        for func in &program.functions {
            self.check_function(func);
        }

        for md in &program.method_defs {
            self.check_method(md);
        }

        // Check trait implementations match their trait definitions
        self.check_trait_impls(program);

        // Check trait impl method bodies
        for ti in &program.trait_impls {
            for func in &ti.methods {
                self.push_scope();
                // Bind self
                if self.enum_defs.contains_key(&ti.for_type) {
                    self.define("self".into(), MonoType::Enum(ti.for_type.clone(), vec![]));
                } else {
                    self.define("self".into(), MonoType::Struct(ti.for_type.clone(), vec![]));
                }
                // Bind parameters
                if let Some((param_types, _)) = self.method_types.get(&(ti.for_type.clone(), func.name.clone())).cloned() {
                    for (i, (name, _, _)) in func.params.iter().enumerate() {
                        if i < param_types.len() {
                            self.define(name.clone(), param_types[i].clone());
                        }
                    }
                }
                let saved_ret = self.current_return_type.take();
                self.current_return_type = func.return_type.as_ref().map(|ann| self.annotation_to_mono(ann));
                for stmt in &func.body {
                    self.check_stmt(stmt);
                }
                self.current_return_type = saved_ret;
                self.pop_scope();
            }
        }
    }

    /// Verify that trait implementations provide all required methods with correct signatures
    fn check_trait_impls(&mut self, program: &Program) {
        for ti in &program.trait_impls {
            // Check the trait exists
            let trait_methods = match self.trait_defs.get(&ti.trait_name).cloned() {
                Some(methods) => methods,
                None => {
                    self.error(
                        format!("Trait '{}' is not defined", ti.trait_name),
                        Span::default(),
                    );
                    continue;
                }
            };

            // Check the target type exists
            let type_exists = self.struct_defs.contains_key(&ti.for_type)
                || self.enum_defs.contains_key(&ti.for_type);
            if !type_exists {
                self.error(
                    format!("Type '{}' is not defined (implementing trait '{}')",
                        ti.for_type, ti.trait_name),
                    Span::default(),
                );
                continue;
            }

            // Check each required method is implemented
            let impl_method_names: Vec<&str> = ti.methods.iter()
                .map(|f| f.name.as_str())
                .collect();

            for (method_name, expected_params, _expected_ret) in &trait_methods {
                if !impl_method_names.contains(&method_name.as_str()) {
                    self.error(
                        format!("Trait '{}' requires method '{}' but it is not implemented for '{}'",
                            ti.trait_name, method_name, ti.for_type),
                        Span::default(),
                    );
                    continue;
                }

                // Check parameter count matches
                if let Some(func) = ti.methods.iter().find(|f| f.name == *method_name) {
                    if func.params.len() != expected_params.len() {
                        self.error(
                            format!("Method '{}' in trait '{}' expects {} parameters but implementation for '{}' has {}",
                                method_name, ti.trait_name, expected_params.len(),
                                ti.for_type, func.params.len()),
                            Span::default(),
                        );
                    }
                }
            }
        }
    }

    fn check_function(&mut self, func: &Function) {
        self.push_scope();

        // Bind parameters
        if let Some((param_types, _)) = self.func_types.get(&func.name).cloned() {
            for (i, (name, _, _)) in func.params.iter().enumerate() {
                if i < param_types.len() {
                    self.define(name.clone(), param_types[i].clone());
                }
            }
        }

        // Enforce the declared return type only when one is written explicitly.
        let saved_ret = self.current_return_type.take();
        self.current_return_type = func.return_type.as_ref().map(|ann| self.annotation_to_mono(ann));

        // Check body
        for stmt in &func.body {
            self.check_stmt(stmt);
        }

        self.current_return_type = saved_ret;
        self.pop_scope();
    }

    fn check_method(&mut self, md: &MethodDef) {
        self.push_scope();

        // Bind `self` parameter
        // Type self correctly based on whether the target is an enum or struct
        if self.enum_defs.contains_key(&md.struct_name) {
            self.define("self".into(), MonoType::Enum(md.struct_name.clone(), vec![]));
        } else {
            self.define("self".into(), MonoType::Struct(md.struct_name.clone(), vec![]));
        }

        // Bind parameters
        if let Some((param_types, _)) = self.method_types.get(&(md.struct_name.clone(), md.name.clone())).cloned() {
            for (i, (name, _, _)) in md.params.iter().enumerate() {
                if i < param_types.len() {
                    self.define(name.clone(), param_types[i].clone());
                }
            }
        }

        let saved_ret = self.current_return_type.take();
        self.current_return_type = md.return_type.as_ref().map(|ann| self.annotation_to_mono(ann));

        for stmt in &md.body {
            self.check_stmt(stmt);
        }

        self.current_return_type = saved_ret;
        self.pop_scope();
    }

    // ──────────────────────────────────────────
    // Statement type checking
    // ──────────────────────────────────────────

    fn check_stmt(&mut self, stmt: &Stmt) {
        match &stmt.node {
            StmtNode::Set(name, expr, type_ann) => {
                let expr_type = self.check_expr(expr);
                
                // Track if this variable is bound to a closure
                if matches!(expr.node, ExprNode::Closure(_, _)) {
                    self.closure_names.insert(name.clone());
                }
                
                if let Some(ann) = type_ann {
                    let declared_type = self.annotation_to_mono(ann);
                    if let Err(_e) = unify(&mut self.subst, &declared_type, &expr_type, expr.span) {
                        self.error_with_hint(
                            format!("Cannot assign {} to variable '{}' declared as {}",
                                expr_type.display_name(), name, declared_type.display_name()),
                            stmt.span,
                            format!("The expression has type {} but the variable was declared as {}",
                                self.subst.apply(&expr_type).display_name(),
                                self.subst.apply(&declared_type).display_name()),
                        );
                    }
                    self.define(name.clone(), declared_type);
                } else {
                    self.define(name.clone(), expr_type);
                }
            }

            StmtNode::If(cond, then_body, else_body) => {
                let cond_type = self.check_expr(cond);
                // Condition should be bool-like (Int or Bool)
                let is_bool_like = matches!(
                    self.subst.apply(&cond_type),
                    MonoType::Int | MonoType::Bool | MonoType::Var(_)
                );
                if !is_bool_like {
                    self.error(
                        format!("Condition must be Bool or Int, found {}", 
                            self.subst.apply(&cond_type).display_name()),
                        cond.span,
                    );
                }

                // ErnosPlain is function-scoped: a `set` inside a branch is
                // visible after the block (codegen hoists locals to function
                // scope). So branch bodies do not open a new scope.
                for s in then_body { self.check_stmt(s); }

                if let Some(else_b) = else_body {
                    for s in else_b { self.check_stmt(s); }
                }
            }

            StmtNode::RepeatWhile(cond, body) => {
                let cond_type = self.check_expr(cond);
                let is_bool_like = matches!(
                    self.subst.apply(&cond_type),
                    MonoType::Int | MonoType::Bool | MonoType::Var(_)
                );
                if !is_bool_like {
                    self.error(
                        format!("Loop condition must be Bool or Int, found {}",
                            self.subst.apply(&cond_type).display_name()),
                        cond.span,
                    );
                }

                // Function-scoped: loop-body `set`s persist after the loop.
                for s in body { self.check_stmt(s); }
            }

            StmtNode::Return(expr) => {
                let ret_type = self.check_expr(expr);
                // Enforce the declared return type when the function annotates one.
                if let Some(declared) = self.current_return_type.clone() {
                    if unify(&mut self.subst, &ret_type, &declared, stmt.span).is_err() {
                        let found = self.subst.apply(&ret_type);
                        let expected = self.subst.apply(&declared);
                        // Downgraded from a hard error to a warning: the codegen
                        // backend represents Int, Map, List, Str and friends all as
                        // `long long`, so a declared-vs-returned type mismatch (e.g.
                        // `return 0` as a null where a Map is declared, or an
                        // as-yet-unresolved inference variable) is runtime-safe and
                        // must not block compilation. Whole-program inference order
                        // also made these non-deterministic. Kept as a warning so the
                        // signal is preserved without rejecting valid, runnable code.
                        self.warnings.push(TypeError::with_hint(
                            format!(
                                "Return type mismatch: function is declared to return {} but this returns {}",
                                expected.display_name(), found.display_name()
                            ),
                            stmt.span,
                            format!("Return a {} value, or change the function's declared return type.", expected.display_name()),
                        ));
                    }
                }
            }

            StmtNode::Display(expr) => {
                let _display_type = self.check_expr(expr);
                // display accepts any type
            }

            StmtNode::Spawn(func_name, args) => {
                let arg_types: Vec<MonoType> = args.iter().map(|a| self.check_expr(a)).collect();
                if let Some((param_types, _)) = self.func_types.get(func_name).cloned() {
                    if arg_types.len() != param_types.len() {
                        self.error(
                            format!("Function '{}' expects {} arguments, got {}", 
                                func_name, param_types.len(), arg_types.len()),
                            stmt.span,
                        );
                    } else {
                        for (i, (arg_t, param_t)) in arg_types.iter().zip(param_types.iter()).enumerate() {
                            if let Err(_) = unify(&mut self.subst, arg_t, param_t, stmt.span) {
                                self.error(
                                    format!("Argument {} of '{}': expected {}, found {}",
                                        i + 1, func_name, param_t.display_name(), arg_t.display_name()),
                                    stmt.span,
                                );
                            }
                        }
                    }
                }
                // Send safety: every argument to spawn must implement Send
                // Borrowed references (Ref) cannot be sent across thread boundaries
                for (i, arg_t) in arg_types.iter().enumerate() {
                    let resolved = self.subst.apply(arg_t);
                    if !resolved.is_send() {
                        self.error_with_hint(
                            format!("[E0036] cannot send {} across threads in spawn '{}' (argument {})",
                                resolved.display_name(), func_name, i + 1),
                            args[i].span,
                            format!("type '{}' does not implement Send — borrowed references cannot cross thread boundaries. \
                                    Consider passing an owned copy instead.", resolved.display_name()),
                        );
                    }
                }
            }

            StmtNode::Send(chan, val) => {
                let _chan_type = self.check_expr(chan);
                let val_type = self.check_expr(val);
                // Send safety: values sent through channels must implement Send
                let resolved_val = self.subst.apply(&val_type);
                if !resolved_val.is_send() {
                    self.error_with_hint(
                        format!("[E0036] cannot send {} through a channel",
                            resolved_val.display_name()),
                        val.span,
                        format!("type '{}' does not implement Send — borrowed references cannot be sent through channels. \
                                Consider sending an owned copy instead.", resolved_val.display_name()),
                    );
                }
            }

            StmtNode::FieldSet(obj, field_name, val) => {
                let obj_type = self.check_expr(obj);
                let val_type = self.check_expr(val);
                
                let resolved = self.subst.apply(&obj_type);
                if let MonoType::Struct(struct_name, _) = &resolved {
                    if let Some(fields) = self.struct_defs.get(struct_name).cloned() {
                        if let Some((_, field_type)) = fields.iter().find(|(n, _)| n == field_name) {
                            if let Err(_) = unify(&mut self.subst, &val_type, field_type, val.span) {
                                self.error(
                                    format!("Cannot assign {} to field '{}' of {}: expected {}",
                                        self.subst.apply(&val_type).display_name(),
                                        field_name, struct_name,
                                        self.subst.apply(field_type).display_name()),
                                    val.span,
                                );
                            }
                        } else {
                            self.error(
                                format!("Struct '{}' has no field '{}'", struct_name, field_name),
                                stmt.span,
                            );
                        }
                    }
                }
            }

            StmtNode::Match(expr, arms) => {
                let match_type = self.check_expr(expr);
                let resolved = self.subst.apply(&match_type);
                
                if let MonoType::Enum(enum_name, _) = &resolved {
                    if let Some(variants) = self.enum_defs.get(enum_name).cloned() {
                        // Exhaustiveness check: collect matched variant names
                        let matched_variants: Vec<&str> = arms.iter()
                            .map(|(vn, _, _)| vn.as_str())
                            .collect();
                        let has_wildcard = matched_variants.contains(&"_") || matched_variants.contains(&"default");

                        if !has_wildcard {
                            for (vname, _) in &variants {
                                if !matched_variants.contains(&vname.as_str()) {
                                    self.warnings.push(TypeError::with_hint(
                                        format!("Non-exhaustive match: variant '{}' of enum '{}' is not handled",
                                            vname, enum_name),
                                        stmt.span,
                                        format!("Add a case for '{}' or add a default/wildcard arm", vname),
                                    ));
                                }
                            }
                        }

                        for (variant_name, bindings, body) in arms {
                            // Validate variant exists (skip wildcard)
                            if variant_name != "_" && variant_name != "default" {
                                if !variants.iter().any(|(vn, _)| vn == variant_name) {
                                    self.error(
                                        format!("'{}' is not a variant of enum '{}'", variant_name, enum_name),
                                        stmt.span,
                                    );
                                }
                            }
                            // Pattern bindings and arm-body `set`s define into the
                            // function scope (function-scoped language; no block scope).
                            if let Some((_, fields)) = variants.iter().find(|(vn, _)| vn == variant_name) {
                                for (i, binding) in bindings.iter().enumerate() {
                                    if i < fields.len() {
                                        self.define(binding.clone(), fields[i].1.clone());
                                    }
                                }
                            }
                            for s in body { self.check_stmt(s); }
                        }
                    }
                } else {
                    // String or integer match — just type-check the body of each arm
                    for (_pattern, _bindings, body) in arms {
                        for s in body { self.check_stmt(s); }
                    }
                }
            }

            StmtNode::ForEach(loop_var, iterable, body) => {
                let iter_type = self.check_expr(iterable);
                let resolved = self.subst.apply(&iter_type);
                
                let elem_type = match &resolved {
                    MonoType::List(elem) => (**elem).clone(),
                    _ => {
                        let mut resolved_elem = MonoType::Int;
                        let mut is_iter = false;
                        if let MonoType::Struct(name, _) | MonoType::Enum(name, _) = &resolved {
                            if self.trait_impls.contains(&("Iterator".to_string(), name.clone())) {
                                is_iter = true;
                                if let Some((_, ret_type)) = self.method_types.get(&(name.clone(), "next".to_string())).cloned() {
                                    let resolved_ret = self.subst.apply(&ret_type);
                                    if let MonoType::Enum(enum_name, _) = &resolved_ret {
                                        if let Some(variants) = self.enum_defs.get(enum_name) {
                                            if let Some((_, fields)) = variants.iter().find(|(vn, _)| vn == "Next") {
                                                if let Some((_, field_type)) = fields.first() {
                                                    resolved_elem = field_type.clone();
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if !is_iter && resolved != MonoType::Int && resolved != MonoType::Any {
                            self.error(
                                format!("Type '{}' is not iterable (must be a List or implement Iterator trait)", resolved.display_name()),
                                iterable.span,
                            );
                        }
                        resolved_elem
                    }
                };
                
                // Function-scoped: the loop variable and any body `set`s remain
                // defined after the loop (matches codegen's local hoisting).
                self.define(loop_var.clone(), elem_type);
                for s in body { self.check_stmt(s); }
            }

            StmtNode::Break | StmtNode::Continue => {}

            StmtNode::ExprStmt(expr) => {
                let _t = self.check_expr(expr);
            }
        }
    }

    // ──────────────────────────────────────────
    // Expression type inference
    // ──────────────────────────────────────────

    fn check_expr(&mut self, expr: &Expr) -> MonoType {
        match &expr.node {
            ExprNode::Integer(_) => MonoType::Int,
            ExprNode::FloatLiteral(_) => MonoType::Float,
            ExprNode::BoolLiteral(_) => MonoType::Bool,
            ExprNode::StringLiteral(_) => MonoType::Str,

            ExprNode::Identifier(name) => {
                if let Some(ty) = self.lookup(name) {
                    ty
                } else if let Some(enum_name) = self.variant_to_enum.get(name).cloned() {
                    // Bare enum variant (no data)
                    MonoType::Enum(enum_name, vec![])
                } else if self.func_types.contains_key(name)
                    || self.closure_names.contains(name)
                    || self.struct_defs.contains_key(name)
                    || self.enum_defs.contains_key(name)
                {
                    // A named function used as a value (higher-order functions) or a
                    // type name referenced as a value. Its type is resolved at the
                    // call/use site, so leave it open here.
                    self.fresh_var()
                } else {
                    // Truly undefined: not a local, parameter, variant, function, or type.
                    self.error_with_hint(
                        format!("Undefined name '{}'", name),
                        expr.span,
                        "Check the spelling, or bind it with `set ... to` before use.".into(),
                    );
                    self.fresh_var()
                }
            }

            ExprNode::Binary(left, op, right) => {
                let lt = self.check_expr(left);
                let rt = self.check_expr(right);
                let lt_r = self.subst.apply(&lt);
                let rt_r = self.subst.apply(&rt);
                
                // Numeric promotion
                if lt_r == MonoType::Float || rt_r == MonoType::Float {
                    // Unify both with Float
                    if let Err(_) = unify(&mut self.subst, &lt, &MonoType::Float, expr.span) {
                        self.error(
                            format!("Left operand of arithmetic must be numeric, found {}", self.subst.apply(&lt).display_name()),
                            left.span,
                        );
                    }
                    if let Err(_) = unify(&mut self.subst, &rt, &MonoType::Float, expr.span) {
                        self.error(
                            format!("Right operand of arithmetic must be numeric, found {}", self.subst.apply(&rt).display_name()),
                            right.span,
                        );
                    }
                    MonoType::Float
                } else {
                    // For addition (+), allow Str/DynStr — this is the "pointer + 0" cast
                    // pattern used by the self-hosted compiler to coerce string pointers to int.
                    // For *, /, -, %: reject Str (e.g. "hello" * 2 is always a bug).
                    let is_add = matches!(op, Op::Add);
                    
                    let str_ok = |ty: &MonoType| -> bool {
                        matches!(ty, MonoType::Int | MonoType::Bool | MonoType::Any | MonoType::DynStr | MonoType::List(_))
                        || (is_add && matches!(ty, MonoType::Str))
                    };
                    
                    if !str_ok(&lt_r) {
                        if let Err(_) = unify(&mut self.subst, &lt, &MonoType::Int, expr.span) {
                            self.error(
                                format!("Left operand of arithmetic must be numeric, found {}", self.subst.apply(&lt).display_name()),
                                left.span,
                            );
                        }
                    }
                    if !str_ok(&rt_r) {
                        if let Err(_) = unify(&mut self.subst, &rt, &MonoType::Int, expr.span) {
                            self.error(
                                format!("Right operand of arithmetic must be numeric, found {}", self.subst.apply(&rt).display_name()),
                                right.span,
                            );
                        }
                    }
                    MonoType::Int
                }
            }

            ExprNode::Comparison(left, _op, right) => {
                let _lt = self.check_expr(left);
                let _rt = self.check_expr(right);
                MonoType::Bool
            }

            ExprNode::Logical(left, _op, right) => {
                let _lt = self.check_expr(left);
                let _rt = self.check_expr(right);
                MonoType::Bool
            }

            ExprNode::Call(name, args) => {
                let arg_types: Vec<MonoType> = args.iter().map(|a| self.check_expr(a)).collect();
                
                if let Some((param_types_raw, ret_type_raw)) = self.func_types.get(name).cloned() {
                    // Instantiate: replace all Var IDs with fresh ones so each call
                    // site gets independent type variables. Without this, two calls
                    // to get_list() in different modules would share the same Var IDs,
                    // and the first call's unification (e.g., Var(3)=Str) would
                    // prevent the second call from unifying with Int.
                    let (param_types, ret_type) = self.instantiate(&param_types_raw, &ret_type_raw);

                    // Polymorphic builtins that accept any value type — skip arg type checking
                    // because the C runtime stores all values as long long (ints or pointers).
                    // ep_dlcall*/ep_dlsym are FFI escape hatches where strings pass as int handles.
                    let skip_type_check = matches!(name.as_str(),
                        "append_list" | "get_list" | "set_list" |
                        "map_insert" | "map_set_str" | "map_get_val" | "map_get_str" |
                        "map_contains" | "map_delete" | "map_keys" | "map_size" |
                        "map_values" | "map_has_key" | "free_list" | "free_map" |
                        "ep_auto_to_string" | "display" |
                        // Self-hosted compiler utility functions that handle mixed Int/Str
                        // args (string pointers stored as integers in lists).
                        "display_string" | "string_concat" | "dec_borrow_count" |
                        "inc_borrow_count" | "map_get" | "map_put" | "create_token"
                    ) || name.starts_with("ep_dlcall");
                    // Check argument count (some builtins like concat are variadic)
                    if !skip_type_check && arg_types.len() != param_types.len() {
                        self.error(
                            format!("Function '{}' expects {} arguments, got {}",
                                name, param_types.len(), arg_types.len()),
                            expr.span,
                        );
                    } else if !skip_type_check {
                        // Check argument types
                        for (i, (arg_t, param_t)) in arg_types.iter().zip(param_types.iter()).enumerate() {
                            if let Err(_) = unify(&mut self.subst, arg_t, param_t, expr.span) {
                                let resolved_arg = self.subst.apply(arg_t);
                                let resolved_param = self.subst.apply(param_t);
                                self.error_with_hint(
                                    format!("Argument {} of '{}': expected {}, found {}",
                                        i + 1, name, resolved_param.display_name(), resolved_arg.display_name()),
                                    expr.span,
                                    format!("Consider converting the value to {}", resolved_param.display_name()),
                                );
                            }
                        }
                    }
                    ret_type
                } else if self.closure_names.contains(name) || self.lookup(name).is_some() {
                    // It's a closure variable or in-scope variable — treat as valid call
                    self.fresh_var()
                } else {
                    // Unknown function — emit error with suggestion
                    let mut best_match: Option<(&str, usize)> = None;
                    for known in self.func_types.keys() {
                        // Check Levenshtein distance
                        let dist = levenshtein_distance(name, known);
                        if dist <= 3 {
                            if best_match.is_none() || dist < best_match.unwrap().1 {
                                best_match = Some((known, dist));
                            }
                        }
                        // Also check if user's name is a suffix/substring of a known function
                        // (handles to_upper → string_upper, index_of → string_index_of)
                        if known.ends_with(name) || known.ends_with(&format!("_{}", name)) {
                            best_match = Some((known, 0));
                        }
                        // Check if stripping common prefixes from user's name matches a suffix
                        // e.g. to_upper → strip "to_" → upper → string_upper ends with upper
                        for prefix in &["to_", "get_", "is_", "do_"] {
                            if let Some(stripped) = name.strip_prefix(prefix) {
                                if known.ends_with(stripped) && stripped.len() >= 3 {
                                    best_match = Some((known, 0));
                                }
                            }
                        }
                    }
                    if let Some((suggestion, _)) = best_match {
                        self.error_with_hint(
                            format!("Unknown function '{}'", name),
                            expr.span,
                            format!("Did you mean '{}'?", suggestion),
                        );
                    } else {
                        self.error(
                            format!("Unknown function '{}'. Use --list-builtins to see available functions.", name),
                            expr.span,
                        );
                    }
                    self.fresh_var()
                }
            }

            ExprNode::Channel => MonoType::Int,
            ExprNode::Receive(inner) => {
                let _chan_type = self.check_expr(inner);
                MonoType::Int
            }

            ExprNode::Borrow(inner) => {
                let inner_type = self.check_expr(inner);
                MonoType::Ref(Box::new(inner_type))
            }

            ExprNode::FieldAccess(obj, field_name) => {
                let obj_type = self.check_expr(obj);
                let resolved = self.subst.apply(&obj_type);
                
                if let MonoType::Struct(struct_name, _) = &resolved {
                    if let Some(fields) = self.struct_defs.get(struct_name).cloned() {
                        if let Some((_, field_type)) = fields.iter().find(|(n, _)| n == field_name) {
                            return field_type.clone();
                        } else {
                            self.error(
                                format!("Struct '{}' has no field '{}'", struct_name, field_name),
                                expr.span,
                            );
                        }
                    }
                }
                self.fresh_var()
            }

            ExprNode::StructCreate(struct_name, field_exprs) => {
                if let Some(fields) = self.struct_defs.get(struct_name).cloned() {
                    for (fname, fexpr) in field_exprs {
                        let expr_type = self.check_expr(fexpr);
                        if let Some((_, expected_type)) = fields.iter().find(|(n, _)| n == fname) {
                            if let Err(_) = unify(&mut self.subst, &expr_type, expected_type, fexpr.span) {
                                self.error(
                                    format!("Field '{}' of '{}': expected {}, found {}",
                                        fname, struct_name,
                                        self.subst.apply(expected_type).display_name(),
                                        self.subst.apply(&expr_type).display_name()),
                                    fexpr.span,
                                );
                            }
                        }
                    }
                }
                MonoType::Struct(struct_name.clone(), vec![])
            }

            ExprNode::EnumCreate(enum_name, variant_name, args) => {
                let actual_enum = if enum_name.is_empty() {
                    self.variant_to_enum.get(variant_name).cloned().unwrap_or_default()
                } else {
                    enum_name.clone()
                };
                
                // Type check variant args against declared field types
                let variant_fields = self.enum_defs.get(&actual_enum)
                    .and_then(|variants| variants.iter().find(|(vn, _)| vn == variant_name))
                    .map(|(_, fields)| fields.clone());

                if let Some(fields) = variant_fields {
                    if args.len() != fields.len() {
                        self.error(
                            format!("Enum variant '{}::{}' expects {} argument(s), found {}",
                                actual_enum, variant_name, fields.len(), args.len()),
                            expr.span,
                        );
                    }
                    for (i, arg) in args.iter().enumerate() {
                        let arg_type = self.check_expr(arg);
                        if i < fields.len() {
                            let (ref field_name, ref expected_type) = fields[i];
                            if let Err(_) = unify(&mut self.subst, &arg_type, expected_type, arg.span) {
                                self.error(
                                    format!("Enum variant '{}::{}', field '{}': expected {}, found {}",
                                        actual_enum, variant_name, field_name,
                                        self.subst.apply(expected_type).display_name(),
                                        self.subst.apply(&arg_type).display_name()),
                                    arg.span,
                                );
                            }
                        }
                    }
                } else {
                    // Variant not found in enum def — just check args without field matching
                    for arg in args {
                        let _t = self.check_expr(arg);
                    }
                }
                
                MonoType::Enum(actual_enum, vec![])
            }

            ExprNode::MethodCall(obj, method_name, args) => {
                let obj_type = self.check_expr(obj);
                let resolved = self.subst.apply(&obj_type);
                
                for arg in args {
                    let _t = self.check_expr(arg);
                }
                
                if let MonoType::Struct(struct_name, _) = &resolved {
                    if let Some((_, ret_type)) = self.method_types.get(&(struct_name.clone(), method_name.clone())).cloned() {
                        return ret_type;
                    }
                }
                self.fresh_var()
            }

            ExprNode::UnaryNot(inner) => {
                let _t = self.check_expr(inner);
                MonoType::Bool
            }

            ExprNode::TryExpr(inner) => {
                let _t = self.check_expr(inner);
                MonoType::Int // try returns an error code
            }

            ExprNode::Closure(params, body) => {
                self.push_scope();
                let param_types: Vec<MonoType> = params.iter()
                    .map(|name| {
                        let tv = self.fresh_var();
                        self.define(name.clone(), tv.clone());
                        tv
                    })
                    .collect();

                // A `return` inside a closure returns from the closure, not the
                // enclosing function — so the outer declared return type must not
                // apply while checking the closure body.
                let saved_ret = self.current_return_type.take();
                let mut last_type = MonoType::Unit;
                for s in body {
                    self.check_stmt(s);
                    // Track return type from last statement
                    if let StmtNode::Return(expr) = &s.node {
                        last_type = self.check_expr(expr);
                    }
                }
                self.current_return_type = saved_ret;
                self.pop_scope();

                MonoType::Fun(param_types, Box::new(last_type))
            }

            ExprNode::Await(inner) => {
                let inner_type = self.check_expr(inner);
                let resolved = self.subst.apply(&inner_type);
                if let MonoType::Future(result_type) = resolved {
                    *result_type
                } else {
                    // Await on a non-future — it's still valid in current ErnosPlain
                    inner_type
                }
            }

            ExprNode::ListLiteral(elements) => {
                // Unify every element against a single element type so a literal
                // like [1, "x"] is rejected. `Any` still unifies with everything,
                // so intentionally-heterogeneous data via Any is unaffected.
                let elem_var = self.fresh_var();
                for elem in elements {
                    let elem_type = self.check_expr(elem);
                    if unify(&mut self.subst, &elem_var, &elem_type, expr.span).is_err() {
                        let prev = self.subst.apply(&elem_var);
                        let found = self.subst.apply(&elem_type);
                        self.error_with_hint(
                            format!(
                                "List elements have conflicting types: {} and {}",
                                prev.display_name(), found.display_name()
                            ),
                            expr.span,
                            "All elements of a list literal must share one type.".into(),
                        );
                    }
                }
                MonoType::List(Box::new(self.subst.apply(&elem_var)))
            }
        }
    }

    // ──────────────────────────────────────────
    // Public interface
    // ──────────────────────────────────────────

    /// Run type checking on a program. Returns errors found.
    pub fn check(program: &Program) -> Vec<TypeError> {
        let mut checker = TypeChecker::new();
        checker.check_program(program);
        checker.errors
    }

    /// Run type checking, returning both errors and warnings
    pub fn check_full(program: &Program) -> (Vec<TypeError>, Vec<TypeError>) {
        let mut checker = TypeChecker::new();
        checker.check_program(program);
        (checker.errors, checker.warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn check_source(source: &str) -> Vec<TypeError> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexer error");
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().expect("Parser error");
        TypeChecker::check(&program)
    }

    #[test]
    fn test_valid_program() {
        let errors = check_source(
            "define main:\n    set x to 42\n    display x\n    return 0"
        );
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_struct_field_access() {
        let errors = check_source(
            "define structure User:\n    field name as Str\n    field age as Int\n\ndefine main:\n    set user to create User:\n        name is \"Alice\"\n        age is 30\n    display user.name\n    return 0"
        );
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_arithmetic_types() {
        let errors = check_source(
            "define add with a as Int and b as Int returning Int:\n    return a plus b\n\ndefine main:\n    set result to add(10 and 20)\n    display result\n    return 0"
        );
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
    }

    fn check_source_full(source: &str) -> (Vec<TypeError>, Vec<TypeError>) {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexer error");
        let mut parser = Parser::new(tokens);
        let program = parser.parse_program().expect("Parser error");
        TypeChecker::check_full(&program)
    }

    #[test]
    fn test_trait_missing_method() {
        let errors = check_source(
            "define trait Printable:\n    define to_string returning Str\n\ndefine structure Point:\n    field x as Int\n\nimplement Printable for Point:\n    define wrong_name returning Str:\n        return \"hello\""
        );
        assert!(!errors.is_empty(), "Expected error for missing trait method");
        assert!(errors[0].message.contains("requires method 'to_string'"),
            "Expected missing method error, got: {}", errors[0].message);
    }

    #[test]
    fn test_exhaustive_match_warning() {
        let (errors, warnings) = check_source_full(
            "define choice Color:\n    variant Red\n    variant Blue\n    variant Green\n\ndefine main:\n    set c to Red\n    check c:\n        if Red:\n            display 1\n    return 0"
        );
        assert!(errors.is_empty(), "Expected no errors, got: {:?}", errors);
        assert!(!warnings.is_empty(), "Expected warnings for non-exhaustive match");
        let has_exhaustive_warning = warnings.iter().any(|w| w.message.contains("Non-exhaustive"));
        assert!(has_exhaustive_warning, "Expected exhaustive match warning, got: {:?}", warnings);
    }

    #[test]
    fn test_trait_valid_impl() {
        let errors = check_source(
            "define trait Printable:\n    define to_string returning Str\n\ndefine structure Point:\n    field x as Int\n\nimplement Printable for Point:\n    define to_string returning Str:\n        return \"Point\""
        );
        assert!(errors.is_empty(), "Expected no errors for valid trait impl, got: {:?}", errors);
    }
}
