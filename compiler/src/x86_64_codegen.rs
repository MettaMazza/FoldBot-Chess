// Native x86_64 code generator for Ernos
// Translates AST to x86_64 assembly (System V ABI) for macOS and Linux

use std::collections::HashMap;
use crate::ast::*;

pub struct X86_64Codegen {
    asm: String,
    string_literals: Vec<String>,
    label_counter: usize,
    func_labels: HashMap<String, String>,
    var_offsets: HashMap<String, i64>,
    stack_size: i64,
    is_macos: bool,
    loop_break_label: Option<String>,
    loop_continue_label: Option<String>,
}

/// Escape a string for use in GAS .asciz directive
fn escape_asm_string(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'"'  => out.push_str("\\\""),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7e => out.push(b as char),
            _ => out.push_str(&format!("\\{:03o}", b)),
        }
    }
    out
}

impl X86_64Codegen {
    pub fn new(is_macos: bool) -> Self {
        X86_64Codegen {
            asm: String::new(),
            string_literals: Vec::new(),
            label_counter: 0,
            func_labels: HashMap::new(),
            var_offsets: HashMap::new(),
            stack_size: 0,
            is_macos,
            loop_break_label: None,
            loop_continue_label: None,
        }
    }

    fn sym(&self, name: &str) -> String {
        if self.is_macos {
            format!("_{}", name)
        } else {
            name.to_string()
        }
    }

    fn new_label(&mut self, prefix: &str) -> String {
        self.label_counter += 1;
        if self.is_macos {
            format!("L_{}_{}", prefix, self.label_counter)
        } else {
            format!(".L_{}_{}", prefix, self.label_counter)
        }
    }

    fn emit(&mut self, line: &str) {
        self.asm.push_str(line);
        self.asm.push('\n');
    }

    fn add_string(&mut self, s: &str) -> usize {
        let idx = self.string_literals.len();
        self.string_literals.push(s.to_string());
        idx
    }

    fn get_var_offset(&mut self, name: &str) -> i64 {
        if let Some(&off) = self.var_offsets.get(name) {
            return off;
        }
        self.stack_size += 8;
        let off = -(self.stack_size);
        self.var_offsets.insert(name.to_string(), off);
        off
    }

    /// Generate assembly for the entire program
    pub fn generate(&mut self, program: &Program) -> Result<String, String> {
        // Collect all function names
        for func in &program.functions {
            let label = if func.name == "main" {
                self.sym("main")
            } else {
                format!("_ep_{}", func.name)
            };
            self.func_labels.insert(func.name.clone(), label);
        }

        // Preamble
        if self.is_macos {
            self.emit(".section __TEXT,__text,regular,pure_instructions");
            self.emit(".build_version macos, 14, 0");
        } else {
            self.emit(".text");
        }
        self.emit(".intel_syntax noprefix");
        self.emit("");

        // Generate each function
        for func in &program.functions {
            self.gen_function(func)?;
        }

        // Entry point wrapper: main/entry calls ep_main and passes result to exit
        let main_sym = self.sym("main");
        self.emit(&format!(".globl {}", main_sym));
        self.emit(&format!("{}:", main_sym));
        self.emit("    push rbp");
        self.emit("    mov rbp, rsp");
        self.emit("    call _ep_main");
        // rax has the return value
        self.emit("    pop rbp");
        self.emit("    ret");
        self.emit("");

        // String literals
        if self.is_macos {
            if !self.string_literals.is_empty() {
                self.emit(".section __TEXT,__cstring,cstring_literals");
                let strings: Vec<_> = self.string_literals.iter().cloned().collect();
                for (i, s) in strings.iter().enumerate() {
                    self.emit(&format!("str_{}: .asciz \"{}\"", i, escape_asm_string(s)));
                }
                self.emit("");
            }
            self.emit(".section __TEXT,__cstring,cstring_literals");
            self.emit("fmt_int: .asciz \"%lld\\n\"");
            self.emit("fmt_str: .asciz \"%s\\n\"");
            self.emit("");
        } else {
            if !self.string_literals.is_empty() {
                self.emit(".section .rodata");
                let strings: Vec<_> = self.string_literals.iter().cloned().collect();
                for (i, s) in strings.iter().enumerate() {
                    self.emit(&format!(".Lstr_{}: .asciz \"{}\"", i, escape_asm_string(s)));
                }
                self.emit("");
            }
            self.emit(".section .rodata");
            self.emit(".Lfmt_int: .asciz \"%lld\\n\"");
            self.emit(".Lfmt_str: .asciz \"%s\\n\"");
            self.emit("");
        }

        // Suppress "missing .note.GNU-stack" linker warning on Linux
        if !self.is_macos {
            self.emit("");
            self.emit(".section .note.GNU-stack,\"\",@progbits");
        }

        Ok(self.asm.clone())
    }

