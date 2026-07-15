use crate::ast::{Program, Function, StructDef, EnumDef, MethodDef, TypeAnnotation, Stmt, StmtNode, Expr, ExprNode, Op, CompOp, LogicalOp};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
enum Type {
    Int,
    Float,
    Bool,
    Str,
    DynStr,
    List,
    Map,
    RefList,
    RefStr,
    Struct(String),
    Enum(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum OwnerState {
    Owned,
    Moved,
}

fn is_tracked(t: &Type) -> bool {
    matches!(t, Type::List | Type::Map | Type::Str | Type::DynStr | Type::RefList | Type::RefStr | Type::Struct(_) | Type::Enum(_))
}

fn expr_contains_var(expr: &Expr, var_name: &str) -> bool {
    match &expr.node {
        ExprNode::Identifier(name) => name == var_name,
        ExprNode::Binary(l, _, r) | ExprNode::Comparison(l, _, r) | ExprNode::Logical(l, _, r) => {
            expr_contains_var(l, var_name) || expr_contains_var(r, var_name)
        }
        ExprNode::Call(_, args) => {
            args.iter().any(|arg| expr_contains_var(arg, var_name))
        }
        ExprNode::MethodCall(obj, _, args) => {
            expr_contains_var(obj, var_name) || args.iter().any(|arg| expr_contains_var(arg, var_name))
        }
        ExprNode::FieldAccess(obj, _) => {
            expr_contains_var(obj, var_name)
        }
        ExprNode::StructCreate(_, field_inits) => {
            field_inits.iter().any(|(_, fexpr)| expr_contains_var(fexpr, var_name))
        }
        ExprNode::EnumCreate(_, _, args) => {
            args.iter().any(|arg| expr_contains_var(arg, var_name))
        }
        ExprNode::UnaryNot(inner) | ExprNode::TryExpr(inner) | ExprNode::Await(inner) | ExprNode::Borrow(inner) | ExprNode::Receive(inner) => {
            expr_contains_var(inner, var_name)
        }
        ExprNode::ListLiteral(elements) => {
            elements.iter().any(|el| expr_contains_var(el, var_name))
        }
        _ => false,
    }
}

fn is_primitive_op_expr(expr: &Expr, param_name: &str) -> bool {
    match &expr.node {
        ExprNode::Integer(_) | ExprNode::FloatLiteral(_) | ExprNode::BoolLiteral(_) | ExprNode::StringLiteral(_) => true,
        ExprNode::Identifier(_name) => {
            // The identifier itself is fine as long as we only use it in primitive context
            true
        }
        ExprNode::Binary(l, _, r) | ExprNode::Comparison(l, _, r) | ExprNode::Logical(l, _, r) => {
            is_primitive_op_expr(l, param_name) && is_primitive_op_expr(r, param_name)
        }
        ExprNode::UnaryNot(inner) => {
            is_primitive_op_expr(inner, param_name)
        }
        ExprNode::Call(name, args) => {
            let primitive_arg_builtins = [
                "int_to_string", "int_to_float", "float_to_int", "char_from_code", "ep_abs",
                "ep_sleep_ms", "sleep_ms", "ep_time_ms", "ep_random_int"
            ];
            if primitive_arg_builtins.contains(&name.as_str()) {
                args.iter().all(|arg| is_primitive_op_expr(arg, param_name))
            } else {
                !expr_contains_var(expr, param_name)
            }
        }
        _ => {
            !expr_contains_var(expr, param_name)
        }
    }
}

fn stmt_contains_non_primitive_usage(stmt: &Stmt, param_name: &str) -> bool {
    match &stmt.node {
        StmtNode::Set(_, expr, _) => {
            !is_primitive_op_expr(expr, param_name)
        }
        StmtNode::If(cond, then_b, else_b) => {
            if !is_primitive_op_expr(cond, param_name) { return true; }
            then_b.iter().any(|s| stmt_contains_non_primitive_usage(s, param_name)) ||
            else_b.as_ref().map_or(false, |eb| eb.iter().any(|s| stmt_contains_non_primitive_usage(s, param_name)))
        }
        StmtNode::RepeatWhile(cond, body) => {
            if !is_primitive_op_expr(cond, param_name) { return true; }
            body.iter().any(|s| stmt_contains_non_primitive_usage(s, param_name))
        }
        StmtNode::ForEach(loop_var, iterable, body) => {
            if loop_var == param_name { return true; }
            if !is_primitive_op_expr(iterable, param_name) { return true; }
            body.iter().any(|s| stmt_contains_non_primitive_usage(s, param_name))
        }
        StmtNode::Return(expr) | StmtNode::Display(expr) | StmtNode::ExprStmt(expr) => {
            !is_primitive_op_expr(expr, param_name)
        }
        StmtNode::Send(expr, body) => {
            expr_contains_var(expr, param_name) || expr_contains_var(body, param_name)
        }
        StmtNode::FieldSet(obj, _, val) => {
            expr_contains_var(obj, param_name) || expr_contains_var(val, param_name)
        }
        StmtNode::Match(expr, arms) => {
            if expr_contains_var(expr, param_name) { return true; }
            for (_, bindings, body) in arms {
                if bindings.contains(&param_name.to_string()) { return true; }
                if body.iter().any(|s| stmt_contains_non_primitive_usage(s, param_name)) { return true; }
            }
            false
        }
        _ => false,
    }
}

/// Whether a local variable needs to be registered as a GC root.
/// Only root variables whose inferred type could hold a heap pointer.
/// Int, Float, and Bool variables are never heap pointers and can skip
/// GC rooting entirely. This eliminates all GC overhead for pure-compute
/// functions like fib() that only use integer parameters.
fn needs_gc_root(t: &Type) -> bool {
    match t {
        Type::Int | Type::Float | Type::Bool => false,
        _ => true,
    }
}

pub struct Codegen {
    out: String,
    func_return_types: HashMap<String, Type>,
    current_return_type: Type,
    spawn_index: usize,
    spawn_wrapper_index: usize,
    pub is_test_mode: bool,
    struct_defs: HashMap<String, StructDef>,
    enum_defs: HashMap<String, EnumDef>,
    /// Maps variant name -> enum name for quick lookup
    variant_to_enum: HashMap<String, String>,
    /// Maps list variable name -> element type (for string list display)
    list_element_types: HashMap<String, Type>,
    /// Maps closure variable name -> generated C function name
    closure_c_names: HashMap<String, String>,
    /// Set by the Set handler to pass the variable name to Closure codegen
    pending_closure_name: Option<String>,
    /// Maps closure C function name -> list of captured variable names from outer scope
    closure_captures: HashMap<String, Vec<String>>,
    /// Set of C runtime builtin function names (used to skip conflicting stdlib imports)
    builtin_c_funcs: std::collections::HashSet<String>,
    /// Set of top-level constant names (emitted as C globals, not re-declared as locals)
    global_constants: std::collections::HashSet<String>,
    is_async_func: bool,
    current_async_func_locals: std::collections::HashSet<String>,
    await_counter: usize,
    trait_impls: std::collections::HashSet<(String, String)>,
}

impl Codegen {
    pub fn new() -> Self {
        Self {
            out: String::new(),
            func_return_types: HashMap::new(),
            current_return_type: Type::Int,
            spawn_index: 0,
            spawn_wrapper_index: 0,
            is_test_mode: false,
            struct_defs: HashMap::new(),
            enum_defs: HashMap::new(),
            variant_to_enum: HashMap::new(),
            list_element_types: HashMap::new(),
            closure_c_names: HashMap::new(),
            pending_closure_name: None,
            closure_captures: HashMap::new(),
            builtin_c_funcs: std::collections::HashSet::new(),
            global_constants: std::collections::HashSet::new(),
            is_async_func: false,
            current_async_func_locals: std::collections::HashSet::new(),
            await_counter: 0,
            trait_impls: std::collections::HashSet::new(),
        }
    }

    /// Sanitize an ErnosPlain identifier to avoid C reserved word collisions
    fn sanitize_c_name(name: &str) -> String {
        const C_KEYWORDS: &[&str] = &[
            "auto", "break", "case", "char", "const", "continue", "default",
            "do", "double", "else", "enum", "extern", "float", "for", "goto",
            "if", "int", "long", "register", "return", "short", "signed",
            "sizeof", "static", "struct", "switch", "typedef", "union",
            "unsigned", "void", "volatile", "while",
            // C99+
            "inline", "restrict", "_Bool", "_Complex", "_Imaginary",
            // C11+
            "_Alignas", "_Alignof", "_Atomic", "_Generic", "_Noreturn",
            "_Static_assert", "_Thread_local",
            // Common clashes
            "main", "printf", "scanf", "malloc", "free", "exit",
            "read", "write", "open", "close", "send", "recv",
            "select", "remove", "rename", "time", "sleep",
        ];
        if C_KEYWORDS.contains(&name) {
            format!("ep_{}", name)
        } else {
            name.to_string()
        }
    }

    fn count_awaits_in_expr(&self, expr: &Expr) -> usize {
        let mut count = 0;
        match &expr.node {
            ExprNode::Await(inner) => {
                count += 1 + self.count_awaits_in_expr(inner);
            }
            ExprNode::Binary(left, _, right) | ExprNode::Comparison(left, _, right) | ExprNode::Logical(left, _, right) => {
                count += self.count_awaits_in_expr(left) + self.count_awaits_in_expr(right);
            }
            ExprNode::UnaryNot(inner) | ExprNode::TryExpr(inner) | ExprNode::Borrow(inner) | ExprNode::Receive(inner) | ExprNode::FieldAccess(inner, _) => {
                count += self.count_awaits_in_expr(inner);
            }
            ExprNode::Call(_, args) | ExprNode::MethodCall(_, _, args) | ExprNode::EnumCreate(_, _, args) | ExprNode::ListLiteral(args) => {
                for arg in args {
                    count += self.count_awaits_in_expr(arg);
                }
            }
            ExprNode::StructCreate(_, fields) => {
                for (_, f_expr) in fields {
                    count += self.count_awaits_in_expr(f_expr);
                }
            }
            _ => {}
        }
        count
    }

    fn count_awaits_in_stmts(&self, stmts: &[Stmt]) -> usize {
        let mut count = 0;
        for stmt in stmts {
            match &stmt.node {
                StmtNode::Set(_, expr, _) | StmtNode::ExprStmt(expr) | StmtNode::Display(expr) | StmtNode::Return(expr) => {
                    count += self.count_awaits_in_expr(expr);
                }
                StmtNode::If(cond, then_branch, else_branch) => {
                    count += self.count_awaits_in_expr(cond);
                    count += self.count_awaits_in_stmts(then_branch);
                    if let Some(else_stmts) = else_branch {
                        count += self.count_awaits_in_stmts(else_stmts);
                    }
                }
                StmtNode::RepeatWhile(cond, body) => {
                    count += self.count_awaits_in_expr(cond);
                    count += self.count_awaits_in_stmts(body);
                }
                StmtNode::ForEach(_, list_expr, body) => {
                    count += self.count_awaits_in_expr(list_expr);
                    count += self.count_awaits_in_stmts(body);
                }
                StmtNode::Match(expr, arms) => {
                    count += self.count_awaits_in_expr(expr);
                    for (_, _, body) in arms {
                        count += self.count_awaits_in_stmts(body);
                    }
                }
                _ => {}
            }
        }
        count
    }

    fn emit_yields_for_expr(&mut self, expr: &Expr, var_types: &HashMap<String, Type>) -> Result<(), String> {
        match &expr.node {
            ExprNode::Await(inner) => {
                self.emit_yields_for_expr(inner, var_types)?;
                self.await_counter += 1;
                let inner_str = self.gen_expr(inner, var_types)?;
                let next_state = self.await_counter;
                
                self.out.push_str(&format!(
                    "            {{\n\
                     \x20               EpFuture* _f = (EpFuture*)({inner});\n\
                     \x20               args->awaited_fut_{id} = _f;\n\
                     \x20               if (_f && !_f->completed) {{\n\
                     \x20                   args->state = {next_state};\n\
                     \x20                   _f->waiting_task = ep_current_task;\n\
                     \x20                   return -999999;\n\
                     \x20               }}\n\
                     \x20           }}\n\
                     \x20           case {next_state}:\n",
                    inner = inner_str,
                    id = self.await_counter,
                    next_state = next_state
                ));
            }
            ExprNode::Binary(left, _, right) | ExprNode::Comparison(left, _, right) | ExprNode::Logical(left, _, right) => {
                self.emit_yields_for_expr(left, var_types)?;
                self.emit_yields_for_expr(right, var_types)?;
            }
            ExprNode::UnaryNot(inner) | ExprNode::TryExpr(inner) | ExprNode::Borrow(inner) | ExprNode::Receive(inner) | ExprNode::FieldAccess(inner, _) => {
                self.emit_yields_for_expr(inner, var_types)?;
            }
            ExprNode::Call(_, args) | ExprNode::MethodCall(_, _, args) | ExprNode::EnumCreate(_, _, args) | ExprNode::ListLiteral(args) => {
                for arg in args {
                    self.emit_yields_for_expr(arg, var_types)?;
                }
            }
            ExprNode::StructCreate(_, fields) => {
                for (_, f_expr) in fields {
                    self.emit_yields_for_expr(f_expr, var_types)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn emit_yields_for_statement(&mut self, stmt: &Stmt, var_types: &HashMap<String, Type>) -> Result<(), String> {
        match &stmt.node {
            StmtNode::Set(_, expr, _) | StmtNode::ExprStmt(expr) | StmtNode::Display(expr) | StmtNode::Return(expr) => {
                self.emit_yields_for_expr(expr, var_types)?;
            }
            StmtNode::If(cond, then_branch, else_branch) => {
                self.emit_yields_for_expr(cond, var_types)?;
                for s in then_branch {
                    self.emit_yields_for_statement(s, var_types)?;
                }
                if let Some(else_stmts) = else_branch {
                    for s in else_stmts {
                        self.emit_yields_for_statement(s, var_types)?;
                    }
                }
            }
            StmtNode::RepeatWhile(cond, body) => {
                self.emit_yields_for_expr(cond, var_types)?;
                for s in body {
                    self.emit_yields_for_statement(s, var_types)?;
                }
            }
            StmtNode::ForEach(_, list_expr, body) => {
                self.emit_yields_for_expr(list_expr, var_types)?;
                for s in body {
                    self.emit_yields_for_statement(s, var_types)?;
                }
            }
            StmtNode::Match(expr, arms) => {
                self.emit_yields_for_expr(expr, var_types)?;
                for (_, _, body) in arms {
                    for s in body {
                        self.emit_yields_for_statement(s, var_types)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn type_annotation_to_type(&self, ann: &TypeAnnotation) -> Type {
        match ann {
            TypeAnnotation::Int => Type::Int,
            TypeAnnotation::Float => Type::Float,
            TypeAnnotation::Bool => Type::Bool,
            TypeAnnotation::Str => Type::Str,
            TypeAnnotation::DynStr => Type::DynStr,
            TypeAnnotation::List => Type::List,
            TypeAnnotation::UserDefined(name) => {
                if name == "Any" {
                    Type::Int
                } else if self.enum_defs.contains_key(name) {
                    Type::Enum(name.clone())
                } else {
                    Type::Struct(name.clone())
                }
            }
            TypeAnnotation::Generic(name, _) => {
                if self.enum_defs.contains_key(name) {
                    Type::Enum(name.clone())
                } else {
                    Type::Struct(name.clone())
                }
            }
        }
    }

    fn analyze_return_types(&mut self, program: &Program) {
        self.func_return_types.clear();
        
        self.func_return_types.insert("read_file_content".to_string(), Type::DynStr);
        self.func_return_types.insert("create_list".to_string(), Type::List);
        self.func_return_types.insert("ep_md5".to_string(), Type::DynStr);
        self.func_return_types.insert("ep_sha256".to_string(), Type::DynStr);
        self.func_return_types.insert("ep_hmac_sha256".to_string(), Type::DynStr);
        self.func_return_types.insert("ep_net_connect".to_string(), Type::Int);
        self.func_return_types.insert("ep_net_listen".to_string(), Type::Int);
        self.func_return_types.insert("ep_net_accept".to_string(), Type::Int);
        self.func_return_types.insert("ep_net_send".to_string(), Type::Int);
        self.func_return_types.insert("ep_net_recv".to_string(), Type::DynStr);
        self.func_return_types.insert("ep_net_close".to_string(), Type::Int);
        self.func_return_types.insert("append_list".to_string(), Type::Int);
        self.func_return_types.insert("get_list".to_string(), Type::Int);
        self.func_return_types.insert("set_list".to_string(), Type::Int);
        self.func_return_types.insert("remove_list".to_string(), Type::Int);
        self.func_return_types.insert("length_list".to_string(), Type::Int);
        self.func_return_types.insert("string_length".to_string(), Type::Int);
        self.func_return_types.insert("get_character".to_string(), Type::Int);
        self.func_return_types.insert("display_string".to_string(), Type::Int);
        self.func_return_types.insert("get_argument_count".to_string(), Type::Int);
        self.func_return_types.insert("get_argument".to_string(), Type::Str);
        self.func_return_types.insert("write_file_content".to_string(), Type::Int);
        self.func_return_types.insert("run_command".to_string(), Type::Int);
        self.func_return_types.insert("substring".to_string(), Type::DynStr);
        self.func_return_types.insert("string_from_list".to_string(), Type::DynStr);
        self.func_return_types.insert("pop_list".to_string(), Type::Int);
        self.func_return_types.insert("get_list_data_ptr".to_string(), Type::Int);
        self.func_return_types.insert("sqlite_get_callback_ptr".to_string(), Type::Int);
        self.func_return_types.insert("ep_sqlite3_open".to_string(), Type::Int);
        self.func_return_types.insert("ep_sqlite3_close".to_string(), Type::Int);
        self.func_return_types.insert("ep_sqlite3_exec".to_string(), Type::Int);
        self.func_return_types.insert("ep_sqlite3_prepare_v2".to_string(), Type::Int);
        self.func_return_types.insert("ep_sqlite3_bind_text".to_string(), Type::Int);
        self.func_return_types.insert("ep_sqlite3_bind_int".to_string(), Type::Int);
        self.func_return_types.insert("ep_sqlite3_step".to_string(), Type::Int);
        self.func_return_types.insert("ep_sqlite3_column_count".to_string(), Type::Int);
        self.func_return_types.insert("ep_sqlite3_column_text".to_string(), Type::DynStr);
        self.func_return_types.insert("ep_sqlite3_column_int".to_string(), Type::Int);
        self.func_return_types.insert("ep_sqlite3_finalize".to_string(), Type::Int);
        self.func_return_types.insert("free_list".to_string(), Type::Int);
        self.func_return_types.insert("create_map".to_string(), Type::Map);
        self.func_return_types.insert("map_insert".to_string(), Type::Int);
        self.func_return_types.insert("map_get_val".to_string(), Type::Int);
        self.func_return_types.insert("map_contains".to_string(), Type::Int);
        self.func_return_types.insert("map_delete".to_string(), Type::Int);
        self.func_return_types.insert("map_keys".to_string(), Type::List);
        self.func_return_types.insert("map_size".to_string(), Type::Int);
        self.func_return_types.insert("map_values".to_string(), Type::List);
        self.func_return_types.insert("free_map".to_string(), Type::Int);
        self.func_return_types.insert("map_set_str".to_string(), Type::Int);
        self.func_return_types.insert("map_get_str".to_string(), Type::DynStr);
        self.func_return_types.insert("create_deque".to_string(), Type::Int);
        self.func_return_types.insert("deque_push_back".to_string(), Type::Int);
        self.func_return_types.insert("deque_push_front".to_string(), Type::Int);
        self.func_return_types.insert("deque_pop_back".to_string(), Type::Int);
        self.func_return_types.insert("deque_pop_front".to_string(), Type::Int);
        self.func_return_types.insert("deque_length".to_string(), Type::Int);
        self.func_return_types.insert("free_deque".to_string(), Type::Int);
        self.func_return_types.insert("fs_scan_dir".to_string(), Type::List);
        self.func_return_types.insert("fs_copy_file".to_string(), Type::Int);
        self.func_return_types.insert("fs_delete_file".to_string(), Type::Int);
        self.func_return_types.insert("fs_move_file".to_string(), Type::Int);
        self.func_return_types.insert("fs_exists".to_string(), Type::Int);
        self.func_return_types.insert("fs_is_dir".to_string(), Type::Int);
        self.func_return_types.insert("fs_is_file".to_string(), Type::Int);
        self.func_return_types.insert("fs_get_size".to_string(), Type::Int);
        self.func_return_types.insert("ep_http_request".to_string(), Type::DynStr);
        self.func_return_types.insert("ep_sleep_ms".to_string(), Type::Int);
        self.func_return_types.insert("ep_dlopen".to_string(), Type::Int);
        self.func_return_types.insert("ep_dlsym".to_string(), Type::Int);
        self.func_return_types.insert("ep_dlclose".to_string(), Type::Int);
        self.func_return_types.insert("ep_dlcall0".to_string(), Type::Int);
        self.func_return_types.insert("ep_dlcall1".to_string(), Type::Int);
        self.func_return_types.insert("ep_dlcall2".to_string(), Type::Int);
        self.func_return_types.insert("ep_dlcall3".to_string(), Type::Int);
        self.func_return_types.insert("ep_dlcall4".to_string(), Type::Int);
        self.func_return_types.insert("ep_dlcall5".to_string(), Type::Int);
        self.func_return_types.insert("ep_dlcall6".to_string(), Type::Int);
        self.func_return_types.insert("ep_dlcall7".to_string(), Type::Int);
        self.func_return_types.insert("ep_dlcall8".to_string(), Type::Int);
        self.func_return_types.insert("ep_dlcall9".to_string(), Type::Int);
        self.func_return_types.insert("ep_dlcall10".to_string(), Type::Int);
        // Float FFI return types
        self.func_return_types.insert("ep_dlcall_f0".to_string(), Type::Float);
        self.func_return_types.insert("ep_dlcall_f1".to_string(), Type::Float);
        self.func_return_types.insert("ep_dlcall_f2".to_string(), Type::Float);
        self.func_return_types.insert("ep_dlcall_f3".to_string(), Type::Float);
        self.func_return_types.insert("ep_dlcall_f4".to_string(), Type::Float);
        self.func_return_types.insert("ep_dlcall_f5".to_string(), Type::Float);
        self.func_return_types.insert("ep_dlcall_f6".to_string(), Type::Float);
        self.func_return_types.insert("ep_dlcall_fd1".to_string(), Type::Int);
        self.func_return_types.insert("ep_dlcall_fd2".to_string(), Type::Int);
        self.func_return_types.insert("ep_dlcall_fd3".to_string(), Type::Int);
        self.func_return_types.insert("ep_double_to_bits".to_string(), Type::Int);
        self.func_return_types.insert("ep_bits_to_double".to_string(), Type::Float);
        self.func_return_types.insert("ep_system".to_string(), Type::Int);
        self.func_return_types.insert("ep_play_sound".to_string(), Type::Int);
        self.func_return_types.insert("concat".to_string(), Type::DynStr);
        self.func_return_types.insert("int_to_string".to_string(), Type::DynStr);
        self.func_return_types.insert("read_line".to_string(), Type::DynStr);
        self.func_return_types.insert("read_int".to_string(), Type::Int);
        self.func_return_types.insert("read_float".to_string(), Type::Float);
        self.func_return_types.insert("int_to_float".to_string(), Type::Float);
        self.func_return_types.insert("float_to_int".to_string(), Type::Int);
        self.func_return_types.insert("file_read".to_string(), Type::DynStr);
        self.func_return_types.insert("file_write".to_string(), Type::Int);
        self.func_return_types.insert("file_append".to_string(), Type::Int);
        self.func_return_types.insert("file_exists".to_string(), Type::Int);
        self.func_return_types.insert("string_contains".to_string(), Type::Int);
        self.func_return_types.insert("string_index_of".to_string(), Type::Int);
        self.func_return_types.insert("string_replace".to_string(), Type::DynStr);
        self.func_return_types.insert("json_get_string".to_string(), Type::DynStr);
        self.func_return_types.insert("json_get_int".to_string(), Type::Int);
        self.func_return_types.insert("json_get_bool".to_string(), Type::Int);
        self.func_return_types.insert("ep_sha1".to_string(), Type::DynStr);
        self.func_return_types.insert("ep_net_recv_bytes".to_string(), Type::DynStr);
        self.func_return_types.insert("channel_try_recv".to_string(), Type::Int);
        self.func_return_types.insert("channel_has_data".to_string(), Type::Int);
        self.func_return_types.insert("channel_select".to_string(), Type::Int);
        self.func_return_types.insert("ep_auto_to_string".to_string(), Type::DynStr);
        self.func_return_types.insert("string_upper".to_string(), Type::DynStr);
        self.func_return_types.insert("string_lower".to_string(), Type::DynStr);
        self.func_return_types.insert("string_trim".to_string(), Type::DynStr);
        self.func_return_types.insert("string_split".to_string(), Type::List);
        self.func_return_types.insert("string_to_list".to_string(), Type::List);
        self.func_return_types.insert("char_at".to_string(), Type::Int);
        self.func_return_types.insert("char_from_code".to_string(), Type::DynStr);
        self.func_return_types.insert("ep_abs".to_string(), Type::Int);
        self.func_return_types.insert("string_to_int".to_string(), Type::Int);
        self.func_return_types.insert("ep_random_int".to_string(), Type::Int);
        self.func_return_types.insert("ep_time_ms".to_string(), Type::Int);
        self.func_return_types.insert("ep_uuid_v4".to_string(), Type::DynStr);
        self.func_return_types.insert("ep_base64_encode".to_string(), Type::DynStr);
        self.func_return_types.insert("ep_os_name".to_string(), Type::Str);
        self.func_return_types.insert("create_channel".to_string(), Type::Int);

        // Structured Concurrency builtins
        self.func_return_types.insert("create_task_group".to_string(), Type::Int);
        self.func_return_types.insert("add_task_group".to_string(), Type::Int);
        self.func_return_types.insert("wait_task_group".to_string(), Type::List);
        self.func_return_types.insert("async_timeout".to_string(), Type::Int);
        self.func_return_types.insert("cancel_task".to_string(), Type::Int);
        self.func_return_types.insert("sleep_ms".to_string(), Type::Int);
        self.func_return_types.insert("async_wait_readable".to_string(), Type::Int);

        // FFI pointer/byte builtins
        self.func_return_types.insert("str_to_ptr".to_string(), Type::Int);
        self.func_return_types.insert("ptr_to_str".to_string(), Type::DynStr);
        self.func_return_types.insert("ep_int_to_str".to_string(), Type::DynStr);
        self.func_return_types.insert("peek_byte".to_string(), Type::Int);
        self.func_return_types.insert("poke_byte".to_string(), Type::Int);
        self.func_return_types.insert("alloc_bytes".to_string(), Type::Int);
        self.func_return_types.insert("free_bytes".to_string(), Type::Int);
        self.func_return_types.insert("list_to_bytes".to_string(), Type::Int);
        self.func_return_types.insert("bytes_to_list".to_string(), Type::List);
        self.func_return_types.insert("ep_gc_get_minor_count".to_string(), Type::Int);
        self.func_return_types.insert("ep_gc_get_major_count".to_string(), Type::Int);
        self.func_return_types.insert("ep_gc_get_nursery_count".to_string(), Type::Int);

        // Save the set of builtin C runtime names before externals/user funcs are added
        self.builtin_c_funcs = self.func_return_types.keys().cloned().collect();

        for ext in &program.externals {
            if let Some(ref rt) = ext.return_type {
                self.func_return_types.insert(ext.name.clone(), self.type_annotation_to_type(rt));
            } else if !self.func_return_types.contains_key(&ext.name) {
                self.func_return_types.insert(ext.name.clone(), Type::Int);
            }
        }

        // Fixed-point iteration for resolution of dependencies/mutual calls
        let mut changed = true;
        let mut pass = 0;
        while changed && pass < 20 {
            changed = false;
            pass += 1;

            // 1. Top-level functions
            for func in &program.functions {
                // If the function has an explicit return type annotation, use it
                if let Some(ref rt) = func.return_type {
                    let ty = self.type_annotation_to_type(rt);
                    if self.func_return_types.get(&func.name) != Some(&ty) {
                        self.func_return_types.insert(func.name.clone(), ty);
                        changed = true;
                    }
                    continue;
                }

                let mut var_types = HashMap::new();
                for param in &func.params {
                    let param_type = if let Some(ref ann) = param.2 {
                        self.type_annotation_to_type(ann)
                    } else if param.1 {
                        Type::RefList
                    } else {
                        // Try to infer struct type from field access patterns in the body
                        self.infer_param_struct_type(&param.0, &func.body).unwrap_or(Type::Int)
                    };
                    var_types.insert(param.0.clone(), param_type);
                }
                self.collect_var_types(&func.body, &mut var_types);
                
                let ret = self.determine_ret_type(&func.body, &var_types).unwrap_or(Type::Int);
                if self.func_return_types.get(&func.name) != Some(&ret) {
                    self.func_return_types.insert(func.name.clone(), ret);
                    changed = true;
                }
            }

            // 2. Methods
            for md in &program.method_defs {
                let key = format!("{}_{}", md.struct_name, md.name);
                if let Some(ref rt) = md.return_type {
                    let ty = self.type_annotation_to_type(rt);
                    if self.func_return_types.get(&key) != Some(&ty) {
                        self.func_return_types.insert(key, ty);
                        changed = true;
                    }
                    continue;
                }

                let mut var_types = HashMap::new();
                if self.enum_defs.contains_key(&md.struct_name) {
                    var_types.insert("self".to_string(), Type::Enum(md.struct_name.clone()));
                } else {
                    var_types.insert("self".to_string(), Type::Struct(md.struct_name.clone()));
                }

                for param in &md.params {
                    let param_type = if let Some(ref ann) = param.2 {
                        self.type_annotation_to_type(ann)
                    } else if param.1 {
                        Type::RefList
                    } else {
                        self.infer_param_struct_type(&param.0, &md.body).unwrap_or(Type::Int)
                    };
                    var_types.insert(param.0.clone(), param_type);
                }
                self.collect_var_types(&md.body, &mut var_types);

                let ret = self.determine_ret_type(&md.body, &var_types).unwrap_or(Type::Int);
                if self.func_return_types.get(&key) != Some(&ret) {
                    self.func_return_types.insert(key, ret);
                    changed = true;
                }
            }

            // 3. Trait impl methods
            for ti in &program.trait_impls {
                for m in &ti.methods {
                    let key = format!("{}_{}", ti.for_type, m.name);
                    if let Some(ref rt) = m.return_type {
                        let ty = self.type_annotation_to_type(rt);
                        if self.func_return_types.get(&key) != Some(&ty) {
                            self.func_return_types.insert(key, ty);
                            changed = true;
                        }
                        continue;
                    }

                    let mut var_types = HashMap::new();
                    if self.enum_defs.contains_key(&ti.for_type) {
                        var_types.insert("self".to_string(), Type::Enum(ti.for_type.clone()));
                    } else {
                        var_types.insert("self".to_string(), Type::Struct(ti.for_type.clone()));
                    }

                    for param in &m.params {
                        let param_type = if let Some(ref ann) = param.2 {
                            self.type_annotation_to_type(ann)
                        } else if param.1 {
                            Type::RefList
                        } else {
                            self.infer_param_struct_type(&param.0, &m.body).unwrap_or(Type::Int)
                        };
                        var_types.insert(param.0.clone(), param_type);
                    }
                    self.collect_var_types(&m.body, &mut var_types);

                    let ret = self.determine_ret_type(&m.body, &var_types).unwrap_or(Type::Int);
                    if self.func_return_types.get(&key) != Some(&ret) {
                        self.func_return_types.insert(key, ret);
                        changed = true;
                    }
                }
            }
        }
    }

    /// Collect all field names accessed on a variable via FieldAccess in the AST
    fn collect_field_accesses_expr(&self, param_name: &str, expr: &Expr, fields: &mut Vec<String>) {
        match &expr.node {
            ExprNode::FieldAccess(obj, field_name) => {
                if let ExprNode::Identifier(name) = &obj.node {
                    if name == param_name {
                        fields.push(field_name.clone());
                    }
                }
                self.collect_field_accesses_expr(param_name, obj, fields);
            }
            ExprNode::Binary(l, _, r) | ExprNode::Comparison(l, _, r) | ExprNode::Logical(l, _, r) => {
                self.collect_field_accesses_expr(param_name, l, fields);
                self.collect_field_accesses_expr(param_name, r, fields);
            }
            ExprNode::Call(_, args) => {
                for a in args { self.collect_field_accesses_expr(param_name, a, fields); }
            }
            ExprNode::MethodCall(obj, _, args) => {
                self.collect_field_accesses_expr(param_name, obj, fields);
                for a in args { self.collect_field_accesses_expr(param_name, a, fields); }
            }
            ExprNode::UnaryNot(inner) | ExprNode::TryExpr(inner) | ExprNode::Await(inner) | ExprNode::Borrow(inner) | ExprNode::Receive(inner) => {
                self.collect_field_accesses_expr(param_name, inner, fields);
            }
            ExprNode::StructCreate(_, field_inits) => {
                for (_, fexpr) in field_inits { self.collect_field_accesses_expr(param_name, fexpr, fields); }
            }
            ExprNode::EnumCreate(_, _, args) => {
                for a in args { self.collect_field_accesses_expr(param_name, a, fields); }
            }
            ExprNode::Closure(_, body) => {
                self.collect_field_accesses_stmts(param_name, body, fields);
            }
            _ => {}
        }
    }

    fn collect_field_accesses_stmts(&self, param_name: &str, stmts: &[Stmt], fields: &mut Vec<String>) {
        for stmt in stmts {
            match &stmt.node {
                StmtNode::Display(e) | StmtNode::ExprStmt(e) | StmtNode::Return(e) => {
                    self.collect_field_accesses_expr(param_name, e, fields);
                }
                StmtNode::Set(_, expr, _) => {
                    self.collect_field_accesses_expr(param_name, expr, fields);
                }
                StmtNode::FieldSet(obj, field_name, expr) => {
                    // Also collect the field name from FieldSet (set p.field to ...)
                    if let ExprNode::Identifier(name) = &obj.node {
                        if name == param_name {
                            fields.push(field_name.clone());
                        }
                    }
                    self.collect_field_accesses_expr(param_name, obj, fields);
                    self.collect_field_accesses_expr(param_name, expr, fields);
                }
                StmtNode::If(cond, then_b, else_b) => {
                    self.collect_field_accesses_expr(param_name, cond, fields);
                    self.collect_field_accesses_stmts(param_name, then_b, fields);
                    if let Some(eb) = else_b { self.collect_field_accesses_stmts(param_name, eb, fields); }
                }
                StmtNode::RepeatWhile(cond, body) => {
                    self.collect_field_accesses_expr(param_name, cond, fields);
                    self.collect_field_accesses_stmts(param_name, body, fields);
                }
                StmtNode::ForEach(_, iter_expr, body) => {
                    self.collect_field_accesses_expr(param_name, iter_expr, fields);
                    self.collect_field_accesses_stmts(param_name, body, fields);
                }
                StmtNode::Send(val, chan) => {
                    self.collect_field_accesses_expr(param_name, val, fields);
                    self.collect_field_accesses_expr(param_name, chan, fields);
                }
                StmtNode::Spawn(_, args) => {
                    for a in args { self.collect_field_accesses_expr(param_name, a, fields); }
                }
                StmtNode::Match(expr, arms) => {
                    self.collect_field_accesses_expr(param_name, expr, fields);
                    for (_, _, body) in arms { self.collect_field_accesses_stmts(param_name, body, fields); }
                }
                _ => {}
            }
        }
    }

    /// Try to infer a struct type for a parameter by matching field access patterns
    fn infer_param_struct_type(&self, param_name: &str, body: &[Stmt]) -> Option<Type> {
        let mut accessed_fields = Vec::new();
        self.collect_field_accesses_stmts(param_name, body, &mut accessed_fields);
        if accessed_fields.is_empty() {
            return None;
        }
        // Deduplicate
        accessed_fields.sort();
        accessed_fields.dedup();
        
        // Find structs where ALL accessed fields exist
        let mut candidates = Vec::new();
        for (struct_name, sd) in &self.struct_defs {
            let struct_field_names: Vec<&str> = sd.fields.iter().map(|(n, _, _)| n.as_str()).collect();
            let all_match = accessed_fields.iter().all(|f| struct_field_names.contains(&f.as_str()));
            if all_match {
                candidates.push(struct_name.clone());
            }
        }
        
        if candidates.len() == 1 {
            Some(Type::Struct(candidates[0].clone()))
        } else {
            None // Ambiguous or no match — require explicit annotation
        }
    }

    fn infer_param_types_from_usage(&self, body: &[Stmt], var_types: &mut HashMap<String, Type>) {
        fn scan_expr(expr: &Expr, var_types: &mut HashMap<String, Type>) {
            match &expr.node {
                ExprNode::Call(name, args) => {
                    if name == "length_list" || name == "append_list" || name == "get_list" || 
                       name == "set_list" || name == "remove_list" || name == "pop_list" {
                        if let Some(ExprNode::Identifier(pname)) = args.first().map(|e| &e.node) {
                            if let Some(ty) = var_types.get_mut(pname) {
                                if *ty == Type::Int {
                                    *ty = Type::List;
                                }
                            }
                        }
                    }
                    if name == "map_insert" || name == "map_get_val" || name == "map_contains" || 
                       name == "map_delete" || name == "map_keys" || name == "map_size" || 
                       name == "map_values" || name == "map_set_str" || name == "map_get_str" {
                        if let Some(ExprNode::Identifier(pname)) = args.first().map(|e| &e.node) {
                            if let Some(ty) = var_types.get_mut(pname) {
                                if *ty == Type::Int {
                                    *ty = Type::Map;
                                }
                            }
                        }
                    }
                    if name == "string_length" || name == "get_character" || name == "char_at" ||
                       name == "string_contains" || name == "string_index_of" || name == "string_replace" ||
                       name == "string_upper" || name == "string_lower" || name == "string_trim" ||
                       name == "string_split" || name == "string_to_list" || name == "json_get_string" || name == "json_get_int" ||
                       name == "json_get_bool" {
                        if let Some(ExprNode::Identifier(pname)) = args.first().map(|e| &e.node) {
                            if let Some(ty) = var_types.get_mut(pname) {
                                if *ty == Type::Int {
                                    *ty = Type::Str;
                                }
                            }
                        }
                    }
                    for arg in args {
                        scan_expr(arg, var_types);
                    }
                }
                ExprNode::MethodCall(obj, _, args) => {
                    scan_expr(obj, var_types);
                    for arg in args {
                        scan_expr(arg, var_types);
                    }
                }
                ExprNode::Binary(l, _, r) | ExprNode::Comparison(l, _, r) | ExprNode::Logical(l, _, r) => {
                    scan_expr(l, var_types);
                    scan_expr(r, var_types);
                }
                ExprNode::UnaryNot(inner) | ExprNode::TryExpr(inner) | ExprNode::Await(inner) | ExprNode::Borrow(inner) | ExprNode::Receive(inner) => {
                    scan_expr(inner, var_types);
                }
                ExprNode::StructCreate(_, field_inits) => {
                    for (_, fexpr) in field_inits {
                        scan_expr(fexpr, var_types);
                    }
                }
                ExprNode::EnumCreate(_, _, args) => {
                    for arg in args {
                        scan_expr(arg, var_types);
                    }
                }
                ExprNode::ListLiteral(elements) => {
                    for el in elements {
                        scan_expr(el, var_types);
                    }
                }
                _ => {}
            }
        }

        fn scan_stmts(stmts: &[Stmt], var_types: &mut HashMap<String, Type>) {
            for stmt in stmts {
                match &stmt.node {
                    StmtNode::Set(_, expr, _) => scan_expr(expr, var_types),
                    StmtNode::If(cond, then_b, else_b) => {
                        scan_expr(cond, var_types);
                        scan_stmts(then_b, var_types);
                        if let Some(eb) = else_b {
                            scan_stmts(eb, var_types);
                        }
                    }
                    StmtNode::RepeatWhile(cond, body) => {
                        scan_expr(cond, var_types);
                        scan_stmts(body, var_types);
                    }
                    StmtNode::ForEach(_, iterable, body) => {
                        scan_expr(iterable, var_types);
                        scan_stmts(body, var_types);
                    }
                    StmtNode::Return(expr) | StmtNode::Display(expr) | StmtNode::ExprStmt(expr) => {
                        scan_expr(expr, var_types);
                    }
                    StmtNode::Send(expr, body) => {
                        scan_expr(expr, var_types);
                        scan_expr(body, var_types);
                    }
                    StmtNode::FieldSet(obj, _, val) => {
                        scan_expr(obj, var_types);
                        scan_expr(val, var_types);
                    }
                    StmtNode::Match(expr, arms) => {
                        scan_expr(expr, var_types);
                        for (_, _, body) in arms {
                            scan_stmts(body, var_types);
                        }
                    }
                    _ => {}
                }
            }
        }

        scan_stmts(body, var_types);
    }

    fn collect_var_types(&mut self, stmts: &[Stmt], var_types: &mut HashMap<String, Type>) {
        for stmt in stmts {
            match &stmt.node {
                StmtNode::Set(name, expr, type_ann) => {
                    let t = if let Some(ann) = type_ann {
                        self.type_annotation_to_type(ann)
                    } else {
                        self.infer_type(expr, var_types)
                    };
                    var_types.insert(name.clone(), t);
                    // Track element type for list literals with string elements
                    if let ExprNode::ListLiteral(elements) = &expr.node {
                        if elements.iter().any(|e| matches!(e.node, ExprNode::StringLiteral(_))) {
                            self.list_element_types.insert(name.clone(), Type::Str);
                        }
                    }
                    // Track element type when append_list(list_var, value) is called
                    // This allows for-each loops to give the loop variable the correct type
                    if let ExprNode::Call(func_name, args) = &expr.node {
                        if func_name == "append_list" && args.len() == 2 {
                            if let ExprNode::Identifier(list_name) = &args[0].node {
                                let elem_type = self.infer_type(&args[1], var_types);
                                match &elem_type {
                                    Type::Struct(_) | Type::Enum(_) | Type::Str => {
                                        self.list_element_types.insert(list_name.clone(), elem_type);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    // Pre-register closure variable → C function name mapping
                    if matches!(expr.node, ExprNode::Closure(_, _)) {
                        let c_name = format!("_ep_closure_{}", Self::sanitize_c_name(name));
                        self.closure_c_names.insert(name.clone(), c_name);
                    }
                }
                StmtNode::FieldSet(_, _, _) => {}
                StmtNode::If(_, then_branch, else_branch) => {
                    self.collect_var_types(then_branch, var_types);
                    if let Some(eb) = else_branch {
                        self.collect_var_types(eb, var_types);
                    }
                }
                StmtNode::RepeatWhile(_, body) => {
                    self.collect_var_types(body, var_types);
                }
                StmtNode::ForEach(loop_var, iterable, body) => {
                    let mut elem_type = Type::Int;
                    let iterable_type = self.infer_type(iterable, var_types);
                    let mut is_iterator = false;
                    let mut iter_type_name = String::new();
                    
                    match &iterable_type {
                        Type::Struct(name) | Type::Enum(name) => {
                            if self.trait_impls.contains(&("Iterator".to_string(), name.clone())) {
                                is_iterator = true;
                                iter_type_name = name.clone();
                            }
                        }
                        _ => {}
                    }
                    
                    if is_iterator {
                        let next_key = format!("{}_next", iter_type_name);
                        if let Some(Type::Enum(ename)) = self.func_return_types.get(&next_key) {
                            if let Some(ed) = self.enum_defs.get(ename) {
                                if let Some((_, fields)) = ed.variants.iter().find(|(vn, _)| vn == "Next") {
                                    if let Some((_, ann)) = fields.first() {
                                        elem_type = self.type_annotation_to_type(ann);
                                    }
                                }
                            }
                        }
                    } else {
                        // Determine loop variable type from the list's element type
                        elem_type = if let ExprNode::Identifier(list_name) = &iterable.node {
                            self.list_element_types.get(list_name).cloned().unwrap_or(Type::Int)
                        } else if let ExprNode::ListLiteral(elements) = &iterable.node {
                            if elements.iter().any(|e| matches!(e.node, ExprNode::StringLiteral(_))) {
                                Type::Str
                            } else {
                                Type::Int
                            }
                        } else {
                            Type::Int
                        };
                    }
                    var_types.insert(loop_var.clone(), elem_type);
                    self.collect_var_types(body, var_types);
                }
                StmtNode::Match(expr, arms) => {
                    let enum_type = self.infer_type(expr, var_types);
                    if let Type::Enum(enum_name) = &enum_type {
                         if let Some(ed) = self.enum_defs.get(enum_name).cloned() {
                            for (variant_name, bindings, body) in arms {
                                if let Some((_, fields)) = ed.variants.iter().find(|(vn, _)| vn == variant_name) {
                                    for (i, binding) in bindings.iter().enumerate() {
                                        if i < fields.len() {
                                            let t = match &fields[i].1 {
                                                TypeAnnotation::Int => Type::Int,
                                                TypeAnnotation::Float => Type::Float,
                                                TypeAnnotation::Bool => Type::Bool,
                                                TypeAnnotation::Str => Type::Str,
                                                TypeAnnotation::DynStr => Type::DynStr,
                                                TypeAnnotation::List => Type::List,
                                                TypeAnnotation::UserDefined(n) => {
                                                    if self.enum_defs.contains_key(n) {
                                                        Type::Enum(n.clone())
                                                    } else {
                                                        Type::Struct(n.clone())
                                                    }
                                                }
                                                TypeAnnotation::Generic(n, _) => {
                                                    if self.enum_defs.contains_key(n) {
                                                        Type::Enum(n.clone())
                                                    } else {
                                                        Type::Struct(n.clone())
                                                    }
                                                }
                                            };
                                            var_types.insert(binding.clone(), t);
                                        }
                                    }
                                }
                                self.collect_var_types(body, var_types);
                            }
                        }
                    } else {
                        for (_, _, body) in arms {
                            self.collect_var_types(body, var_types);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn determine_ret_type(&self, stmts: &[Stmt], var_types: &HashMap<String, Type>) -> Option<Type> {
        for stmt in stmts {
            match &stmt.node {
                StmtNode::Return(expr) => {
                    return Some(self.infer_type(expr, var_types));
                }
                StmtNode::If(_, then_branch, else_branch) => {
                    if let Some(t) = self.determine_ret_type(then_branch, var_types) {
                        return Some(t);
                    }
                    if let Some(eb) = else_branch {
                        if let Some(t) = self.determine_ret_type(eb, var_types) {
                            return Some(t);
                        }
                    }
                }
                StmtNode::RepeatWhile(_, body) => {
                    if let Some(t) = self.determine_ret_type(body, var_types) {
                        return Some(t);
                    }
                }
                StmtNode::ForEach(_, _, body) => {
                    if let Some(t) = self.determine_ret_type(body, var_types) {
                        return Some(t);
                    }
                }
                StmtNode::Match(_, arms) => {
                    for (_, _, body) in arms {
                        if let Some(t) = self.determine_ret_type(body, var_types) {
                            return Some(t);
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn infer_type(&self, expr: &Expr, var_types: &HashMap<String, Type>) -> Type {
        match &expr.node {
            ExprNode::Integer(_) => Type::Int,
            ExprNode::FloatLiteral(_) => Type::Float,
            ExprNode::BoolLiteral(_) => Type::Bool,
            ExprNode::StringLiteral(_) => Type::Str,
            ExprNode::Identifier(name) => {
                // Check if it's a known enum variant (bare variant without data)
                if let Some(enum_name) = self.variant_to_enum.get(name) {
                    Type::Enum(enum_name.clone())
                } else {
                    var_types.get(name).cloned().unwrap_or(Type::Int)
                }
            }
            ExprNode::Binary(left, _, right) => {
                let lt = self.infer_type(left, var_types);
                let rt = self.infer_type(right, var_types);
                if lt == Type::Float || rt == Type::Float { Type::Float } else { Type::Int }
            }
            ExprNode::Comparison(_, _, _) => Type::Int,
            ExprNode::Logical(_, _, _) => Type::Int,
            ExprNode::Call(name, _) => {
                self.func_return_types.get(name).cloned().unwrap_or(Type::Int)
            }
            ExprNode::Channel => Type::Int,
            ExprNode::Receive(_) => Type::Int,
            ExprNode::Borrow(inner) => {
                let t = self.infer_type(inner, var_types);
                match t {
                    Type::List | Type::RefList => Type::RefList,
                    Type::Str | Type::DynStr | Type::RefStr => Type::RefStr,
                    _ => Type::RefList,
                }
            }
            ExprNode::FieldAccess(obj, field_name) => {
                let obj_type = self.infer_type(obj, var_types);
                if let Type::Struct(struct_name) = obj_type {
                    if let Some(sd) = self.struct_defs.get(&struct_name) {
                        for (fname, ftype, _) in &sd.fields {
                            if fname == field_name {
                                return match ftype {
                                    TypeAnnotation::Int => Type::Int,
                                    TypeAnnotation::Float => Type::Float,
                                    TypeAnnotation::Bool => Type::Bool,
                                    TypeAnnotation::Str => Type::Str,
                                    TypeAnnotation::DynStr => Type::DynStr,
                                    TypeAnnotation::List => Type::List,
                                    TypeAnnotation::UserDefined(name) => Type::Struct(name.clone()),
                                    TypeAnnotation::Generic(name, _) => Type::Struct(name.clone()),
                                };
                            }
                        }
                    }
                }
                Type::Int
            }
            ExprNode::StructCreate(struct_name, _) => Type::Struct(struct_name.clone()),
            ExprNode::EnumCreate(enum_name, variant_name, _) => {
                // If enum_name is empty, look it up from variant_to_enum
                if enum_name.is_empty() {
                    if let Some(en) = self.variant_to_enum.get(variant_name) {
                        Type::Enum(en.clone())
                    } else {
                        Type::Int
                    }
                } else {
                    Type::Enum(enum_name.clone())
                }
            }
            ExprNode::MethodCall(obj, method_name, _) => {
                let obj_type = self.infer_type(obj, var_types);
                let type_name = match &obj_type {
                    Type::Struct(s) => Some(s.clone()),
                    Type::Enum(e) => Some(e.clone()),
                    _ => None,
                };
                if let Some(name) = type_name {
                    let key = format!("{}_{}", name, method_name);
                    self.func_return_types.get(&key).cloned().unwrap_or(Type::Int)
                } else {
                    Type::Int
                }
            }
            ExprNode::UnaryNot(_) => Type::Int,
            ExprNode::TryExpr(inner) => {
                // Preserve the inner expression's type
                self.infer_type(inner, var_types)
            }
            ExprNode::Closure(_, _) => Type::Int, // closure is a function pointer (long long)
            ExprNode::Await(inner) => self.infer_type(inner, var_types),
            ExprNode::ListLiteral(_) => Type::List,
        }
    }

    fn check_expr_reads(
        &self,
        expr: &Expr,
        var_types: &HashMap<String, Type>,
        owner_states: &HashMap<String, OwnerState>,
    ) -> Result<(), String> {
        match &expr.node {
            ExprNode::Integer(_) | ExprNode::FloatLiteral(_) | ExprNode::BoolLiteral(_) | ExprNode::StringLiteral(_) | ExprNode::Channel => Ok(()),
            ExprNode::Identifier(name) => {
                let t = var_types.get(name).cloned().unwrap_or(Type::Int);
                if is_tracked(&t) {
                    if let Some(OwnerState::Moved) = owner_states.get(name) {
                        return Err(format!("Safety Error: Use of moved value: {}", name));
                    }
                }
                Ok(())
            }
            ExprNode::Borrow(inner) => {
                self.check_expr_reads(inner, var_types, owner_states)
            }
            ExprNode::Binary(left, _, right) | ExprNode::Comparison(left, _, right) | ExprNode::Logical(left, _, right) => {
                self.check_expr_reads(left, var_types, owner_states)?;
                self.check_expr_reads(right, var_types, owner_states)?;
                Ok(())
            }
            ExprNode::Call(_, args) => {
                for arg in args {
                    self.check_expr_reads(arg, var_types, owner_states)?;
                }
                Ok(())
            }
            ExprNode::Receive(inner) => {
                self.check_expr_reads(inner, var_types, owner_states)
            }
            ExprNode::FieldAccess(inner, _) => {
                self.check_expr_reads(inner, var_types, owner_states)
            }
            ExprNode::StructCreate(_, fields) => {
                for (_, expr) in fields {
                    self.check_expr_reads(expr, var_types, owner_states)?;
                }
                Ok(())
            }
            ExprNode::EnumCreate(_, _, args) => {
                for arg in args {
                    self.check_expr_reads(arg, var_types, owner_states)?;
                }
                Ok(())
            }
            ExprNode::MethodCall(obj, _, args) => {
                self.check_expr_reads(obj, var_types, owner_states)?;
                for arg in args {
                    self.check_expr_reads(arg, var_types, owner_states)?;
                }
                Ok(())
            }
            ExprNode::UnaryNot(inner) => {
                self.check_expr_reads(inner, var_types, owner_states)?;
                Ok(())
            }
            ExprNode::TryExpr(inner) => {
                self.check_expr_reads(inner, var_types, owner_states)?;
                Ok(())
            }
            ExprNode::Closure(_, _) => Ok(()), // closures don't read from outer scope at safety-check time
            ExprNode::ListLiteral(elements) => {
                for e in elements { self.check_expr_reads(e, var_types, owner_states)?; }
                Ok(())
            }
            ExprNode::Await(inner) => {
                self.check_expr_reads(inner, var_types, owner_states)?;
                Ok(())
            }
        }
    }

    fn check_safety_stmts(
        &self,
        func: &Function,
        stmts: &[Stmt],
        var_types: &HashMap<String, Type>,
        owner_states: &mut HashMap<String, OwnerState>,
        borrows: &mut HashMap<String, String>,
        borrow_counts: &mut HashMap<String, usize>,
    ) -> Result<(), String> {
        for stmt in stmts {
            match &stmt.node {
                StmtNode::Set(name, expr, _type_ann) => {
                    self.check_expr_reads(expr, var_types, owner_states)?;

                    if let Some(old_target) = borrows.remove(name) {
                        if let Some(count) = borrow_counts.get_mut(&old_target) {
                            if *count > 0 {
                                *count -= 1;
                            }
                        }
                    }

                    match &expr.node {
                        ExprNode::Borrow(inner) => {
                            if let ExprNode::Identifier(target) = &inner.node {
                                if let Some(OwnerState::Moved) = owner_states.get(target) {
                                    return Err(format!("Safety Error: Cannot borrow moved variable: {}", target));
                                }
                                borrows.insert(name.clone(), target.clone());
                                *borrow_counts.entry(target.clone()).or_insert(0) += 1;
                            } else {
                                return Err("Safety Error: Expected identifier in borrow expression".to_string());
                            }
                        }
                        _ => {
                            let bc = *borrow_counts.get(name).unwrap_or(&0);
                            if bc > 0 {
                                return Err(format!("Safety Error: Cannot modify variable because it is currently borrowed: {}", name));
                            }

                            let t = var_types.get(name).cloned().unwrap_or(Type::Int);
                            if is_tracked(&t) {
                                if let ExprNode::Identifier(src) = &expr.node {
                                    let src_t = var_types.get(src).cloned().unwrap_or(Type::Int);
                                    if is_tracked(&src_t) {
                                        if let Some(OwnerState::Moved) = owner_states.get(src) {
                                            return Err(format!("Safety Error: Use of moved value: {}", src));
                                        }
                                        let src_bc = *borrow_counts.get(src).unwrap_or(&0);
                                        if src_bc > 0 {
                                            return Err(format!("Safety Error: Cannot move variable because it is currently borrowed: {}", src));
                                        }
                                        if src_t != Type::RefList && src_t != Type::RefStr
                                            && src_t != Type::Str && src_t != Type::DynStr {
                                            owner_states.insert(src.clone(), OwnerState::Moved);
                                        }
                                    }
                                }
                                owner_states.insert(name.clone(), OwnerState::Owned);
                            }
                        }
                    }
                }
                StmtNode::Send(chan, val) => {
                    self.check_expr_reads(chan, var_types, owner_states)?;
                    if let ExprNode::Identifier(src) = &val.node {
                        let src_t = var_types.get(src).cloned().unwrap_or(Type::Int);
                        if is_tracked(&src_t) {
                            if let Some(OwnerState::Moved) = owner_states.get(src) {
                                return Err(format!("Safety Error: Use of moved value: {}", src));
                            }
                            let src_bc = *borrow_counts.get(src).unwrap_or(&0);
                            if src_bc > 0 {
                                return Err(format!("Safety Error: Cannot move variable because it is currently borrowed: {}", src));
                            }
                            if src_t != Type::RefList && src_t != Type::RefStr
                                && src_t != Type::Str && src_t != Type::DynStr {
                                owner_states.insert(src.clone(), OwnerState::Moved);
                            }
                        } else {
                            self.check_expr_reads(val, var_types, owner_states)?;
                        }
                    } else {
                        self.check_expr_reads(val, var_types, owner_states)?;
                    }
                }
                StmtNode::Spawn(_func_name, args) => {
                    for arg in args {
                        if let ExprNode::Identifier(src) = &arg.node {
                            let src_t = var_types.get(src).cloned().unwrap_or(Type::Int);
                            if is_tracked(&src_t) {
                                if let Some(OwnerState::Moved) = owner_states.get(src) {
                                    return Err(format!("Safety Error: Use of moved value: {}", src));
                                }
                                let src_bc = *borrow_counts.get(src).unwrap_or(&0);
                                if src_bc > 0 {
                                    return Err(format!("Safety Error: Cannot move variable because it is currently borrowed: {}", src));
                                }
                                if src_t != Type::RefList && src_t != Type::RefStr
                                    && src_t != Type::Str && src_t != Type::DynStr {
                                    owner_states.insert(src.clone(), OwnerState::Moved);
                                }
                            } else {
                                self.check_expr_reads(arg, var_types, owner_states)?;
                            }
                        } else {
                            self.check_expr_reads(arg, var_types, owner_states)?;
                        }
                    }
                }
                StmtNode::Display(expr) => {
                    self.check_expr_reads(expr, var_types, owner_states)?;
                }
                StmtNode::Return(expr) => {
                    self.check_expr_reads(expr, var_types, owner_states)?;
                    if let ExprNode::Borrow(inner) = &expr.node {
                        if let ExprNode::Identifier(target) = &inner.node {
                            let is_borrowed_param = func.params.iter().any(|(p_name, is_borrow, _)| {
                                p_name == target && *is_borrow
                            });
                            if !is_borrowed_param {
                                return Err(format!("Safety Error: Cannot return reference to local variable: {}", target));
                            }
                        }
                    } else if let ExprNode::Identifier(name) = &expr.node {
                        let t = var_types.get(name).cloned().unwrap_or(Type::Int);
                        if t == Type::RefList || t == Type::RefStr {
                            let target = borrows.get(name).unwrap_or(name);
                            let is_borrowed_param = func.params.iter().any(|(p_name, is_borrow, _)| {
                                p_name == target && *is_borrow
                            });
                            if !is_borrowed_param {
                                return Err(format!("Safety Error: Cannot return reference to local variable: {}", target));
                            }
                        }
                    }
                }
                StmtNode::If(cond, then_branch, else_branch) => {
                    self.check_expr_reads(cond, var_types, owner_states)?;

                    let mut then_owner_states = owner_states.clone();
                    let mut then_borrows = borrows.clone();
                    let mut then_borrow_counts = borrow_counts.clone();

                    self.check_safety_stmts(
                        func,
                        then_branch,
                        var_types,
                        &mut then_owner_states,
                        &mut then_borrows,
                        &mut then_borrow_counts,
                    )?;

                    let mut else_owner_states = owner_states.clone();
                    let mut else_borrows = borrows.clone();
                    let mut else_borrow_counts = borrow_counts.clone();

                    if let Some(eb) = else_branch {
                        self.check_safety_stmts(
                            func,
                            eb,
                            var_types,
                            &mut else_owner_states,
                            &mut else_borrows,
                            &mut else_borrow_counts,
                        )?;
                    }

                    // Merge owner states
                    for (var_name, state) in owner_states.iter_mut() {
                        let then_val = then_owner_states.get(var_name).unwrap_or(&OwnerState::Owned);
                        let else_val = else_owner_states.get(var_name).unwrap_or(&OwnerState::Owned);
                        if *then_val == OwnerState::Moved || *else_val == OwnerState::Moved {
                            *state = OwnerState::Moved;
                        }
                    }

                    // Merge borrows
                    for (k, v) in then_borrows {
                        borrows.insert(k, v);
                    }
                    for (k, v) in else_borrows {
                        borrows.insert(k, v);
                    }

                    // Merge borrow counts
                    for (k, v) in then_borrow_counts {
                        let cur_v = borrow_counts.entry(k).or_insert(0);
                        if v > *cur_v {
                            *cur_v = v;
                        }
                    }
                    for (k, v) in else_borrow_counts {
                        let cur_v = borrow_counts.entry(k).or_insert(0);
                        if v > *cur_v {
                            *cur_v = v;
                        }
                    }
                }
                StmtNode::RepeatWhile(cond, body) => {
                    self.check_expr_reads(cond, var_types, owner_states)?;

                    let start_owner_states = owner_states.clone();

                    self.check_safety_stmts(
                        func,
                        body,
                        var_types,
                        owner_states,
                        borrows,
                        borrow_counts,
                    )?;

                    for (var_name, start_state) in start_owner_states {
                        let end_state = owner_states.get(&var_name).unwrap_or(&OwnerState::Owned);
                        if start_state == OwnerState::Owned && *end_state == OwnerState::Moved {
                            return Err(format!("Safety Error: Variable is moved inside a loop and not reinitialized: {}", var_name));
                        }
                    }

                    self.check_expr_reads(cond, var_types, owner_states)?;
                }
                StmtNode::ForEach(loop_var, iterable, body) => {
                    self.check_expr_reads(iterable, var_types, owner_states)?;

                    // The loop variable is owned within the loop body
                    owner_states.insert(loop_var.clone(), OwnerState::Owned);

                    let start_owner_states = owner_states.clone();

                    self.check_safety_stmts(
                        func,
                        body,
                        var_types,
                        owner_states,
                        borrows,
                        borrow_counts,
                    )?;

                    for (var_name, start_state) in start_owner_states {
                        let end_state = owner_states.get(&var_name).unwrap_or(&OwnerState::Owned);
                        if start_state == OwnerState::Owned && *end_state == OwnerState::Moved {
                            return Err(format!("Safety Error: Variable is moved inside a loop and not reinitialized: {}", var_name));
                        }
                    }
                }
                StmtNode::FieldSet(obj, _, expr) => {
                    self.check_expr_reads(obj, var_types, owner_states)?;
                    self.check_expr_reads(expr, var_types, owner_states)?;
                }
                StmtNode::Match(expr, arms) => {
                    self.check_expr_reads(expr, var_types, owner_states)?;
                    for (_, _, body) in arms {
                        let mut arm_owner_states = owner_states.clone();
                        let mut arm_borrows = borrows.clone();
                        let mut arm_borrow_counts = borrow_counts.clone();
                        self.check_safety_stmts(
                            func,
                            body,
                            var_types,
                            &mut arm_owner_states,
                            &mut arm_borrows,
                            &mut arm_borrow_counts,
                        )?;
                        // Merge: if moved in any arm, mark as moved
                        for (var_name, state) in owner_states.iter_mut() {
                            if let Some(OwnerState::Moved) = arm_owner_states.get(var_name) {
                                *state = OwnerState::Moved;
                            }
                        }
                    }
                }
                StmtNode::Break | StmtNode::Continue => {}
                StmtNode::ExprStmt(expr) => {
                    self.check_expr_reads(expr, var_types, owner_states)?;
                }
            }
        }
        Ok(())
    }

    fn gen_statement(
        &mut self,
        stmt: &Stmt,
        var_types: &HashMap<String, Type>,
    ) -> Result<(), String> {
        match &stmt.node {
            StmtNode::Set(name, expr, _type_ann) => {
                let safe_name = if self.is_async_func {
                    format!("args->{}", Self::sanitize_c_name(name))
                } else {
                    Self::sanitize_c_name(name)
                };
                let _t = var_types.get(name);
                // If this is a closure assignment, pass the variable name to Closure codegen
                if matches!(expr.node, ExprNode::Closure(_, _)) {
                    self.pending_closure_name = Some(name.clone());
                }
                let expr_str = self.gen_expr(expr, var_types)?;

                // For all types: simple assignment.
                // We intentionally do NOT pre-free lists/enums/structs here because
                // the new value may alias the old value through another variable
                // (e.g. set current_level to next_level; set next_level to create_list()
                // would free the list that current_level still references).
                // The GC handles the old allocations.
                self.out.push_str(&format!("    {} = {};\n", safe_name, expr_str));

                // After free_list/free_map calls, null-out the freed variable to prevent
                // double-free when the variable is later reassigned (pre-free pattern)
                if let ExprNode::Call(func_name, args) = &expr.node {
                    if (func_name == "free_list" || func_name == "free_map") && !args.is_empty() {
                        if let ExprNode::Identifier(arg_name) = &args[0].node {
                            let safe_arg = if self.is_async_func {
                                format!("args->{}", Self::sanitize_c_name(arg_name))
                            } else {
                                Self::sanitize_c_name(arg_name)
                            };
                            self.out.push_str(&format!("    {} = 0;\n", safe_arg));
                        }
                    }
                }
            }
            StmtNode::Return(expr) => {
                let expr_str = self.gen_expr(expr, var_types)?;
                if self.is_async_func {
                    self.out.push_str(&format!("    return {};\n", expr_str));
                } else {
                    self.out.push_str(&format!("    ret_val = {};\n", expr_str));

                    if let ExprNode::Identifier(name) = &expr.node {
                        let t = var_types.get(name);
                        if t == Some(&Type::List) || t == Some(&Type::Map) {
                            self.out.push_str(&format!("    {} = 0;\n", name));
                        } else if matches!(t, Some(Type::Enum(_))) {
                            self.out.push_str(&format!("    {} = 0;\n", name));
                        }
                    }
                    self.out.push_str("    goto L_cleanup;\n");
                }
            }
            StmtNode::Display(expr) => {
                let t = self.infer_type(expr, var_types);
                let expr_str = self.gen_expr(expr, var_types)?;
                match t {
                    Type::Str | Type::DynStr | Type::RefStr => {
                        self.out.push_str(&format!("    printf(\"%s\\n\", (char*){});\n", expr_str));
                    }
                    Type::Float => {
                        self.out.push_str(&format!("    {{ long long _ftmp = {}; double _dv; memcpy(&_dv, &_ftmp, sizeof(double)); printf(\"%.15g\\n\", _dv); }}\n", expr_str));
                    }
                    Type::Bool => {
                        self.out.push_str(&format!("    printf(\"%s\\n\", ({}) ? \"true\" : \"false\");\n", expr_str));
                    }
                    Type::Enum(ref enum_name) => {
                        self.out.push_str(&format!("    printf(\"%s\\n\", display_enum_{}({}));\n", enum_name, expr_str));
                    }
                    _ => {
                        // Smart display: detect if value is a string pointer or an integer
                        // ep_auto_to_string returns the pointer unchanged if it's a string,
                        // or allocates a new buffer with the int-to-string conversion
                        self.out.push_str(&format!(
                            "    {{ long long _dv = (long long){0}; long long _sv = ep_auto_to_string(_dv); printf(\"%s\\n\", (char*)_sv); }}\n",
                            expr_str
                        ));
                    }
                }
            }
            StmtNode::If(cond, then_branch, else_branch) => {
                let cond_str = self.gen_expr(cond, var_types)?;
                self.out.push_str(&format!("    if ({}) {{\n", cond_str));
                for s in then_branch {
                    self.gen_statement(s, var_types)?;
                }
                self.out.push_str("    }");
                if let Some(eb) = else_branch {
                    self.out.push_str(" else {\n");
                    for s in eb {
                        self.gen_statement(s, var_types)?;
                    }
                    self.out.push_str("    }\n");
                } else {
                    self.out.push_str("\n");
                }
            }
            StmtNode::RepeatWhile(cond, body) => {
                let cond_str = self.gen_expr(cond, var_types)?;
                self.out.push_str(&format!("    while ({}) {{\n", cond_str));
                for s in body {
                    self.gen_statement(s, var_types)?;
                }
                self.out.push_str("    }\n");
            }
            StmtNode::Spawn(_func_name, args) => {
                let idx = self.spawn_wrapper_index;
                self.spawn_wrapper_index += 1;
                self.out.push_str("    {\n");
                self.out.push_str(&format!("        spawn_args_{}* s_args = malloc(sizeof(spawn_args_{}));\n", idx, idx));
                for (j, arg) in args.iter().enumerate() {
                    let arg_str = self.gen_expr(arg, var_types)?;
                    self.out.push_str(&format!("        s_args->arg{} = {};\n", j, arg_str));
                }
                self.out.push_str("        #ifdef _WIN32\n");
                self.out.push_str(&format!("        HANDLE t = CreateThread(NULL, 0, (LPTHREAD_START_ROUTINE)spawn_wrapper_{}, s_args, 0, NULL);\n", idx));
                self.out.push_str("        if (t) CloseHandle(t);\n");
                self.out.push_str("        #else\n");
                self.out.push_str("        pthread_t t;\n");
                self.out.push_str(&format!("        int rc = pthread_create(&t, NULL, spawn_wrapper_{}, s_args);\n", idx));
                self.out.push_str("        if (rc != 0) { printf(\"DEBUG: pthread_create failed: %d\\n\", rc); }\n");
                self.out.push_str("        pthread_detach(t);\n");
                self.out.push_str("        #endif\n");
                self.out.push_str("    }\n");
            }
            StmtNode::Send(chan, val) => {
                let chan_str = self.gen_expr(chan, var_types)?;
                let val_str = self.gen_expr(val, var_types)?;
                self.out.push_str(&format!("    send_channel({}, {});\n", chan_str, val_str));
                if let ExprNode::Identifier(name) = &val.node {
                    let vt = var_types.get(name);
                    if vt == Some(&Type::List) || vt == Some(&Type::Map) {
                        self.out.push_str(&format!("    {} = 0;\n", name));
                    }
                }
            }
            StmtNode::FieldSet(obj, field_name, expr) => {
                let obj_str = self.gen_expr(obj, var_types)?;
                let expr_str = self.gen_expr(expr, var_types)?;
                let obj_type = self.infer_type(obj, var_types);
                if let Type::Struct(struct_name) = obj_type {
                    self.out.push_str(&format!("    ((EpStruct_{}*)({}))->{} = {};\n", struct_name, obj_str, field_name, expr_str));
                    self.out.push_str(&format!("    ep_gc_write_barrier((void*)({}), {});\n", obj_str, expr_str));
                } else {
                    // Fail loudly rather than silently dropping the assignment —
                    // emitting nothing here produced a struct whose field was
                    // never written (silent miscompile). Mirrors FieldAccess reads.
                    let var_name = if let ExprNode::Identifier(n) = &obj.node { n.clone() } else { "?".to_string() };
                    return Err(format!("Field assignment '.{}' on non-struct variable '{}' (type: {:?}) at line {}:{}. Add a type annotation like 'with {} as StructName'",
                        field_name, var_name, obj_type, stmt.span.line, stmt.span.col, var_name));
                }
            }
            StmtNode::Match(match_expr, arms) => {
                let expr_str = self.gen_expr(match_expr, var_types)?;
                let enum_type = self.infer_type(match_expr, var_types);
                if let Type::Enum(enum_name) = enum_type {
                    self.out.push_str("    {\n");
                    self.out.push_str(&format!("        EpEnum_{}* _match_ptr = (EpEnum_{}*){};\n", enum_name, enum_name, expr_str));
                    self.out.push_str("        long long _match_tag = _match_ptr->tag;\n");

                    let ed = self.enum_defs.get(&enum_name).cloned();
                    for (arm_idx, (variant_name, bindings, body)) in arms.iter().enumerate() {
                        let keyword = if arm_idx == 0 { "if" } else { "else if" };
                        self.out.push_str(&format!("        {} (_match_tag == EP_TAG_{}_{}) {{\n", keyword, enum_name, variant_name));

                        if let Some(ref ed) = ed {
                            if let Some((_, fields)) = ed.variants.iter().find(|(vn, _)| vn == variant_name) {
                                for (j, binding) in bindings.iter().enumerate() {
                                    if j < fields.len() {
                                        self.out.push_str(&format!("            long long {} = _match_ptr->data{};\n", binding, j));
                                    }
                                }
                            }
                        }

                        for s in body {
                            self.gen_statement(s, var_types)?;
                        }
                        self.out.push_str("        }\n");
                    }

                    // Exhaustive match check: warn about missing variants
                    if let Some(ref ed) = ed {
                        let matched_variants: Vec<&String> = arms.iter().map(|(vn, _, _)| vn).collect();
                        let missing: Vec<&String> = ed.variants.iter()
                            .map(|(vn, _)| vn)
                            .filter(|vn| !matched_variants.contains(vn))
                            .collect();
                        if !missing.is_empty() {
                            let missing_names: Vec<&str> = missing.iter().map(|s| s.as_str()).collect();
                            eprintln!("\x1b[1;33mWarning\x1b[0m: Non-exhaustive check on '{}' — missing variants: {}", 
                                enum_name, missing_names.join(", "));
                        }
                    }

                    self.out.push_str("    }\n");
                } else if matches!(enum_type, Type::Str | Type::DynStr) {
                    // String matching via strcmp
                    self.out.push_str("    {\n");
                    self.out.push_str(&format!("        const char* _match_val = (const char*){};\n", expr_str));
                    self.out.push_str("        if (_match_val) {\n");
                    for (arm_idx, (pattern, _bindings, body)) in arms.iter().enumerate() {
                        let keyword = if arm_idx == 0 { "if" } else { "else if" };
                        // Escape the pattern string for C
                        let escaped = pattern.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
                        self.out.push_str(&format!("        {} (strcmp(_match_val, \"{}\") == 0) {{\n", keyword, escaped));
                        for s in body {
                            self.gen_statement(s, var_types)?;
                        }
                        self.out.push_str("        }\n");
                    }
                    self.out.push_str("        }\n"); // close if (_match_val)
                    self.out.push_str("    }\n");
                } else if matches!(enum_type, Type::Int) {
                    // Integer matching via ==
                    self.out.push_str("    {\n");
                    self.out.push_str(&format!("        long long _match_val = {};\n", expr_str));
                    for (arm_idx, (pattern, _bindings, body)) in arms.iter().enumerate() {
                        let keyword = if arm_idx == 0 { "if" } else { "else if" };
                        self.out.push_str(&format!("        {} (_match_val == {}) {{\n", keyword, pattern));
                        for s in body {
                            self.gen_statement(s, var_types)?;
                        }
                        self.out.push_str("        }\n");
                    }
                    self.out.push_str("    }\n");
                } else {
                    return Err("Match/check statement on non-enum type".to_string());
                }
            }
            StmtNode::ForEach(loop_var, iterable, body) => {
                let iterable_type = self.infer_type(iterable, var_types);
                let mut is_iterator = false;
                let mut iter_type_name = String::new();
                
                match &iterable_type {
                    Type::Struct(name) | Type::Enum(name) => {
                        if self.trait_impls.contains(&("Iterator".to_string(), name.clone())) {
                            is_iterator = true;
                            iter_type_name = name.clone();
                        }
                    }
                    _ => {}
                }
                
                if is_iterator {
                    let iter_expr_str = self.gen_expr(iterable, var_types)?;
                    let next_key = format!("{}_next", iter_type_name);
                    let return_enum_name = match self.func_return_types.get(&next_key) {
                        Some(Type::Enum(ename)) => ename.clone(),
                        _ => "IterResult".to_string(),
                    };
                    
                    let unique_idx = self.spawn_index;
                    self.spawn_index += 1;
                    
                    self.out.push_str("    {\n");
                    self.out.push_str(&format!("        long long _foreach_iter_{} = {};\n", unique_idx, iter_expr_str));
                    self.out.push_str("        while (1) {\n");
                    self.out.push_str(&format!("            long long _res_{} = {}__next(_foreach_iter_{});\n", unique_idx, iter_type_name, unique_idx));
                    self.out.push_str(&format!("            if (_res_{} == 0) break;\n", unique_idx));
                    self.out.push_str(&format!("            EpEnum_{}* _res_ptr_{} = (EpEnum_{}*)_res_{};\n", return_enum_name, unique_idx, return_enum_name, unique_idx));
                    self.out.push_str(&format!("            if (_res_ptr_{}->tag == EP_TAG_{}_Done) break;\n", unique_idx, return_enum_name));
                    self.out.push_str(&format!("            {} = _res_ptr_{}->data0;\n", loop_var, unique_idx));
                    for s in body {
                        self.gen_statement(s, var_types)?;
                    }
                    self.out.push_str("        }\n");
                    self.out.push_str("    }\n");
                } else if let ExprNode::Call(func_name, args) = &iterable.node {
                    if func_name == "range" {
                        let (start_str, end_str) = if args.len() == 2 {
                            let s = self.gen_expr(&args[0], var_types)?;
                            let e = self.gen_expr(&args[1], var_types)?;
                            (s, e)
                        } else if args.len() == 1 {
                            let e = self.gen_expr(&args[0], var_types)?;
                            ("0".to_string(), e)
                        } else {
                            return Err("range() expects 1 or 2 arguments".to_string());
                        };
                        self.out.push_str(&format!("    for ({} = {}; {} < {}; {}++) {{\n",
                            loop_var, start_str, loop_var, end_str, loop_var));
                        for s in body {
                            self.gen_statement(s, var_types)?;
                        }
                        self.out.push_str("    }\n");
                    } else {
                        let list_str = self.gen_expr(iterable, var_types)?;
                        self.out.push_str("    {\n");
                        self.out.push_str(&format!("        long long _foreach_list = {};\n", list_str));
                        self.out.push_str("        long long _foreach_len = length_list(_foreach_list);\n");
                        self.out.push_str("        for (long long _foreach_i = 0; _foreach_i < _foreach_len; _foreach_i++) {\n");
                        self.out.push_str(&format!("            {} = get_list(_foreach_list, _foreach_i);\n", loop_var));
                        for s in body {
                            self.gen_statement(s, var_types)?;
                        }
                        self.out.push_str("        }\n");
                        self.out.push_str("    }\n");
                    }
                } else {
                    let list_str = self.gen_expr(iterable, var_types)?;
                    self.out.push_str("    {\n");
                    self.out.push_str(&format!("        long long _foreach_len = length_list({});\n", list_str));
                    self.out.push_str("        for (long long _foreach_i = 0; _foreach_i < _foreach_len; _foreach_i++) {\n");
                    self.out.push_str(&format!("            {} = get_list({}, _foreach_i);\n", loop_var, list_str));
                    for s in body {
                        self.gen_statement(s, var_types)?;
                    }
                    self.out.push_str("        }\n");
                    self.out.push_str("    }\n");
                }
            }
            StmtNode::Break => {
                self.out.push_str("    break;\n");
            }
            StmtNode::Continue => {
                self.out.push_str("    continue;\n");
            }
            StmtNode::ExprStmt(expr) => {
                let expr_str = self.gen_expr(expr, var_types)?;
                self.out.push_str(&format!("    {};\n", expr_str));

                // After free_list/free_map calls, null-out the freed variable to prevent
                // double-free when the variable is later reassigned (pre-free pattern)
                if let ExprNode::Call(func_name, args) = &expr.node {
                    if (func_name == "free_list" || func_name == "free_map") && !args.is_empty() {
                        if let ExprNode::Identifier(arg_name) = &args[0].node {
                            let safe_arg = Self::sanitize_c_name(arg_name);
                            self.out.push_str(&format!("    {} = 0;\n", safe_arg));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn gen_expr(
        &mut self,
        expr: &Expr,
        var_types: &HashMap<String, Type>,
    ) -> Result<String, String> {
        match &expr.node {
            ExprNode::Integer(val) => Ok(format!("{}", val)),
            ExprNode::FloatLiteral(val) => {
                Ok(format!("({{ double _fl = {:.17}; long long _fv; memcpy(&_fv, &_fl, sizeof(double)); _fv; }})", val))
            }
            ExprNode::BoolLiteral(b) => Ok(if *b { "1LL".to_string() } else { "0LL".to_string() }),
            ExprNode::StringLiteral(s) => {
                let escaped = s
                    .replace("\\", "\\\\")
                    .replace("\"", "\\\"")
                    .replace("\n", "\\n")
                    .replace("\t", "\\t")
                    .replace("\r", "\\r");
                Ok(format!("(long long)\"{}\"", escaped))
            }
            ExprNode::Identifier(name) => {
                // Check if this is a bare enum variant (no data)
                if name.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
                    if let Some(enum_name) = self.variant_to_enum.get(name) {
                        // It's a bare variant — generate enum creation with no args
                        return Ok(format!("create_{}_{}()", enum_name, name));
                    }
                }
                // If this identifier is a known function name (not a variable),
                // cast to (long long) so it can be passed as a HOF argument
                if !var_types.contains_key(name) && self.func_return_types.contains_key(name) {
                    return Ok(format!("(long long){}", Self::sanitize_c_name(name)));
                }
                if self.is_async_func && (var_types.contains_key(name) || self.current_async_func_locals.contains(name)) {
                    return Ok(format!("args->{}", Self::sanitize_c_name(name)));
                }
                Ok(name.clone())
            }
            ExprNode::Binary(left, op, right) => {
                let left_str = self.gen_expr(left, var_types)?;
                let right_str = self.gen_expr(right, var_types)?;
                let lt = self.infer_type(left, var_types);
                let rt = self.infer_type(right, var_types);
                let is_float = lt == Type::Float || rt == Type::Float;
                if is_float {
                    let op_str = match op {
                        Op::Add => "+",
                        Op::Sub => "-",
                        Op::Mul => "*",
                        Op::Div => "/",
                        Op::Mod => return Err("Modulo is not supported on floats".to_string()),
                    };
                    // Unpack both as double, compute, repack as long long
                    let l_unpack = if lt == Type::Float {
                        format!("({{ long long _lt = {}; double _d; memcpy(&_d, &_lt, sizeof(double)); _d; }})", left_str)
                    } else {
                        format!("(double)({})", left_str)
                    };
                    let r_unpack = if rt == Type::Float {
                        format!("({{ long long _rt = {}; double _d; memcpy(&_d, &_rt, sizeof(double)); _d; }})", right_str)
                    } else {
                        format!("(double)({})", right_str)
                    };
                    Ok(format!("({{ double _r = {} {} {}; long long _v; memcpy(&_v, &_r, sizeof(double)); _v; }})", l_unpack, op_str, r_unpack))
                } else {
                    let op_str = match op {
                        Op::Add => "+",
                        Op::Sub => "-",
                        Op::Mul => "*",
                        Op::Div => "/",
                        Op::Mod => "%",
                    };
                    Ok(format!("({} {} {})", left_str, op_str, right_str))
                }
            }
            ExprNode::Comparison(left, op, right) => {
                let left_str = self.gen_expr(left, var_types)?;
                let right_str = self.gen_expr(right, var_types)?;
                
                let left_type = self.infer_type(left, var_types);
                let right_type = self.infer_type(right, var_types);
                let inferred_string = left_type == Type::Str || left_type == Type::DynStr
                    || right_type == Type::Str || right_type == Type::DynStr;
                let has_string_literal = matches!(&left.node, ExprNode::StringLiteral(_))
                    || matches!(&right.node, ExprNode::StringLiteral(_));
                let is_string = inferred_string || has_string_literal;
                // Check if comparing a string to integer 0 (null check pattern from try expressions)
                let right_is_zero = matches!(&right.node, ExprNode::Integer(0));
                let left_is_zero = matches!(&left.node, ExprNode::Integer(0));
                if is_string && (right_is_zero || left_is_zero) {
                    // Null check: use pointer comparison, not strcmp (strcmp(str, NULL) crashes)
                    let op_str = match op {
                        CompOp::Equals => "==",
                        CompOp::NotEquals => "!=",
                        _ => "==",
                    };
                    Ok(format!("({} {} {})", left_str, op_str, right_str))
                } else if is_string {
                    match op {
                        CompOp::Equals => {
                            Ok(format!("ep_str_equals({}, {})", left_str, right_str))
                        }
                        CompOp::NotEquals => {
                            Ok(format!("(!ep_str_equals({}, {}))", left_str, right_str))
                        }
                        _ => {
                            // Ordering comparisons — null-safe: if either is null, return 0 (false)
                            let cmp_op = match op {
                                CompOp::LessThan => "< 0",
                                CompOp::GreaterThan => "> 0",
                                CompOp::LessEqual => "<= 0",
                                CompOp::GreaterEqual => ">= 0",
                                _ => unreachable!(),
                            };
                            Ok(format!("((char*){l} && (char*){r} ? strcmp((char*){l}, (char*){r}) {cmp} : 0)",
                                l=left_str, r=right_str, cmp=cmp_op))
                        }
                    }
                } else {
                    match op {
                        CompOp::Equals => {
                            Ok(format!("{} == {}", left_str, right_str))
                        }
                        CompOp::NotEquals => {
                            Ok(format!("{} != {}", left_str, right_str))
                        }
                        _ => {
                            let op_str = match op {
                                CompOp::LessThan => "<",
                                CompOp::GreaterThan => ">",
                                CompOp::LessEqual => "<=",
                                CompOp::GreaterEqual => ">=",
                                _ => unreachable!(),
                            };
                            Ok(format!("{} {} {}", left_str, op_str, right_str))
                        }
                    }
                }
            }
            ExprNode::Logical(left, op, right) => {
                let left_str = self.gen_expr(left, var_types)?;
                let right_str = self.gen_expr(right, var_types)?;
                let op_str = match op {
                    LogicalOp::And => "&&",
                    LogicalOp::Or => "||",
                };
                Ok(format!("({} {} {})", left_str, op_str, right_str))
            }
            ExprNode::Call(name, args) => {
                let mut args_str = Vec::new();
                for arg in args {
                    args_str.push(self.gen_expr(arg, var_types)?);
                }

                let mut formatted_args = Vec::new();
                for (i, arg_val) in args_str.iter().enumerate() {
                    let casted = match name.as_str() {
                        "read_file_content" | "string_length" | "display_string" | "run_command" | "ep_md5" | "ep_sha256" | "ep_net_connect" if i == 0 => {
                            format!("(char*){}", arg_val)
                        }
                        "get_character" | "substring" | "string_to_list" if i == 0 => {
                            format!("(char*){}", arg_val)
                        }
                        "write_file_content" => {
                            if i == 0 || i == 1 {
                                format!("(char*){}", arg_val)
                            } else {
                                arg_val.clone()
                            }
                        }
                        "ep_net_send" if i == 1 => {
                            format!("(char*){}", arg_val)
                        }
                        _ => arg_val.clone(),
                    };
                    formatted_args.push(casted);
                }

                let safe_name = Self::sanitize_c_name(name);
                let call_str = format!("{}({})", safe_name, formatted_args.join(", "));
                
                // Check if this is a closure call (variable holding a function pointer)
                if let Some(c_func_name) = self.closure_c_names.get(name).cloned() {
                    // Direct call to a known closure — pass env from its EpClosure struct
                    let args_str = formatted_args.join(", ");
                    let env_arg = format!("(long long)((EpClosure*){})->env", safe_name);
                    if args_str.is_empty() {
                        Ok(format!("{}({})", c_func_name, env_arg))
                    } else {
                        Ok(format!("{}({}, {})", c_func_name, env_arg, args_str))
                    }
                } else if var_types.contains_key(name) && !self.func_return_types.contains_key(name) {
                    // Indirect call through a variable — could be an EpClosure or a raw fn ptr
                    // Use magic number dispatch to handle both
                    let n_args = args.len();
                    let mut env_arg_types = vec!["long long".to_string()]; // env param
                    for _ in 0..n_args { env_arg_types.push("long long".to_string()); }
                    let closure_fn_type = format!("long long(*)({})", env_arg_types.join(", "));

                    let mut raw_arg_types: Vec<String> = Vec::new();
                    for _ in 0..n_args { raw_arg_types.push("long long".to_string()); }
                    let raw_fn_type = format!("long long(*)({})", if raw_arg_types.is_empty() { "void".to_string() } else { raw_arg_types.join(", ") });

                    let args_str = formatted_args.join(", ");
                    let closure_call = if args_str.is_empty() {
                        format!("(({})_cl->fn_ptr)((long long)_cl->env)", closure_fn_type)
                    } else {
                        format!("(({})_cl->fn_ptr)((long long)_cl->env, {})", closure_fn_type, args_str)
                    };
                    let raw_call = if args_str.is_empty() {
                        format!("(({}){})()", raw_fn_type, safe_name)
                    } else {
                        format!("(({}){safe_name})({})", raw_fn_type, args_str, safe_name = safe_name)
                    };

                    Ok(format!(
                        "({{ long long _fv = {sv}; EpClosure* _cl = (EpClosure*)_fv; (_fv != 0 && _cl->magic == EP_CLOSURE_MAGIC) ? {cl_call} : {raw_call}; }})",
                        sv = safe_name, cl_call = closure_call, raw_call = raw_call
                    ))
                } else {
                    match name.as_str() {
                        "read_file_content" | "get_argument" | "substring" | "string_from_list" | "ep_net_recv" | "ep_md5" | "ep_sha256" => {
                            Ok(format!("(long long){}", call_str))
                        }
                        _ => Ok(call_str),
                    }
                }
            }
            ExprNode::Channel => Ok("create_channel()".to_string()),
            ExprNode::Receive(chan) => {
                let chan_str = self.gen_expr(chan, var_types)?;
                Ok(format!("receive_channel({})", chan_str))
            }
            ExprNode::Borrow(inner) => self.gen_expr(inner, var_types),
            ExprNode::FieldAccess(obj, field_name) => {
                let obj_str = self.gen_expr(obj, var_types)?;
                let obj_type = self.infer_type(obj, var_types);
                if let Type::Struct(struct_name) = obj_type {
                    // Null pointer protection: check before dereferencing
                    Ok(format!(
                        "({{ long long _fap = {}; if (_fap == 0) {{ fprintf(stderr, \"Error: Null pointer when accessing field '{}' on '{}'\\n\"); exit(1); }} ((EpStruct_{}*)(_fap))->{}; }})",
                        obj_str, field_name, struct_name, struct_name, field_name
                    ))
                } else {
                    let var_name = if let ExprNode::Identifier(n) = &obj.node { n.clone() } else { "?".to_string() };
                    Err(format!("Field access '.{}' on non-struct variable '{}' (type: {:?}) at line {}:{}. Add a type annotation like 'with {} as StructName'",
                        field_name, var_name, obj_type, expr.span.line, expr.span.col, var_name))
                }
            }
            ExprNode::StructCreate(struct_name, fields) => {
                let c_name = format!("EpStruct_{}", struct_name);
                let mut lines = Vec::new();
                lines.push(format!("({{"));
                lines.push(format!("    {}* _s = ({}*)malloc(sizeof({}));", c_name, c_name, c_name));
                
                // Collect explicitly provided field names
                let provided: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
                
                // Set explicitly provided fields
                for (fname, fexpr) in fields {
                    let fval = self.gen_expr(fexpr, var_types)?;
                    lines.push(format!("    _s->{} = {};", fname, fval));
                }
                
                // Fill in missing fields with defaults from struct definition
                if let Some(sd) = self.struct_defs.get(struct_name).cloned() {
                    for (fname, ftype, default_expr) in &sd.fields {
                        if !provided.contains(&fname.as_str()) {
                            if let Some(def_expr) = default_expr {
                                let def_val = self.gen_expr(def_expr, var_types)?;
                                lines.push(format!("    _s->{} = {};", fname, def_val));
                            } else {
                                // Use type-appropriate zero value
                                let zero = match ftype {
                                    TypeAnnotation::Str | TypeAnnotation::DynStr => "(long long)\"\"".to_string(),
                                    TypeAnnotation::List => "create_list()".to_string(),
                                    _ => "0".to_string(),
                                };
                                lines.push(format!("    _s->{} = {};", fname, zero));
                            }
                        }
                    }
                }
                
                let total_fields = if let Some(sd) = self.struct_defs.get(struct_name) {
                    sd.fields.len()
                } else {
                    fields.len()
                };
                lines.push(format!("    {{ EpGCObject* _go = ep_gc_register(_s, EP_OBJ_STRUCT); if(_go) _go->num_fields = {}; }}", total_fields));
                lines.push(format!("    (long long)_s;"));
                lines.push(format!("}})")); 
                Ok(lines.join("\n"))
            }
            ExprNode::EnumCreate(enum_name, variant_name, args) => {
                let resolved_enum = if enum_name.is_empty() {
                    self.variant_to_enum.get(variant_name).cloned().unwrap_or_default()
                } else {
                    enum_name.clone()
                };
                let mut args_str = Vec::new();
                for arg in args {
                    args_str.push(self.gen_expr(arg, var_types)?);
                }
                Ok(format!("create_{}_{}({})", resolved_enum, variant_name, args_str.join(", ")))
            }
            ExprNode::MethodCall(obj, method_name, args) => {
                let obj_str = self.gen_expr(obj, var_types)?;
                let obj_type = self.infer_type(obj, var_types);
                let struct_name = match &obj_type {
                    Type::Struct(s) => s.clone(),
                    Type::Enum(e) => e.clone(),
                    _ => {
                        let var_name = if let ExprNode::Identifier(n) = &obj.node { n.clone() } else { "?".to_string() };
                        return Err(format!("Method '.{}()' called on non-struct/enum variable '{}' (type: {:?}) at line {}:{}",
                            method_name, var_name, obj_type, expr.span.line, expr.span.col));
                    }
                };
                let mut all_args = vec![obj_str];
                for arg in args {
                    all_args.push(self.gen_expr(arg, var_types)?);
                }
                Ok(format!("{}__{}({})", struct_name, method_name, all_args.join(", ")))
            }
            ExprNode::UnaryNot(inner) => {
                let inner_str = self.gen_expr(inner, var_types)?;
                Ok(format!("(!({}))", inner_str))
            }
            ExprNode::TryExpr(inner) => {
                let inner_str = self.gen_expr(inner, var_types)?;
                let try_id = self.spawn_index;
                self.spawn_index += 1;
                let inner_type = self.infer_type(inner, var_types);
                
                // If the inner expression returns an Enum (Result-style), do proper unwrapping
                if let Type::Enum(ref enum_name) = inner_type {
                    if let Some(ed) = self.enum_defs.get(enum_name).cloned() {
                        // Convention: first variant is the "success" variant (Ok),
                        // second variant is the "error" variant (Error) that gets propagated
                        if ed.variants.len() >= 2 {
                            return Ok(format!(
                                "({{ \
                                    long long _try_enum_{id} = {inner}; \
                                    EpEnum_{ename}* _try_ptr_{id} = (EpEnum_{ename}*)_try_enum_{id}; \
                                    if (_try_ptr_{id}->tag != EP_TAG_{ename}_{ok_variant}) {{ \
                                        ret_val = _try_enum_{id}; \
                                        goto L_cleanup; \
                                    }} \
                                    _try_ptr_{id}->data0; \
                                }})",
                                id = try_id,
                                inner = inner_str,
                                ename = enum_name,
                                ok_variant = ed.variants[0].0,
                            ));
                        }
                    }
                }
                
                // Fallback: non-enum try — use setjmp/longjmp crash guard
                let zero_val = match inner_type {
                    Type::Str | Type::DynStr | Type::RefStr => "(long long)\"\"",
                    _ => "0",
                };
                Ok(format!(
                    "({{ volatile long long _try_r_{id} = {zero}; \
                    if (setjmp(ep_try_buf) == 0) {{ \
                        ep_try_active = 1; \
                        _try_r_{id} = {inner}; \
                        ep_try_active = 0; \
                    }} else {{ \
                        _try_r_{id} = {zero}; \
                    }} \
                    _try_r_{id}; }})",
                    id = try_id,
                    inner = inner_str,
                    zero = zero_val
                ))
            }
            ExprNode::Await(inner) => {
                if self.is_async_func {
                    self.await_counter += 1;
                    Ok(format!("(args->awaited_fut_{} ? args->awaited_fut_{}->value : 0)", self.await_counter, self.await_counter))
                } else {
                    let inner_str = self.gen_expr(inner, var_types)?;
                    Ok(format!("ep_await_future((EpFuture*){})", inner_str))
                }
            }
            ExprNode::Closure(params, body) => {
                // Generate a static closure function
                // Use pre-registered name if this closure is being assigned to a named variable
                let closure_name = if let Some(var_name) = self.pending_closure_name.take() {
                    self.closure_c_names.get(&var_name)
                        .cloned()
                        .unwrap_or_else(|| {
                            let name = format!("_ep_closure_{}", self.spawn_index);
                            self.spawn_index += 1;
                            name
                        })
                } else {
                    let name = format!("_ep_closure_{}", self.spawn_index);
                    self.spawn_index += 1;
                    name
                };

                // Detect captured variables: identifiers in body that exist in outer
                // var_types but are NOT closure parameters and NOT known functions
                let param_set: std::collections::HashSet<&String> = params.iter().collect();
                let mut captured: Vec<String> = Vec::new();
                let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
                Self::collect_identifiers_in_stmts(body, &mut |ident| {
                    if !param_set.contains(ident)
                        && !seen.contains(ident)
                        && var_types.contains_key(ident)
                        && !self.func_return_types.contains_key(ident)
                        && ident != "ret_val"
                    {
                        captured.push(ident.clone());
                        seen.insert(ident.clone());
                    }
                });

                // Build parameter list: _ep_env (first), then explicit params
                let mut c_params: Vec<String> = vec!["long long _ep_env".to_string()];
                for p in params.iter() {
                    c_params.push(format!("long long {}", p));
                }
                let c_param_str = c_params.join(", ");

                let n_captures = captured.len();

                // Store captures for this closure so call sites can pass them
                self.closure_captures.insert(closure_name.clone(), captured.clone());

                // Generate the closure body using buffer swapping
                // Start with a COPY of outer scope's var_types so closures can reference
                // outer variables (including other closures)
                let mut closure_var_types = var_types.clone();
                for p in params {
                    closure_var_types.insert(p.clone(), Type::Int);
                }

                // Generate closure body into a fresh buffer
                let saved_out = std::mem::take(&mut self.out);

                self.out.push_str(&format!("long long {}({}) {{\n", closure_name, c_param_str));
                self.out.push_str("    long long ret_val = 0;\n");

                // Unpack captured variables from _ep_env array
                for (i, cap) in captured.iter().enumerate() {
                    let safe_cap = Self::sanitize_c_name(cap);
                    self.out.push_str(&format!("    long long {} = ((long long*)_ep_env)[{}];\n", safe_cap, i));
                }

                // Declare local variables in the closure body
                // (variables defined by set/for-each that aren't params or captures)
                let mut body_var_types = HashMap::new();
                self.collect_var_types(body, &mut body_var_types);
                for (var_name, _) in &body_var_types {
                    let safe = Self::sanitize_c_name(var_name);
                    let is_param = params.iter().any(|p| p == var_name);
                    let is_capture = captured.iter().any(|c| c == var_name);
                    if !is_param && !is_capture && var_name != "ret_val" {
                        self.out.push_str(&format!("    long long {} = 0;\n", safe));
                    }
                }

                for stmt in body {
                    self.gen_statement(stmt, &closure_var_types)?;
                }
                self.out.push_str("L_cleanup:\n");
                self.out.push_str("    return ret_val;\n");
                self.out.push_str("}\n\n");

                // Insert closure function at the bodies marker (after runtime defs and forward decls)
                let closure_code = std::mem::take(&mut self.out);
                let bodies_marker = "/* EP_CLOSURE_BODIES */\n";
                if let Some(marker_pos) = saved_out.find(bodies_marker) {
                    let insert_pos = marker_pos + bodies_marker.len();
                    let (before, after) = saved_out.split_at(insert_pos);
                    self.out = format!("{}{}{}", before, closure_code, after);
                } else {
                    // Fallback: append closure before current content
                    self.out = closure_code + &saved_out;
                }

                // Return an EpClosure struct allocation with captures packed in env[]
                let mut create_parts = Vec::new();
                create_parts.push(format!(
                    "({{ EpClosure* _cl_{0} = (EpClosure*)malloc(sizeof(EpClosure) + {1} * sizeof(long long)); _cl_{0}->magic = EP_CLOSURE_MAGIC; _cl_{0}->fn_ptr = (long long){2};",
                    self.spawn_index, n_captures, closure_name
                ));
                for (i, cap) in captured.iter().enumerate() {
                    create_parts.push(format!(" _cl_{}->env[{}] = {};", self.spawn_index, i, Self::sanitize_c_name(cap)));
                }
                create_parts.push(format!(" (long long)_cl_{}; }})", self.spawn_index));
                self.spawn_index += 1;
                Ok(create_parts.join(""))
            }
            ExprNode::ListLiteral(elements) => {
                // Generate: ({ long long _lit = create_list(); append_list(_lit, e1); ... _lit; })
                let mut parts = Vec::new();
                parts.push("({ long long _lit_list = create_list();".to_string());
                for elem in elements {
                    let elem_str = self.gen_expr(elem, var_types)?;
                    let elem_type = self.infer_type(elem, var_types);
                    let cast = match elem_type {
                        Type::Str | Type::DynStr => format!("(long long){}", elem_str),
                        _ => elem_str,
                    };
                    parts.push(format!(" append_list(_lit_list, {});", cast));
                }
                parts.push(" _lit_list; })".to_string());
                Ok(parts.join(""))
            }
        }
    }
}

// ========== Closure capture helpers ==========

impl Codegen {
    /// Walk statements and call `callback` with each identifier found
    fn collect_identifiers_in_stmts(stmts: &[Stmt], callback: &mut dyn FnMut(&String)) {
        for stmt in stmts {
            match &stmt.node {
                StmtNode::Set(_, expr, _) | StmtNode::ExprStmt(expr) | StmtNode::Return(expr) | StmtNode::Display(expr) => {
                    Self::collect_identifiers_in_expr(expr, callback);
                }
                StmtNode::FieldSet(obj, _, val) => {
                    Self::collect_identifiers_in_expr(obj, callback);
                    Self::collect_identifiers_in_expr(val, callback);
                }
                StmtNode::If(cond, then_b, else_b) => {
                    Self::collect_identifiers_in_expr(cond, callback);
                    Self::collect_identifiers_in_stmts(then_b, callback);
                    if let Some(eb) = else_b {
                        Self::collect_identifiers_in_stmts(eb, callback);
                    }
                }
                StmtNode::RepeatWhile(cond, body) => {
                    Self::collect_identifiers_in_expr(cond, callback);
                    Self::collect_identifiers_in_stmts(body, callback);
                }
                StmtNode::ForEach(_, iter, body) => {
                    Self::collect_identifiers_in_expr(iter, callback);
                    Self::collect_identifiers_in_stmts(body, callback);
                }
                StmtNode::Match(expr, arms) => {
                    Self::collect_identifiers_in_expr(expr, callback);
                    for (_, _, body) in arms {
                        Self::collect_identifiers_in_stmts(body, callback);
                    }
                }
                StmtNode::Spawn(_, args) => {
                    for a in args { Self::collect_identifiers_in_expr(a, callback); }
                }
                StmtNode::Send(a, b) => {
                    Self::collect_identifiers_in_expr(a, callback);
                    Self::collect_identifiers_in_expr(b, callback);
                }
                StmtNode::Break | StmtNode::Continue => {}
            }
        }
    }

    /// Walk an expression and call `callback` with each identifier found
    fn collect_identifiers_in_expr(expr: &Expr, callback: &mut dyn FnMut(&String)) {
        match &expr.node {
            ExprNode::Identifier(name) => callback(name),
            ExprNode::Binary(l, _, r) | ExprNode::Comparison(l, _, r) | ExprNode::Logical(l, _, r) => {
                Self::collect_identifiers_in_expr(l, callback);
                Self::collect_identifiers_in_expr(r, callback);
            }
            ExprNode::Call(_, args) | ExprNode::EnumCreate(_, _, args) => {
                for a in args { Self::collect_identifiers_in_expr(a, callback); }
            }
            ExprNode::UnaryNot(inner) | ExprNode::TryExpr(inner) | ExprNode::Await(inner)
            | ExprNode::Borrow(inner) | ExprNode::Receive(inner) => {
                Self::collect_identifiers_in_expr(inner, callback);
            }
            ExprNode::FieldAccess(obj, _) => {
                Self::collect_identifiers_in_expr(obj, callback);
            }
            ExprNode::MethodCall(obj, _, args) => {
                Self::collect_identifiers_in_expr(obj, callback);
                for a in args { Self::collect_identifiers_in_expr(a, callback); }
            }
            ExprNode::StructCreate(_, field_inits) => {
                for (_, fexpr) in field_inits { Self::collect_identifiers_in_expr(fexpr, callback); }
            }
            ExprNode::ListLiteral(elems) => {
                for e in elems { Self::collect_identifiers_in_expr(e, callback); }
            }
            ExprNode::Closure(_, body) => {
                Self::collect_identifiers_in_stmts(body, callback);
            }
            _ => {} // Integer, Float, Bool, String literals, Channel
        }
    }
}

// ========== analyze_safety, generate, gen_function ==========

impl Codegen {
    fn analyze_safety(&mut self, program: &Program) -> Result<(), String> {
        for func in &program.functions {
            let mut var_types = HashMap::new();
            for param in &func.params {
                let param_type = if let Some(ref ann) = param.2 {
                    self.type_annotation_to_type(ann)
                } else if param.1 {
                    Type::RefList
                } else {
                    Type::Int
                };
                var_types.insert(param.0.clone(), param_type);
            }
            self.collect_var_types(&func.body, &mut var_types);

            let mut owner_states = HashMap::new();
            for param in &func.params {
                let t = var_types.get(&param.0).cloned().unwrap_or(Type::Int);
                if is_tracked(&t) {
                    owner_states.insert(param.0.clone(), OwnerState::Owned);
                }
            }

            let mut borrows = HashMap::new();
            let mut borrow_counts = HashMap::new();

            self.check_safety_stmts(
                func,
                &func.body,
                &var_types,
                &mut owner_states,
                &mut borrows,
                &mut borrow_counts,
            )?;
        }
        Ok(())
    }

    fn collect_spawns_in_stmts(&self, stmts: &[Stmt], spawn_list: &mut Vec<Stmt>) {
        for stmt in stmts {
            match &stmt.node {
                StmtNode::Spawn(_, _) => spawn_list.push(stmt.clone()),
                StmtNode::If(_, then_branch, else_branch) => {
                    self.collect_spawns_in_stmts(then_branch, spawn_list);
                    if let Some(eb) = else_branch {
                        self.collect_spawns_in_stmts(eb, spawn_list);
                    }
                }
                StmtNode::RepeatWhile(_, body) => {
                    self.collect_spawns_in_stmts(body, spawn_list);
                }
                StmtNode::ForEach(_, _, body) => {
                    self.collect_spawns_in_stmts(body, spawn_list);
                }
                StmtNode::Match(_, arms) => {
                    for (_, _, body) in arms {
                        self.collect_spawns_in_stmts(body, spawn_list);
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_all_spawns(&self, program: &Program) -> Vec<Stmt> {
        let mut spawn_list = Vec::new();
        for func in &program.functions {
            self.collect_spawns_in_stmts(&func.body, &mut spawn_list);
        }
        spawn_list
    }

    fn get_c_test_main_source(&self, program: &Program) -> String {
        let mut test_cases = Vec::new();
        for func in &program.functions {
            if func.name.starts_with("test_") {
                test_cases.push(func.name.clone());
            }
        }
        
        let test_count = test_cases.len();
        let mut lines = Vec::new();
        lines.push("\n/* Test runner C main */\n".to_string());
        lines.push("#ifndef __wasm__\n".to_string());
        lines.push("#include <sys/types.h>\n".to_string());
        lines.push("#include <sys/wait.h>\n".to_string());
        lines.push("#include <unistd.h>\n".to_string());
        lines.push("#endif\n".to_string());
        lines.push("#include <stdio.h>\n".to_string());
        lines.push("#include <stdlib.h>\n".to_string());
        lines.push("#include <stdint.h>\n\n".to_string());
        
        lines.push("#ifdef __wasm__\n".to_string());
        lines.push("int run_test(long long (*test_func)(void), const char* name) {\n".to_string());
        lines.push("    printf(\"test_%s ... \", name);\n".to_string());
        lines.push("    fflush(stdout);\n".to_string());
        lines.push("    long long res = test_func();\n".to_string());
        lines.push("    if (res == 0) {\n".to_string());
        lines.push("        printf(\"OK\\n\");\n".to_string());
        lines.push("        return 1;\n".to_string());
        lines.push("    } else {\n".to_string());
        lines.push("        printf(\"FAILED\\n\");\n".to_string());
        lines.push("        return 0;\n".to_string());
        lines.push("    }\n".to_string());
        lines.push("}\n".to_string());
        lines.push("#else\n".to_string());
        lines.push("int run_test(long long (*test_func)(void), const char* name) {\n".to_string());
        lines.push("    printf(\"test_%s ... \", name);\n".to_string());
        lines.push("    fflush(stdout);\n".to_string());
        lines.push("    pid_t pid = fork();\n".to_string());
        lines.push("    if (pid < 0) {\n".to_string());
        lines.push("        printf(\"FAILED (fork failed)\\n\");\n".to_string());
        lines.push("        return 0;\n".to_string());
        lines.push("    }\n".to_string());
        lines.push("    if (pid == 0) {\n".to_string());
        lines.push("        exit((int)test_func());\n".to_string());
        lines.push("    } else {\n".to_string());
        lines.push("        int status;\n".to_string());
        lines.push("        waitpid(pid, &status, 0);\n".to_string());
        lines.push("        if (WIFEXITED(status)) {\n".to_string());
        lines.push("            int exit_code = WEXITSTATUS(status);\n".to_string());
        lines.push("            if (exit_code == 0) {\n".to_string());
        lines.push("                printf(\"OK\\n\");\n".to_string());
        lines.push("                return 1;\n".to_string());
        lines.push("            } else {\n".to_string());
        lines.push("                printf(\"FAILED (exit code %d)\\n\", exit_code);\n".to_string());
        lines.push("                return 0;\n".to_string());
        lines.push("            }\n".to_string());
        lines.push("        } else if (WIFSIGNALED(status)) {\n".to_string());
        lines.push("            int sig = WTERMSIG(status);\n".to_string());
        lines.push("            printf(\"FAILED (crashed/signal %d)\\n\", sig);\n".to_string());
        lines.push("            return 0;\n".to_string());
        lines.push("        } else {\n".to_string());
        lines.push("            printf(\"FAILED\\n\");\n".to_string());
        lines.push("            return 0;\n".to_string());
        lines.push("        }\n".to_string());
        lines.push("    }\n".to_string());
        lines.push("}\n".to_string());
        lines.push("#endif\n\n".to_string());
        
        lines.push("int main(int argc, char** argv) {\n".to_string());
        lines.push("    init_ep_args(argc, argv);\n".to_string());
        lines.push(format!("    printf(\"Running {} tests...\\n\");\n", test_count));
        lines.push("    int passed = 0;\n".to_string());
        lines.push("    int failed = 0;\n".to_string());
        lines.push("    int total = 0;\n\n".to_string());
        
        for name in &test_cases {
            lines.push("    total++;\n".to_string());
            lines.push(format!("    if (run_test((long long (*)(void)){}, \"{}\")) passed++; else failed++;\n", name, name));
        }
        
        lines.push("\n    printf(\"\\nResult: %d passed; %d failed\\n\", passed, failed);\n".to_string());
        lines.push("    if (failed > 0) return 1;\n".to_string());
        lines.push("    return 0;\n".to_string());
        lines.push("}\n".to_string());
        
        lines.concat()
    }

    /// Emit a standalone C file containing ONLY the runtime and program-specific
    /// struct/enum definitions. No user function code, no main wrapper.
    /// Used by the native backend: the runtime compiles to a .o that links
    /// with the native assembly .o to produce a complete binary.
    pub fn emit_runtime_c(&mut self, program: &Program) -> String {
        let mut out = String::new();

        // Register struct/enum definitions (needed for struct/enum codegen)
        for sd in &program.struct_defs {
            self.struct_defs.insert(sd.name.clone(), sd.clone());
        }
        for ed in &program.enum_defs {
            self.enum_defs.insert(ed.name.clone(), ed.clone());
            for (variant_name, _) in &ed.variants {
                self.variant_to_enum.insert(variant_name.clone(), ed.name.clone());
            }
        }

        // 1. The full C runtime header + implementation
        out.push_str(RUNTIME_HEADER_AND_SRC);

        // 2. User-defined struct typedefs and free functions
        if !program.struct_defs.is_empty() {
            out.push_str("\n/* User-Defined Structures */\n");
            for sd in &program.struct_defs {
                out.push_str(&format!("typedef struct {{\n"));
                for (fname, _ftype, _) in &sd.fields {
                    out.push_str(&format!("    long long {};\n", fname));
                }
                out.push_str(&format!("}} EpStruct_{};\n\n", sd.name));

                out.push_str(&format!("void free_struct_{}(long long ptr) {{\n", sd.name));
                out.push_str("    if (ptr == 0) return;\n");
                out.push_str("    if (!ep_gc_find((void*)ptr)) return;\n");
                out.push_str(&format!("    EpStruct_{}* s = (EpStruct_{}*)ptr;\n", sd.name, sd.name));
                for (fname, ftype, _) in &sd.fields {
                    match ftype {
                        TypeAnnotation::List => {
                            out.push_str(&format!("    free_list(s->{});\n", fname));
                        }
                        TypeAnnotation::DynStr => {
                            out.push_str(&format!("    if (s->{}) free((void*)s->{});\n", fname, fname));
                        }
                        TypeAnnotation::UserDefined(inner_name) => {
                            out.push_str(&format!("    free_struct_{}(s->{});\n", inner_name, fname));
                        }
                        TypeAnnotation::Generic(inner_name, _) => {
                            out.push_str(&format!("    free_struct_{}(s->{});\n", inner_name, fname));
                        }
                        _ => {}
                    }
                }
                out.push_str("    ep_gc_unregister(s);\n");
                out.push_str("    free(s);\n");
                out.push_str("}\n\n");
            }
        }

        // 3. User-defined enum typedefs, constructors, and free functions
        if !program.enum_defs.is_empty() {
            out.push_str("\n/* User-Defined Choices (Enums) */\n");
            for ed in &program.enum_defs {
                for (i, (vname, _)) in ed.variants.iter().enumerate() {
                    out.push_str(&format!("#define EP_TAG_{}_{} {}\n", ed.name, vname, i));
                }
                out.push_str("\n");

                let max_fields = ed.variants.iter().map(|(_, fields)| fields.len()).max().unwrap_or(0);
                out.push_str("typedef struct {\n");
                out.push_str("    long long tag;\n");
                for j in 0..max_fields {
                    out.push_str(&format!("    long long data{};\n", j));
                }
                out.push_str(&format!("}} EpEnum_{};\n\n", ed.name));

                out.push_str(&format!("void free_enum_{}(long long ptr) {{\n", ed.name));
                out.push_str("    if (ptr == 0) return;\n");
                out.push_str("    if (!ep_gc_find((void*)ptr)) return;\n");
                out.push_str("    ep_gc_unregister((void*)ptr);\n");
                out.push_str("    free((void*)ptr);\n");
                out.push_str("}\n\n");

                for (_i, (vname, fields)) in ed.variants.iter().enumerate() {
                    let mut params = Vec::new();
                    for (j, _) in fields.iter().enumerate() {
                        params.push(format!("long long arg{}", j));
                    }
                    let params_str = if params.is_empty() { "void".to_string() } else { params.join(", ") };
                    out.push_str(&format!("long long create_{}_{}({}) {{\n", ed.name, vname, params_str));
                    out.push_str(&format!("    EpEnum_{}* e = (EpEnum_{}*)malloc(sizeof(EpEnum_{}));\n", ed.name, ed.name, ed.name));
                    out.push_str(&format!("    e->tag = EP_TAG_{}_{};\n", ed.name, vname));
                    for (j, _) in fields.iter().enumerate() {
                        out.push_str(&format!("    e->data{} = arg{};\n", j, j));
                    }
                    out.push_str("    ep_gc_register(e, EP_OBJ_CLOSURE);\n");
                    out.push_str("    return (long long)e;\n");
                    out.push_str("}\n\n");
                }

                out.push_str(&format!("const char* display_enum_{}(long long ptr) {{\n", ed.name));
                out.push_str("    if (ptr == 0) return \"(null)\";\n");
                out.push_str(&format!("    EpEnum_{}* e = (EpEnum_{}*)ptr;\n", ed.name, ed.name));
                for (i, (vname, _)) in ed.variants.iter().enumerate() {
                    out.push_str(&format!("    if (e->tag == {}) return \"{}\";\n", i, vname));
                }
                out.push_str("    return \"(unknown)\";\n");
                out.push_str("}\n\n");
            }
        }

        // 4. Builtin helper functions that are emitted inline by generate()
        out.push_str("\n/* Built-in: string concatenation */\n");
        out.push_str("long long concat(long long a, long long b) {\n");
        out.push_str("    const char* sa = (const char*)a;\n");
        out.push_str("    const char* sb = (const char*)b;\n");
        out.push_str("    long long la = strlen(sa);\n");
        out.push_str("    long long lb = strlen(sb);\n");
        out.push_str("    char* result = malloc(la + lb + 1);\n");
        out.push_str("    memcpy(result, sa, la);\n");
        out.push_str("    memcpy(result + la, sb, lb);\n");
        out.push_str("    result[la + lb] = '\\0';\n");
        out.push_str("    ep_gc_register(result, EP_OBJ_STRING);\n");
        out.push_str("    return (long long)result;\n");
        out.push_str("}\n\n");

        out.push_str("long long int_to_string(long long val) {\n");
        out.push_str("    char* buf = malloc(32);\n");
        out.push_str("    snprintf(buf, 32, \"%lld\", val);\n");
        out.push_str("    ep_gc_register(buf, EP_OBJ_STRING);\n");
        out.push_str("    return (long long)buf;\n");
        out.push_str("}\n\n");

        // ep_int_to_str alias
        out.push_str("long long ep_int_to_str(long long val) { return int_to_string(val); }\n\n");

        // FFI pointer/byte builtins
        out.push_str("long long str_to_ptr(long long s) { return s; }\n");
        out.push_str("long long ptr_to_str(long long p) {\n");
        out.push_str("    if (p == 0) return (long long)strdup(\"\");\n");
        out.push_str("    char* copy = strdup((const char*)p);\n");
        out.push_str("    ep_gc_register(copy, EP_OBJ_STRING);\n");
        out.push_str("    return (long long)copy;\n");
        out.push_str("}\n\n");
        out.push_str("long long peek_byte(long long ptr, long long offset) {\n");
        out.push_str("    return (long long)((unsigned char*)ptr)[offset];\n");
        out.push_str("}\n");
        out.push_str("long long poke_byte(long long ptr, long long offset, long long value) {\n");
        out.push_str("    ((unsigned char*)ptr)[offset] = (unsigned char)value;\n");
        out.push_str("    return 0;\n");
        out.push_str("}\n");
        out.push_str("long long alloc_bytes(long long size) {\n");
        out.push_str("    return (long long)calloc((size_t)size, 1);\n");
        out.push_str("}\n");
        out.push_str("long long free_bytes(long long ptr) {\n");
        out.push_str("    free((void*)ptr);\n");
        out.push_str("    return 0;\n");
        out.push_str("}\n");
        out.push_str("long long list_to_bytes(long long list_ptr) {\n");
        out.push_str("    long long len = length_list(list_ptr);\n");
        out.push_str("    unsigned char* buf = (unsigned char*)malloc(len);\n");
        out.push_str("    for (long long i = 0; i < len; i++) {\n");
        out.push_str("        buf[i] = (unsigned char)get_list(list_ptr, i);\n");
        out.push_str("    }\n");
        out.push_str("    return (long long)buf;\n");
        out.push_str("}\n");
        out.push_str("long long bytes_to_list(long long ptr, long long len) {\n");
        out.push_str("    long long list = create_list();\n");
        out.push_str("    unsigned char* buf = (unsigned char*)ptr;\n");
        out.push_str("    for (long long i = 0; i < len; i++) {\n");
        out.push_str("        append_list(list, (long long)buf[i]);\n");
        out.push_str("    }\n");
        out.push_str("    return list;\n");
        out.push_str("}\n\n");
        out.push_str("long long ep_gc_get_minor_count() {\n");
        out.push_str("    return ep_gc_minor_count;\n");
        out.push_str("}\n");
        out.push_str("long long ep_gc_get_major_count() {\n");
        out.push_str("    return ep_gc_major_count;\n");
        out.push_str("}\n");
        out.push_str("long long ep_gc_get_nursery_count() {\n");
        out.push_str("    return ep_gc_nursery_count;\n");
        out.push_str("}\n\n");

        out.push_str("long long string_to_int(long long s) {\n");
        out.push_str("    if (s == 0) return 0;\n");
        out.push_str("    return atoll((const char*)s);\n");
        out.push_str("}\n\n");

        out.push_str("long long read_line() {\n");
        out.push_str("    char buf[4096];\n");
        out.push_str("    if (fgets(buf, sizeof(buf), stdin) == NULL) { buf[0] = '\\0'; }\n");
        out.push_str("    size_t len = strlen(buf);\n");
        out.push_str("    if (len > 0 && buf[len-1] == '\\n') buf[len-1] = '\\0';\n");
        out.push_str("    char* result = strdup(buf);\n");
        out.push_str("    ep_gc_register(result, EP_OBJ_STRING);\n");
        out.push_str("    return (long long)result;\n");
        out.push_str("}\n\n");

        out.push_str("long long read_int() {\n");
        out.push_str("    long long val = 0;\n");
        out.push_str("    scanf(\"%lld\", &val);\n");
        out.push_str("    while(getchar() != '\\n');\n");
        out.push_str("    return val;\n");
        out.push_str("}\n\n");

        out
    }

    pub fn generate(&mut self, program: &Program) -> Result<String, String> {
        self.out.clear();

        // Register struct definitions
        for sd in &program.struct_defs {
            self.struct_defs.insert(sd.name.clone(), sd.clone());
        }

        // Register enum definitions and build variant lookup
        for ed in &program.enum_defs {
            self.enum_defs.insert(ed.name.clone(), ed.clone());
            for (variant_name, _) in &ed.variants {
                self.variant_to_enum.insert(variant_name.clone(), ed.name.clone());
            }
        }

        // Register trait implementations
        for ti in &program.trait_impls {
            self.trait_impls.insert((ti.trait_name.clone(), ti.for_type.clone()));
        }

        self.analyze_return_types(program);
        self.analyze_safety(program)?;

        // Register top-level constant names as globals
        for stmt in &program.top_level_constants {
            if let StmtNode::Set(name, _, _) = &stmt.node {
                self.global_constants.insert(name.clone());
            }
        }

        // Register method return types
        for md in &program.method_defs {
            let key = format!("{}_{}", md.struct_name, md.name);
            if let Some(ref rt) = md.return_type {
                self.func_return_types.insert(key, self.type_annotation_to_type(rt));
            }
        }
        for ti in &program.trait_impls {
            for m in &ti.methods {
                let key = format!("{}_{}", ti.for_type, m.name);
                if let Some(ref rt) = m.return_type {
                    self.func_return_types.insert(key, self.type_annotation_to_type(rt));
                }
            }
        }

        // Write C Runtime
        self.out.push_str(RUNTIME_HEADER_AND_SRC);

        // Emit C struct typedefs
        if !program.struct_defs.is_empty() {
            self.out.push_str("\n/* User-Defined Structures */\n");
            for sd in &program.struct_defs {
                self.out.push_str(&format!("typedef struct {{\n"));
                for (fname, ftype, _) in &sd.fields {
                    let c_type = match ftype {
                        TypeAnnotation::Int => "long long",
                        TypeAnnotation::Float => "long long",
                        TypeAnnotation::Bool => "long long",
                        TypeAnnotation::Str => "long long",
                        TypeAnnotation::DynStr => "long long",
                        TypeAnnotation::List => "long long",
                        TypeAnnotation::UserDefined(_) => "long long",
                        TypeAnnotation::Generic(_, _) => "long long",
                    };
                    self.out.push_str(&format!("    {} {};\n", c_type, fname));
                }
                self.out.push_str(&format!("}} EpStruct_{};\n\n", sd.name));

                self.out.push_str(&format!("void free_struct_{}(long long ptr) {{\n", sd.name));
                self.out.push_str(&format!("    if (ptr == 0) return;\n"));
                self.out.push_str("    /* Skip if already freed (idempotent — prevents double-free with shared refs) */\n");
                self.out.push_str("    if (!ep_gc_find((void*)ptr)) return;\n");
                self.out.push_str(&format!("    EpStruct_{}* s = (EpStruct_{}*)ptr;\n", sd.name, sd.name));
                for (fname, ftype, _) in &sd.fields {
                    match ftype {
                        TypeAnnotation::List => {
                            self.out.push_str(&format!("    free_list(s->{});\n", fname));
                        }
                        TypeAnnotation::DynStr => {
                            self.out.push_str(&format!("    if (s->{}) free((void*)s->{});\n", fname, fname));
                        }
                        TypeAnnotation::UserDefined(inner_name) => {
                            if inner_name != "Any" {
                                if self.enum_defs.contains_key(inner_name) {
                                    self.out.push_str(&format!("    free_enum_{}(s->{});\n", inner_name, fname));
                                } else if self.struct_defs.contains_key(inner_name) {
                                    self.out.push_str(&format!("    free_struct_{}(s->{});\n", inner_name, fname));
                                }
                            }
                        }
                        TypeAnnotation::Generic(inner_name, _) => {
                            if inner_name != "Any" {
                                if self.enum_defs.contains_key(inner_name) {
                                    self.out.push_str(&format!("    free_enum_{}(s->{});\n", inner_name, fname));
                                } else if self.struct_defs.contains_key(inner_name) {
                                    self.out.push_str(&format!("    free_struct_{}(s->{});\n", inner_name, fname));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                self.out.push_str("    ep_gc_unregister(s);\n");
                self.out.push_str("    free(s);\n");
                self.out.push_str("}\n\n");
            }
        }

        // Emit C tagged-union structs for enums
        if !program.enum_defs.is_empty() {
            self.out.push_str("\n/* User-Defined Choices (Enums) */\n");
            for ed in &program.enum_defs {
                for (i, (vname, _)) in ed.variants.iter().enumerate() {
                    self.out.push_str(&format!("#define EP_TAG_{}_{} {}\n", ed.name, vname, i));
                }
                self.out.push_str("\n");

                let max_fields = ed.variants.iter().map(|(_, fields)| fields.len()).max().unwrap_or(0);
                self.out.push_str(&format!("typedef struct {{\n"));
                self.out.push_str("    long long tag;\n");
                for j in 0..max_fields {
                    self.out.push_str(&format!("    long long data{};\n", j));
                }
                self.out.push_str(&format!("}} EpEnum_{};\n\n", ed.name));

                self.out.push_str(&format!("void free_enum_{}(long long ptr) {{\n", ed.name));
                self.out.push_str("    if (ptr == 0) return;\n");
                self.out.push_str("    /* Skip if already freed (idempotent — prevents double-free in recursive enums) */\n");
                self.out.push_str("    if (!ep_gc_find((void*)ptr)) return;\n");
                self.out.push_str("    ep_gc_unregister((void*)ptr);\n");
                self.out.push_str(&format!("    EpEnum_{}* e = (EpEnum_{}*)ptr;\n", ed.name, ed.name));
                for (_i, (vname, fields)) in ed.variants.iter().enumerate() {
                    if fields.iter().any(|(_, ft)| matches!(ft, TypeAnnotation::List | TypeAnnotation::DynStr | TypeAnnotation::UserDefined(_) | TypeAnnotation::Generic(_, _))) {
                        self.out.push_str(&format!("    if (e->tag == EP_TAG_{}_{}) {{\n", ed.name, vname));
                        for (j, (_fname, ftype)) in fields.iter().enumerate() {
                            match ftype {
                                TypeAnnotation::List => {
                                    self.out.push_str(&format!("        free_list(e->data{});\n", j));
                                }
                                TypeAnnotation::DynStr => {
                                    self.out.push_str(&format!("        if (e->data{}) free((void*)e->data{});\n", j, j));
                                }
                                TypeAnnotation::UserDefined(inner_name) => {
                                    if inner_name != "Any" {
                                        if self.enum_defs.contains_key(inner_name) {
                                            self.out.push_str(&format!("        free_enum_{}(e->data{});\n", inner_name, j));
                                        } else if self.struct_defs.contains_key(inner_name) {
                                            self.out.push_str(&format!("        free_struct_{}(e->data{});\n", inner_name, j));
                                        }
                                    }
                                }
                                TypeAnnotation::Generic(inner_name, _) => {
                                    if inner_name != "Any" {
                                        if self.enum_defs.contains_key(inner_name) {
                                            self.out.push_str(&format!("        free_enum_{}(e->data{});\n", inner_name, j));
                                        } else if self.struct_defs.contains_key(inner_name) {
                                            self.out.push_str(&format!("        free_struct_{}(e->data{});\n", inner_name, j));
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        self.out.push_str("    }\n");
                    }
                }
                self.out.push_str("    free(e);\n");
                self.out.push_str("}\n\n");

                for (_i, (vname, fields)) in ed.variants.iter().enumerate() {
                    let mut params = Vec::new();
                    for (j, _) in fields.iter().enumerate() {
                        params.push(format!("long long arg{}", j));
                    }
                    let params_str = if params.is_empty() { "void".to_string() } else { params.join(", ") };
                    self.out.push_str(&format!("long long create_{}_{}({}) {{\n", ed.name, vname, params_str));
                    self.out.push_str(&format!("    EpEnum_{}* e = (EpEnum_{}*)malloc(sizeof(EpEnum_{}));\n", ed.name, ed.name, ed.name));
                    self.out.push_str(&format!("    e->tag = EP_TAG_{}_{};
", ed.name, vname));
                    for (j, _) in fields.iter().enumerate() {
                        self.out.push_str(&format!("    e->data{} = arg{};\n", j, j));
                    }
                    self.out.push_str("    ep_gc_register(e, EP_OBJ_CLOSURE);\n"); // reuse CLOSURE kind for enums
                    self.out.push_str("    return (long long)e;\n");
                    self.out.push_str("}\n\n");
                }

                // Generate display helper: maps tag to variant name string
                self.out.push_str(&format!("const char* display_enum_{}(long long ptr) {{\n", ed.name));
                self.out.push_str(&format!("    if (ptr == 0) return \"(null)\";\n"));
                self.out.push_str(&format!("    EpEnum_{}* e = (EpEnum_{}*)ptr;\n", ed.name, ed.name));
                for (i, (vname, _)) in ed.variants.iter().enumerate() {
                    self.out.push_str(&format!("    if (e->tag == {}) return \"{}\";\n", i, vname));
                }
                self.out.push_str("    return \"(unknown)\";\n");
                self.out.push_str("}\n\n");
            }
        }

        // Static builtin layer shared with the self-hosted compiler — single
        // source of truth in runtime/ep_builtins.c (see runtime/ep_runtime.c note).
        self.out.push_str(EP_BUILTINS_SRC);

        self.out.push_str("\n/* External Function Prototypes (FFI) */\n");
        for ext in &program.externals {
            // Skip forward declarations for functions already in the C runtime
            if self.builtin_c_funcs.contains(&ext.name) {
                continue;
            }
            let mut params_str = Vec::new();
            for _ in &ext.params {
                params_str.push("long long");
            }
            self.out.push_str(&format!("long long {}({});\n", ext.name, params_str.join(", ")));
        }
        self.out.push_str("\n");

        // Declare top-level constant globals BEFORE function prototypes/definitions
        // so bridge constants (e.g. SDL_HANDLE) are visible to all functions
        if !program.top_level_constants.is_empty() {
            self.out.push_str("\n/* Top-Level Constant Globals (Bridge Libraries) */\n");
            for stmt in &program.top_level_constants {
                if let StmtNode::Set(name, _, _) = &stmt.node {
                    self.out.push_str(&format!("long long {} = 0;\n", name));
                }
            }
            self.out.push_str("\n");
        }

        self.out.push_str("\n/* User Function Prototypes */\n");
        for func in &program.functions {
            // Skip prototypes for functions that shadow C runtime builtins
            if self.builtin_c_funcs.contains(&func.name) {
                continue;
            }
            let mut params_str = Vec::new();
            for _ in &func.params {
                params_str.push("long long");
            }
            let name = if func.name == "main" { "_main".to_string() } else { Self::sanitize_c_name(&func.name) };
            self.out.push_str(&format!("long long {}({});\n", name, params_str.join(", ")));
            if func.is_async {
                self.out.push_str(&format!("long long {}_impl({});\n", name, params_str.join(", ")));
                self.out.push_str(&format!("void* {}_async_wrapper(void* r);\n", name));
            }
        }
        self.out.push_str("\n");

        // Method prototypes
        for md in &program.method_defs {
            let mut params_str = vec!["long long".to_string()]; // self
            for _ in &md.params {
                params_str.push("long long".to_string());
            }
            self.out.push_str(&format!("long long {}__{}({});\n", md.struct_name, md.name, params_str.join(", ")));
        }
        for ti in &program.trait_impls {
            for m in &ti.methods {
                let mut params_str = vec!["long long".to_string()]; // self
                for _ in &m.params {
                    params_str.push("long long".to_string());
                }
                self.out.push_str(&format!("long long {}__{}({});\n", ti.for_type, m.name, params_str.join(", ")));
            }
        }
        self.out.push_str("\n");
        // Marker for closure forward declarations (inserted after codegen)
        self.out.push_str("/* EP_CLOSURE_FWD_DECLS */\n");
        // Marker for closure function bodies (inserted during codegen)
        self.out.push_str("/* EP_CLOSURE_BODIES */\n");

        let spawn_list = self.collect_all_spawns(program);
        self.out.push_str("\n/* Thread Spawn Wrappers */\n");
        for (idx, spawn_stmt) in spawn_list.iter().enumerate() {
            if let StmtNode::Spawn(func_name, args) = &spawn_stmt.node {
                self.out.push_str(&format!("typedef struct {{\n"));
                for j in 0..args.len() {
                    self.out.push_str(&format!("    long long arg{};\n", j));
                }
                if args.is_empty() {
                    self.out.push_str("    int dummy;\n");
                }
                self.out.push_str(&format!("}} spawn_args_{};\n\n", idx));

                let c_name = if func_name == "main" { "_main".to_string() } else { func_name.clone() };
                self.out.push_str(&format!("void* spawn_wrapper_{}(void* r) {{\n", idx));
                self.out.push_str("    int stack_dummy;\n");
                self.out.push_str("    ep_gc_register_thread(&stack_dummy);\n");
                self.out.push_str(&format!("    spawn_args_{}* args = (spawn_args_{}*)r;\n", idx, idx));
                
                let mut args_joined = Vec::new();
                for j in 0..args.len() {
                    args_joined.push(format!("args->arg{}", j));
                }
                self.out.push_str(&format!("    {}({});\n", c_name, args_joined.join(", ")));
                self.out.push_str("    free(args);\n");
                self.out.push_str("    ep_gc_unregister_thread();\n");
                self.out.push_str("    return NULL;\n");
                self.out.push_str("}\n\n");
            }
        }
        self.out.push_str("\n");

        self.spawn_index = 0;
        self.spawn_wrapper_index = 0;

        // Pre-scan all functions to collect closure names and emit forward declarations
        self.closure_c_names.clear();
        self.closure_captures.clear();
        for func in &program.functions {
            let mut var_types = HashMap::new();
            for param in &func.params {
                let t = if let Some(ann) = &param.2 {
                    self.type_annotation_to_type(ann)
                } else {
                    Type::Int
                };
                var_types.insert(param.0.clone(), t);
            }
            self.collect_var_types(&func.body, &mut var_types);
        }
        
        // Note: closure forward declarations are deferred until after gen_function
        // because capture analysis happens during codegen
        let closure_fwd_decls_placeholder = true; // marker for post-codegen generation

        for func in &program.functions {
            // Skip user-defined functions that shadow C runtime builtins
            if self.builtin_c_funcs.contains(&func.name) {
                continue;
            }
            self.gen_function(func)?;
        }

        // Generate method implementations
        for md in &program.method_defs {
            self.gen_method(md)?;
        }
        for ti in &program.trait_impls {
            for m in &ti.methods {
                let md = MethodDef {
                    name: m.name.clone(),
                    struct_name: ti.for_type.clone(),
                    params: m.params.clone(),
                    return_type: m.return_type.clone(),
                    body: m.body.clone(),
                    doc_comment: m.doc_comment.clone(),
                };
                self.gen_method(&md)?;
            }
        }

        if self.is_test_mode {
            self.out.push_str(&self.get_c_test_main_source(program));
        } else {
            // Emit top-level constant init function (globals already declared earlier)
            if !program.top_level_constants.is_empty() {
                // Generate GC marking functions for top-level globals. These are
                // roots that live outside any function frame, so the collector
                // must trace them or objects reachable only via a global get swept.
                let global_names: Vec<&String> = program.top_level_constants.iter()
                    .filter_map(|stmt| match &stmt.node {
                        StmtNode::Set(name, _, _) => Some(name),
                        _ => None,
                    })
                    .collect();
                self.out.push_str("\nstatic void __ep_mark_globals_major(void) {\n");
                for name in &global_names {
                    self.out.push_str(&format!("    if ({} != 0) ep_gc_mark_object((void*){});\n", name, name));
                }
                self.out.push_str("}\n");
                self.out.push_str("static void __ep_mark_globals_minor(void) {\n");
                for name in &global_names {
                    self.out.push_str(&format!("    if ({} != 0) ep_gc_mark_object_minor((void*){});\n", name, name));
                }
                self.out.push_str("}\n");

                // Generate init function
                self.out.push_str("\nvoid __ep_init_constants(void) {\n");
                self.out.push_str("    ep_gc_mark_globals_major = __ep_mark_globals_major;\n");
                self.out.push_str("    ep_gc_mark_globals_minor = __ep_mark_globals_minor;\n");
                for stmt in &program.top_level_constants {
                    if let StmtNode::Set(name, expr, _) = &stmt.node {
                        let empty_types: HashMap<String, Type> = HashMap::new();
                        let val = self.gen_expr(expr, &empty_types)?;
                        self.out.push_str(&format!("    {} = {};\n", name, val));
                    }
                }
                self.out.push_str("}\n\n");
                let main_is_async = program.functions.iter().any(|f| f.name == "main" && f.is_async);
                let main_call = if main_is_async {
                    "    EpFuture* fut = (EpFuture*)_main();\n    ep_async_loop_run();\n    int result = (int)(fut ? fut->value : 0);"
                } else {
                    "    int result = (int)_main();\n    ep_async_loop_run();"
                };
                self.out.push_str(&format!(r#"/* Bootstrapper C main */
int main(int argc, char** argv) {{
    {{
        unsigned int seed;
        FILE* urand = fopen("/dev/urandom", "rb");
        if (urand && fread(&seed, sizeof(seed), 1, urand) == 1) {{
            fclose(urand);
        }} else {{
            if (urand) fclose(urand);
            seed = (unsigned int)time(NULL) ^ (unsigned int)getpid();
        }}
        srand(seed);
    }}
    init_ep_args(argc, argv);
    __ep_init_constants();
{}
    ep_gc_shutdown();
    return result;
}}
"#, main_call));
            } else {
                let main_is_async = program.functions.iter().any(|f| f.name == "main" && f.is_async);
                let main_call = if main_is_async {
                    "    EpFuture* fut = (EpFuture*)_main();\n    ep_async_loop_run();\n    int result = (int)(fut ? fut->value : 0);"
                } else {
                    "    int result = (int)_main();\n    ep_async_loop_run();"
                };
                self.out.push_str(&format!(r#"
/* Bootstrapper C main */
int main(int argc, char** argv) {{
    {{
        unsigned int seed;
        FILE* urand = fopen("/dev/urandom", "rb");
        if (urand && fread(&seed, sizeof(seed), 1, urand) == 1) {{
            fclose(urand);
        }} else {{
            if (urand) fclose(urand);
            seed = (unsigned int)time(NULL) ^ (unsigned int)getpid();
        }}
        srand(seed);
    }}
    init_ep_args(argc, argv);
{}
    ep_gc_shutdown();
    return result;
}}
"#, main_call));
            }
        }

        // Generate closure forward declarations now that captures are known
        let _ = closure_fwd_decls_placeholder; // suppress unused warning
        if !self.closure_c_names.is_empty() {
            let mut closure_fwd_decls = String::new();
            closure_fwd_decls.push_str("\n/* Forward declarations for closures */\n");
            for func in &program.functions {
                for stmt in &func.body {
                    if let StmtNode::Set(name, expr, _) = &stmt.node {
                        if let ExprNode::Closure(params, _) = &expr.node {
                            if let Some(c_name) = self.closure_c_names.get(name) {
                                // Always: _ep_env + user params
                                let param_count = 1 + params.len();
                                let param_str = (0..param_count).map(|_| "long long").collect::<Vec<_>>().join(", ");
                                closure_fwd_decls.push_str(&format!("long long {}({});\n", c_name, param_str));
                            }
                        }
                    }
                }
            }
            closure_fwd_decls.push('\n');
            // Insert at marker position (after runtime defs, before user functions)
            self.out = self.out.replace("/* EP_CLOSURE_FWD_DECLS */\n", &closure_fwd_decls);
        } else {
            // Remove the markers if no closures
            self.out = self.out.replace("/* EP_CLOSURE_FWD_DECLS */\n", "");
        }
        // Clean up bodies marker (already consumed during codegen, but might remain)
        self.out = self.out.replace("/* EP_CLOSURE_BODIES */\n", "");

        Ok(self.out.clone())
    }

    fn gen_function(&mut self, func: &Function) -> Result<(), String> {
        let mut var_types = HashMap::new();

        /* Clear per-function state that would otherwise leak across functions.
           list_element_types: bare variable names like "votes" collide between
           functions, causing wrong for-each loop variable types.
           closure_c_names: a closure named "x" in function A would cause
           function B's call to a regular function "x" to be misrouted.
           We clear both and re-collect for THIS function only. */
        self.list_element_types.clear();
        self.closure_c_names.clear();

        for param in &func.params {
            let param_type = if let Some(ref ann) = param.2 {
                self.type_annotation_to_type(ann)
            } else if param.1 {
                Type::RefList
            } else {
                // Try to infer struct type from field access patterns
                self.infer_param_struct_type(&param.0, &func.body).unwrap_or(Type::Int)
            };
            var_types.insert(param.0.clone(), param_type);
        }
        /* collect_var_types populates list_element_types and closure_c_names
           for THIS function's body only (since we cleared them above). */
        self.collect_var_types(&func.body, &mut var_types);
        self.infer_param_types_from_usage(&func.body, &mut var_types);

        self.current_return_type = self.func_return_types.get(&func.name).cloned().unwrap_or(Type::Int);

        let name = if func.name == "main" {
            "_main".to_string()
        } else {
            Self::sanitize_c_name(&func.name)
        };
        
        let mut params_decl = Vec::new();
        for param in &func.params {
            params_decl.push(format!("long long {}", param.0));
        }

        if func.is_async {
            // 1. Generate the argument struct
            self.out.push_str(&format!("typedef struct {{\n"));
            self.out.push_str("    int state;\n");
            self.out.push_str("    EpFuture* fut;\n");
            // Parameters:
            for param in &func.params {
                self.out.push_str(&format!("    long long {};\n", Self::sanitize_c_name(&param.0)));
            }
            // Local variables:
            // Sorted iteration: HashMap order is nondeterministic and would make the
        // emitted C differ run-to-run (breaks reproducible builds + the
        // byte-identical parity gate between ernos and epc).
        for var_name in { let mut _sv: Vec<&String> = var_types.keys().collect(); _sv.sort(); _sv } {
                let is_param = func.params.iter().any(|p| &p.0 == var_name);
                let is_global = self.global_constants.contains(var_name);
                if !is_param && !is_global {
                    self.out.push_str(&format!("    long long {};\n", Self::sanitize_c_name(var_name)));
                }
            }
            // Awaited futures:
            let await_count = self.count_awaits_in_stmts(&func.body);
            for i in 1..=await_count {
                self.out.push_str(&format!("    EpFuture* awaited_fut_{};\n", i));
            }
            if func.params.is_empty() && var_types.len() == func.params.len() && await_count == 0 {
                self.out.push_str("    int dummy;\n");
            }
            self.out.push_str(&format!("}} {}_async_args;\n\n", name));

            // 2. Generate the step function
            self.out.push_str(&format!("long long {}_step(void* r) {{\n", name));
            self.out.push_str(&format!("    {}_async_args* args = ({}_async_args*)r;\n", name, name));
            self.out.push_str("    switch (args->state) {\n");
            self.out.push_str("        case 0:\n");

            // Compile the statements:
            self.await_counter = 0;
            self.is_async_func = true;
            self.current_async_func_locals.clear();
            for k in var_types.keys() {
                self.current_async_func_locals.insert(k.clone());
            }

            for stmt in &func.body {
                let saved_await_counter = self.await_counter;
                self.emit_yields_for_statement(stmt, &var_types)?;
                let post_scan_await_counter = self.await_counter;
                self.await_counter = saved_await_counter;

                self.gen_statement(stmt, &var_types)?;

                self.await_counter = post_scan_await_counter;
            }

            self.out.push_str("            args->state = -1;\n");
            self.out.push_str("            return 0;\n");
            self.out.push_str("    }\n");
            self.out.push_str("    return 0;\n");
            self.out.push_str("}\n\n");

            // 3. Generate the public wrapper function (the entry point for calling the async function)
            self.out.push_str(&format!("long long {}({}) {{\n", name, params_decl.join(", ")));
            self.out.push_str("    EpFuture* fut = (EpFuture*)malloc(sizeof(EpFuture));\n");
            self.out.push_str("    fut->completed = 0;\n");
            self.out.push_str("    fut->value = 0;\n");
            self.out.push_str("    fut->waiting_task = NULL;\n");
            self.out.push_str("    { EpGCObject* _go = ep_gc_register(fut, EP_OBJ_STRUCT); if(_go) _go->num_fields = 3; }\n");
            self.out.push_str(&format!("    {}_async_args* args = ({}_async_args*)malloc(sizeof({}_async_args));\n", name, name, name));
            self.out.push_str(&format!("    memset(args, 0, sizeof({}_async_args));\n", name));
            self.out.push_str("    args->state = 0;\n");
            self.out.push_str("    args->fut = fut;\n");
            for param in &func.params {
                self.out.push_str(&format!("    args->{} = {};\n", Self::sanitize_c_name(&param.0), Self::sanitize_c_name(&param.0)));
            }
            self.out.push_str("    EpTask* task = (EpTask*)malloc(sizeof(EpTask));\n");
            self.out.push_str(&format!("    task->step = {}_step;\n", name));
            self.out.push_str("    task->args = args;\n");
            self.out.push_str(&format!("    task->args_size_bytes = sizeof({}_async_args);\n", name));
            self.out.push_str("    task->fut = fut;\n");
            self.out.push_str("    task->state = 0;\n");
            self.out.push_str("    task->is_cancelled = 0;\n");
            self.out.push_str("    task->parent = ep_current_task;\n");
            self.out.push_str("    ep_task_enqueue(task);\n");
            self.out.push_str("    return (long long)fut;\n");
            self.out.push_str("}\n\n");

            self.is_async_func = false;
            self.current_async_func_locals.clear();
            return Ok(());
        }

        let impl_name = if func.is_async {
            format!("{}_impl", name)
        } else {
            name.clone()
        };
        self.out.push_str(&format!("long long {}({}) {{\n", impl_name, params_decl.join(", ")));

        let mut borrowed_vars = std::collections::HashSet::new();
        for param in &func.params {
            borrowed_vars.insert(param.0.clone());
        }

        fn is_borrow_expr(expr: &Expr, borrowed: &std::collections::HashSet<String>) -> bool {
            match &expr.node {
                ExprNode::Borrow(_) => true,
                ExprNode::FieldAccess(_, _) => true,
                ExprNode::Identifier(name) => borrowed.contains(name),
                ExprNode::Call(name, _) => {
                    let name_lower = name.to_lowercase();
                    name_lower.contains("get") || name_lower.contains("peek")
                }
                ExprNode::MethodCall(_, name, _) => {
                    let name_lower = name.to_lowercase();
                    name_lower.contains("get") || name_lower.contains("peek")
                }
                _ => false,
            }
        }

        fn scan_stmts_for_borrows(
            stmts: &[Stmt],
            borrowed: &mut std::collections::HashSet<String>,
        ) {
            for stmt in stmts {
                match &stmt.node {
                    StmtNode::Set(name, expr, _) => {
                        if is_borrow_expr(expr, borrowed) {
                            borrowed.insert(name.clone());
                        } else {
                            borrowed.remove(name);
                        }
                    }
                    StmtNode::If(_, then_branch, else_branch) => {
                        scan_stmts_for_borrows(then_branch, borrowed);
                        if let Some(else_stmts) = else_branch {
                            scan_stmts_for_borrows(else_stmts, borrowed);
                        }
                    }
                    StmtNode::RepeatWhile(_, body) => {
                        scan_stmts_for_borrows(body, borrowed);
                    }
                    StmtNode::ForEach(loop_var, _, body) => {
                        borrowed.insert(loop_var.clone());
                        scan_stmts_for_borrows(body, borrowed);
                    }
                    StmtNode::Match(_, arms) => {
                        for (_, bindings, body) in arms {
                            for binding in bindings {
                                borrowed.insert(binding.clone());
                            }
                            scan_stmts_for_borrows(body, borrowed);
                        }
                    }
                    _ => {}
                }
            }
        }

        scan_stmts_for_borrows(&func.body, &mut borrowed_vars);
        
        // Sorted iteration: HashMap order is nondeterministic and would make the
        // emitted C differ run-to-run (breaks reproducible builds + the
        // byte-identical parity gate between ernos and epc).
        for var_name in { let mut _sv: Vec<&String> = var_types.keys().collect(); _sv.sort(); _sv } {
            let is_param = func.params.iter().any(|p| &p.0 == var_name);
            let is_global = self.global_constants.contains(var_name);
            if !is_param && !is_global {
                let safe_var = Self::sanitize_c_name(var_name);
                self.out.push_str(&format!("    long long {} = 0;\n", safe_var));
            }
        }
        self.out.push_str("    long long ret_val = 0;\n\n");

        // Push GC roots for all locals
        let mut gc_root_count = 0;
        // Sorted iteration: HashMap order is nondeterministic and would make the
        // emitted C differ run-to-run (breaks reproducible builds + the
        // byte-identical parity gate between ernos and epc).
        for var_name in { let mut _sv: Vec<&String> = var_types.keys().collect(); _sv.sort(); _sv } {
            let is_param = func.params.iter().any(|p| &p.0 == var_name);
            let is_global = self.global_constants.contains(var_name);
            if !is_param && !is_global {
                let t = var_types.get(var_name);
                let should_root = t.map(|ty| {
                    if !needs_gc_root(ty) {
                        func.body.iter().any(|s| stmt_contains_non_primitive_usage(s, var_name))
                    } else {
                        true
                    }
                }).unwrap_or(true);
                if should_root {
                    self.out.push_str(&format!("    ep_gc_push_root(&{});\n", Self::sanitize_c_name(var_name)));
                    gc_root_count += 1;
                }
            }
        }
        // Also push params as GC roots
        for param in &func.params {
            let t = var_types.get(&param.0);
            let should_root = t.map(|ty| {
                if !needs_gc_root(ty) {
                    func.body.iter().any(|s| stmt_contains_non_primitive_usage(s, &param.0))
                } else {
                    true
                }
            }).unwrap_or(true);
            if should_root {
                self.out.push_str(&format!("    ep_gc_push_root(&{});\n", param.0));
                gc_root_count += 1;
            }
        }
        if gc_root_count > 0 {
            self.out.push_str("\n");
        }

        // GC safe point: collect only if this function uses heap-allocated data
        let needs_gc = gc_root_count > 0 || var_types.values().any(|t| 
            matches!(t, Type::List | Type::Map | Type::DynStr | Type::Struct(_) | Type::Enum(_) | Type::RefList)
        );
        if needs_gc {
            self.out.push_str("    ep_gc_maybe_collect();\n\n");
        }

        for stmt in &func.body {
            self.gen_statement(stmt, &var_types)?;
        }

        self.out.push_str("L_cleanup:\n");
        // Pop GC roots
        if gc_root_count > 0 {
            self.out.push_str(&format!("    ep_gc_pop_roots({});\n", gc_root_count));
        }

        // Scan function body for explicit free_list/free_map calls
        // to avoid double-freeing in auto-cleanup
        let mut user_freed: std::collections::HashSet<String> = std::collections::HashSet::new();
        Self::collect_user_freed(&func.body, &mut user_freed);

        // Scan for ownership transfers: variables passed to container-insertion
        // functions (map_insert, append_list, set_list) should NOT be auto-freed
        // because the container now owns them. Also tracks variables passed to
        // user-defined functions, since the callee could transfer ownership.
        let mut transferred: std::collections::HashSet<String> = std::collections::HashSet::new();
        Self::collect_transferred(&func.body, &mut transferred, &self.builtin_c_funcs);

        // Get the function's return type to avoid freeing the returned value
        let func_ret_type = self.func_return_types.get(&func.name).cloned();
        // Sorted iteration: HashMap order is nondeterministic and would make the
        // emitted C differ run-to-run (breaks reproducible builds + the
        // byte-identical parity gate between ernos and epc).
        for var_name in { let mut _sv: Vec<&String> = var_types.keys().collect(); _sv.sort(); _sv } {
            let is_param = func.params.iter().any(|p| &p.0 == var_name);
            let is_global = self.global_constants.contains(var_name);
            let is_borrowed = borrowed_vars.contains(var_name);
            if !is_param && !is_global && !is_borrowed {
                let t = var_types.get(var_name);
                // Skip freeing if this local has the same struct/enum type as the return type
                // because it may hold the value being returned via ret_val
                if let Some(ref frt) = func_ret_type {
                    if t == Some(frt) {
                        continue;
                    }
                }
                // Skip if user already explicitly freed this variable
                if user_freed.contains(var_name) {
                    continue;
                }
                // Skip if ownership was transferred to a container
                if transferred.contains(var_name) {
                    continue;
                }
                if t == Some(&Type::List) {
                    self.out.push_str(&format!("    free_list({});\n", var_name));
                    self.out.push_str(&format!("    {} = 0;\n", var_name));
                } else if t == Some(&Type::Map) {
                    self.out.push_str(&format!("    free_map({});\n", var_name));
                    self.out.push_str(&format!("    {} = 0;\n", var_name));
                } else if let Some(Type::Struct(sname)) = t {
                    if sname == "Map" {
                        self.out.push_str(&format!("    free_map({});\n", var_name));
                        self.out.push_str(&format!("    {} = 0;\n", var_name));
                    } else if sname == "Deque" {
                        // Deque is a built-in, skip struct free
                    } else {
                        self.out.push_str(&format!("    free_struct_{}({});\n", sname, var_name));
                        self.out.push_str(&format!("    {} = 0;\n", var_name));
                    }
                } else if let Some(Type::Enum(ename)) = t {
                    self.out.push_str(&format!("    free_enum_{}({});\n", ename, var_name));
                    self.out.push_str(&format!("    {} = 0;\n", var_name));
                }
            }
        }
        self.out.push_str("    return ret_val;\n}\n\n");

        Ok(())
    }

    /// Scan statements for explicit free_list/free_map calls and collect the variable names
    fn collect_user_freed(stmts: &[Stmt], freed: &mut std::collections::HashSet<String>) {
        for stmt in stmts {
            match &stmt.node {
                StmtNode::Set(_, expr, _) | StmtNode::ExprStmt(expr) => {
                    // Check if the expression is a call to free_list or free_map
                    if let ExprNode::Call(name, args) = &expr.node {
                        if (name == "free_list" || name == "free_map") && !args.is_empty() {
                            if let ExprNode::Identifier(var_name) = &args[0].node {
                                freed.insert(var_name.clone());
                            }
                        }
                    }
                }
                StmtNode::If(_, then_branch, else_branch) => {
                    Self::collect_user_freed(then_branch, freed);
                    if let Some(else_stmts) = else_branch {
                        Self::collect_user_freed(else_stmts, freed);
                    }
                }
                StmtNode::RepeatWhile(_, body) => {
                    Self::collect_user_freed(body, freed);
                }
                StmtNode::ForEach(_, _, body) => {
                    Self::collect_user_freed(body, freed);
                }
                _ => {}
            }
        }
    }

    /// Scan statements for ownership-transferring calls and collect variable names.
    /// When a list/map/struct variable is passed as a value to a container-insertion
    /// function, ownership is transferred — the container now owns it.
    /// Also tracks variables passed to user-defined functions, since the caller
    /// can't know whether the callee stores the value in a container.
    fn collect_transferred(stmts: &[Stmt], transferred: &mut std::collections::HashSet<String>,
                           builtins: &std::collections::HashSet<String>) {
        for stmt in stmts {
            match &stmt.node {
                StmtNode::Set(_, expr, _) | StmtNode::ExprStmt(expr) => {
                    Self::collect_transferred_from_expr(expr, transferred, builtins);
                }
                StmtNode::If(_, then_branch, else_branch) => {
                    Self::collect_transferred(then_branch, transferred, builtins);
                    if let Some(else_stmts) = else_branch {
                        Self::collect_transferred(else_stmts, transferred, builtins);
                    }
                }
                StmtNode::RepeatWhile(_, body) => {
                    Self::collect_transferred(body, transferred, builtins);
                }
                StmtNode::ForEach(_, _, body) => {
                    Self::collect_transferred(body, transferred, builtins);
                }
                StmtNode::Send(_, val) => {
                    // Sending a value over a channel transfers ownership
                    if let ExprNode::Identifier(var_name) = &val.node {
                        transferred.insert(var_name.clone());
                    }
                }
                StmtNode::Return(expr) => {
                    // A return expression may call user functions that consume locals
                    Self::collect_transferred_from_expr(expr, transferred, builtins);
                    // Also mark any identifier directly returned as transferred
                    // (it's being moved out of the function via ret_val)
                    if let ExprNode::Identifier(var_name) = &expr.node {
                        transferred.insert(var_name.clone());
                    }
                }
                StmtNode::Display(expr) => {
                    Self::collect_transferred_from_expr(expr, transferred, builtins);
                }
                StmtNode::Match(_, arms) => {
                    for (_, _, body) in arms {
                        Self::collect_transferred(body, transferred, builtins);
                    }
                }
                _ => {}
            }
        }
    }

    fn collect_transferred_from_expr(expr: &Expr, transferred: &mut std::collections::HashSet<String>,
                                     builtins: &std::collections::HashSet<String>) {
        if let ExprNode::Call(name, args) = &expr.node {
            match name.as_str() {
                // map_insert(map, key, value) — 3rd arg (index 2) is transferred
                "map_insert" if args.len() >= 3 => {
                    if let ExprNode::Identifier(var_name) = &args[2].node {
                        transferred.insert(var_name.clone());
                    }
                }
                // append_list(list, value) — 2nd arg (index 1) is transferred
                "append_list" if args.len() >= 2 => {
                    if let ExprNode::Identifier(var_name) = &args[1].node {
                        transferred.insert(var_name.clone());
                    }
                }
                // set_list(list, index, value) — 3rd arg (index 2) is transferred
                "set_list" if args.len() >= 3 => {
                    if let ExprNode::Identifier(var_name) = &args[2].node {
                        transferred.insert(var_name.clone());
                    }
                }
                _ => {
                    // For user-defined functions (not builtins), any argument
                    // could be stored in a container by the callee — we can't
                    // know, so mark all identifier args as potentially transferred.
                    if !builtins.contains(name) {
                        for arg in args {
                            if let ExprNode::Identifier(var_name) = &arg.node {
                                transferred.insert(var_name.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    fn gen_method(&mut self, md: &MethodDef) -> Result<(), String> {
        let mut var_types = HashMap::new();
        self.list_element_types.clear();
        self.closure_c_names.clear();

        // Type self correctly based on whether the target is an enum or struct
        if self.enum_defs.contains_key(&md.struct_name) {
            var_types.insert("self".to_string(), Type::Enum(md.struct_name.clone()));
        } else {
            var_types.insert("self".to_string(), Type::Struct(md.struct_name.clone()));
        }

        for param in &md.params {
            let param_type = if let Some(ref ann) = param.2 {
                self.type_annotation_to_type(ann)
            } else if param.1 {
                Type::RefList
            } else {
                self.infer_param_struct_type(&param.0, &md.body).unwrap_or(Type::Int)
            };
            var_types.insert(param.0.clone(), param_type);
        }
        self.collect_var_types(&md.body, &mut var_types);
        self.infer_param_types_from_usage(&md.body, &mut var_types);

        let key = format!("{}_{}", md.struct_name, md.name);
        self.current_return_type = self.func_return_types.get(&key).cloned().unwrap_or(Type::Int);

        let mut params_decl = vec!["long long self".to_string()];
        for param in &md.params {
            params_decl.push(format!("long long {}", param.0));
        }

        self.out.push_str(&format!("long long {}__{}({}) {{\n", md.struct_name, md.name, params_decl.join(", ")));

        // Sorted iteration: HashMap order is nondeterministic and would make the
        // emitted C differ run-to-run (breaks reproducible builds + the
        // byte-identical parity gate between ernos and epc).
        for var_name in { let mut _sv: Vec<&String> = var_types.keys().collect(); _sv.sort(); _sv } {
            let is_param = var_name == "self" || md.params.iter().any(|p| &p.0 == var_name);
            if !is_param {
                let safe_var = Self::sanitize_c_name(var_name);
                self.out.push_str(&format!("    long long {} = 0;\n", safe_var));
            }
        }
        self.out.push_str("    long long ret_val = 0;\n\n");

        // Push GC roots for all locals
        let mut gc_root_count = 0;
        // Sorted iteration: HashMap order is nondeterministic and would make the
        // emitted C differ run-to-run (breaks reproducible builds + the
        // byte-identical parity gate between ernos and epc).
        for var_name in { let mut _sv: Vec<&String> = var_types.keys().collect(); _sv.sort(); _sv } {
            let is_param = var_name == "self" || md.params.iter().any(|p| &p.0 == var_name);
            if !is_param {
                let t = var_types.get(var_name);
                let should_root = t.map(|ty| {
                    if !needs_gc_root(ty) {
                        md.body.iter().any(|s| stmt_contains_non_primitive_usage(s, var_name))
                    } else {
                        true
                    }
                }).unwrap_or(true);
                if should_root {
                    self.out.push_str(&format!("    ep_gc_push_root(&{});\n", Self::sanitize_c_name(var_name)));
                    gc_root_count += 1;
                }
            }
        }
        // Also push self as GC root
        {
            let t = var_types.get("self");
            let should_root = t.map(|ty| {
                if !needs_gc_root(ty) {
                    md.body.iter().any(|s| stmt_contains_non_primitive_usage(s, "self"))
                } else {
                    true
                }
            }).unwrap_or(true);
            if should_root {
                self.out.push_str("    ep_gc_push_root(&self);\n");
                gc_root_count += 1;
            }
        }
        // Also push params as GC roots
        for param in &md.params {
            let t = var_types.get(&param.0);
            let should_root = t.map(|ty| {
                if !needs_gc_root(ty) {
                    md.body.iter().any(|s| stmt_contains_non_primitive_usage(s, &param.0))
                } else {
                    true
                }
            }).unwrap_or(true);
            if should_root {
                self.out.push_str(&format!("    ep_gc_push_root(&{});\n", param.0));
                gc_root_count += 1;
            }
        }
        if gc_root_count > 0 {
            self.out.push_str("\n");
        }

        // GC safe point: collect only if this function uses heap-allocated data
        let needs_gc = gc_root_count > 0 || var_types.values().any(|t| 
            matches!(t, Type::List | Type::Map | Type::DynStr | Type::Struct(_) | Type::Enum(_) | Type::RefList)
        );
        if needs_gc {
            self.out.push_str("    ep_gc_maybe_collect();\n\n");
        }

        for stmt in &md.body {
            self.gen_statement(stmt, &var_types)?;
        }

        self.out.push_str("L_cleanup:\n");
        // Pop GC roots
        if gc_root_count > 0 {
            self.out.push_str(&format!("    ep_gc_pop_roots({});\n", gc_root_count));
        }
        self.out.push_str("    return ret_val;\n}\n\n");

        Ok(())
    }
}

const RUNTIME_HEADER_AND_SRC: &str = include_str!("../runtime/ep_runtime.c");
const EP_BUILTINS_SRC: &str = include_str!("../runtime/ep_builtins.c");

#[allow(dead_code)]
const C_MAIN_BOOTSTRAPPER: &str = r#"
/* Bootstrapper C main */
int main(int argc, char** argv) {
    {
        unsigned int seed;
        FILE* urand = fopen("/dev/urandom", "rb");
        if (urand && fread(&seed, sizeof(seed), 1, urand) == 1) {
            fclose(urand);
        } else {
            if (urand) fclose(urand);
            seed = (unsigned int)time(NULL) ^ (unsigned int)getpid();
        }
        srand(seed);
    }
    init_ep_args(argc, argv);
    int result = (int)_main();
    ep_gc_shutdown();
    return result;
}
"#;
