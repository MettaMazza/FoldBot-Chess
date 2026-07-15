/// ErnosPlain → JavaScript Emitter
///
/// Takes a parsed ErnosPlain Program AST and emits equivalent JavaScript source code.

use crate::ast::*;
use std::collections::HashSet;

/// Recursively collect all variable names from Set statements
fn collect_set_names_js(stmts: &[Stmt], names: &mut HashSet<String>) {
    for stmt in stmts {
        match &stmt.node {
            StmtNode::Set(name, _, _) => { names.insert(name.clone()); }
            StmtNode::If(_, then_b, else_b) => {
                collect_set_names_js(then_b, names);
                if let Some(eb) = else_b { collect_set_names_js(eb, names); }
            }
            StmtNode::RepeatWhile(_, body) => collect_set_names_js(body, names),
            StmtNode::ForEach(_, _, body) => collect_set_names_js(body, names),
            StmtNode::Match(_, arms) => {
                for (_, _, body) in arms { collect_set_names_js(body, names); }
            }
            _ => {}
        }
    }
}

/// Main entry point: emit JavaScript source from an ErnosPlain AST
pub fn emit_js_from_ep(program: &Program) -> String {
    let mut out = String::new();
    out.push_str("// Auto-generated JavaScript from ErnosPlain\n\n");

    // Top-level constants
    for stmt in &program.top_level_constants {
        if let StmtNode::Set(name, expr, _) = &stmt.node {
            out.push_str(&format!("const {} = ", name));
            emit_expr(&mut out, &expr.node);
            out.push_str(";\n");
        }
    }
    if !program.top_level_constants.is_empty() {
        out.push('\n');
    }

    // Struct definitions → JS classes
    for sd in &program.struct_defs {
        emit_struct_def(&mut out, sd);
        out.push('\n');
    }

    // Enum definitions → frozen objects
    for ed in &program.enum_defs {
        emit_enum_def(&mut out, ed);
        out.push('\n');
    }

    // Method definitions → standalone functions or class methods
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
        out.push_str("main();\n");
    }

    out
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("    ");
    }
}

fn emit_struct_def(out: &mut String, sd: &StructDef) {
    out.push_str(&format!("class {} {{\n", sd.name));
    // constructor
    let params: Vec<String> = sd.fields.iter().map(|(n, _, def)| {
        if let Some(d) = def {
            let mut s = String::new();
            emit_expr(&mut s, &d.node);
            format!("{} = {}", n, s)
        } else {
            format!("{} = {}", n, type_default_js(&sd.fields.iter().find(|(name, _, _)| name == n).unwrap().1))
        }
    }).collect();
    out.push_str(&format!("    constructor({}) {{\n", params.join(", ")));
    for (name, _, _) in &sd.fields {
        out.push_str(&format!("        this.{name} = {name};\n"));
    }
    out.push_str("    }\n");
    out.push_str("}\n");
}

fn type_default_js(ta: &TypeAnnotation) -> &'static str {
    match ta {
        TypeAnnotation::Int => "0",
        TypeAnnotation::Float => "0.0",
        TypeAnnotation::Bool => "false",
        TypeAnnotation::Str | TypeAnnotation::DynStr => "\"\"",
        TypeAnnotation::List => "[]",
        _ => "null",
    }
}

fn emit_enum_def(out: &mut String, ed: &EnumDef) {
    out.push_str(&format!("const {} = Object.freeze({{\n", ed.name));
    for (vname, fields) in &ed.variants {
        if fields.is_empty() {
            out.push_str(&format!("    {}: \"{}\",\n", vname, vname));
        } else {
            let params: Vec<String> = fields.iter().map(|(n, _)| n.clone()).collect();
            out.push_str(&format!(
                "    {}: ({}) => ({{ _variant: \"{}\", {} }}),\n",
                vname,
                params.join(", "),
                vname,
                params.iter().map(|p| format!("{p}")).collect::<Vec<_>>().join(", ")
            ));
        }
    }
    out.push_str("});\n");
}

fn emit_method_def(out: &mut String, md: &MethodDef) {
    let func_name = format!("{}_{}", md.struct_name.to_lowercase(), md.name);
    let mut params = vec!["self".to_string()];
    let param_names: HashSet<String> = md.params.iter().map(|(n, _, _)| n.clone()).collect();
    for (name, _, _) in &md.params {
        params.push(name.clone());
    }
    out.push_str(&format!("function {}({}) {{\n", func_name, params.join(", ")));
    // Pre-declare all local variables
    let mut local_vars = HashSet::new();
    collect_set_names_js(&md.body, &mut local_vars);
    let locals: Vec<&String> = local_vars.iter().filter(|n| !param_names.contains(*n) && *n != "self").collect();
    if !locals.is_empty() {
        indent(out, 1);
        out.push_str(&format!("let {};\n", locals.iter().map(|n| n.as_str()).collect::<Vec<_>>().join(", ")));
    }
    for stmt in &md.body {
        emit_stmt(out, &stmt.node, 1);
    }
    out.push_str("}\n");
}