    fn gen_function(&mut self, func: &Function) -> Result<(), String> {
        self.var_offsets.clear();
        self.stack_size = 0;

        // Pre-scan to allocate space for all variables
        let mut var_names = Vec::new();
        for param in &func.params {
            var_names.push(param.0.clone());
        }
        self.collect_var_names(&func.body, &mut var_names);

        // Allocate stack space
        for name in &var_names {
            self.get_var_offset(name);
        }
        // Round up stack frame size to 16-byte alignment
        let frame_size = ((self.stack_size + 15) / 16) * 16;

        let label = if func.name == "main" {
            "_ep_main".to_string()
        } else {
            self.func_labels.get(&func.name).cloned().unwrap_or(format!("_ep_{}", func.name))
        };

        self.emit(&format!(".globl {}", label));
        self.emit(&format!("{}:", label));

        // Prologue
        self.emit("    push rbp");
        self.emit("    mov rbp, rsp");
        if frame_size > 0 {
            self.emit(&format!("    sub rsp, {}", frame_size));
        }

        // Store parameters (rdi, rsi, rdx, rcx, r8, r9) to their stack slots
        let param_regs = ["rdi", "rsi", "rdx", "rcx", "r8", "r9"];
        for (i, param) in func.params.iter().enumerate() {
            if i < 6 {
                let off = self.var_offsets.get(&param.0).copied().unwrap_or(0);
                self.emit(&format!("    mov [rbp + {}], {}", off, param_regs[i]));
            }
        }

        let cleanup_label = format!("{}_cleanup", label);
        
        // Generate body
        for stmt in &func.body {
            self.gen_statement(stmt, &cleanup_label, frame_size)?;
        }

        // Cleanup and return
        self.emit(&format!("{}:", cleanup_label));
        if frame_size > 0 {
            self.emit("    mov rsp, rbp");
        }
        self.emit("    pop rbp");
        self.emit("    ret");
        self.emit("");

        Ok(())
    }

    fn collect_var_names(&self, stmts: &[Stmt], names: &mut Vec<String>) {
        for stmt in stmts {
            match &stmt.node {
                StmtNode::Set(name, _, _) => {
                    if !names.contains(name) {
                        names.push(name.clone());
                    }
                }
                StmtNode::If(_, then_branch, else_branch) => {
                    self.collect_var_names(then_branch, names);
                    if let Some(eb) = else_branch {
                        self.collect_var_names(eb, names);
                    }
                }
                StmtNode::RepeatWhile(_, body) => {
                    self.collect_var_names(body, names);
                }
                StmtNode::ForEach(var, _, body) => {
                    if !names.contains(var) {
                        names.push(var.clone());
                    }
                    for internal in &["_foreach_list", "_foreach_len", "_foreach_i"] {
                        let s = internal.to_string();
                        if !names.contains(&s) {
                            names.push(s);
                        }
                    }
                    self.collect_var_names(body, names);
                }
                _ => {}
            }
        }
    }

    fn var_fp_offset(&self, name: &str) -> i64 {
        self.var_offsets.get(name).copied().unwrap_or(0)
    }

    /// Check if an expression produces a string value (pointer to char data)
    fn is_string_expr(expr: &Expr) -> bool {
        match &expr.node {
            ExprNode::StringLiteral(_) => true,
            ExprNode::Call(name, _) => matches!(name.as_str(),
                "concat" | "ep_auto_to_string" | "int_to_string" |
                "string_upper" | "string_lower" | "substring" |
                "string_trim" | "string_repeat" | "string_replace" |
                "char_at" | "read_input" | "read_file"
            ),
            _ => false,
        }
    }

