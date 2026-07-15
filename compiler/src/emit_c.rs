/// ErnosPlain → C Emitter
///
/// Takes a parsed ErnosPlain Program AST and emits readable C source code.
/// This produces standalone, human-readable C — not the embedded runtime form from codegen.rs.

use crate::ast::*;
use std::collections::HashSet;

/// Recursively collect all variable names from Set statements in a body
fn collect_set_names(stmts: &[Stmt], names: &mut HashSet<String>) {
    for stmt in stmts {
        match &stmt.node {
            StmtNode::Set(name, _, _) => { names.insert(name.clone()); }
            StmtNode::If(_, then_b, else_b) => {
                collect_set_names(then_b, names);
                if let Some(eb) = else_b { collect_set_names(eb, names); }
            }
            StmtNode::RepeatWhile(_, body) => collect_set_names(body, names),
            StmtNode::ForEach(_, _, body) => collect_set_names(body, names),
            StmtNode::Match(_, arms) => {
                for (_, _, body) in arms { collect_set_names(body, names); }
            }
            _ => {}
        }
    }
}

/// Main entry point: emit C source from an ErnosPlain AST
pub fn emit_c_from_ep(program: &Program) -> String {
    let mut out = String::new();
    out.push_str("/* Auto-generated C from ErnosPlain */\n");
    out.push_str("#include <stdio.h>\n");
    out.push_str("#include <stdlib.h>\n");
    out.push_str("#include <string.h>\n");
    out.push_str("#include <stdint.h>\n\n");

    // Forward declarations for all functions
    for func in &program.functions {
        if func.name == "main" { continue; }
        out.push_str("long long ");
        out.push_str(&func.name);
        out.push('(');
        let params: Vec<String> = func.params.iter().map(|(n, _, _)| format!("long long {}", n)).collect();
        out.push_str(&params.join(", "));
        if params.is_empty() { out.push_str("void"); }
        out.push_str(");\n");
    }
    if !program.functions.is_empty() {
        out.push('\n');
    }

    // Struct definitions → C typedefs
    for sd in &program.struct_defs {
        emit_struct_def(&mut out, sd);
        out.push('\n');
    }

    // Enum definitions → C enums + structs
    for ed in &program.enum_defs {
        emit_enum_def(&mut out, ed);
        out.push('\n');
    }

    // Top-level constants → globals
    for stmt in &program.top_level_constants {
        if let StmtNode::Set(name, expr, _) = &stmt.node {
            out.push_str(&format!("long long {} = ", name));
            emit_expr(&mut out, &expr.node);
            out.push_str(";\n");
        }
    }
    if !program.top_level_constants.is_empty() {
        out.push('\n');
    }

    // Method definitions
    for md in &program.method_defs {
        emit_method_def(&mut out, md);
        out.push('\n');
    }

    // Functions
    for func in &program.functions {
        emit_function(&mut out, func);
        out.push('\n');
    }

    out
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("    ");
    }
}

fn emit_struct_def(out: &mut String, sd: &StructDef) {
    out.push_str(&format!("typedef struct {{\n"));
    for (name, ta, _) in &sd.fields {
        out.push_str(&format!("    {} {};\n", type_to_c(ta), name));
    }
    out.push_str(&format!("}} {};\n", sd.name));
}

fn type_to_c(ta: &TypeAnnotation) -> &'static str {
    match ta {
        TypeAnnotation::Int => "long long",
        TypeAnnotation::Float => "double",
        TypeAnnotation::Bool => "long long",
        TypeAnnotation::Str | TypeAnnotation::DynStr => "const char*",
        TypeAnnotation::List => "long long",  // Opaque list pointer
        _ => "long long",
    }
}

fn emit_enum_def(out: &mut String, ed: &EnumDef) {
    // Emit as enum for tag + struct for data
    out.push_str(&format!("typedef enum {{\n"));
    for (vname, _) in &ed.variants {
        out.push_str(&format!("    {}_{},\n", ed.name.to_uppercase(), vname.to_uppercase()));
    }
    out.push_str(&format!("}} {}_Tag;\n\n", ed.name));

    out.push_str(&format!("typedef struct {{\n"));
    out.push_str(&format!("    {}_Tag tag;\n", ed.name));
    out.push_str("    long long data[4];  /* variant payload */\n");
    out.push_str(&format!("}} {};\n", ed.name));
}