fn emit_function(out: &mut String, func: &Function) {
    let params: Vec<String> = func.params.iter().map(|(n, _, _)| n.clone()).collect();
    let param_names: HashSet<String> = func.params.iter().map(|(n, _, _)| n.clone()).collect();

    if func.is_async {
        out.push_str(&format!("async function {}({}) {{\n", func.name, params.join(", ")));
    } else {
        out.push_str(&format!("function {}({}) {{\n", func.name, params.join(", ")));
    }

    // Pre-declare all local variables (excluding parameters)
    let mut local_vars = HashSet::new();
    collect_set_names_js(&func.body, &mut local_vars);
    let locals: Vec<&String> = local_vars.iter().filter(|n| !param_names.contains(*n)).collect();
    if !locals.is_empty() {
        indent(out, 1);
        out.push_str(&format!("let {};\n", locals.iter().map(|n| n.as_str()).collect::<Vec<_>>().join(", ")));
    }

    for stmt in &func.body {
        emit_stmt(out, &stmt.node, 1);
    }
    out.push_str("}\n");
}

fn emit_stmt(out: &mut String, stmt: &StmtNode, depth: usize) {
    match stmt {
        StmtNode::Set(name, expr, _) => {
            indent(out, depth);
            // Plain assignment — variables are pre-declared at function scope
            out.push_str(&format!("{} = ", name));
            emit_expr(out, &expr.node);
            out.push_str(";\n");
        }

        StmtNode::Display(expr) => {
            indent(out, depth);
            out.push_str("console.log(");
            emit_expr(out, &expr.node);
            out.push_str(");\n");
        }

        StmtNode::Return(expr) => {
            indent(out, depth);
            out.push_str("return ");
            emit_expr(out, &expr.node);
            out.push_str(";\n");
        }

        StmtNode::If(cond, then_branch, else_branch) => {
            indent(out, depth);
            out.push_str("if (");
            emit_expr(out, &cond.node);
            out.push_str(") {\n");
            for s in then_branch {
                emit_stmt(out, &s.node, depth + 1);
            }
            indent(out, depth);
            out.push('}');
            if let Some(eb) = else_branch {
                if eb.len() == 1 {
                    if let StmtNode::If(..) = &eb[0].node {
                        out.push_str(" else ");
                        // Re-emit without indent for else-if chain
                        emit_stmt_inline_if(out, &eb[0].node, depth);
                        return;
                    }
                }
                out.push_str(" else {\n");
                for s in eb {
                    emit_stmt(out, &s.node, depth + 1);
                }
                indent(out, depth);
                out.push_str("}\n");
            } else {
                out.push('\n');
            }
        }

        StmtNode::RepeatWhile(cond, body) => {
            indent(out, depth);
            out.push_str("while (");
            emit_expr(out, &cond.node);
            out.push_str(") {\n");
            for s in body {
                emit_stmt(out, &s.node, depth + 1);
            }
            indent(out, depth);
            out.push_str("}\n");
        }

        StmtNode::ForEach(var, iterable, body) => {
            indent(out, depth);
            out.push_str(&format!("for (const {} of ", var));
            emit_expr(out, &iterable.node);
            out.push_str(") {\n");
            for s in body {
                emit_stmt(out, &s.node, depth + 1);
            }
            indent(out, depth);
            out.push_str("}\n");
        }

        StmtNode::Break => {
            indent(out, depth);
            out.push_str("break;\n");
        }

        StmtNode::Continue => {
            indent(out, depth);
            out.push_str("continue;\n");
        }

        StmtNode::ExprStmt(expr) => {
            indent(out, depth);
            emit_expr(out, &expr.node);
            out.push_str(";\n");
        }

        StmtNode::FieldSet(obj, field, val) => {
            indent(out, depth);
            emit_expr(out, &obj.node);
            out.push_str(&format!(".{} = ", field));
            emit_expr(out, &val.node);
            out.push_str(";\n");
        }

        StmtNode::Match(expr, arms) => {
            indent(out, depth);
            out.push_str("switch (");
            emit_expr(out, &expr.node);
            out.push_str(") {\n");
            for (variant, bindings, body) in arms {
                indent(out, depth + 1);
                out.push_str(&format!("case \"{}\":\n", variant));
                if !bindings.is_empty() {
                    for (i, b) in bindings.iter().enumerate() {
                        indent(out, depth + 2);
                        out.push_str(&format!("const {} = ", b));
                        emit_expr(out, &expr.node);
                        out.push_str(&format!(".data[{}];\n", i));
                    }
                }
                for s in body {
                    emit_stmt(out, &s.node, depth + 2);
                }
                indent(out, depth + 2);
                out.push_str("break;\n");
            }
            indent(out, depth);
            out.push_str("}\n");
        }

        StmtNode::Spawn(func_name, args) => {
            indent(out, depth);
            let arg_strs: Vec<String> = args.iter().map(|a| {
                let mut s = String::new();
                emit_expr(&mut s, &a.node);
                s
            }).collect();
            out.push_str(&format!(
                "setTimeout(() => {}({}), 0);\n",
                func_name,
                arg_strs.join(", ")
            ));
        }

        StmtNode::Send(chan, val) => {
            indent(out, depth);
            emit_expr(out, &chan.node);
            out.push_str(".push(");
            emit_expr(out, &val.node);
            out.push_str(");\n");
        }
    }
}

