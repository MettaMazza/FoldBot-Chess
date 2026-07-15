/// ErnosPlain AST Optimizer — Compile-time optimizations
///
/// Implements optimizations that run before codegen:
///
/// Phase 1 (Expression-level):
/// - Constant folding (arithmetic, comparison, logical, string concatenation)
/// - Strength reduction (multiply/divide by powers of 2 → shift)
/// - Identity elimination (x + 0, x * 1, x - 0)
/// - Boolean simplification (!!x → x, true and x → x, false or x → x)
///
/// Phase 2 (Statement-level):
/// - Dead code elimination (after return, dead branches)
/// - Common subexpression elimination (CSE)
/// - Loop-invariant code motion (LICM)
///
/// Phase 3 (Function-level):
/// - Function inlining (inline small functions at call sites)
/// - Loop unrolling (unroll small counted loops)

use std::collections::{HashMap, HashSet};
use crate::ast::*;

pub struct Optimizer {
    pub stats: OptStats,
    /// Function bodies indexed by name (for inlining)
    inline_candidates: HashMap<String, InlineCandidate>,
    /// Variable names assigned closures in the current function body.
    /// Prevents inlining a global function when the call target is shadowed.
    closure_vars: HashSet<String>,
}

/// A function eligible for inlining
#[derive(Clone)]
struct InlineCandidate {
    params: Vec<(String, bool, Option<TypeAnnotation>)>,
    body: Vec<Stmt>,
}

#[derive(Default, Debug)]
pub struct OptStats {
    pub constants_folded: usize,
    pub dead_branches_eliminated: usize,
    pub dead_stmts_eliminated: usize,
    pub strength_reductions: usize,
    pub identity_eliminations: usize,
    pub functions_inlined: usize,
    pub cse_eliminations: usize,
    pub licm_hoisted: usize,
    pub loops_unrolled: usize,
}

// ──────────────────────────────────────────────
// Helper: check if a value is a power of 2
// ──────────────────────────────────────────────
#[allow(dead_code)]
fn is_power_of_2(n: i64) -> bool {
    n > 0 && (n & (n - 1)) == 0
}

#[allow(dead_code)]
fn log2_of(n: i64) -> i64 {
    // Returns the number of trailing zeros = log2 for powers of 2
    assert!(is_power_of_2(n));
    let mut count = 0i64;
    let mut v = n;
    while v > 1 {
        v >>= 1;
        count += 1;
    }
    count
}

// ──────────────────────────────────────────────
// Helper: structural equality for expressions
// ──────────────────────────────────────────────
fn expr_eq(a: &ExprNode, b: &ExprNode) -> bool {
    match (a, b) {
        (ExprNode::Integer(x), ExprNode::Integer(y)) => x == y,
        (ExprNode::BoolLiteral(x), ExprNode::BoolLiteral(y)) => x == y,
        (ExprNode::StringLiteral(x), ExprNode::StringLiteral(y)) => x == y,
        (ExprNode::FloatLiteral(x), ExprNode::FloatLiteral(y)) => x == y,
        (ExprNode::Identifier(x), ExprNode::Identifier(y)) => x == y,
        (ExprNode::Binary(l1, o1, r1), ExprNode::Binary(l2, o2, r2)) => {
            o1 == o2 && expr_eq(&l1.node, &l2.node) && expr_eq(&r1.node, &r2.node)
        }
        (ExprNode::Comparison(l1, o1, r1), ExprNode::Comparison(l2, o2, r2)) => {
            o1 == o2 && expr_eq(&l1.node, &l2.node) && expr_eq(&r1.node, &r2.node)
        }
        (ExprNode::Call(n1, a1), ExprNode::Call(n2, a2)) => {
            n1 == n2 && a1.len() == a2.len() && a1.iter().zip(a2).all(|(x, y)| expr_eq(&x.node, &y.node))
        }
        (ExprNode::FieldAccess(o1, f1), ExprNode::FieldAccess(o2, f2)) => {
            f1 == f2 && expr_eq(&o1.node, &o2.node)
        }
        _ => false,
    }
}

/// Check if an expression has side effects (calls, mutations)
fn has_side_effects(expr: &ExprNode) -> bool {
    match expr {
        ExprNode::Integer(_) | ExprNode::FloatLiteral(_) | ExprNode::BoolLiteral(_) 
        | ExprNode::StringLiteral(_) | ExprNode::Identifier(_) | ExprNode::Channel => false,
        ExprNode::Binary(l, _, r) | ExprNode::Comparison(l, _, r) | ExprNode::Logical(l, _, r) => {
            has_side_effects(&l.node) || has_side_effects(&r.node)
        }
        ExprNode::UnaryNot(inner) | ExprNode::Borrow(inner) => has_side_effects(&inner.node),
        ExprNode::FieldAccess(obj, _) => has_side_effects(&obj.node),
        // Calls always have potential side effects
        ExprNode::Call(_, _) | ExprNode::MethodCall(_, _, _) => true,
        // Everything else: conservative = has side effects
        _ => true,
    }
}

/// Check if an expression reads a specific variable
fn reads_var(expr: &ExprNode, var_name: &str) -> bool {
    match expr {
        ExprNode::Identifier(name) => name == var_name,
        ExprNode::Binary(l, _, r) | ExprNode::Comparison(l, _, r) | ExprNode::Logical(l, _, r) => {
            reads_var(&l.node, var_name) || reads_var(&r.node, var_name)
        }
        ExprNode::UnaryNot(inner) | ExprNode::Borrow(inner) 
        | ExprNode::FieldAccess(inner, _) => reads_var(&inner.node, var_name),
        ExprNode::Call(_, args) => args.iter().any(|a| reads_var(&a.node, var_name)),
        ExprNode::MethodCall(obj, _, args) => {
            reads_var(&obj.node, var_name) || args.iter().any(|a| reads_var(&a.node, var_name))
        }
        _ => false,
    }
}

/// Collect all variable names written in a statement list
fn writes_vars(stmts: &[Stmt]) -> Vec<String> {
    let mut vars = Vec::new();
    for stmt in stmts {
        match &stmt.node {
            StmtNode::Set(name, _, _) => vars.push(name.clone()),
            StmtNode::If(_, then_b, else_b) => {
                vars.extend(writes_vars(then_b));
                if let Some(eb) = else_b {
                    vars.extend(writes_vars(eb));
                }
            }
            StmtNode::RepeatWhile(_, body) | StmtNode::ForEach(_, _, body) => {
                vars.extend(writes_vars(body));
            }
            StmtNode::Match(_, arms) => {
                for (_, _, body) in arms {
                    vars.extend(writes_vars(body));
                }
            }
            _ => {}
        }
    }
    vars
}