    fn gen_statement(&mut self, stmt: &Stmt, cleanup_label: &str, frame_size: i64) -> Result<(), String> {
        match &stmt.node {
            StmtNode::Display(expr) => {
                self.gen_expr(expr, "rax", frame_size)?;

                // System V x86_64: 1st arg in rdi, 2nd arg in rsi, al = 0 (no floats)
                if Self::is_string_expr(expr) {
                    if self.is_macos {
                        self.emit("    lea rdi, [rip + fmt_str]");
                    } else {
                        self.emit("    lea rdi, [rip + .Lfmt_str]");
                    }
                } else {
                    if self.is_macos {
                        self.emit("    lea rdi, [rip + fmt_int]");
                    } else {
                        self.emit("    lea rdi, [rip + .Lfmt_int]");
                    }
                }
                self.emit("    mov rsi, rax");
                self.emit("    xor al, al");
                let printf_sym = self.sym("printf");
                self.emit(&format!("    call {}", printf_sym));
            }

            StmtNode::Set(name, expr, _) => {
                self.gen_expr(expr, "rax", frame_size)?;
                let off = self.var_fp_offset(name);
                self.emit(&format!("    mov [rbp + {}], rax", off));
            }

            StmtNode::Return(expr) => {
                self.gen_expr(expr, "rax", frame_size)?;
                self.emit(&format!("    jmp {}", cleanup_label));
            }

            StmtNode::If(cond, then_branch, else_branch) => {
                let else_label = self.new_label("else");
                let end_label = self.new_label("endif");

                self.gen_expr(cond, "rax", frame_size)?;
                self.emit("    cmp rax, 0");
                self.emit(&format!("    je {}", else_label));

                for s in then_branch {
                    self.gen_statement(s, cleanup_label, frame_size)?;
                }
                self.emit(&format!("    jmp {}", end_label));

                self.emit(&format!("{}:", else_label));
                if let Some(eb) = else_branch {
                    for s in eb {
                        self.gen_statement(s, cleanup_label, frame_size)?;
                    }
                }
                self.emit(&format!("{}:", end_label));
            }

            StmtNode::RepeatWhile(cond, body) => {
                let loop_label = self.new_label("loop");
                let end_label = self.new_label("endloop");

                let prev_break = self.loop_break_label.take();
                let prev_continue = self.loop_continue_label.take();
                self.loop_break_label = Some(end_label.clone());
                self.loop_continue_label = Some(loop_label.clone());

                self.emit(&format!("{}:", loop_label));
                self.gen_expr(cond, "rax", frame_size)?;
                self.emit("    cmp rax, 0");
                self.emit(&format!("    je {}", end_label));

                for s in body {
                    self.gen_statement(s, cleanup_label, frame_size)?;
                }
                self.emit(&format!("    jmp {}", loop_label));
                self.emit(&format!("{}:", end_label));

                self.loop_break_label = prev_break;
                self.loop_continue_label = prev_continue;
            }

            StmtNode::ExprStmt(expr) => {
                self.gen_expr(expr, "rax", frame_size)?;
            }

            StmtNode::Break => {
                if let Some(ref lbl) = self.loop_break_label {
                    self.emit(&format!("    jmp {}", lbl));
                }
            }

            StmtNode::Continue => {
                if let Some(ref lbl) = self.loop_continue_label {
                    self.emit(&format!("    jmp {}", lbl));
                }
            }

            StmtNode::ForEach(var_name, iterable, body) => {
                let loop_label = self.new_label("foreach");
                let end_label = self.new_label("foreach_end");

                let prev_break = self.loop_break_label.take();
                let prev_continue = self.loop_continue_label.take();
                self.loop_break_label = Some(end_label.clone());
                self.loop_continue_label = Some(loop_label.clone());

                // Evaluate iterable, save as _foreach_list
                self.gen_expr(iterable, "rax", frame_size)?;
                let list_off = self.get_var_offset("_foreach_list");
                self.emit(&format!("    mov [rbp + {}], rax", list_off));

                // _foreach_len = length_list(_foreach_list)
                self.emit("    mov rdi, rax");
                let len_sym = self.sym("length_list");
                self.emit(&format!("    call {}", len_sym));
                let len_off = self.get_var_offset("_foreach_len");
                self.emit(&format!("    mov [rbp + {}], rax", len_off));

                // _foreach_i = 0
                let i_off = self.get_var_offset("_foreach_i");
                self.emit(&format!("    mov QWORD PTR [rbp + {}], 0", i_off));

                // Loop start
                self.emit(&format!("{}:", loop_label));
                self.emit(&format!("    mov rax, [rbp + {}]", i_off));
                self.emit(&format!("    cmp rax, [rbp + {}]", len_off));
                self.emit(&format!("    jge {}", end_label));

                // VAR = get_list(_foreach_list, _foreach_i)
                self.emit(&format!("    mov rdi, [rbp + {}]", list_off));
                self.emit(&format!("    mov rsi, [rbp + {}]", i_off));
                let get_sym = self.sym("get_list");
                self.emit(&format!("    call {}", get_sym));
                let var_off = self.var_fp_offset(var_name);
                self.emit(&format!("    mov [rbp + {}], rax", var_off));

                // Body
                for s in body {
                    self.gen_statement(s, cleanup_label, frame_size)?;
                }

                // _foreach_i++
                self.emit(&format!("    mov rax, [rbp + {}]", i_off));
                self.emit("    inc rax");
                self.emit(&format!("    mov [rbp + {}], rax", i_off));
                self.emit(&format!("    jmp {}", loop_label));

                self.emit(&format!("{}:", end_label));

                self.loop_break_label = prev_break;
                self.loop_continue_label = prev_continue;
            }

            _ => {
                return Err(format!("Native x86_64 backend: unsupported statement type: {:?}", stmt.node));
            }
        }
        Ok(())
    }

