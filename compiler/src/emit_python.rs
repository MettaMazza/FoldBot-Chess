/// ErnosPlain → Python Emitter
///
/// Takes a parsed ErnosPlain Program AST and emits equivalent Python source code.

use crate::ast::*;

/// Main entry point: emit Python source from an ErnosPlain AST
pub fn emit_python_from_ep(program: &Program) -> String {
    let mut out = String::new();
    out.push_str("# Auto-generated Python from ErnosPlain\n\n");

    // Top-level constants
    for stmt in &program.top_level_constants {
        emit_stmt(&mut out, &stmt.node, 0);
    }
    if !program.top_level_constants.is_empty() {
        out.push('\n');
    }

    // Struct definitions → Python classes
    for sd in &program.struct_defs {
        emit_struct_def(&mut out, sd);
        out.push('\n');
    }

    // Enum definitions → Python classes with class variables
    for ed in &program.enum_defs {
        emit_enum_def(&mut out, ed);
        out.push('\n');
    }

    // Method definitions
    // Group methods by struct name
    let mut method_map: std::collections::HashMap<String, Vec<&MethodDef>> = std::collections::HashMap::new();
    for md in &program.method_defs {
        method_map.entry(md.struct_name.clone()).or_default().push(md);
    }

    // We'll emit methods as standalone functions with a self-like first param
    // (since Python classes were already emitted above, we inject methods below)
    for md in &program.method_defs {
        emit_method_def(&mut out, md);
        out.push('\n');
    }

    // Functions
    for func in &program.functions {
        emit_function(&mut out, func);
        out.push('\n');
    }

    // Call main if it exists
    let has_main = program.functions.iter().any(|f| f.name == "main");
    if has_main {
        out.push_str("if __name__ == \"__main__\":\n");
        out.push_str("    main()\n");
    }

    out
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("    ");
    }
}

fn emit_struct_def(out: &mut String, sd: &StructDef) {
    out.push_str(&format!("class {}:\n", sd.name));
    if sd.fields.is_empty() {
        out.push_str("    pass\n");
        return;
    }
    // __init__ method
    let params: Vec<String> = sd.fields.iter().map(|(name, ta, default)| {
        if let Some(def) = default {
            format!("{}={}", name, expr_to_python_default(def))
        } else {
            format!("{}={}", name, type_default(ta))
        }
    }).collect();
    out.push_str(&format!("    def __init__(self, {}):\n", params.join(", ")));
    for (name, _, _) in &sd.fields {
        out.push_str(&format!("        self.{name} = {name}\n"));
    }
    out.push('\n');
    // __repr__
    let field_strs: Vec<String> = sd.fields.iter().map(|(n, _, _)| format!("{n}={{self.{n}!r}}")).collect();
    out.push_str(&format!("    def __repr__(self):\n        return f\"{}({})\"\n", sd.name, field_strs.join(", ")));
}

fn expr_to_python_default(expr: &Expr) -> String {
    let mut s = String::new();
    emit_expr(&mut s, &expr.node);
    s
}

fn type_default(ta: &TypeAnnotation) -> &'static str {
    match ta {
        TypeAnnotation::Int => "0",
        TypeAnnotation::Float => "0.0",
        TypeAnnotation::Bool => "False",
        TypeAnnotation::Str | TypeAnnotation::DynStr => "\"\"",
        TypeAnnotation::List => "None",
        _ => "None",
    }
}

fn type_to_python_hint(ta: &TypeAnnotation) -> String {
    match ta {
        TypeAnnotation::Int => "int".to_string(),
        TypeAnnotation::Float => "float".to_string(),
        TypeAnnotation::Bool => "bool".to_string(),
        TypeAnnotation::Str | TypeAnnotation::DynStr => "str".to_string(),
        TypeAnnotation::List => "list".to_string(),
        TypeAnnotation::UserDefined(name) => name.clone(),
        TypeAnnotation::Generic(name, _) => name.clone(),
    }
}

fn emit_enum_def(out: &mut String, ed: &EnumDef) {
    out.push_str(&format!("class {}:\n", ed.name));
    if ed.variants.is_empty() {
        out.push_str("    pass\n");
        return;
    }
    for (i, (vname, fields)) in ed.variants.iter().enumerate() {
        if fields.is_empty() {
            out.push_str(&format!("    {} = {}\n", vname, i));
        } else {
            // Variant with data → nested class
            out.push_str(&format!("    class {}:\n", vname));
            let params: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
            out.push_str(&format!("        def __init__(self, {}):\n", params.join(", ")));
            for (n, _) in fields {
                out.push_str(&format!("            self.{n} = {n}\n"));
            }
            out.push_str(&format!("        def __repr__(self):\n            return \"{}.{}({{}})\".format({})\n",
                ed.name, vname, params.iter().map(|p| format!("self.{p}")).collect::<Vec<_>>().join(", ")));
        }
    }
}

