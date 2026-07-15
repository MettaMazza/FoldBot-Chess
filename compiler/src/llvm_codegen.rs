// LLVM IR backend for Ernos
// Translates Ernos C AST representation to textual LLVM IR via clang -S -emit-llvm

use std::process::Command;
use std::io::Write;
use crate::ast::Program;
use crate::codegen::Codegen;

pub struct LLVMCodegen;

impl LLVMCodegen {
    pub fn compile(
        program: &Program,
        stem: &str,
        is_test_mode: bool,
        opt_level: &[&str],
        sanitizer_flags: &[&str],
        link_flags: &[&str],
    ) -> Result<String, String> {
        // 1. Generate C source code using the standard Codegen
        let mut codegen = Codegen::new();
        codegen.is_test_mode = is_test_mode;
        let c_src = codegen.generate(program)?;

        // 2. Compile C source to LLVM IR using clang -S -emit-llvm
        let ll_path = format!("{}_compiled.ll", stem);
        
        let mut child = Command::new("clang")
            .arg("-S")
            .arg("-emit-llvm")
            .arg("-x")
            .arg("c")
            .arg("-o")
            .arg(&ll_path)
            .arg("-")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn clang for LLVM IR generation: {}", e))?;

        {
            let stdin = child.stdin.as_mut().ok_or("Failed to open stdin for LLVM generation")?;
            stdin.write_all(c_src.as_bytes()).map_err(|e| format!("Failed to write to clang: {}", e))?;
        }

        let output = child.wait_with_output().map_err(|e| format!("Failed to wait for clang: {}", e))?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Clang failed to emit LLVM IR: {}", err));
        }

        // 3. Compile and link the .ll file into the final native binary
        let mut clang_cmd = Command::new("clang");
        clang_cmd.arg(&ll_path)
                 .arg("-o")
                 .arg(stem);
        
        for flag in opt_level {
            clang_cmd.arg(flag);
        }
        for flag in sanitizer_flags {
            clang_cmd.arg(flag);
        }
        for flag in link_flags {
            clang_cmd.arg(flag);
        }

        let link_status = clang_cmd.status().map_err(|e| format!("Failed to run clang: {}", e))?;
        if !link_status.success() {
            return Err("Clang failed to compile/link the LLVM IR file".to_string());
        }

        Ok(ll_path)
    }
}