fn emit_method_def(out: &mut String, md: &MethodDef) {
    let func_name = format!("{}_{}", md.struct_name.to_lowercase(), md.name);
    let mut params = vec![format!("{}* self", md.struct_name)];
    let param_names: HashSet<String> = md.params.iter().map(|(n, _, _)| n.clone()).collect();
    for (name, _, _) in &md.params {
        params.push(format!("long long {}", name));
    }
    out.push_str(&format!("long long {}({}) {{\n", func_name, params.join(", ")));
    // Pre-declare all local variables (excluding parameters)
    let mut local_vars = HashSet::new();
    collect_set_names(&md.body, &mut local_vars);
    let locals: Vec<&String> = local_vars.iter().filter(|n| !param_names.contains(*n)).collect();
    if !locals.is_empty() {
        indent(out, 1);
        out.push_str(&format!("long long {};", locals.iter().map(|n| format!("{} = 0LL", n)).collect::<Vec<_>>().join(", ")));
        out.push('\n');
    }
    for stmt in &md.body {
        emit_stmt(out, &stmt.node, 1);
    }
    if md.body.is_empty() || !ends_with_return(&md.body) {
        out.push_str("    return 0;\n");
    }
    out.push_str("}\n");
}

fn ends_with_return(stmts: &[Stmt]) -> bool {
    stmts.last().map_or(false, |s| matches!(s.node, StmtNode::Return(_)))
}

fn emit_function(out: &mut String, func: &Function) {
    let params: Vec<String> = func.params.iter().map(|(n, _, _)| format!("long long {}", n)).collect();
    let param_names: HashSet<String> = func.params.iter().map(|(n, _, _)| n.clone()).collect();

    if func.name == "main" {
        out.push_str("int main(void) {\n");
    } else {
        out.push_str(&format!("long long {}({}) {{\n",
            func.name,
            if params.is_empty() { "void".to_string() } else { params.join(", ") }
        ));
    }

    // Pre-declare all local variables (excluding parameters)
    let mut local_vars = HashSet::new();
    collect_set_names(&func.body, &mut local_vars);
    let locals: Vec<&String> = local_vars.iter().filter(|n| !param_names.contains(*n)).collect();
    if !locals.is_empty() {
        indent(out, 1);
        out.push_str(&format!("long long {};\n", locals.iter().map(|n| format!("{} = 0LL", n)).collect::<Vec<_>>().join(", ")));
    }

    for stmt in &func.body {
        emit_stmt(out, &stmt.node, 1);
    }

    if func.name == "main" && !ends_with_return(&func.body) {
        out.push_str("    return 0;\n");
    } else if !ends_with_return(&func.body) && func.name != "main" {
        out.push_str("    return 0;\n");
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
            if is_string_expr(&expr.node) {
                out.push_str("printf(\"%s\\n\", (const char*)");
                emit_expr(out, &expr.node);
                out.push_str(");\n");
            } else {
                out.push_str("printf(\"%lld\\n\", (long long)");
                emit_expr(out, &expr.node);
                out.push_str(");\n");
            }
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
                        emit_stmt_inline(out, &eb[0].node, depth);
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
            out.push_str("{\n");
            indent(out, depth + 1);
            out.push_str("long long _foreach_list = ");
            emit_expr(out, &iterable.node);
            out.push_str(";\n");
            indent(out, depth + 1);
            out.push_str("long long _foreach_len = length_list(_foreach_list);\n");
            indent(out, depth + 1);
            out.push_str("for (long long _foreach_i = 0; _foreach_i < _foreach_len; _foreach_i++) {\n");
            indent(out, depth + 2);
            out.push_str(&format!("long long {} = get_list(_foreach_list, _foreach_i);\n", var));
            for s in body {
                emit_stmt(out, &s.node, depth + 2);
            }
            indent(out, depth + 1);
            out.push_str("}\n");
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
            let mut first = true;
            for (variant, bindings, body) in arms {
                indent(out, depth);
                if first {
                    out.push_str("if (");
                    first = false;
                } else {
                    out.push_str("} else if (");
                }
                emit_expr(out, &expr.node);
                out.push_str(&format!(" == {}) {{\n", variant));
                for (i, b) in bindings.iter().enumerate() {
                    indent(out, depth + 1);
                    out.push_str(&format!("long long {} = ", b));
                    emit_expr(out, &expr.node);
                    out.push_str(&format!(".data[{}];\n", i));
                }
                for s in body {
                    emit_stmt(out, &s.node, depth + 1);
                }
            }
            if !arms.is_empty() {
                indent(out, depth);
                out.push_str("}\n");
            }
        }

        StmtNode::Spawn(func_name, args) => {
            indent(out, depth);
            out.push_str(&format!("/* spawn {} — requires threading library */\n", func_name));
            indent(out, depth);
            out.push_str(&format!("{}(", func_name));
            for (i, a) in args.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                emit_expr(out, &a.node);
            }
            out.push_str(");\n");
        }

        StmtNode::Send(chan, val) => {
            indent(out, depth);
            out.push_str("/* send to channel */\n");
            indent(out, depth);
            out.push_str("channel_send(");
            emit_expr(out, &chan.node);
            out.push_str(", ");
            emit_expr(out, &val.node);
            out.push_str(");\n");
        }
    }
}