fn emit_method_def(out: &mut String, md: &MethodDef) {
    // Emit as a standalone function: struct_name_method_name(self, ...)
    let func_name = format!("{}_{}", md.struct_name.to_lowercase(), md.name);
    let mut params = vec!["self".to_string()];
    for (name, _, _) in &md.params {
        params.push(name.clone());
    }
    out.push_str(&format!("def {}({}):\n", func_name, params.join(", ")));
    if md.body.is_empty() {
        out.push_str("    pass\n");
    } else {
        for stmt in &md.body {
            emit_stmt(out, &stmt.node, 1);
        }
    }
}

fn emit_function(out: &mut String, func: &Function) {
    let mut params: Vec<String> = Vec::new();
    for (name, _, type_ann) in &func.params {
        if let Some(ta) = type_ann {
            params.push(format!("{}: {}", name, type_to_python_hint(ta)));
        } else {
            params.push(name.clone());
        }
    }

    let ret_hint = if let Some(rt) = &func.return_type {
        format!(" -> {}", type_to_python_hint(rt))
    } else {
        String::new()
    };

    if func.is_async {
        out.push_str(&format!("async def {}({}){}:\n", func.name, params.join(", "), ret_hint));
    } else {
        out.push_str(&format!("def {}({}){}:\n", func.name, params.join(", "), ret_hint));
    }

    if func.body.is_empty() {
        out.push_str("    pass\n");
    } else {
        for stmt in &func.body {
            emit_stmt(out, &stmt.node, 1);
        }
    }
}

fn emit_stmt(out: &mut String, stmt: &StmtNode, depth: usize) {
    match stmt {
        StmtNode::Set(name, expr, _type_ann) => {
            indent(out, depth);
            out.push_str(&format!("{} = ", name));
            emit_expr(out, &expr.node);
            out.push('\n');
        }

        StmtNode::Display(expr) => {
            indent(out, depth);
            out.push_str("print(");
            emit_expr(out, &expr.node);
            out.push_str(")\n");
        }

        StmtNode::Return(expr) => {
            indent(out, depth);
            out.push_str("return ");
            emit_expr(out, &expr.node);
            out.push('\n');
        }

        StmtNode::If(cond, then_branch, else_branch) => {
            indent(out, depth);
            out.push_str("if ");
            emit_expr(out, &cond.node);
            out.push_str(":\n");
            for s in then_branch {
                emit_stmt(out, &s.node, depth + 1);
            }
            if let Some(eb) = else_branch {
                // Check if it's an else-if chain
                if eb.len() == 1 {
                    if let StmtNode::If(cond2, then2, else2) = &eb[0].node {
                        indent(out, depth);
                        out.push_str("elif ");
                        emit_expr(out, &cond2.node);
                        out.push_str(":\n");
                        for s in then2 {
                            emit_stmt(out, &s.node, depth + 1);
                        }
                        if let Some(eb2) = else2 {
                            indent(out, depth);
                            out.push_str("else:\n");
                            for s in eb2 {
                                emit_stmt(out, &s.node, depth + 1);
                            }
                        }
                        return;
                    }
                }
                indent(out, depth);
                out.push_str("else:\n");
                for s in eb {
                    emit_stmt(out, &s.node, depth + 1);
                }
            }
        }

        StmtNode::RepeatWhile(cond, body) => {
            indent(out, depth);
            out.push_str("while ");
            emit_expr(out, &cond.node);
            out.push_str(":\n");
            for s in body {
                emit_stmt(out, &s.node, depth + 1);
            }
        }

        StmtNode::ForEach(var, iterable, body) => {
            indent(out, depth);
            out.push_str(&format!("for {} in ", var));
            emit_expr(out, &iterable.node);
            out.push_str(":\n");
            for s in body {
                emit_stmt(out, &s.node, depth + 1);
            }
        }

        StmtNode::Break => {
            indent(out, depth);
            out.push_str("break\n");
        }

        StmtNode::Continue => {
            indent(out, depth);
            out.push_str("continue\n");
        }

        StmtNode::ExprStmt(expr) => {
            indent(out, depth);
            emit_expr(out, &expr.node);
            out.push('\n');
        }

        StmtNode::FieldSet(obj, field, val) => {
            indent(out, depth);
            emit_expr(out, &obj.node);
            out.push_str(&format!(".{} = ", field));
            emit_expr(out, &val.node);
            out.push('\n');
        }

        StmtNode::Match(expr, arms) => {
            // Python 3.10+ match/case, but for compatibility use if/elif
            let mut first = true;
            for (variant, bindings, body) in arms {
                indent(out, depth);
                if first {
                    out.push_str("if ");
                    first = false;
                } else {
                    out.push_str("elif ");
                }
                emit_expr(out, &expr.node);
                out.push_str(&format!(" == \"{}\":\n", variant));
                if !bindings.is_empty() {
                    // Destructure bindings
                    for (i, b) in bindings.iter().enumerate() {
                        indent(out, depth + 1);
                        out.push_str(&format!("{} = ", b));
                        emit_expr(out, &expr.node);
                        out.push_str(&format!(".data[{}]\n", i));
                    }
                }
                for s in body {
                    emit_stmt(out, &s.node, depth + 1);
                }
            }
        }

        StmtNode::Spawn(func_name, args) => {
            indent(out, depth);
            out.push_str("import threading\n");
            indent(out, depth);
            let arg_strs: Vec<String> = args.iter().map(|a| {
                let mut s = String::new();
                emit_expr(&mut s, &a.node);
                s
            }).collect();
            out.push_str(&format!(
                "threading.Thread(target={}, args=({}{})).start()\n",
                func_name,
                arg_strs.join(", "),
                if arg_strs.len() == 1 { "," } else { "" }
            ));
        }

        StmtNode::Send(chan, val) => {
            indent(out, depth);
            emit_expr(out, &chan.node);
            out.push_str(".put(");
            emit_expr(out, &val.node);
            out.push_str(")\n");
        }
    }
}