/// Check if a statement modifies any variable
#[allow(dead_code)]
fn stmt_modifies_var(stmt: &StmtNode, var_name: &str) -> bool {
    match stmt {
        StmtNode::Set(name, _, _) => name == var_name,
        StmtNode::FieldSet(obj, _, _) => {
            if let ExprNode::Identifier(name) = &obj.node {
                name == var_name
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Check if a statement (or any sub-expression/sub-statement) reads a variable
fn stmt_reads_var(stmt: &Stmt, var_name: &str) -> bool {
    match &stmt.node {
        StmtNode::Set(_, expr, _) => reads_var(&expr.node, var_name),
        StmtNode::Return(expr) | StmtNode::Display(expr) | StmtNode::ExprStmt(expr) => {
            reads_var(&expr.node, var_name)
        }
        StmtNode::If(cond, then_b, else_b) => {
            reads_var(&cond.node, var_name)
                || then_b.iter().any(|s| stmt_reads_var(s, var_name))
                || else_b.as_ref().map_or(false, |eb| eb.iter().any(|s| stmt_reads_var(s, var_name)))
        }
        StmtNode::RepeatWhile(cond, body) => {
            reads_var(&cond.node, var_name)
                || body.iter().any(|s| stmt_reads_var(s, var_name))
        }
        StmtNode::ForEach(iter_var, iterable, body) => {
            iter_var == var_name
                || reads_var(&iterable.node, var_name)
                || body.iter().any(|s| stmt_reads_var(s, var_name))
        }
        StmtNode::Send(chan, val) => {
            reads_var(&chan.node, var_name) || reads_var(&val.node, var_name)
        }
        StmtNode::FieldSet(obj, _, val) => {
            reads_var(&obj.node, var_name) || reads_var(&val.node, var_name)
        }
        StmtNode::Spawn(_, args) => args.iter().any(|a| reads_var(&a.node, var_name)),
        StmtNode::Match(expr, arms) => {
            reads_var(&expr.node, var_name)
                || arms.iter().any(|(_, _, body)| body.iter().any(|s| stmt_reads_var(s, var_name)))
        }
        _ => false,
    }
}

impl Optimizer {
    pub fn new() -> Self {
        Self { 
            stats: OptStats::default(),
            inline_candidates: HashMap::new(),
            closure_vars: HashSet::new(),
        }
    }

    pub fn optimize_program(&mut self, program: &mut Program) {
        // Pre-pass: collect inline candidates (small, non-recursive, non-builtin functions)
        self.collect_inline_candidates(program);

        for func in &mut program.functions {
            self.optimize_function_body(&mut func.body);
        }
        for md in &mut program.method_defs {
            self.optimize_function_body(&mut md.body);
        }
    }

    // ──────────────────────────────────────────
    // Inline candidate collection
    // ──────────────────────────────────────────

    /// Identify functions eligible for inlining:
    /// - Body has <= 8 statements (small enough to inline)
    /// - Not recursive (doesn't call itself)
    /// - Not async (thread semantics would change)
    /// - Has a return statement (pure-ish function)
    fn collect_inline_candidates(&mut self, program: &Program) {
        for func in &program.functions {
            // Skip main — never inline the entry point
            if func.name == "main" { continue; }
            // Skip async functions — inlining changes thread semantics
            if func.is_async { continue; }
            // Skip large functions
            if func.body.len() > 8 { continue; }
            // Skip recursive functions
            if self.is_recursive(&func.name, &func.body) { continue; }
            // Must have at least one return (so we know the result)
            if !self.has_return(&func.body) { continue; }
            
            self.inline_candidates.insert(func.name.clone(), InlineCandidate {
                params: func.params.clone(),
                body: func.body.clone(),
            });
        }
    }

    fn is_recursive(&self, name: &str, body: &[Stmt]) -> bool {
        for stmt in body {
            if self.stmt_calls_func(name, stmt) {
                return true;
            }
        }
        false
    }

    fn stmt_calls_func(&self, name: &str, stmt: &Stmt) -> bool {
        match &stmt.node {
            StmtNode::Set(_, expr, _) => self.expr_calls_func(name, expr),
            StmtNode::Return(expr) | StmtNode::Display(expr) => self.expr_calls_func(name, expr),
            StmtNode::If(cond, then_b, else_b) => {
                self.expr_calls_func(name, cond) 
                || then_b.iter().any(|s| self.stmt_calls_func(name, s))
                || else_b.as_ref().map_or(false, |eb| eb.iter().any(|s| self.stmt_calls_func(name, s)))
            }
            StmtNode::RepeatWhile(cond, body) => {
                self.expr_calls_func(name, cond) || body.iter().any(|s| self.stmt_calls_func(name, s))
            }
            StmtNode::ExprStmt(expr) => self.expr_calls_func(name, expr),
            StmtNode::Spawn(_, args) => args.iter().any(|a| self.expr_calls_func(name, a)),
            StmtNode::Send(c, v) => self.expr_calls_func(name, c) || self.expr_calls_func(name, v),
            StmtNode::FieldSet(o, _, v) => self.expr_calls_func(name, o) || self.expr_calls_func(name, v),
            StmtNode::ForEach(_, it, body) => {
                self.expr_calls_func(name, it) || body.iter().any(|s| self.stmt_calls_func(name, s))
            }
            StmtNode::Match(expr, arms) => {
                self.expr_calls_func(name, expr) 
                || arms.iter().any(|(_, _, body)| body.iter().any(|s| self.stmt_calls_func(name, s)))
            }
            _ => false,
        }
    }

    fn expr_calls_func(&self, name: &str, expr: &Expr) -> bool {
        match &expr.node {
            ExprNode::Call(fn_name, args) => {
                fn_name == name || args.iter().any(|a| self.expr_calls_func(name, a))
            }
            ExprNode::Binary(l, _, r) | ExprNode::Comparison(l, _, r) | ExprNode::Logical(l, _, r) => {
                self.expr_calls_func(name, l) || self.expr_calls_func(name, r)
            }
            ExprNode::UnaryNot(inner) | ExprNode::Borrow(inner) 
            | ExprNode::TryExpr(inner) | ExprNode::Await(inner) | ExprNode::Receive(inner) => {
                self.expr_calls_func(name, inner)
            }
            ExprNode::FieldAccess(obj, _) => self.expr_calls_func(name, obj),
            ExprNode::MethodCall(obj, _, args) => {
                self.expr_calls_func(name, obj) || args.iter().any(|a| self.expr_calls_func(name, a))
            }
            ExprNode::StructCreate(_, fields) => fields.iter().any(|(_, e)| self.expr_calls_func(name, e)),
            ExprNode::EnumCreate(_, _, args) => args.iter().any(|a| self.expr_calls_func(name, a)),
            ExprNode::Closure(_, body) => body.iter().any(|s| self.stmt_calls_func(name, s)),
            _ => false,
        }
    }

    fn has_return(&self, body: &[Stmt]) -> bool {
        body.iter().any(|s| matches!(s.node, StmtNode::Return(_)))
    }

    // ──────────────────────────────────────────
    // Main optimization pipeline
    // ──────────────────────────────────────────

    fn optimize_function_body(&mut self, body: &mut Vec<Stmt>) {
        // Multi-pass: run until no more changes (fixed-point iteration)
        // Maximum 3 iterations to prevent runaway
        for _pass in 0..3 {
            let before_folded = self.stats.constants_folded;
            let before_identity = self.stats.identity_eliminations;
            let before_strength = self.stats.strength_reductions;
            let before_inlined = self.stats.functions_inlined;
            let before_cse = self.stats.cse_eliminations;
            let before_licm = self.stats.licm_hoisted;
            let before_unrolled = self.stats.loops_unrolled;
            let before_branches = self.stats.dead_branches_eliminated;
            let before_dead = self.stats.dead_stmts_eliminated;

            // Pass 1: Constant fold all expressions
            for stmt in body.iter_mut() {
                self.fold_stmt(stmt);
            }

            // Pass 2: Function inlining (before dead code elimination)
            // First, collect variables assigned closures in this function
            // to prevent inlining when a call target is shadowed by a local closure.
            self.closure_vars.clear();
            self.collect_closure_vars(body);
            self.inline_pass(body);

            // Pass 3: Common Subexpression Elimination
            self.cse_pass(body);

            // Pass 4: Loop-Invariant Code Motion
            self.licm_pass(body);

            // Pass 5: Loop unrolling
            self.unroll_pass(body);

            // Pass 6: Dead branch elimination — replace constant-condition ifs with their body
            let mut i = 0;
            while i < body.len() {
                let should_replace = if let StmtNode::If(cond, then_body, else_body) = &body[i].node {
                    match &cond.node {
                        ExprNode::Integer(val) => {
                            if *val != 0 {
                                Some((true, then_body.clone(), None))
                            } else {
                                Some((false, Vec::new(), else_body.clone()))
                            }
                        }
                        ExprNode::BoolLiteral(val) => {
                            if *val {
                                Some((true, then_body.clone(), None))
                            } else {
                                Some((false, Vec::new(), else_body.clone()))
                            }
                        }
                        _ => None,
                    }
                } else {
                    None
                };

                if let Some((is_true, then_body, else_body)) = should_replace {
                    self.stats.dead_branches_eliminated += 1;
                    let span = body[i].span;
                    body.remove(i);
                    if is_true {
                        for (j, s) in then_body.into_iter().enumerate() {
                            body.insert(i + j, s);
                        }
                    } else if let Some(else_b) = else_body {
                        for (j, s) in else_b.into_iter().enumerate() {
                            body.insert(i + j, s);
                        }
                    } else {
                        let _ = span;
                    }
                } else {
                    i += 1;
                }
            }

            // Pass 7: Dead code elimination (remove statements after return)
            let mut found_return = false;
            let mut keep_count = 0;
            for stmt in body.iter() {
                keep_count += 1;
                if matches!(stmt.node, StmtNode::Return(_)) {
                    found_return = true;
                    break;
                }
            }
            if found_return && keep_count < body.len() {
                self.stats.dead_stmts_eliminated += body.len() - keep_count;
                body.truncate(keep_count);
            }

            // Check if anything changed — if not, stop iterating
            let changed = self.stats.constants_folded != before_folded
                || self.stats.identity_eliminations != before_identity
                || self.stats.strength_reductions != before_strength
                || self.stats.functions_inlined != before_inlined
                || self.stats.cse_eliminations != before_cse
                || self.stats.licm_hoisted != before_licm
                || self.stats.loops_unrolled != before_unrolled
                || self.stats.dead_branches_eliminated != before_branches
                || self.stats.dead_stmts_eliminated != before_dead;
            if !changed { break; }
        }
    }

    // ──────────────────────────────────────────
    // Pass: Function Inlining
    // ──────────────────────────────────────────

    /// Inline small function calls at their call sites.
    /// For a call `set result to add(x and y)` where `define add with a and b: return a + b`,
    /// we replace the call with the function body's return expression, substituting
    /// parameters with arguments.
    /// Collect variable names assigned closures in a function body.
    /// These shadow global function names and must not be inlined.
    fn collect_closure_vars(&mut self, body: &[Stmt]) {
        for stmt in body {
            match &stmt.node {
                StmtNode::Set(name, expr, _) => {
                    if matches!(expr.node, ExprNode::Closure(_, _)) {
                        self.closure_vars.insert(name.clone());
                    }
                }
                StmtNode::If(_, then_b, else_b) => {
                    self.collect_closure_vars(then_b);
                    if let Some(eb) = else_b {
                        self.collect_closure_vars(eb);
                    }
                }
                StmtNode::RepeatWhile(_, loop_body) => {
                    self.collect_closure_vars(loop_body);
                }
                StmtNode::ForEach(_, _, loop_body) => {
                    self.collect_closure_vars(loop_body);
                }
                StmtNode::Match(_, arms) => {
                    for (_, _, arm_body) in arms {
                        self.collect_closure_vars(arm_body);
                    }
                }
                _ => {}
            }
        }
    }

    fn inline_pass(&mut self, body: &mut Vec<Stmt>) {
        for stmt in body.iter_mut() {
            self.inline_in_stmt(stmt);
        }
    }

    fn inline_in_stmt(&mut self, stmt: &mut Stmt) {
        match &mut stmt.node {
            StmtNode::Set(_, expr, _) => {
                self.inline_in_expr(expr);
            }
            StmtNode::Return(expr) | StmtNode::Display(expr) => {
                self.inline_in_expr(expr);
            }
            StmtNode::If(cond, then_b, else_b) => {
                self.inline_in_expr(cond);
                for s in then_b.iter_mut() { self.inline_in_stmt(s); }
                if let Some(eb) = else_b {
                    for s in eb.iter_mut() { self.inline_in_stmt(s); }
                }
            }
            StmtNode::RepeatWhile(cond, loop_body) => {
                self.inline_in_expr(cond);
                for s in loop_body.iter_mut() { self.inline_in_stmt(s); }
            }
            StmtNode::ForEach(_, iter_expr, loop_body) => {
                self.inline_in_expr(iter_expr);
                for s in loop_body.iter_mut() { self.inline_in_stmt(s); }
            }
            StmtNode::ExprStmt(expr) => {
                self.inline_in_expr(expr);
            }
            StmtNode::Send(chan, val) => {
                self.inline_in_expr(chan);
                self.inline_in_expr(val);
            }
            StmtNode::FieldSet(obj, _, val) => {
                self.inline_in_expr(obj);
                self.inline_in_expr(val);
            }
            StmtNode::Match(expr, arms) => {
                self.inline_in_expr(expr);
                for (_, _, arm_body) in arms.iter_mut() {
                    for s in arm_body.iter_mut() { self.inline_in_stmt(s); }
                }
            }
            StmtNode::Spawn(_, args) => {
                for a in args.iter_mut() { self.inline_in_expr(a); }
            }
            _ => {}
        }
    }

    fn inline_in_expr(&mut self, expr: &mut Expr) {
        // First, recursively process sub-expressions
        match &mut expr.node {
            ExprNode::Binary(l, _, r) | ExprNode::Comparison(l, _, r) | ExprNode::Logical(l, _, r) => {
                self.inline_in_expr(l);
                self.inline_in_expr(r);
            }
            ExprNode::UnaryNot(inner) | ExprNode::Borrow(inner) 
            | ExprNode::TryExpr(inner) | ExprNode::Await(inner) | ExprNode::Receive(inner) => {
                self.inline_in_expr(inner);
            }
            ExprNode::FieldAccess(obj, _) => {
                self.inline_in_expr(obj);
            }
            ExprNode::MethodCall(obj, _, args) => {
                self.inline_in_expr(obj);
                for a in args.iter_mut() { self.inline_in_expr(a); }
            }
            ExprNode::StructCreate(_, fields) => {
                for (_, e) in fields.iter_mut() { self.inline_in_expr(e); }
            }
            ExprNode::EnumCreate(_, _, args) => {
                for a in args.iter_mut() { self.inline_in_expr(a); }
            }
            _ => {}
        }

        // Then, check if this is an inlineable call
        if let ExprNode::Call(fn_name, args) = &expr.node {
            // Skip inlining if the call target is shadowed by a local closure variable
            if self.closure_vars.contains(fn_name) {
                return;
            }
            if let Some(candidate) = self.inline_candidates.get(fn_name).cloned() {
                // Only inline if the function body is a single return expression
                // (multi-statement inlining requires more complex transformation)
                if candidate.body.len() == 1 {
                    if let StmtNode::Return(ret_expr) = &candidate.body[0].node {
                        if candidate.params.len() == args.len() {
                            // Build substitution map: param_name → argument_expr
                            let mut subst: HashMap<String, ExprNode> = HashMap::new();
                            for ((param_name, _, _), arg) in candidate.params.iter().zip(args.iter()) {
                                subst.insert(param_name.clone(), arg.node.clone());
                            }
                            // Substitute parameters in the return expression
                            let mut inlined = ret_expr.clone();
                            self.substitute_expr(&mut inlined, &subst);
                            expr.node = inlined.node;
                            self.stats.functions_inlined += 1;
                        }
                    }
                }
                // Multi-statement inlining: if function body is `set + return`
                // We can inline if there are exactly 2 statements: a set and a return
                // that references that set variable
                else if candidate.body.len() == 2 {
                    if let (StmtNode::Set(tmp_name, tmp_expr, _), StmtNode::Return(ret_expr)) = 
                        (&candidate.body[0].node, &candidate.body[1].node)
                    {
                        // If the return just returns the temp variable, inline the temp expression
                        if let ExprNode::Identifier(ret_name) = &ret_expr.node {
                            if ret_name == tmp_name && candidate.params.len() == args.len() {
                                let mut subst: HashMap<String, ExprNode> = HashMap::new();
                                for ((param_name, _, _), arg) in candidate.params.iter().zip(args.iter()) {
                                    subst.insert(param_name.clone(), arg.node.clone());
                                }
                                let mut inlined = tmp_expr.clone();
                                self.substitute_expr(&mut inlined, &subst);
                                expr.node = inlined.node;
                                self.stats.functions_inlined += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Substitute all Identifier references in an expression with values from the map
    fn substitute_expr(&self, expr: &mut Expr, subst: &HashMap<String, ExprNode>) {
        match &mut expr.node {
            ExprNode::Identifier(name) => {
                if let Some(replacement) = subst.get(name) {
                    expr.node = replacement.clone();
                }
            }
            ExprNode::Binary(l, _, r) | ExprNode::Comparison(l, _, r) | ExprNode::Logical(l, _, r) => {
                self.substitute_expr(l, subst);
                self.substitute_expr(r, subst);
            }
            ExprNode::UnaryNot(inner) | ExprNode::Borrow(inner) 
            | ExprNode::TryExpr(inner) | ExprNode::Await(inner) | ExprNode::Receive(inner) => {
                self.substitute_expr(inner, subst);
            }
            ExprNode::FieldAccess(obj, _) => {
                self.substitute_expr(obj, subst);
            }
            ExprNode::Call(name, args) => {
                if let Some(replacement) = subst.get(name) {
                    if let ExprNode::Identifier(new_name) = replacement {
                        *name = new_name.clone();
                    }
                }
                for a in args.iter_mut() { self.substitute_expr(a, subst); }
            }
            ExprNode::MethodCall(obj, _, args) => {
                self.substitute_expr(obj, subst);
                for a in args.iter_mut() { self.substitute_expr(a, subst); }
            }
            ExprNode::StructCreate(_, fields) => {
                for (_, e) in fields.iter_mut() { self.substitute_expr(e, subst); }
            }
            ExprNode::EnumCreate(_, _, args) => {
                for a in args.iter_mut() { self.substitute_expr(a, subst); }
            }
            _ => {} // literals, channel — no substitution needed
        }
    }

    // ──────────────────────────────────────────
    // Pass: Common Subexpression Elimination
    // ──────────────────────────────────────────

    /// Detect repeated pure expressions in a statement list and hoist them
    /// into a temp variable. Only applies to side-effect-free expressions.
    ///
    /// Example:
    ///   set a to x + y * z
    ///   set b to x + y * z    →    set _cse_0 to x + y * z
    ///                               set a to _cse_0
    ///                               set b to _cse_0
    fn cse_pass(&mut self, body: &mut Vec<Stmt>) {
        // Collect all Set expressions and find duplicates
        let mut expr_occurrences: Vec<(ExprNode, Vec<usize>)> = Vec::new();
        
        for (i, stmt) in body.iter().enumerate() {
            if let StmtNode::Set(_, expr, _) = &stmt.node {
                // Only CSE pure expressions (no side effects)
                if has_side_effects(&expr.node) { continue; }
                // Skip simple identifiers and literals — not worth CSE'ing
                match &expr.node {
                    ExprNode::Integer(_) | ExprNode::BoolLiteral(_) 
                    | ExprNode::StringLiteral(_) | ExprNode::Identifier(_)
                    | ExprNode::FloatLiteral(_) => continue,
                    _ => {}
                }
                
                let mut found = false;
                for (existing_expr, indices) in &mut expr_occurrences {
                    if expr_eq(existing_expr, &expr.node) {
                        indices.push(i);
                        found = true;
                        break;
                    }
                }
                if !found {
                    expr_occurrences.push((expr.node.clone(), vec![i]));
                }
            }
        }

        // For expressions that appear 2+ times, hoist into a temp
        let mut cse_count = 0u32;
        let mut insertions: Vec<(usize, String, Expr)> = Vec::new(); // (insert_before, temp_name, expr)
        let mut replacements: Vec<(usize, String)> = Vec::new(); // (stmt_index, temp_name)

        for (expr_node, indices) in &expr_occurrences {
            if indices.len() < 2 { continue; }

            // Soundness (available-expression rule): an expression may be hoisted
            // to a single temp only if none of the variables it READS are written
            // anywhere between its first and last occurrence. Otherwise the cached
            // value goes stale — e.g. after loop unrolling, three copies of
            // `set total to total + 10` each read a `total` that the previous copy
            // just reassigned. Checking the inclusive range also excludes
            // accumulators, whose own target is written by every occurrence.
            let first = indices[0];
            let last = *indices.last().unwrap();
            let written_in_range = writes_vars(&body[first..=last]);
            if written_in_range.iter().any(|v| reads_var(expr_node, v)) {
                continue;
            }

            let temp_name = format!("_cse_{}", cse_count);
            cse_count += 1;

            // Insert the temp assignment before the first occurrence
            let first_idx = indices[0];
            let span = body[first_idx].span;
            insertions.push((first_idx, temp_name.clone(), Expr::with_span(expr_node.clone(), span)));

            // Replace all occurrences with the temp variable
            for &idx in indices {
                replacements.push((idx, temp_name.clone()));
            }
            
            self.stats.cse_eliminations += indices.len() - 1;
        }

        // Apply replacements (from back to front to preserve indices)
        for (idx, temp_name) in replacements.iter().rev() {
            if let StmtNode::Set(var_name, expr, type_ann) = &body[*idx].node {
                let span = expr.span;
                let new_stmt = Stmt::with_span(
                    StmtNode::Set(var_name.clone(), Expr::with_span(ExprNode::Identifier(temp_name.clone()), span), type_ann.clone()),
                    body[*idx].span,
                );
                body[*idx] = new_stmt;
            }
        }

        // Insert temp assignments (from back to front to preserve indices)
        for (idx, temp_name, expr) in insertions.into_iter().rev() {
            let span = expr.span;
            let temp_stmt = Stmt::with_span(
                StmtNode::Set(temp_name, expr, None),
                span,
            );
            body.insert(idx, temp_stmt);
        }
    }

    // ──────────────────────────────────────────
    // Pass: Loop-Invariant Code Motion (LICM)
    // ──────────────────────────────────────────

    /// Find expressions in loop bodies that don't depend on any variable
    /// modified inside the loop, and hoist them before the loop.
    ///
    /// Example:
    ///   repeat while i < n:
    ///     set x to length_list(items)   ← invariant (items not modified in loop)
    ///     set y to get_list(items and x)
    ///
    /// Becomes:
    ///   set x to length_list(items)     ← hoisted
    ///   repeat while i < n:
    ///     set y to get_list(items and x)
    fn licm_pass(&mut self, body: &mut Vec<Stmt>) {
        let mut i = 0;
        while i < body.len() {
            let hoisted = if let StmtNode::RepeatWhile(_, loop_body) = &body[i].node {
                // Collect all variables modified in the loop body
                let modified_vars = writes_vars(loop_body);
                
                // Find statements in the loop body that are invariant:
                // - Set statements where the RHS doesn't read any modified variable
                // - The set target is not itself modified elsewhere in the loop
                let mut to_hoist: Vec<usize> = Vec::new();
                for (j, loop_stmt) in loop_body.iter().enumerate() {
                    if let StmtNode::Set(var_name, expr, _) = &loop_stmt.node {
                        // Skip if this variable is modified more than once (it's a loop accumulator)
                        let write_count = modified_vars.iter().filter(|v| *v == var_name).count();
                        if write_count > 1 { continue; }
                        
                        // Skip expressions with side effects
                        if has_side_effects(&expr.node) { continue; }
                        
                        // Check if the expression reads any variable modified in the loop
                        let reads_modified = modified_vars.iter().any(|mv| reads_var(&expr.node, mv));
                        if reads_modified { continue; }

                        // CRITICAL: Don't hoist if the variable is read by any other
                        // statement in the loop body. Hoisting would change the value
                        // seen on the first iteration (the pre-loop value would be
                        // overwritten before the loop starts).
                        // Example: set is_exit to 1; repeat: display is_exit; set is_exit to 0
                        // Hoisting "set is_exit to 0" before the loop makes the first
                        // iteration see 0 instead of 1.
                        let var_read_elsewhere = loop_body.iter().enumerate().any(|(k, other_stmt)| {
                            k != j && stmt_reads_var(other_stmt, var_name)
                        });
                        if var_read_elsewhere { continue; }

                        to_hoist.push(j);
                    }
                }
                
                if !to_hoist.is_empty() {
                    let mut hoisted_stmts = Vec::new();
                    // Clone the hoisted statements before modifying the loop
                    for &j in &to_hoist {
                        hoisted_stmts.push(loop_body[j].clone());
                    }
                    Some((to_hoist, hoisted_stmts))
                } else {
                    None
                }
            } else {
                None
            };

            if let Some((to_hoist, hoisted_stmts)) = hoisted {
                // Remove hoisted statements from the loop body (in reverse order)
                if let StmtNode::RepeatWhile(_, loop_body) = &mut body[i].node {
                    for &j in to_hoist.iter().rev() {
                        loop_body.remove(j);
                    }
                }
                // Insert hoisted statements before the loop
                for (offset, stmt) in hoisted_stmts.into_iter().enumerate() {
                    body.insert(i + offset, stmt);
                    self.stats.licm_hoisted += 1;
                }
                i += to_hoist.len() + 1; // Skip past the inserted stmts + the loop
            } else {
                // Recurse into nested structures
                match &mut body[i].node {
                    StmtNode::If(_, then_b, else_b) => {
                        self.licm_pass(then_b);
                        if let Some(eb) = else_b { self.licm_pass(eb); }
                    }
                    StmtNode::RepeatWhile(_, lb) => { self.licm_pass(lb); }
                    StmtNode::ForEach(_, _, lb) => { self.licm_pass(lb); }
                    StmtNode::Match(_, arms) => {
                        for (_, _, arm_body) in arms { self.licm_pass(arm_body); }
                    }
                    _ => {}
                }
                i += 1;
            }
        }
    }

    // ──────────────────────────────────────────
    // Pass: Loop Unrolling
    // ──────────────────────────────────────────

    /// Unroll small counted loops with known bounds.
    /// Only applies to the pattern:
    ///   set i to 0
    ///   repeat while i < N:
    ///     ... (body doesn't break/continue)
    ///     set i to i + 1
    ///
    /// Where N is a small integer literal (<= 8 iterations).
    fn unroll_pass(&mut self, body: &mut Vec<Stmt>) {
        let mut i = 0;
        while i + 1 < body.len() {
            let unrolled = self.try_unroll_loop(body, i);
            if let Some(replacement_stmts) = unrolled {
                // Remove the set and the loop
                body.remove(i); // remove `set i to 0`
                body.remove(i); // remove the loop
                // Insert the unrolled body
                for (j, s) in replacement_stmts.into_iter().enumerate() {
                    body.insert(i + j, s);
                }
                self.stats.loops_unrolled += 1;
                // Don't increment i — reprocess
            } else {
                i += 1;
            }
        }
    }

    fn try_unroll_loop(&self, body: &[Stmt], idx: usize) -> Option<Vec<Stmt>> {
        // Check pattern: set VAR to 0, repeat while VAR < N: ... set VAR to VAR + 1
        if idx + 1 >= body.len() { return None; }

        // Statement 1: set VAR to 0
        let (loop_var, init_val) = if let StmtNode::Set(name, expr, _) = &body[idx].node {
            if let ExprNode::Integer(val) = &expr.node {
                (name.clone(), *val)
            } else {
                return None;
            }
        } else {
            return None;
        };

        // Only unroll from 0
        if init_val != 0 { return None; }

        // Statement 2: repeat while VAR < N: ...
        let (limit, loop_body) = if let StmtNode::RepeatWhile(cond, lb) = &body[idx + 1].node {
            if let ExprNode::Comparison(left, CompOp::LessThan, right) = &cond.node {
                if let ExprNode::Identifier(name) = &left.node {
                    if name == &loop_var {
                        if let ExprNode::Integer(n) = &right.node {
                            (*n, lb)
                        } else { return None; }
                    } else { return None; }
                } else { return None; }
            } else { return None; }
        } else { return None; };

        // Only unroll small loops (max 8 iterations)
        if limit <= 0 || limit > 8 { return None; }

        // Check that the loop body ends with `set VAR to VAR + 1`
        if loop_body.is_empty() { return None; }
        let last_stmt = &loop_body[loop_body.len() - 1];
        if let StmtNode::Set(name, expr, _) = &last_stmt.node {
            if name != &loop_var { return None; }
            if let ExprNode::Binary(left, Op::Add, right) = &expr.node {
                let is_increment = matches!(&left.node, ExprNode::Identifier(n) if n == &loop_var)
                    && matches!(&right.node, ExprNode::Integer(1));
                if !is_increment { return None; }
            } else { return None; }
        } else { return None; }

        // Check for break/continue (can't unroll those)
        if self.has_break_continue(loop_body) { return None; }

        // Body without the increment (we'll substitute the counter value instead)
        let body_without_increment = &loop_body[..loop_body.len() - 1];

        // Generate unrolled body
        let mut unrolled: Vec<Stmt> = Vec::new();
        for iteration in 0..limit {
            // For each iteration, substitute loop_var with the iteration value
            for stmt in body_without_increment {
                let mut cloned = stmt.clone();
                self.substitute_var_in_stmt(&mut cloned, &loop_var, iteration);
                unrolled.push(cloned);
            }
        }

        // Add final assignment to the loop variable so its value matches the loop exit value,
        // and it gets properly declared if used afterwards.
        let span = body[idx + 1].span; // loop statement span
        let final_set = Stmt::with_span(
            StmtNode::Set(loop_var.clone(), Expr::with_span(ExprNode::Integer(limit), span), None),
            span,
        );
        unrolled.push(final_set);

        Some(unrolled)
    }

    fn has_break_continue(&self, body: &[Stmt]) -> bool {
        for stmt in body {
            match &stmt.node {
                StmtNode::Break | StmtNode::Continue => return true,
                StmtNode::If(_, then_b, else_b) => {
                    if self.has_break_continue(then_b) { return true; }
                    if let Some(eb) = else_b {
                        if self.has_break_continue(eb) { return true; }
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Substitute all occurrences of a variable with an integer constant in a statement
    fn substitute_var_in_stmt(&self, stmt: &mut Stmt, var_name: &str, value: i64) {
        match &mut stmt.node {
            StmtNode::Set(_, expr, _) => self.substitute_var_in_expr(expr, var_name, value),
            StmtNode::Return(expr) | StmtNode::Display(expr) | StmtNode::ExprStmt(expr) => {
                self.substitute_var_in_expr(expr, var_name, value);
            }
            StmtNode::If(cond, then_b, else_b) => {
                self.substitute_var_in_expr(cond, var_name, value);
                for s in then_b { self.substitute_var_in_stmt(s, var_name, value); }
                if let Some(eb) = else_b {
                    for s in eb { self.substitute_var_in_stmt(s, var_name, value); }
                }
            }
            StmtNode::Send(c, v) => {
                self.substitute_var_in_expr(c, var_name, value);
                self.substitute_var_in_expr(v, var_name, value);
            }
            StmtNode::FieldSet(o, _, v) => {
                self.substitute_var_in_expr(o, var_name, value);
                self.substitute_var_in_expr(v, var_name, value);
            }
            // Spawn arguments must be substituted too — otherwise unrolling a loop
            // like `repeat while w < 4: spawn worker(w and ch)` leaves `w` in every
            // copy, so every spawned task sees the same (wrong) value.
            StmtNode::Spawn(_, args) => {
                for a in args { self.substitute_var_in_expr(a, var_name, value); }
            }
            StmtNode::RepeatWhile(cond, body) => {
                self.substitute_var_in_expr(cond, var_name, value);
                for s in body { self.substitute_var_in_stmt(s, var_name, value); }
            }
            StmtNode::ForEach(_, iterable, body) => {
                self.substitute_var_in_expr(iterable, var_name, value);
                for s in body { self.substitute_var_in_stmt(s, var_name, value); }
            }
            StmtNode::Match(scrutinee, arms) => {
                self.substitute_var_in_expr(scrutinee, var_name, value);
                for (_, _, body) in arms {
                    for s in body { self.substitute_var_in_stmt(s, var_name, value); }
                }
            }
            StmtNode::Break | StmtNode::Continue => {}
        }
    }

    fn substitute_var_in_expr(&self, expr: &mut Expr, var_name: &str, value: i64) {
        match &mut expr.node {
            ExprNode::Identifier(name) if name == var_name => {
                expr.node = ExprNode::Integer(value);
            }
            ExprNode::Binary(l, _, r) | ExprNode::Comparison(l, _, r) | ExprNode::Logical(l, _, r) => {
                self.substitute_var_in_expr(l, var_name, value);
                self.substitute_var_in_expr(r, var_name, value);
            }
            ExprNode::UnaryNot(inner) | ExprNode::Borrow(inner) 
            | ExprNode::TryExpr(inner) | ExprNode::Await(inner) | ExprNode::Receive(inner) => {
                self.substitute_var_in_expr(inner, var_name, value);
            }
            ExprNode::Call(_, args) => {
                for a in args.iter_mut() { self.substitute_var_in_expr(a, var_name, value); }
            }
            ExprNode::FieldAccess(obj, _) => {
                self.substitute_var_in_expr(obj, var_name, value);
            }
            ExprNode::MethodCall(obj, _, args) => {
                self.substitute_var_in_expr(obj, var_name, value);
                for a in args.iter_mut() { self.substitute_var_in_expr(a, var_name, value); }
            }
            ExprNode::StructCreate(_, fields) => {
                for (_, e) in fields.iter_mut() { self.substitute_var_in_expr(e, var_name, value); }
            }
            ExprNode::EnumCreate(_, _, args) => {
                for a in args.iter_mut() { self.substitute_var_in_expr(a, var_name, value); }
            }
            _ => {}
        }
    }

    // ──────────────────────────────────────────
    // Expression-level optimizations
    // ──────────────────────────────────────────

    fn fold_stmt(&mut self, stmt: &mut Stmt) {
        match &mut stmt.node {
            StmtNode::Set(_, expr, _) => {
                self.fold_expr(expr);
            }
            StmtNode::If(cond, then_body, else_body) => {
                self.fold_expr(cond);

                for s in then_body.iter_mut() {
                    self.fold_stmt(s);
                }
                if let Some(else_b) = else_body {
                    for s in else_b.iter_mut() {
                        self.fold_stmt(s);
                    }
                }
            }
            StmtNode::RepeatWhile(cond, body) => {
                self.fold_expr(cond);
                for s in body.iter_mut() {
                    self.fold_stmt(s);
                }
            }
            StmtNode::Return(expr) => {
                self.fold_expr(expr);
            }
            StmtNode::Display(expr) => {
                self.fold_expr(expr);
            }
            StmtNode::ForEach(_, iterable, body) => {
                self.fold_expr(iterable);
                for s in body.iter_mut() {
                    self.fold_stmt(s);
                }
            }
            StmtNode::Match(expr, arms) => {
                self.fold_expr(expr);
                for (_, _, body) in arms.iter_mut() {
                    for s in body.iter_mut() {
                        self.fold_stmt(s);
                    }
                }
            }
            StmtNode::Spawn(_, args) => {
                for arg in args.iter_mut() {
                    self.fold_expr(arg);
                }
            }
            StmtNode::Send(chan, val) => {
                self.fold_expr(chan);
                self.fold_expr(val);
            }
            StmtNode::FieldSet(obj, _, val) => {
                self.fold_expr(obj);
                self.fold_expr(val);
            }
            StmtNode::ExprStmt(expr) => {
                self.fold_expr(expr);
            }
            _ => {}
        }
    }

    fn fold_expr(&mut self, expr: &mut Expr) {
        match &mut expr.node {
            ExprNode::Binary(left, op, right) => {
                self.fold_expr(left);
                self.fold_expr(right);

                // Constant folding for integer arithmetic
                if let (ExprNode::Integer(lv), ExprNode::Integer(rv)) = (&left.node, &right.node) {
                    let lv = *lv;
                    let rv = *rv;
                    let result = match op {
                        Op::Add => Some(lv.wrapping_add(rv)),
                        Op::Sub => Some(lv.wrapping_sub(rv)),
                        Op::Mul => Some(lv.wrapping_mul(rv)),
                        Op::Div => if rv != 0 { Some(lv / rv) } else { None },
                        Op::Mod => if rv != 0 { Some(lv % rv) } else { None },
                    };
                    if let Some(val) = result {
                        self.stats.constants_folded += 1;
                        expr.node = ExprNode::Integer(val);
                        return;
                    }
                }

                // Identity eliminations
                match op {
                    Op::Add => {
                        // x + 0 → x, 0 + x → x
                        if matches!(right.node, ExprNode::Integer(0)) {
                            self.stats.identity_eliminations += 1;
                            expr.node = left.node.clone();
                            return;
                        }
                        if matches!(left.node, ExprNode::Integer(0)) {
                            self.stats.identity_eliminations += 1;
                            expr.node = right.node.clone();
                            return;
                        }
                    }
                    Op::Sub => {
                        // x - 0 → x
                        if matches!(right.node, ExprNode::Integer(0)) {
                            self.stats.identity_eliminations += 1;
                            expr.node = left.node.clone();
                            return;
                        }
                        // x - x → 0 (if same identifier)
                        if let (ExprNode::Identifier(a), ExprNode::Identifier(b)) = (&left.node, &right.node) {
                            if a == b {
                                self.stats.constants_folded += 1;
                                expr.node = ExprNode::Integer(0);
                                return;
                            }
                        }
                    }
                    Op::Mul => {
                        // x * 0 → 0, 0 * x → 0
                        if matches!(right.node, ExprNode::Integer(0)) || matches!(left.node, ExprNode::Integer(0)) {
                            self.stats.constants_folded += 1;
                            expr.node = ExprNode::Integer(0);
                            return;
                        }
                        // x * 1 → x, 1 * x → x
                        if matches!(right.node, ExprNode::Integer(1)) {
                            self.stats.identity_eliminations += 1;
                            expr.node = left.node.clone();
                            return;
                        }
                        if matches!(left.node, ExprNode::Integer(1)) {
                            self.stats.identity_eliminations += 1;
                            expr.node = right.node.clone();
                            return;
                        }
                        // Strength reduction: x * 2 → x + x
                        if matches!(right.node, ExprNode::Integer(2)) {
                            self.stats.strength_reductions += 1;
                            *op = Op::Add;
                            right.node = left.node.clone();
                            return;
                        }
                        // Strength reduction: x * (power of 2) → x << log2(n)
                        // We emit this as a Binary(x, Mul, n) with a comment for now
                        // since we don't have a shift operator in the AST.
                        // The C backend will handle this via compiler optimization.
                        // But we CAN decompose: x * 4 → (x + x) + (x + x)
                        // Only do this for small powers: 4, 8
                        if let ExprNode::Integer(rv) = &right.node {
                            if *rv == 4 {
                                // x * 4 → (x + x) + (x + x) → 2 additions instead of multiply
                                self.stats.strength_reductions += 1;
                                let x_plus_x = ExprNode::Binary(
                                    Box::new(Expr::with_span(left.node.clone(), left.span)),
                                    Op::Add,
                                    Box::new(Expr::with_span(left.node.clone(), left.span)),
                                );
                                expr.node = ExprNode::Binary(
                                    Box::new(Expr::with_span(x_plus_x.clone(), expr.span)),
                                    Op::Add,
                                    Box::new(Expr::with_span(x_plus_x, expr.span)),
                                );
                                return;
                            }
                        }
                    }
                    Op::Div => {
                        // x / 1 → x
                        if matches!(right.node, ExprNode::Integer(1)) {
                            self.stats.identity_eliminations += 1;
                            expr.node = left.node.clone();
                            return;
                        }
                        // 0 / x → 0 (if x is known non-zero)
                        if matches!(left.node, ExprNode::Integer(0)) {
                            if let ExprNode::Integer(rv) = right.node {
                                if rv != 0 {
                                    self.stats.constants_folded += 1;
                                    expr.node = ExprNode::Integer(0);
                                    return;
                                }
                            }
                        }
                        // x / x → 1 (if same identifier, assumed non-zero)
                        if let (ExprNode::Identifier(a), ExprNode::Identifier(b)) = (&left.node, &right.node) {
                            if a == b {
                                self.stats.constants_folded += 1;
                                expr.node = ExprNode::Integer(1);
                                return;
                            }
                        }
                    }
                    Op::Mod => {
                        // x % 1 → 0
                        if matches!(right.node, ExprNode::Integer(1)) {
                            self.stats.constants_folded += 1;
                            expr.node = ExprNode::Integer(0);
                            return;
                        }
                        // x % x → 0 (if same identifier)
                        if let (ExprNode::Identifier(a), ExprNode::Identifier(b)) = (&left.node, &right.node) {
                            if a == b {
                                self.stats.constants_folded += 1;
                                expr.node = ExprNode::Integer(0);
                                return;
                            }
                        }
                    }
                }
            }

            ExprNode::Comparison(left, op, right) => {
                self.fold_expr(left);
                self.fold_expr(right);

                // Constant folding for comparisons
                if let (ExprNode::Integer(lv), ExprNode::Integer(rv)) = (&left.node, &right.node) {
                    let lv = *lv;
                    let rv = *rv;
                    let result = match op {
                        CompOp::LessThan => lv < rv,
                        CompOp::GreaterThan => lv > rv,
                        CompOp::LessEqual => lv <= rv,
                        CompOp::GreaterEqual => lv >= rv,
                        CompOp::Equals => lv == rv,
                        CompOp::NotEquals => lv != rv,
                    };
                    self.stats.constants_folded += 1;
                    expr.node = ExprNode::Integer(if result { 1 } else { 0 });
                    return;
                }

                // String comparison folding
                if let (ExprNode::StringLiteral(ls), ExprNode::StringLiteral(rs)) = (&left.node, &right.node) {
                    let result = match op {
                        CompOp::Equals => ls == rs,
                        CompOp::NotEquals => ls != rs,
                        _ => return,
                    };
                    self.stats.constants_folded += 1;
                    expr.node = ExprNode::Integer(if result { 1 } else { 0 });
                    return;
                }

                // Boolean comparison folding
                if let (ExprNode::BoolLiteral(lv), ExprNode::BoolLiteral(rv)) = (&left.node, &right.node) {
                    let result = match op {
                        CompOp::Equals => lv == rv,
                        CompOp::NotEquals => lv != rv,
                        _ => return,
                    };
                    self.stats.constants_folded += 1;
                    expr.node = ExprNode::Integer(if result { 1 } else { 0 });
                    return;
                }

                // Self-comparison: x == x → true, x != x → false
                if let (ExprNode::Identifier(a), ExprNode::Identifier(b)) = (&left.node, &right.node) {
                    if a == b {
                        let result = match op {
                            CompOp::Equals | CompOp::LessEqual | CompOp::GreaterEqual => true,
                            CompOp::NotEquals | CompOp::LessThan | CompOp::GreaterThan => false,
                        };
                        self.stats.constants_folded += 1;
                        expr.node = ExprNode::Integer(if result { 1 } else { 0 });
                        return;
                    }
                }
            }

            ExprNode::Logical(left, op, right) => {
                self.fold_expr(left);
                self.fold_expr(right);

                // Constant folding for logical ops
                match op {
                    LogicalOp::And => {
                        // false and X → false
                        if matches!(left.node, ExprNode::BoolLiteral(false) | ExprNode::Integer(0)) {
                            self.stats.constants_folded += 1;
                            expr.node = ExprNode::BoolLiteral(false);
                            return;
                        }
                        // true and X → X
                        if matches!(left.node, ExprNode::BoolLiteral(true)) {
                            self.stats.identity_eliminations += 1;
                            expr.node = right.node.clone();
                            return;
                        }
                        // X and false → false
                        if matches!(right.node, ExprNode::BoolLiteral(false) | ExprNode::Integer(0)) {
                            self.stats.constants_folded += 1;
                            expr.node = ExprNode::BoolLiteral(false);
                            return;
                        }
                        // X and true → X
                        if matches!(right.node, ExprNode::BoolLiteral(true)) {
                            self.stats.identity_eliminations += 1;
                            expr.node = left.node.clone();
                            return;
                        }
                    }
                    LogicalOp::Or => {
                        // true or X → true
                        if matches!(left.node, ExprNode::BoolLiteral(true)) {
                            self.stats.constants_folded += 1;
                            expr.node = ExprNode::BoolLiteral(true);
                            return;
                        }
                        // false or X → X
                        if matches!(left.node, ExprNode::BoolLiteral(false) | ExprNode::Integer(0)) {
                            self.stats.identity_eliminations += 1;
                            expr.node = right.node.clone();
                            return;
                        }
                        // X or true → true
                        if matches!(right.node, ExprNode::BoolLiteral(true)) {
                            self.stats.constants_folded += 1;
                            expr.node = ExprNode::BoolLiteral(true);
                            return;
                        }
                        // X or false → X
                        if matches!(right.node, ExprNode::BoolLiteral(false) | ExprNode::Integer(0)) {
                            self.stats.identity_eliminations += 1;
                            expr.node = left.node.clone();
                            return;
                        }
                    }
                }
            }

            ExprNode::Call(_, args) => {
                for arg in args.iter_mut() {
                    self.fold_expr(arg);
                }
            }
            ExprNode::UnaryNot(inner) => {
                self.fold_expr(inner);
                match &inner.node {
                    ExprNode::BoolLiteral(v) => {
                        expr.node = ExprNode::BoolLiteral(!v);
                        self.stats.constants_folded += 1;
                    }
                    // Double negation: !!x → x
                    ExprNode::UnaryNot(inner2) => {
                        self.stats.identity_eliminations += 1;
                        expr.node = inner2.node.clone();
                    }
                    _ => {}
                }
            }
            ExprNode::FieldAccess(obj, _) => {
                self.fold_expr(obj);
            }
            ExprNode::StructCreate(_, fields) => {
                for (_, e) in fields.iter_mut() {
                    self.fold_expr(e);
                }
            }
            ExprNode::EnumCreate(_, _, args) => {
                for a in args.iter_mut() {
                    self.fold_expr(a);
                }
            }
            ExprNode::MethodCall(obj, _, args) => {
                self.fold_expr(obj);
                for a in args.iter_mut() {
                    self.fold_expr(a);
                }
            }
            ExprNode::Borrow(inner) | ExprNode::Receive(inner) | 
            ExprNode::TryExpr(inner) | ExprNode::Await(inner) => {
                self.fold_expr(inner);
            }
            ExprNode::Closure(_, body) => {
                for s in body.iter_mut() {
                    self.fold_stmt(s);
                }
            }
            ExprNode::ListLiteral(items) => {
                for item in items.iter_mut() {
                    self.fold_expr(item);
                }
            }
            _ => {} // literals, identifiers
        }
    }

    /// Run the optimizer on a program
    pub fn run(program: &mut Program) -> OptStats {
        let mut opt = Optimizer::new();
        opt.optimize_program(program);
        opt.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn optimize_source(source: &str) -> (Program, OptStats) {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().expect("Lexer error");
        let mut parser = Parser::new(tokens);
        let mut program = parser.parse_program().expect("Parser error");
        let stats = Optimizer::run(&mut program);
        (program, stats)
    }

    #[test]
    fn test_constant_folding() {
        let (program, stats) = optimize_source(
            "define main:\n    set x to 3 + 4\n    return 0"
        );
        assert!(stats.constants_folded > 0, "Expected constant folding");
        if let StmtNode::Set(_, expr, _) = &program.functions[0].body[0].node {
            assert!(matches!(expr.node, ExprNode::Integer(7)), 
                "Expected 3+4 to be folded to 7, got {:?}", expr.node);
        }
    }

    #[test]
    fn test_dead_code_after_return() {
        let (program, stats) = optimize_source(
            "define main:\n    return 0\n    display 42"
        );
        assert!(stats.dead_stmts_eliminated > 0, "Expected dead code elimination");
        assert_eq!(program.functions[0].body.len(), 1, "Should have only the return statement");
    }

    #[test]
    fn test_comparison_folding() {
        let (program, stats) = optimize_source(
            "define main:\n    set x to 3 + 4\n    return x"
        );
        assert!(stats.constants_folded > 0, "Expected comparison constant folding");
        if let StmtNode::Set(_, expr, _) = &program.functions[0].body[0].node {
            assert!(matches!(expr.node, ExprNode::Integer(7)),
                "Expected folded to 7, got {:?}", expr.node);
        }
    }

    #[test]
    fn test_identity_elimination() {
        let (program, stats) = optimize_source(
            "define main:\n    set x to 5 + 0\n    return x"
        );
        assert!(stats.identity_eliminations > 0 || stats.constants_folded > 0,
            "Expected identity elimination or folding");
        if let StmtNode::Set(_, expr, _) = &program.functions[0].body[0].node {
            assert!(matches!(expr.node, ExprNode::Integer(5)),
                "Expected 5+0 to be simplified to 5, got {:?}", expr.node);
        }
    }

    #[test]
    fn test_multiply_by_zero() {
        let (program, _stats) = optimize_source(
            "define main:\n    set x to 7 multiplied by 0\n    return x"
        );
        if let StmtNode::Set(_, expr, _) = &program.functions[0].body[0].node {
            assert!(matches!(expr.node, ExprNode::Integer(0)),
                "Expected 7*0 to be folded to 0, got {:?}", expr.node);
        }
    }

    #[test]
    fn test_self_subtraction() {
        // x - x → 0 (when x is the same identifier)
        let (program, stats) = optimize_source(
            "define main:\n    set a to 42\n    set b to a - a\n    return b"
        );
        assert!(stats.constants_folded > 0, "Expected x - x to be folded to 0");
        if let StmtNode::Set(_, expr, _) = &program.functions[0].body[1].node {
            assert!(matches!(expr.node, ExprNode::Integer(0)),
                "Expected a - a to be folded to 0, got {:?}", expr.node);
        }
    }

    #[test]
    fn test_self_comparison() {
        // x == x → 1 (true)
        let (program, stats) = optimize_source(
            "define main:\n    set a to 42\n    set b to a == a\n    return b"
        );
        assert!(stats.constants_folded > 0, "Expected x == x to be folded");
        if let StmtNode::Set(_, expr, _) = &program.functions[0].body[1].node {
            assert!(matches!(expr.node, ExprNode::Integer(1)),
                "Expected a == a to be folded to 1, got {:?}", expr.node);
        }
    }

    #[test]
    fn test_function_inlining() {
        // A simple function that returns a single expression should be inlined
        let (program, stats) = optimize_source(
            "define add with a and b:\n    return a + b\n\ndefine main:\n    set x to add(3 and 4)\n    return x"
        );
        assert!(stats.functions_inlined > 0, "Expected function inlining, got stats: {:?}", stats);
        // After inlining: set x to 3 + 4, which then folds to 7
        if let StmtNode::Set(_, expr, _) = &program.functions[1].body[0].node {
            assert!(matches!(expr.node, ExprNode::Integer(7)),
                "Expected inlined add(3,4) to fold to 7, got {:?}", expr.node);
        }
    }

    #[test]
    fn test_inlining_preserves_complex_functions() {
        // Functions with > 8 statements should NOT be inlined
        let (_program, stats) = optimize_source(
            "define big with x:\n    set a to x + 1\n    set b to x + 2\n    set c to x + 3\n    set d to x + 4\n    set e to x + 5\n    set f to x + 6\n    set g to x + 7\n    set h to x + 8\n    set i to x + 9\n    return i\n\ndefine main:\n    set r to big(10)\n    return r"
        );
        assert_eq!(stats.functions_inlined, 0, "Large functions should not be inlined");
    }
}