fn emit_stmt_inline(out: &mut String, stmt: &StmtNode, depth: usize) {
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
                    emit_stmt_inline(out, &eb[0].node, depth);
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

fn is_string_expr(expr: &ExprNode) -> bool {
    matches!(expr, ExprNode::StringLiteral(_))
}

fn emit_expr(out: &mut String, expr: &ExprNode) {
    match expr {
        ExprNode::Integer(n) => out.push_str(&format!("{}LL", n)),
        ExprNode::FloatLiteral(f) => out.push_str(&format!("{}", f)),
        ExprNode::BoolLiteral(b) => out.push_str(if *b { "1" } else { "0" }),
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
            let c_name = match name.as_str() {
                "concat" if args.len() == 2 => {
                    out.push_str("ep_concat(");
                    emit_expr(out, &args[0].node);
                    out.push_str(", ");
                    emit_expr(out, &args[1].node);
                    out.push(')');
                    return;
                }
                "int_to_string" | "ep_auto_to_string" => {
                    out.push_str("int_to_string(");
                    if !args.is_empty() { emit_expr(out, &args[0].node); }
                    out.push(')');
                    return;
                }
                "string_to_int" | "parse_int" => {
                    out.push_str("atoll((const char*)");
                    if !args.is_empty() { emit_expr(out, &args[0].node); }
                    out.push(')');
                    return;
                }
                _ => name.as_str(),
            };
            out.push_str(c_name);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                emit_expr(out, &a.node);
            }
            out.push(')');
        }

        ExprNode::MethodCall(obj, method, _args) => {
            // C has no methods — emit as function_name(obj, args...)
            emit_expr(out, &obj.node);
            out.push_str(&format!("/* .{}() */", method));
        }

        ExprNode::FieldAccess(obj, field) => {
            emit_expr(out, &obj.node);
            out.push('.');
            out.push_str(field);
        }

        ExprNode::StructCreate(name, fields) => {
            out.push_str(&format!("({})", name));
            out.push('{');
            for (i, (fname, fval)) in fields.iter().enumerate() {
                if i > 0 { out.push_str(", "); }
                out.push_str(&format!(".{} = ", fname));
                emit_expr(out, &fval.node);
            }
            out.push('}');
        }

        ExprNode::EnumCreate(_enum_name, variant, args) => {
            out.push_str(&format!("/* {} */", variant));
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
            // C has no list literals — emit create_list + append_list
            if elements.is_empty() {
                out.push_str("create_list()");
            } else {
                out.push_str("({ long long _lit = create_list(); ");
                for e in elements {
                    out.push_str("append_list(_lit, ");
                    emit_expr(out, &e.node);
                    out.push_str("); ");
                }
                out.push_str("_lit; })");
            }
        }

        ExprNode::Closure(params, _body) => {
            out.push_str(&format!("/* closure({}) */", params.join(", ")));
            out.push_str("0");  // C can't represent closures inline
        }

        ExprNode::Channel => {
            out.push_str("create_channel()");
        }

        ExprNode::Receive(chan) => {
            out.push_str("channel_recv(");
            emit_expr(out, &chan.node);
            out.push(')');
        }

        ExprNode::Borrow(inner) => {
            out.push('&');
            emit_expr(out, &inner.node);
        }

        ExprNode::TryExpr(inner) => {
            emit_expr(out, &inner.node);
        }

        ExprNode::Await(inner) => {
            out.push_str("/* await */ ");
            emit_expr(out, &inner.node);
        }
    }
}