    fn gen_expr(&mut self, expr: &Expr, dest: &str, frame_size: i64) -> Result<(), String> {
        match &expr.node {
            ExprNode::Integer(n) => {
                self.emit(&format!("    mov {}, {}", dest, n));
            }

            ExprNode::StringLiteral(s) => {
                let idx = self.add_string(s);
                if self.is_macos {
                    self.emit(&format!("    lea {}, [rip + str_{}]", dest, idx));
                } else {
                    self.emit(&format!("    lea {}, [rip + .Lstr_{}]", dest, idx));
                }
            }

            ExprNode::Identifier(name) => {
                let off = self.var_fp_offset(name);
                self.emit(&format!("    mov {}, [rbp + {}]", dest, off));
            }

            ExprNode::Binary(left, op, right) => {
                self.gen_expr(left, "rax", frame_size)?;
                self.emit("    push rax");
                self.gen_expr(right, "rcx", frame_size)?;
                self.emit("    pop rax");

                match op {
                    Op::Add => self.emit("    add rax, rcx"),
                    Op::Sub => self.emit("    sub rax, rcx"),
                    Op::Mul => self.emit("    imul rax, rcx"),
                    Op::Div => {
                        self.emit("    cqo"); // Sign-extend rax into rdx:rax
                        self.emit("    idiv rcx");
                    }
                    Op::Mod => {
                        self.emit("    cqo");
                        self.emit("    idiv rcx");
                        self.emit("    mov rax, rdx"); // Remainder in rdx
                    }
                }
                if dest != "rax" {
                    self.emit(&format!("    mov {}, rax", dest));
                }
            }

            ExprNode::Comparison(left, cmp_op, right) => {
                self.gen_expr(left, "rax", frame_size)?;
                self.emit("    push rax");
                self.gen_expr(right, "rcx", frame_size)?;
                self.emit("    pop rax");

                self.emit("    cmp rax, rcx");
                let cond = match cmp_op {
                    CompOp::Equals => "e",
                    CompOp::NotEquals => "ne",
                    CompOp::LessThan => "l",
                    CompOp::GreaterThan => "g",
                    CompOp::LessEqual => "le",
                    CompOp::GreaterEqual => "ge",
                };
                self.emit(&format!("    set{} al", cond));
                self.emit("    movzx rax, al");
                if dest != "rax" {
                    self.emit(&format!("    mov {}, rax", dest));
                }
            }

            ExprNode::Logical(left, log_op, right) => {
                match log_op {
                    LogicalOp::And => {
                        let false_label = self.new_label("and_false");
                        let end_label = self.new_label("and_end");
                        self.gen_expr(left, "rax", frame_size)?;
                        self.emit("    cmp rax, 0");
                        self.emit(&format!("    je {}", false_label));
                        self.gen_expr(right, "rax", frame_size)?;
                        self.emit(&format!("    jmp {}", end_label));
                        self.emit(&format!("{}:", false_label));
                        self.emit("    mov rax, 0");
                        self.emit(&format!("{}:", end_label));
                    }
                    LogicalOp::Or => {
                        let true_label = self.new_label("or_true");
                        let end_label = self.new_label("or_end");
                        self.gen_expr(left, "rax", frame_size)?;
                        self.emit("    cmp rax, 0");
                        self.emit(&format!("    jne {}", true_label));
                        self.gen_expr(right, "rax", frame_size)?;
                        self.emit(&format!("    jmp {}", end_label));
                        self.emit(&format!("{}:", true_label));
                        self.emit("    mov rax, 1");
                        self.emit(&format!("{}:", end_label));
                    }
                }
                if dest != "rax" {
                    self.emit(&format!("    mov {}, rax", dest));
                }
            }

            ExprNode::Call(name, args) => {
                // System V x86_64 calling convention parameters
                let param_regs = ["rdi", "rsi", "rdx", "rcx", "r8", "r9"];
                
                // Evaluate arguments and push them to the stack temporarily
                for arg in args {
                    self.gen_expr(arg, "rax", frame_size)?;
                    self.emit("    push rax");
                }
                
                // Pop arguments in reverse order into System V argument registers
                for i in (0..args.len()).rev() {
                    if i < 6 {
                        self.emit(&format!("    pop {}", param_regs[i]));
                    } else {
                        // Args 7+ are passed on stack directly, they are already on the stack!
                        // But System V expects them on stack in order. For simplicity of the
                        // initial native codegen we limit to 6 registers for now.
                    }
                }

                // Call the function
                let target = self.func_labels.get(name).cloned()
                    .unwrap_or(self.sym(name));
                self.emit(&format!("    call {}", target));

                if dest != "rax" {
                    self.emit(&format!("    mov {}, rax", dest));
                }
            }

            ExprNode::UnaryNot(inner) => {
                self.gen_expr(inner, "rax", frame_size)?;
                self.emit("    cmp rax, 0");
                self.emit("    sete al");
                self.emit("    movzx rax, al");
                if dest != "rax" {
                    self.emit(&format!("    mov {}, rax", dest));
                }
            }

            ExprNode::BoolLiteral(b) => {
                self.emit(&format!("    mov {}, {}", dest, if *b { 1 } else { 0 }));
            }

            ExprNode::ListLiteral(elements) => {
                // create_list() takes no args, returns list ptr in rax
                let create_sym = self.sym("create_list");
                self.emit(&format!("    call {}", create_sym));
                // rax holds list ptr; save it
                self.emit("    push rax");

                for elem in elements {
                    // Evaluate element into rax (may clobber registers)
                    self.gen_expr(elem, "rax", frame_size)?;
                    // rax = element value; list ptr is on stack
                    self.emit("    mov rsi, rax");           // rsi = value (2nd arg)
                    self.emit("    mov rdi, [rsp]");         // rdi = list ptr (1st arg)
                    let append_sym = self.sym("append_list");
                    self.emit(&format!("    call {}", append_sym));
                    // NOTE: append_list returns the VALUE, not the list ptr
                    // Don't update stack — list pointer is stable
                }

                // Pop list pointer
                self.emit("    pop rax");
                if dest != "rax" {
                    self.emit(&format!("    mov {}, rax", dest));
                }
            }

            ExprNode::FloatLiteral(f) => {
                // Encode the float as a 64-bit integer (bit-cast)
                let bits = f.to_bits() as i64;
                self.emit(&format!("    movabs {}, {}", dest, bits));
            }

            _ => {
                return Err(format!("Native x86_64 backend: unsupported expression type: {:?}", expr.node));
            }
        }
        Ok(())
    }
}