fn emit_expr(out: &mut String, expr: &ExprNode) {
    match expr {
        ExprNode::Integer(n) => out.push_str(&n.to_string()),
        ExprNode::FloatLiteral(f) => out.push_str(&format!("{}", f)),
        ExprNode::BoolLiteral(b) => out.push_str(if *b { "True" } else { "False" }),
        ExprNode::StringLiteral(s) => {
            // Escape for Python string
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', "\\r").replace('\t', "\\t");
            out.push_str(&format!("\"{}\"", escaped));
        }
        ExprNode::Identifier(name) => out.push_str(name),

        ExprNode::Binary(left, op, right) => {
            out.push('(');
            emit_expr(out, &left.node);
            let op_str = match op {
                Op::Add => " + ",
                Op::Sub => " - ",
                Op::Mul => " * ",
                Op::Div => " // ",  // Integer division in Python
                Op::Mod => " % ",
            };
            out.push_str(op_str);
            emit_expr(out, &right.node);
            out.push(')');
        }

        ExprNode::Comparison(left, op, right) => {
            out.push('(');
            emit_expr(out, &left.node);
            let op_str = match op {
                CompOp::Equals => " == ",
                CompOp::NotEquals => " != ",
                CompOp::LessThan => " < ",
                CompOp::GreaterThan => " > ",
                CompOp::LessEqual => " <= ",
                CompOp::GreaterEqual => " >= ",
            };
            out.push_str(op_str);
            emit_expr(out, &right.node);
            out.push(')');
        }

        ExprNode::Logical(left, op, right) => {
            out.push('(');
            emit_expr(out, &left.node);
            let op_str = match op {
                LogicalOp::And => " and ",
                LogicalOp::Or => " or ",
            };
            out.push_str(op_str);
            emit_expr(out, &right.node);
            out.push(')');
        }

        ExprNode::UnaryNot(inner) => {
            out.push_str("not ");
            emit_expr(out, &inner.node);
        }

        ExprNode::Call(name, args) => {
            // Map EP builtins to Python equivalents
            let py_name = match name.as_str() {
                "concat" => {
                    // concat(a and b) → str(a) + str(b) or a + b
                    if args.len() == 2 {
                        out.push_str("(");
                        emit_expr(out, &args[0].node);
                        out.push_str(" + ");
                        emit_expr(out, &args[1].node);
                        out.push_str(")");
                        return;
                    }
                    "concat"
                }
                "length" | "length_list" => {
                    out.push_str("len(");
                    if !args.is_empty() { emit_expr(out, &args[0].node); }
                    out.push(')');
                    return;
                }
                "string_length" => {
                    out.push_str("len(");
                    if !args.is_empty() { emit_expr(out, &args[0].node); }
                    out.push(')');
                    return;
                }
                "int_to_string" | "ep_auto_to_string" => {
                    out.push_str("str(");
                    if !args.is_empty() { emit_expr(out, &args[0].node); }
                    out.push(')');
                    return;
                }
                "string_to_int" | "parse_int" => {
                    out.push_str("int(");
                    if !args.is_empty() { emit_expr(out, &args[0].node); }
                    out.push(')');
                    return;
                }
                "random" => {
                    out.push_str("__import__('random').randint(");
                    if args.len() >= 2 {
                        emit_expr(out, &args[0].node);
                        out.push_str(", ");
                        emit_expr(out, &args[1].node);
                    }
                    out.push(')');
                    return;
                }
                "range" => {
                    out.push_str("range(");
                    for (i, a) in args.iter().enumerate() {
                        if i > 0 { out.push_str(", "); }
                        emit_expr(out, &a.node);
                    }
                    out.push(')');
                    return;
                }
                "append" | "append_list" => {
                    if args.len() >= 2 {
                        emit_expr(out, &args[0].node);
                        out.push_str(".append(");
                        emit_expr(out, &args[1].node);
                        out.push(')');
                        return;
                    }
                    "append"
                }
                "get_list" => {
                    if args.len() >= 2 {
                        emit_expr(out, &args[0].node);
                        out.push('[');
                        emit_expr(out, &args[1].node);
                        out.push(']');
                        return;
                    }
                    "get_list"
                }
                "set_list" => {
                    if args.len() >= 3 {
                        emit_expr(out, &args[0].node);
                        out.push('[');
                        emit_expr(out, &args[1].node);
                        out.push_str("] = ");
                        emit_expr(out, &args[2].node);
                        return;
                    }
                    "set_list"
                }
                "sleep" => {
                    out.push_str("__import__('time').sleep(");
                    if !args.is_empty() { emit_expr(out, &args[0].node); }
                    out.push_str(" / 1000)");  // EP sleep is in ms
                    return;
                }
                "time" | "current_time" => {
                    out.push_str("int(__import__('time').time() * 1000)");
                    return;
                }
                "exit" => {
                    out.push_str("__import__('sys').exit(");
                    if !args.is_empty() { emit_expr(out, &args[0].node); }
                    out.push(')');
                    return;
                }
                "input" => {
                    out.push_str("input(");
                    if !args.is_empty() { emit_expr(out, &args[0].node); }
                    out.push(')');
                    return;
                }
                "file_read" => {
                    out.push_str("open(");
                    if !args.is_empty() { emit_expr(out, &args[0].node); }
                    out.push_str(").read()");
                    return;
                }
                "file_write" => {
                    if args.len() >= 2 {
                        out.push_str("open(");
                        emit_expr(out, &args[0].node);
                        out.push_str(", 'w').write(");
                        emit_expr(out, &args[1].node);
                        out.push(')');
                        return;
                    }
                    "file_write"
                }
                _ => name.as_str(),
            };
            out.push_str(py_name);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                emit_expr(out, &a.node);
            }
            out.push(')');
        }

        ExprNode::MethodCall(obj, method, args) => {
            emit_expr(out, &obj.node);
            out.push('.');
            out.push_str(method);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                emit_expr(out, &a.node);
            }
            out.push(')');
        }

        ExprNode::FieldAccess(obj, field) => {
            emit_expr(out, &obj.node);
            out.push('.');
            out.push_str(field);
        }

        ExprNode::StructCreate(name, fields) => {
            out.push_str(name);
            out.push('(');
            for (i, (fname, fval)) in fields.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                out.push_str(&format!("{}=", fname));
                emit_expr(out, &fval.node);
            }
            out.push(')');
        }

        ExprNode::EnumCreate(_enum_name, variant, args) => {
            out.push_str(variant);
            if !args.is_empty() {
                out.push('(');
                for (i, a) in args.iter().enumerate() {
                    if i > 0 { out.push_str(", "); }
                    emit_expr(out, &a.node);
                }
                out.push(')');
            }
        }

        ExprNode::ListLiteral(elements) => {
            out.push('[');
            for (i, e) in elements.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                emit_expr(out, &e.node);
            }
            out.push(']');
        }

        ExprNode::Closure(params, body) => {
            if body.len() == 1 {
                if let StmtNode::Return(expr) = &body[0].node {
                    out.push_str("lambda ");
                    out.push_str(&params.join(", "));
                    out.push_str(": ");
                    emit_expr(out, &expr.node);
                    return;
                }
            }
            // Multi-statement closure → can't be a lambda, emit as named function
            out.push_str("(lambda ");
            out.push_str(&params.join(", "));
            out.push_str(": None)  # Complex closure — manual translation needed");
        }

        ExprNode::Channel => {
            out.push_str("__import__('queue').Queue()");
        }

        ExprNode::Receive(chan) => {
            emit_expr(out, &chan.node);
            out.push_str(".get()");
        }

        ExprNode::Borrow(inner) => {
            // Python has no borrow concept — just emit the value
            emit_expr(out, &inner.node);
        }

        ExprNode::TryExpr(inner) => {
            // Python has no ? operator — emit the expression
            emit_expr(out, &inner.node);
        }

        ExprNode::Await(inner) => {
            out.push_str("await ");
            emit_expr(out, &inner.node);
        }
    }
}