fn emit_stmt_inline_if(out: &mut String, stmt: &StmtNode, depth: usize) {
    if let StmtNode::If(cond, then_branch, else_branch) = stmt {
        out.push_str("if (");
        emit_expr(out, &cond.node);
        out.push_str(") {\n");
        for s in then_branch {
            emit_stmt(out, &s.node, depth + 1);
        }
        indent(out, depth);
        out.push('}');
        if let Some(eb) = else_branch {
            if eb.len() == 1 {
                if let StmtNode::If(..) = &eb[0].node {
                    out.push_str(" else ");
                    emit_stmt_inline_if(out, &eb[0].node, depth);
                    return;
                }
            }
            out.push_str(" else {\n");
            for s in eb {
                emit_stmt(out, &s.node, depth + 1);
            }
            indent(out, depth);
            out.push_str("}\n");
        } else {
            out.push('\n');
        }
    }
}

fn emit_expr(out: &mut String, expr: &ExprNode) {
    match expr {
        ExprNode::Integer(n) => out.push_str(&n.to_string()),
        ExprNode::FloatLiteral(f) => out.push_str(&format!("{}", f)),
        ExprNode::BoolLiteral(b) => out.push_str(if *b { "true" } else { "false" }),
        ExprNode::StringLiteral(s) => {
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
                Op::Div => " / ",
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
                CompOp::Equals => " === ",
                CompOp::NotEquals => " !== ",
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
                LogicalOp::And => " && ",
                LogicalOp::Or => " || ",
            };
            out.push_str(op_str);
            emit_expr(out, &right.node);
            out.push(')');
        }

        ExprNode::UnaryNot(inner) => {
            out.push('!');
            emit_expr(out, &inner.node);
        }

        ExprNode::Call(name, args) => {
            let js_name = match name.as_str() {
                "concat" if args.len() == 2 => {
                    out.push('(');
                    emit_expr(out, &args[0].node);
                    out.push_str(" + ");
                    emit_expr(out, &args[1].node);
                    out.push(')');
                    return;
                }
                "length" | "length_list" | "string_length" => {
                    if !args.is_empty() {
                        emit_expr(out, &args[0].node);
                        out.push_str(".length");
                    }
                    return;
                }
                "int_to_string" | "ep_auto_to_string" => {
                    out.push_str("String(");
                    if !args.is_empty() { emit_expr(out, &args[0].node); }
                    out.push(')');
                    return;
                }
                "string_to_int" | "parse_int" => {
                    out.push_str("parseInt(");
                    if !args.is_empty() { emit_expr(out, &args[0].node); }
                    out.push(')');
                    return;
                }
                "random" => {
                    out.push_str("Math.floor(Math.random() * (");
                    if args.len() >= 2 {
                        emit_expr(out, &args[1].node);
                        out.push_str(" - ");
                        emit_expr(out, &args[0].node);
                        out.push_str(" + 1) + ");
                        emit_expr(out, &args[0].node);
                    }
                    out.push(')');
                    return;
                }
                "append" | "append_list" => {
                    if args.len() >= 2 {
                        emit_expr(out, &args[0].node);
                        out.push_str(".push(");
                        emit_expr(out, &args[1].node);
                        out.push(')');
                        return;
                    }
                    "push"
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
                "sleep" => {
                    out.push_str("await new Promise(r => setTimeout(r, ");
                    if !args.is_empty() { emit_expr(out, &args[0].node); }
                    out.push_str("))");
                    return;
                }
                "exit" => {
                    out.push_str("process.exit(");
                    if !args.is_empty() { emit_expr(out, &args[0].node); }
                    out.push(')');
                    return;
                }
                "range" => "range",  // Keep as-is, user must polyfill
                _ => name.as_str(),
            };
            out.push_str(js_name);
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
            out.push_str(&format!("new {}(", name));
            // Emit as positional args matching constructor order
            for (i, (_, fval)) in fields.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
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
            out.push('(');
            out.push_str(&params.join(", "));
            out.push_str(") => ");
            if body.len() == 1 {
                if let StmtNode::Return(expr) = &body[0].node {
                    emit_expr(out, &expr.node);
                    return;
                }
            }
            out.push_str("{\n");
            for s in body {
                emit_stmt(out, &s.node, 1);
            }
            out.push('}');
        }

        ExprNode::Channel => {
            out.push_str("[]");  // Simple array as channel
        }

        ExprNode::Receive(chan) => {
            emit_expr(out, &chan.node);
            out.push_str(".shift()");
        }

        ExprNode::Borrow(inner) => {
            emit_expr(out, &inner.node);
        }

        ExprNode::TryExpr(inner) => {
            emit_expr(out, &inner.node);
        }

        ExprNode::Await(inner) => {
            out.push_str("await ");
            emit_expr(out, &inner.node);
        }
    }
}
