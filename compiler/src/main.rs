#![deny(unused_imports)]
pub mod token;
pub mod lexer;
pub mod ast;
pub mod parser;
pub mod codegen;
pub mod type_check;
pub mod diagnostics;
pub mod borrow_check;
pub mod optimizer;
pub mod arm64;
pub mod native_codegen;
pub mod x86_64_codegen;
pub mod llvm_codegen;
pub mod bind_c;
pub mod lsp;
pub mod transpile_py;
pub mod transpile_c;
pub mod transpile_js;
pub mod transpile_go;
pub mod transpile_rb;
pub mod transpile_rs;
pub mod transpile_java;
pub mod transpile_ts;
pub mod emit_python;
pub mod emit_js;
pub mod emit_c;

use std::env;
use std::fs;
use std::process::Command;
use std::path::{Path, PathBuf};
use std::collections::HashSet;

fn resolve_import_path(current_file: &Path, import_path: &str) -> PathBuf {
    let stdlib_modules = ["math", "hash", "net", "json", "string", "sql", "gui", "crypto", "fs", "http", "collections", "sort", "datetime", "os", "test", "log", "sync", "regex", "csv", "websocket", "static_server", "toml", "select", "structured"];
    if stdlib_modules.contains(&import_path) {
        let stdlib_path = Path::new("stdlib").join(format!("{}.ep", import_path));
        if stdlib_path.exists() {
            return stdlib_path;
        }
        if let Ok(exe_path) = env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let exe_stdlib = exe_dir.join("stdlib").join(format!("{}.ep", import_path));
                if exe_stdlib.exists() {
                    return exe_stdlib;
                }
            }
        }
    }

    let mut resolved = current_file.parent().unwrap_or(Path::new("")).join(import_path);
    if !resolved.exists() && !import_path.ends_with(".ep") {
        resolved.set_extension("ep");
    }
    if resolved.exists() {
        return resolved;
    }

    // Try resolving from local packages directory 'ernos_modules'
    // 1. Relative to the current source file being compiled
    if let Some(parent) = current_file.parent() {
        let rel_modules = parent.join("ernos_modules").join(import_path);
        let src_lib = rel_modules.join("src").join("lib.ep");
        if src_lib.exists() {
            return src_lib;
        }
        let lib_ep = rel_modules.join("lib.ep");
        if lib_ep.exists() {
            return lib_ep;
        }
    }

    // 2. Relative to CWD
    let ernos_modules_path = Path::new("ernos_modules").join(import_path);
    let src_lib = ernos_modules_path.join("src").join("lib.ep");
    if src_lib.exists() {
        return src_lib;
    }
    let lib_ep = ernos_modules_path.join("lib.ep");
    if lib_ep.exists() {
        return lib_ep;
    }

    resolved
}

fn parse_all_modules(
    entry_path: &Path,
    parsed_files: &mut HashSet<PathBuf>,
    all_functions: &mut Vec<ast::Function>,
    all_externals: &mut Vec<ast::ExternalFunction>,
    all_struct_defs: &mut Vec<ast::StructDef>,
    all_enum_defs: &mut Vec<ast::EnumDef>,
    all_method_defs: &mut Vec<ast::MethodDef>,
    all_trait_defs: &mut Vec<ast::TraitDef>,
    all_trait_impls: &mut Vec<ast::TraitImpl>,
    all_top_level_constants: &mut Vec<ast::Stmt>,
) -> Result<(), String> {
    let canonical_path = entry_path.canonicalize().map_err(|e| format!("Could not canonicalize path '{}': {}", entry_path.display(), e))?;
    
    if parsed_files.contains(&canonical_path) {
        return Ok(());
    }
    parsed_files.insert(canonical_path.clone());

    let source = fs::read_to_string(&canonical_path).map_err(|e| format!("Error reading file '{}': {}", canonical_path.display(), e))?;
    
    let mut lexer = lexer::Lexer::new(&source);
    let tokens = match lexer.tokenize() {
        Ok(toks) => toks,
        Err(e) => {
            print_diagnostic(canonical_path.to_str().unwrap_or(""), &source, &e.message, e.span.line, e.span.col);
            return Err("Lexer error".to_string());
        }
    };

    let mut parser = parser::Parser::new(tokens);
    let program = match parser.parse_program() {
        Ok(prog) => prog,
        Err(errors) => {
            for e in &errors {
                print_diagnostic(canonical_path.to_str().unwrap_or(""), &source, &e.message, e.span.line, e.span.col);
            }
            return Err(format!("{} parse error(s)", errors.len()));
        }
    };

    all_functions.extend(program.functions);
    all_externals.extend(program.externals);
    all_struct_defs.extend(program.struct_defs);
    all_enum_defs.extend(program.enum_defs);
    all_method_defs.extend(program.method_defs);
    all_trait_defs.extend(program.trait_defs);
    all_trait_impls.extend(program.trait_impls);
    all_top_level_constants.extend(program.top_level_constants);

    for (imp, alias) in program.imports {
        let resolved_path = resolve_import_path(&canonical_path, &imp);
        if !resolved_path.exists() {
            return Err(format!("Import error in '{}': module '{}' not found at '{}'", canonical_path.display(), imp, resolved_path.display()));
        }

        if let Some(ref prefix) = alias {
            // Aliased import: parse the module, then add BOTH prefixed and original names.
            // Prefixed names allow the caller to use alias_function_name().
            // Original names are needed because the module's own functions reference each other.
            let mut mod_funcs: Vec<ast::Function> = Vec::new();
            let mut mod_externs: Vec<ast::ExternalFunction> = Vec::new();
            let mut mod_structs: Vec<ast::StructDef> = Vec::new();
            let mut mod_enums: Vec<ast::EnumDef> = Vec::new();
            let mut mod_methods: Vec<ast::MethodDef> = Vec::new();
            let mut mod_traits: Vec<ast::TraitDef> = Vec::new();
            let mut mod_trait_impls: Vec<ast::TraitImpl> = Vec::new();
            let mut mod_constants: Vec<ast::Stmt> = Vec::new();
            parse_all_modules(&resolved_path, parsed_files, &mut mod_funcs, &mut mod_externs, &mut mod_structs, &mut mod_enums, &mut mod_methods, &mut mod_traits, &mut mod_trait_impls, &mut mod_constants)?;
            all_top_level_constants.extend(mod_constants);

            // Add original-named functions (for internal module calls)
            for f in &mod_funcs {
                all_functions.push(f.clone());
            }
            // Add prefixed aliases
            for f in mod_funcs {
                let mut aliased = f;
                aliased.name = format!("{}_{}", prefix, aliased.name);
                all_functions.push(aliased);
            }
            for e in &mod_externs {
                all_externals.push(e.clone());
            }
            for e in mod_externs {
                let mut aliased = e;
                aliased.name = format!("{}_{}", prefix, aliased.name);
                all_externals.push(aliased);
            }
            all_struct_defs.extend(mod_structs);
            all_enum_defs.extend(mod_enums);
            all_method_defs.extend(mod_methods);
            all_trait_defs.extend(mod_traits);
            all_trait_impls.extend(mod_trait_impls);
        } else {
            // Unaliased import: dump everything into global namespace (backward compatible)
            parse_all_modules(&resolved_path, parsed_files, all_functions, all_externals, all_struct_defs, all_enum_defs, all_method_defs, all_trait_defs, all_trait_impls, all_top_level_constants)?;
        }
    }

    Ok(())
}

struct PackageManifest {
    name: String,
    version: String,
    dependencies: Vec<(String, String)>,
}

fn parse_manifest(content: &str) -> Result<PackageManifest, String> {
    let mut name = String::new();
    let mut version = String::new();
    let mut dependencies = Vec::new();
    let mut section = "";

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            continue;
        }
        if let Some(idx) = line.find('=') {
            let key = line[..idx].trim();
            let mut val = line[idx + 1..].trim();
            if (val.starts_with('"') && val.ends_with('"')) || (val.starts_with('\'') && val.ends_with('\'')) {
                val = &val[1..val.len() - 1];
            }
            match section {
                "package" => {
                    if key == "name" {
                        name = val.to_string();
                    } else if key == "version" {
                        version = val.to_string();
                    }
                }
                "dependencies" => {
                    dependencies.push((key.to_string(), val.to_string()));
                }
                _ => {}
            }
        }
    }

    if name.is_empty() {
        return Err("Manifest missing package name".to_string());
    }
    Ok(PackageManifest { name, version, dependencies })
}

fn format_type_annotation(ta: &ast::TypeAnnotation) -> String {
    match ta {
        ast::TypeAnnotation::Int => "Int".to_string(),
        ast::TypeAnnotation::Float => "Float".to_string(),
        ast::TypeAnnotation::Bool => "Bool".to_string(),
        ast::TypeAnnotation::Str => "Str".to_string(),
        ast::TypeAnnotation::DynStr => "DynStr".to_string(),
        ast::TypeAnnotation::List => "List".to_string(),
        ast::TypeAnnotation::UserDefined(name) => name.clone(),
        ast::TypeAnnotation::Generic(name, params) => {
            if params.is_empty() {
                name.clone()
            } else {
                let params_str: Vec<_> = params.iter().map(format_type_annotation).collect();
                format!("{} of {}", name, params_str.join(" and "))
            }
        }
    }
}

fn doc_gen_module(
    file_path: &Path,
    parsed_files: &mut HashSet<PathBuf>,
    output_dir: &Path,
) -> Result<(), String> {
    let canonical_path = file_path.canonicalize().map_err(|e| format!("Could not canonicalize path '{}': {}", file_path.display(), e))?;
    if parsed_files.contains(&canonical_path) {
        return Ok(());
    }
    parsed_files.insert(canonical_path.clone());

    let source = fs::read_to_string(&canonical_path).map_err(|e| format!("Error reading file '{}': {}", canonical_path.display(), e))?;
    
    let mut lexer = lexer::Lexer::new(&source);
    let tokens = match lexer.tokenize() {
        Ok(toks) => toks,
        Err(e) => {
            print_diagnostic(canonical_path.to_str().unwrap_or(""), &source, &e.message, e.span.line, e.span.col);
            return Err("Lexer error".to_string());
        }
    };

    let mut parser = parser::Parser::new(tokens);
    let program = match parser.parse_program() {
        Ok(prog) => prog,
        Err(errors) => {
            for e in &errors {
                print_diagnostic(canonical_path.to_str().unwrap_or(""), &source, &e.message, e.span.line, e.span.col);
            }
            return Err(format!("{} parse error(s)", errors.len()));
        }
    };

    // Recursively parse imports first
    for (imp, _) in &program.imports {
        let resolved_path = resolve_import_path(&canonical_path, imp);
        if resolved_path.exists() {
            doc_gen_module(&resolved_path, parsed_files, output_dir)?;
        }
    }

    // Now document the current file
    let mut markdown = String::new();
    let filename = canonical_path.file_name().and_then(|s| s.to_str()).unwrap_or("module.ep");
    markdown.push_str(&format!("# API Reference: {}\n\n", filename));

    let mut has_docs = false;

    // 1. Structures
    let documented_structs: Vec<_> = program.struct_defs.iter().filter(|s| s.doc_comment.is_some()).collect();
    if !documented_structs.is_empty() {
        has_docs = true;
        markdown.push_str("## Structures\n\n");
        for sd in documented_structs {
            markdown.push_str(&format!("### `structure {}`\n\n", sd.name));
            if let Some(ref doc) = sd.doc_comment {
                markdown.push_str(&format!("{}\n\n", doc));
            }
            markdown.push_str("**Fields:**\n");
            for (fname, ftype, default_val) in &sd.fields {
                let ftype_str = format_type_annotation(ftype);
                let default_str = if let Some(ref def) = default_val {
                    format!(" (default: {:?})", def)
                } else {
                    "".to_string()
                };
                markdown.push_str(&format!("- `{}` as `{}`{}\n", fname, ftype_str, default_str));
            }
            markdown.push_str("\n");
        }
    }

    // 2. Choices (Enums)
    let documented_enums: Vec<_> = program.enum_defs.iter().filter(|e| e.doc_comment.is_some()).collect();
    if !documented_enums.is_empty() {
        has_docs = true;
        markdown.push_str("## Choices\n\n");
        for ed in documented_enums {
            markdown.push_str(&format!("### `choice {}`\n\n", ed.name));
            if let Some(ref doc) = ed.doc_comment {
                markdown.push_str(&format!("{}\n\n", doc));
            }
            markdown.push_str("**Variants:**\n");
            for (vname, fields) in &ed.variants {
                if fields.is_empty() {
                    markdown.push_str(&format!("- `variant {}`\n", vname));
                } else {
                    let fields_str: Vec<_> = fields.iter().map(|(fname, ftype)| {
                        format!("`{}` as `{}`", fname, format_type_annotation(ftype))
                    }).collect();
                    markdown.push_str(&format!("- `variant {}` with {}\n", vname, fields_str.join(" and ")));
                }
            }
            markdown.push_str("\n");
        }
    }

    // 3. Traits
    let documented_traits: Vec<_> = program.trait_defs.iter().filter(|t| t.doc_comment.is_some()).collect();
    if !documented_traits.is_empty() {
        has_docs = true;
        markdown.push_str("## Traits\n\n");
        for td in documented_traits {
            markdown.push_str(&format!("### `trait {}`\n\n", td.name));
            if let Some(ref doc) = td.doc_comment {
                markdown.push_str(&format!("{}\n\n", doc));
            }
            markdown.push_str("**Methods:**\n");
            for (mname, params, ret_type) in &td.method_signatures {
                let params_str: Vec<_> = params.iter().map(|(pname, is_borrow, ptype)| {
                    let borrow_prefix = if *is_borrow { "borrow " } else { "" };
                    let type_suffix = if let Some(ref pt) = ptype {
                        format!(" as {}", format_type_annotation(pt))
                    } else {
                        "".to_string()
                    };
                    format!("{}{}{}", borrow_prefix, pname, type_suffix)
                }).collect();
                let ret_str = if let Some(ref rt) = ret_type {
                    format!(" returning {}", format_type_annotation(rt))
                } else {
                    "".to_string()
                };
                let with_str = if params_str.is_empty() {
                    "".to_string()
                } else {
                    format!(" with {}", params_str.join(" and "))
                };
                markdown.push_str(&format!("- `define {}{}{}`\n", mname, with_str, ret_str));
            }
            markdown.push_str("\n");
        }
    }

    // 4. Functions
    let documented_funcs: Vec<_> = program.functions.iter().filter(|f| f.doc_comment.is_some()).collect();
    if !documented_funcs.is_empty() {
        has_docs = true;
        markdown.push_str("## Functions\n\n");
        for f in documented_funcs {
            let async_prefix = if f.is_async { "async " } else { "" };
            let params_str: Vec<_> = f.params.iter().map(|(pname, is_borrow, ptype)| {
                let borrow_prefix = if *is_borrow { "borrow " } else { "" };
                let type_suffix = if let Some(ref pt) = ptype {
                    format!(" as {}", format_type_annotation(pt))
                } else {
                    "".to_string()
                };
                format!("{}{}{}", borrow_prefix, pname, type_suffix)
            }).collect();
            let ret_str = if let Some(ref rt) = f.return_type {
                format!(" returning {}", format_type_annotation(rt))
            } else {
                "".to_string()
            };
            let with_str = if params_str.is_empty() {
                "".to_string()
            } else {
                format!(" with {}", params_str.join(" and "))
            };
            markdown.push_str(&format!("### `{}define {}{}{}`\n\n", async_prefix, f.name, with_str, ret_str));
            if let Some(ref doc) = f.doc_comment {
                markdown.push_str(&format!("{}\n\n", doc));
            }
        }
    }

    // 5. Methods
    let documented_methods: Vec<_> = program.method_defs.iter().filter(|m| m.doc_comment.is_some()).collect();
    if !documented_methods.is_empty() {
        has_docs = true;
        markdown.push_str("## Methods\n\n");
        for m in documented_methods {
            let params_str: Vec<_> = m.params.iter().map(|(pname, is_borrow, ptype)| {
                let borrow_prefix = if *is_borrow { "borrow " } else { "" };
                let type_suffix = if let Some(ref pt) = ptype {
                    format!(" as {}", format_type_annotation(pt))
                } else {
                    "".to_string()
                };
                format!("{}{}{}", borrow_prefix, pname, type_suffix)
            }).collect();
            let ret_str = if let Some(ref rt) = m.return_type {
                format!(" returning {}", format_type_annotation(rt))
            } else {
                "".to_string()
            };
            let with_str = if params_str.is_empty() {
                "".to_string()
            } else {
                format!(" with {}", params_str.join(" and "))
            };
            markdown.push_str(&format!("### `define {} on {}{}{}`\n\n", m.name, m.struct_name, with_str, ret_str));
            if let Some(ref doc) = m.doc_comment {
                markdown.push_str(&format!("{}\n\n", doc));
            }
        }
    }

    if has_docs {
        let out_file_stem = canonical_path.file_stem().and_then(|s| s.to_str()).unwrap_or("module");
        let out_file_path = output_dir.join(format!("{}.md", out_file_stem));
        fs::write(&out_file_path, &markdown).map_err(|e| format!("Error writing doc file '{}': {}", out_file_path.display(), e))?;
        println!("Generated documentation for {} -> {}", filename, out_file_path.display());
    }

    Ok(())
}

fn handle_package_command(args: &[String]) {
    if args.len() < 3 {
        eprintln!("Usage: ernos package <init|install|publish>");
        std::process::exit(1);
    }
    let sub = &args[2];
    match sub.as_str() {
        "init" => {
            let cur_dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("ernos_package"));
            let name = cur_dir.file_name().and_then(|n| n.to_str()).unwrap_or("ernos_package");
            let manifest_content = format!(
                "[package]\nname = \"{}\"\nversion = \"0.1.0\"\ndescription = \"An ErnosPlain package\"\n\n[dependencies]\n",
                name
            );
            let toml_path = Path::new("ernos.toml");
            if toml_path.exists() {
                eprintln!("Error: ernos.toml already exists!");
                std::process::exit(1);
            }
            fs::write(toml_path, manifest_content).expect("Failed to write ernos.toml");
            fs::create_dir_all("src").expect("Failed to create src dir");
            let lib_path = Path::new("src").join("lib.ep");
            if !lib_path.exists() {
                fs::write(&lib_path, "define greet:\n    display \"Hello from package!\"\n    return 0\n").expect("Failed to write src/lib.ep");
            }
            println!("Initialized package '{}' in current directory.", name);
        }
        "publish" => {
            let toml_content = fs::read_to_string("ernos.toml")
                .unwrap_or_else(|_| {
                    eprintln!("Error: ernos.toml not found in current directory!");
                    std::process::exit(1);
                });
            let manifest = parse_manifest(&toml_content).unwrap_or_else(|e| {
                eprintln!("Error parsing manifest: {}", e);
                std::process::exit(1);
            });
            let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
            let reg_dir = Path::new(&home).join(".ernos_registry").join(&manifest.name).join(&manifest.version);
            fs::create_dir_all(&reg_dir).expect("Failed to create registry dir");
            fs::copy("ernos.toml", reg_dir.join("ernos.toml")).expect("Failed to copy ernos.toml");
            if Path::new("src").exists() {
                let dst_src = reg_dir.join("src");
                fs::create_dir_all(&dst_src).expect("Failed to create src dir in registry");
                if let Ok(entries) = fs::read_dir("src") {
                    for entry in entries {
                        if let Ok(entry) = entry {
                            let path = entry.path();
                            if path.is_file() {
                                let filename = path.file_name().unwrap();
                                fs::copy(&path, dst_src.join(filename)).expect("Failed to copy file");
                            }
                        }
                    }
                }
            } else if Path::new("lib.ep").exists() {
                fs::copy("lib.ep", reg_dir.join("lib.ep")).expect("Failed to copy lib.ep");
            }
            println!("Successfully published package '{}' version {} to local registry.", manifest.name, manifest.version);
        }
        "install" => {
            let toml_content = fs::read_to_string("ernos.toml")
                .unwrap_or_else(|_| {
                    eprintln!("Error: ernos.toml not found in current directory!");
                    std::process::exit(1);
                });
            let manifest = parse_manifest(&toml_content).unwrap_or_else(|e| {
                eprintln!("Error parsing manifest: {}", e);
                std::process::exit(1);
            });
            
            fs::create_dir_all("ernos_modules").expect("Failed to create ernos_modules");
            for (dep_name, dep_ver) in &manifest.dependencies {
                let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
                let reg_dep_dir = Path::new(&home).join(".ernos_registry").join(dep_name).join(dep_ver);
                if !reg_dep_dir.exists() {
                    eprintln!("Error: Dependency '{}' version '{}' not found in registry!", dep_name, dep_ver);
                    std::process::exit(1);
                }
                let target_dir = Path::new("ernos_modules").join(dep_name);
                if target_dir.exists() {
                    let _ = fs::remove_dir_all(&target_dir);
                }
                fs::create_dir_all(&target_dir).expect("Failed to create module dir");
                
                fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
                    if !dst.exists() {
                        fs::create_dir_all(dst).map_err(|e| e.to_string())?;
                    }
                    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
                        let entry = entry.map_err(|e| e.to_string())?;
                        let path = entry.path();
                        let dest_path = dst.join(entry.file_name());
                        if path.is_dir() {
                            copy_dir_all(&path, &dest_path)?;
                        } else {
                            fs::copy(&path, &dest_path).map_err(|e| e.to_string())?;
                        }
                    }
                    Ok(())
                }
                copy_dir_all(&reg_dep_dir, &target_dir).unwrap_or_else(|e| {
                    eprintln!("Failed to copy package contents: {}", e);
                    std::process::exit(1);
                });
                println!("Installed dependency: {} ({})", dep_name, dep_ver);
            }
            println!("All dependencies installed successfully.");
        }
        _ => {
            eprintln!("Unknown package command: {}", sub);
            std::process::exit(1);
        }
    }
}

fn find_wasi_sysroot() -> Option<String> {
    let paths = [
        "/opt/homebrew/Cellar/wasi-libc/32/share/wasi-sysroot",
        "/opt/homebrew/opt/wasi-libc/share/wasi-sysroot",
        "/usr/local/share/wasi-sysroot",
        "/usr/share/wasi-sysroot",
    ];
    for p in &paths {
        if Path::new(p).exists() {
            return Some(p.to_string());
        }
    }
    // Check if homebrew has wasi-libc dynamically
    if let Ok(output) = Command::new("brew").args(["--prefix", "wasi-libc"]).output() {
        if output.status.success() {
            let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let p = format!("{}/share/wasi-sysroot", prefix);
            if Path::new(&p).exists() {
                return Some(p);
            }
        }
    }
    None
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    // Handle package manager subcommand
    if args[1] == "package" {
        handle_package_command(&args);
        return;
    }

    // Handle doc subcommand
    if args[1] == "doc" || args[1] == "--doc" {
        if args.len() < 3 {
            eprintln!("Usage: ernos doc <filename.ep> [-o output_dir]");
            std::process::exit(1);
        }
        let entry_file = &args[2];
        
        let output_dir_str = if let Some(idx) = args.iter().position(|a| a == "-o") {
            args.get(idx + 1).cloned().unwrap_or_else(|| "docs".to_string())
        } else {
            "docs".to_string()
        };
        let output_dir = Path::new(&output_dir_str);
        if let Err(e) = fs::create_dir_all(output_dir) {
            eprintln!("Error creating output directory '{}': {}", output_dir.display(), e);
            std::process::exit(1);
        }

        let mut parsed_files = HashSet::new();
        if let Err(e) = doc_gen_module(Path::new(entry_file), &mut parsed_files, output_dir) {
            eprintln!("Documentation generation failed: {}", e);
            std::process::exit(1);
        }
        println!("Documentation generated successfully in '{}'.", output_dir.display());
        return;
    }

    // Handle LSP mode
    if args[1] == "lsp" || args[1] == "--lsp" {
        lsp::run_lsp();
        return;
    }

    // Handle REPL mode
    if args[1] == "--repl" || args[1] == "repl" {
        run_repl();
        return;
    }

    // Handle 'ernos bind header.h [-o output.ep]' subcommand
    if args[1] == "bind" {
        if args.len() < 3 {
            eprintln!("Usage: ernos bind <header.h> [-o output.ep]");
            std::process::exit(1);
        }
        let header_path = &args[2];
        let source = match fs::read_to_string(header_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error reading '{}': {}", header_path, e);
                std::process::exit(1);
            }
        };

        let bindings = bind_c::emit_ernos_bindings(header_path, &source);

        // Determine output path
        let output_path = if let Some(idx) = args.iter().position(|a| a == "-o") {
            args.get(idx + 1).cloned().unwrap_or_else(|| {
                let stem = Path::new(header_path).file_stem()
                    .and_then(|s| s.to_str()).unwrap_or("bindings");
                format!("bindings_{}.ep", stem)
            })
        } else {
            let stem = Path::new(header_path).file_stem()
                .and_then(|s| s.to_str()).unwrap_or("bindings");
            format!("bindings_{}.ep", stem)
        };

        if let Err(e) = fs::write(&output_path, &bindings) {
            eprintln!("Error writing '{}': {}", output_path, e);
            std::process::exit(1);
        }

        println!("Generated bindings: {}", output_path);
        println!("  Parsed: {}", header_path);
        // Count what was generated
        let extern_count = bindings.matches("external define").count();
        let struct_count = bindings.matches("define structure").count();
        let const_count = bindings.matches("set BIND_").count();
        println!("  Functions: {}", extern_count);
        println!("  Structures: {}", struct_count);
        println!("  Constants: {}", const_count);
        return;
    }

    // Handle 'ernos transpile file.py [-o output.ep]' subcommand
    if args[1] == "transpile" {
        if args.len() < 3 {
            eprintln!("Usage: ernos transpile <file.py> [-o output.ep]");
            eprintln!("Supported: .py .c .h .js .mjs .go .rb .rs .java .ts .tsx");
            std::process::exit(1);
        }
        let source_path = &args[2];
        let ext = Path::new(source_path).extension()
            .and_then(|e| e.to_str()).unwrap_or("");

        let source = match fs::read_to_string(source_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error reading '{}': {}", source_path, e);
                std::process::exit(1);
            }
        };

        let (output, lang) = match ext {
            "py" => (transpile_py::emit_ernos_from_python(source_path, &source), "Python"),
            "c" | "h" => (transpile_c::emit_ernos_from_c(source_path, &source), "C"),
            "js" | "mjs" => (transpile_js::emit_ernos_from_js(source_path, &source), "JavaScript"),
            "go" => (transpile_go::emit_ernos_from_go(source_path, &source), "Go"),
            "rb" => (transpile_rb::emit_ernos_from_ruby(source_path, &source), "Ruby"),
            "rs" => (transpile_rs::emit_ernos_from_rust(source_path, &source), "Rust"),
            "java" => (transpile_java::emit_ernos_from_java(source_path, &source), "Java"),
            "ts" | "tsx" => (transpile_ts::emit_ernos_from_typescript(source_path, &source), "TypeScript"),
            _ => {
                eprintln!("Unsupported source language: .{}", ext);
                eprintln!("Supported: .py .c .h .js .mjs .go .rb .rs .java .ts .tsx");
                std::process::exit(1);
            }
        };

        let output_path = if let Some(idx) = args.iter().position(|a| a == "-o") {
            args.get(idx + 1).cloned().unwrap_or_else(|| {
                let stem = Path::new(source_path).file_stem()
                    .and_then(|s| s.to_str()).unwrap_or("transpiled");
                format!("{}.ep", stem)
            })
        } else {
            let stem = Path::new(source_path).file_stem()
                .and_then(|s| s.to_str()).unwrap_or("transpiled");
            format!("{}.ep", stem)
        };

        if let Err(e) = fs::write(&output_path, &output) {
            eprintln!("Error writing '{}': {}", output_path, e);
            std::process::exit(1);
        }

        println!("Transpiled {} → ErnosPlain: {}", lang, output_path);
        println!("  Source: {}", source_path);
        let func_count = output.matches("define ").count();
        let line_count = output.lines().count();
        println!("  Functions: {}", func_count);
        println!("  Lines: {}", line_count);
        return;
    }

    // Handle 'ernos emit file.ep --python|--js|--c [-o output]' subcommand
    if args[1] == "emit" {
        if args.len() < 4 {
            eprintln!("Usage: ernos emit <file.ep> --python|--js|--c [-o output]");
            std::process::exit(1);
        }
        let source_path = &args[2];
        let target = &args[3];

        let source = match fs::read_to_string(source_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error reading '{}': {}", source_path, e);
                std::process::exit(1);
            }
        };

        // Parse the ErnosPlain source
        let mut lex = lexer::Lexer::new(&source);
        let tokens = match lex.tokenize() {
            Ok(toks) => toks,
            Err(e) => {
                eprintln!("Lexer error: {}", e.message);
                std::process::exit(1);
            }
        };
        let mut par = parser::Parser::new(tokens);
        let program = match par.parse_program() {
            Ok(prog) => prog,
            Err(errors) => {
                for e in &errors {
                    eprintln!("Parser error at line {}:{}: {}", e.span.line, e.span.col, e.message);
                }
                std::process::exit(1);
            }
        };

        let (output, ext_out, lang) = match target.as_str() {
            "--python" | "--py" => (emit_python::emit_python_from_ep(&program), "py", "Python"),
            "--js" | "--javascript" => (emit_js::emit_js_from_ep(&program), "js", "JavaScript"),
            "--c" => (emit_c::emit_c_from_ep(&program), "c", "C"),
            _ => {
                eprintln!("Unsupported emit target: {}", target);
                eprintln!("Supported: --python, --js, --c");
                std::process::exit(1);
            }
        };

        let output_path = if let Some(idx) = args.iter().position(|a| a == "-o") {
            args.get(idx + 1).cloned().unwrap_or_else(|| {
                let stem = Path::new(source_path).file_stem()
                    .and_then(|s| s.to_str()).unwrap_or("emitted");
                format!("{}.{}", stem, ext_out)
            })
        } else {
            let stem = Path::new(source_path).file_stem()
                .and_then(|s| s.to_str()).unwrap_or("emitted");
            format!("{}.{}", stem, ext_out)
        };

        if let Err(e) = fs::write(&output_path, &output) {
            eprintln!("Error writing '{}': {}", output_path, e);
            std::process::exit(1);
        }

        println!("Emitted ErnosPlain → {}: {}", lang, output_path);
        println!("  Source: {}", source_path);
        println!("  Lines: {}", output.lines().count());
        return;
    }

    // Handle global flags
    if args[1] == "--version" || args[1] == "-v" {
        println!("Ernos Compiler v1.0.0");
        println!("  Backend: C (Clang) / native assembly");
        println!("  Target:  {}", std::env::consts::ARCH);
        println!("  OS:      {}", std::env::consts::OS);
        return;
    }

    if args[1] == "--help" || args[1] == "-h" {
        print_usage();
        return;
    }

    if args[1] == "--list-builtins" {
        println!("Ernos Built-in Functions:");
        println!();
        println!("  ── Strings ──");
        println!("  concat(a: Str, b: Str) -> Str              Concatenate two strings");
        println!("  string_length(s: Str) -> Int                Get string length");
        println!("  substring(s: Str, start: Int, len: Int) -> Str  Extract substring");
        println!("  int_to_string(n: Int) -> Str                Convert integer to string");
        println!("  get_character(s: Str, idx: Int) -> Int      Get character code at index");
        println!("  string_contains(s: Str, sub: Str) -> Int    Check if string contains substring");
        println!("  string_index_of(s: Str, sub: Str) -> Int    Find first index of substring (-1 if not found)");
        println!("  string_replace(s: Str, old: Str, new: Str) -> Str  Replace occurrences");
        println!("  string_from_list(list: List) -> Str         Build string from list of char codes");
        println!("  string_upper(s: Str) -> Str                  Convert to uppercase");
        println!("  string_lower(s: Str) -> Str                  Convert to lowercase");
        println!("  string_trim(s: Str) -> Str                   Strip leading/trailing whitespace");
        println!("  string_split(s: Str, delim: Str) -> List     Split string into list of parts");
        println!("  char_at(s: Str, index: Int) -> Int           Get character code at index");
        println!("  char_from_code(code: Int) -> Str             Create single-char string from code");
        println!("  f\"Hello {{name}}!\"                           F-string interpolation");
        println!();
        println!("  ── Lists ──");
        println!("  create_list() -> List                       Create empty list");
        println!("  append_list(list: List, item: Int) -> Int   Append item to list");
        println!("  get_list(list: List, index: Int) -> Int     Get item at index");
        println!("  set_list(list: List, index: Int, val: Int) -> Int  Set item at index");
        println!("  length_list(list: List) -> Int              Get list length");
        println!("  pop_list(list: List) -> Int                 Remove and return last element");
        println!("  free_list(list: List) -> Int                Manually free a list");
        println!();
        println!("  ── Maps (Key-Value) ──");
        println!("  create_map() -> Map                         Create empty hashmap");
        println!("  map_insert(map: Map, key, val) -> Int       Insert/update key-value pair (key: Str or Int)");
        println!("  map_get_val(map: Map, key: Str) -> Int      Get value by key (0 if not found)");
        println!("  map_contains(map: Map, key: Str) -> Int     Check if key exists (1/0)");
        println!("  map_delete(map: Map, key: Str) -> Int       Remove a key");
        println!("  map_keys(map: Map) -> List                  Get list of all keys");
        println!("  map_values(map: Map) -> List                Get list of all values");
        println!("  map_size(map: Map) -> Int                   Get number of entries");
        println!();
        println!("  ── I/O ──");
        println!("  read_line() -> Str                          Read line from stdin");
        println!("  read_int() -> Int                           Read integer from stdin");
        println!("  display <expr>                              Print value to stdout");
        println!();
        println!("  ── Math & Random ──");
        println!("  ep_random_int(min: Int, max: Int) -> Int    Random integer in range [min, max]");
        println!("  ep_abs(n: Int) -> Int                       Absolute value");
        println!();
        println!("  ── Concurrency ──");
        println!("  create_channel() -> Channel                 Create a message channel");
        println!("  send <value> to <channel>                   Send value to channel");
        println!("  receive from <channel> -> Int               Receive value from channel");
        println!("  spawn <function>(args...)                    Run function in new thread");
        println!("  ep_sleep_ms(ms: Int) -> Int                 Sleep for milliseconds");
        println!("  channel_select(chans: List, ms: Int) -> Int  Wait on multiple channels");
        println!("  channel_has_data(ch: Channel) -> Int         Check if channel has data");
        println!("  channel_try_recv(ch: Channel, out: Ptr) -> Int  Non-blocking receive");
        println!();
        println!("  ── File I/O ──");
        println!("  file_read(path: Str) -> Str                 Read entire file");
        println!("  file_write(path: Str, content: Str) -> Int  Write to file");
        println!("  file_append(path: Str, content: Str) -> Int Append to file");
        println!("  file_exists(path: Str) -> Int               Check if file exists");
        println!();
        println!("  ── Time ──");
        println!("  ep_time_now_ms() -> Int                     Current time in milliseconds");
        println!("  ep_time_now_sec() -> Int                    Current time in seconds (epoch)");
        println!();
        println!("  ── JSON ──");
        println!("  json_get_string(json: Str, key: Str) -> Str  Extract string value from JSON");
        println!("  json_get_int(json: Str, key: Str) -> Int     Extract integer value from JSON");
        println!("  json_get_bool(json: Str, key: Str) -> Int    Extract boolean value from JSON");
        println!();
        println!("  ── Hashing ──");
        println!("  ep_sha256(data: Str) -> Str                 SHA-256 hash (hex string)");
        println!("  ep_md5(data: Str) -> Str                    MD5 hash (hex string)");
        println!("  ep_sha1(data: Str) -> Str                   SHA-1 hash (hex string)");
        println!();
        println!("  ── Networking ──");
        println!("  ep_http_request(method: Str, url: Str, headers: Str, body: Str) -> Str");
        println!("  ep_net_connect(host: Str, port: Int) -> Int  TCP connect");
        println!("  ep_net_listen(port: Int) -> Int              TCP listen");
        println!("  ep_net_accept(fd: Int) -> Int                TCP accept");
        println!("  ep_net_send(fd: Int, data: Str) -> Int       Send data");
        println!("  ep_net_recv(fd: Int, max: Int) -> Str        Receive data");
        println!("  ep_net_close(fd: Int) -> Int                 Close connection");
        println!();
        return;
    }

    // Handle --check (syntax check only, no codegen)
    if args[1] == "--check" || args[1] == "check" {
        if args.len() < 3 {
            eprintln!("Usage: epc --check <filename.ep>");
            std::process::exit(1);
        }
        let input_path = Path::new(&args[2]);
        if !input_path.exists() {
            eprintln!("Error: File '{}' does not exist.", args[2]);
            std::process::exit(1);
        }
        let mut all_functions = Vec::new();
        let mut all_externals = Vec::new();
        let mut all_struct_defs = Vec::new();
        let mut all_enum_defs = Vec::new();
        let mut all_method_defs = Vec::new();
        let mut all_trait_defs = Vec::new();
        let mut all_trait_impls = Vec::new();
        let mut all_constants = Vec::new();
        let mut parsed_files = HashSet::new();
        match parse_all_modules(input_path, &mut parsed_files, &mut all_functions, &mut all_externals, &mut all_struct_defs, &mut all_enum_defs, &mut all_method_defs, &mut all_trait_defs, &mut all_trait_impls, &mut all_constants) {
            Ok(()) => {
                // Build program AST and run the type checker — not just syntax checking
                let program = ast::Program {
                    imports: Vec::new(),
                    externals: all_externals,
                    functions: all_functions.clone(),
                    struct_defs: all_struct_defs.clone(),
                    enum_defs: all_enum_defs.clone(),
                    method_defs: all_method_defs,
                    trait_defs: all_trait_defs,
                    trait_impls: all_trait_impls,
                    top_level_constants: all_constants,
                };

                let (type_errors, _type_warnings) = type_check::TypeChecker::check_full(&program);
                if !type_errors.is_empty() {
                    eprintln!("\n\x1b[1;31m── Type Errors ({}) ──\x1b[0m", type_errors.len());
                    for err in &type_errors {
                        eprintln!("  \x1b[1;31merror\x1b[0m: {}", err);
                    }
                    eprintln!();
                    eprintln!("\x1b[1;31m✗\x1b[0m {} — {} type error(s) found",
                        args[2], type_errors.len());
                    std::process::exit(1);
                }

                println!("\x1b[1;32m✓\x1b[0m {} — no errors ({} functions, {} structs, {} enums)", 
                    args[2], all_functions.len(), all_struct_defs.len(), all_enum_defs.len());
            }
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    // Handle --format (code formatter)
    if args[1] == "--format" || args[1] == "format" || args[1] == "fmt" {
        if args.len() < 3 {
            eprintln!("Usage: epc --format <filename.ep>");
            std::process::exit(1);
        }
        let input_path = Path::new(&args[2]);
        if !input_path.exists() {
            eprintln!("Error: File '{}' does not exist.", args[2]);
            std::process::exit(1);
        }
        format_file(input_path);
        return;
    }

    let is_test_mode = args.len() >= 3 && args[1] == "test";
    let input_path_str = if is_test_mode { &args[2] } else { &args[1] };
    let input_path = Path::new(input_path_str);
    
    if !input_path.exists() {
        eprintln!("Error: File '{}' does not exist.", input_path_str);
        std::process::exit(1);
    }

    // Place output binary next to the source file, not in CWD
    let stem = {
        let parent = input_path.parent().unwrap_or(Path::new("."));
        let name = input_path.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
        if parent == Path::new("") || parent == Path::new(".") {
            name.to_string()
        } else {
            format!("{}/{}", parent.display(), name)
        }
    };
    
    println!("[1/3] Tokenizing and Parsing '{}'...", input_path_str);

    let mut all_functions = Vec::new();
    let mut all_externals = Vec::new();
    let mut all_struct_defs = Vec::new();
    let mut all_enum_defs = Vec::new();
    let mut all_method_defs = Vec::new();
    let mut all_trait_defs = Vec::new();
    let mut all_trait_impls = Vec::new();
    let mut all_top_level_constants = Vec::new();
    let mut parsed_files = HashSet::new();
    if let Err(err_msg) = parse_all_modules(input_path, &mut parsed_files, &mut all_functions, &mut all_externals, &mut all_struct_defs, &mut all_enum_defs, &mut all_method_defs, &mut all_trait_defs, &mut all_trait_impls, &mut all_top_level_constants) {
        eprintln!("Compiler Error: {}", err_msg);
        std::process::exit(1);
    }

    // Deduplicate functions (aliased imports add both original and prefixed names)
    let mut function_names = HashSet::new();
    let mut deduped_functions = Vec::new();
    for func in all_functions {
        if function_names.insert(func.name.clone()) {
            deduped_functions.push(func);
        }
        // Silently skip duplicates — expected from aliased imports
    }
    let all_functions = deduped_functions;

    let mut program = ast::Program {
        imports: Vec::new(),
        externals: all_externals,
        functions: all_functions,
        struct_defs: all_struct_defs,
        enum_defs: all_enum_defs,
        method_defs: all_method_defs,
        trait_defs: all_trait_defs,
        trait_impls: all_trait_impls,
        top_level_constants: all_top_level_constants,
    };

    // Validate that main function exists
    if !is_test_mode {
        let has_main = program.functions.iter().any(|f| f.name == "main");
        if !has_main {
            eprintln!("Compiler Error: Every program must have a 'main' function.");
            std::process::exit(1);
        }
    }

    // Validate compile flags — reject unknown --flags instead of silently ignoring them
    let known_flags = ["--native", "--llvm", "--release", "--debug", "--asan", "--sanitize", "--wasm"];
    let start_idx = if is_test_mode { 3 } else { 2 }; // skip binary name + source/test arg
    for arg in args.iter().skip(start_idx) {
        if arg.starts_with("--") && !known_flags.contains(&arg.as_str()) {
            eprintln!("Error: Unknown flag '{}'", arg);
            eprintln!("Valid flags: {}", known_flags.join(", "));
            std::process::exit(1);
        }
    }

    let use_wasm = args.iter().any(|a| a == "--wasm");
    if use_wasm && args.iter().any(|a| a == "--native") {
        eprintln!("Error: Cannot specify both --native and --wasm.");
        std::process::exit(1);
    }

    // Check if native/LLVM backend is requested or auto-detected
    let llc_available = Command::new("llc").arg("--version").output().is_ok();
    let use_llvm = !use_wasm && (args.iter().any(|a| a == "--llvm") || (llc_available && !args.iter().any(|a| a == "--native")));
    let use_native = !use_wasm && args.iter().any(|a| a == "--native");

    if use_native {
        let arch = std::env::consts::ARCH;
        let os = std::env::consts::OS;
        
        let (asm, is_x86_64) = if arch == "x86_64" {
            println!("[2/3] Generating Native x86_64 Assembly...");
            let mut ncg = x86_64_codegen::X86_64Codegen::new(os == "macos");
            (ncg.generate(&program), true)
        } else {
            println!("[2/3] Generating Native ARM64 Assembly...");
            let mut ncg = native_codegen::NativeCodegen::new(os == "macos");
            (ncg.generate(&program), false)
        };

        let asm = match asm {
            Ok(a) => a,
            Err(e) => {
                eprintln!("Native Code Generation Error: {}", e);
                std::process::exit(1);
            }
        };

        // Write assembly to temp file
        let asm_path = format!("{}_native.s", stem);
        if let Err(e) = fs::write(&asm_path, &asm) {
            eprintln!("Error writing assembly: {}", e);
            std::process::exit(1);
        }

        println!("[3/4] Assembling Native Code...");

        // Assemble with system 'as'
        let obj_path = format!("{}_native.o", stem);
        let as_status = if os == "macos" {
            let target_arch = if is_x86_64 { "x86_64" } else { "arm64" };
            Command::new("as")
                .arg("-arch").arg(target_arch)
                .arg("-o").arg(&obj_path)
                .arg(&asm_path)
                .status()
        } else {
            Command::new("as")
                .arg("-o").arg(&obj_path)
                .arg(&asm_path)
                .status()
        };
        match as_status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                eprintln!("Error: Assembler failed with exit code: {}", s);
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("Error invoking assembler: {}", e);
                std::process::exit(1);
            }
        }

        // Hybrid compilation: compile C runtime to .o, then link with native .o
        // This provides all the runtime functions (create_list, length_list, channels, GC, etc.)
        // that the native assembly references as external symbols.
        println!("[4/4] Compiling C Runtime and Linking...");

        let mut cg = codegen::Codegen::new();
        let runtime_c_src = cg.emit_runtime_c(&program);
        let runtime_c_path = format!("{}_runtime.c", stem);
        if let Err(e) = fs::write(&runtime_c_path, &runtime_c_src) {
            eprintln!("Error writing runtime C source: {}", e);
            std::process::exit(1);
        }

        // Compile runtime C to .o
        let runtime_obj_path = format!("{}_runtime.o", stem);
        let cc = if Command::new("clang").arg("--version").output().is_ok() { "clang" } else { "gcc" };
        let cc_status = Command::new(cc)
            .arg("-c")
            .arg("-O2")
            .arg("-o").arg(&runtime_obj_path)
            .arg(&runtime_c_path)
            .status();
        match cc_status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                eprintln!("Error: C compiler failed on runtime with exit code: {}", s);
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("Error invoking C compiler for runtime: {}", e);
                std::process::exit(1);
            }
        }

        // Link native assembly .o + C runtime .o into final binary
        let ld_status = if os == "macos" {
            let target_arch = if is_x86_64 { "x86_64" } else { "arm64" };
            Command::new("ld")
                .arg("-o").arg(&stem)
                .arg(&obj_path)
                .arg(&runtime_obj_path)
                .arg("-lSystem")
                .arg("-syslibroot").arg("/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk")
                .arg("-arch").arg(target_arch)
                .status()
        } else {
            // Linux: use gcc to link, include pthread and math libs
            Command::new("gcc")
                .arg("-no-pie")
                .arg("-o").arg(&stem)
                .arg(&obj_path)
                .arg(&runtime_obj_path)
                .arg("-lpthread")
                .arg("-lm")
                .arg("-ldl")
                .status()
        };

        match ld_status {
            Ok(s) if s.success() => {
                // Clean up temp files
                let _ = fs::remove_file(&asm_path);
                let _ = fs::remove_file(&obj_path);
                let _ = fs::remove_file(&runtime_c_path);
                let _ = fs::remove_file(&runtime_obj_path);
                println!("\nSuccessfully compiled into native binary: {}", stem);
            }
            Ok(s) => {
                eprintln!("Error: Linker failed with exit code: {}", s);
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("Error invoking linker: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    // Type checking (Phase 1A) — HARD ERRORS: reject programs with type errors
    let (type_errors, _type_warnings) = type_check::TypeChecker::check_full(&program);
    if !type_errors.is_empty() {
        eprintln!("\n\x1b[1;31m── Type Errors ({}) ──\x1b[0m", type_errors.len());
        for err in &type_errors {
            eprintln!("  \x1b[1;31merror\x1b[0m: {}", err);
        }
        eprintln!();
        eprintln!("\x1b[1;31mCompilation failed:\x1b[0m {} type error(s) found. Fix all type errors before compiling.", type_errors.len());
        std::process::exit(1);
    }

    // Borrow checking (Phase 3) — HARD ERRORS: reject programs with ownership violations
    let borrow_errors = borrow_check::BorrowChecker::check(&program);
    if !borrow_errors.is_empty() {
        eprintln!("\n\x1b[1;31m── Ownership Errors ({}) ──\x1b[0m", borrow_errors.len());
        for err in &borrow_errors {
            eprint!("{}", err);
        }
        eprintln!();
        eprintln!("\x1b[1;31mCompilation failed:\x1b[0m {} ownership/borrowing error(s) found. Fix all safety violations before compiling.", borrow_errors.len());
        std::process::exit(1);
    }
    // Optimization pass (Phase 4B)
    let opt_stats = optimizer::Optimizer::run(&mut program);
    if opt_stats.constants_folded > 0 || opt_stats.dead_stmts_eliminated > 0 {
        eprintln!("\x1b[2m  optimizer: {} constants folded, {} dead statements eliminated\x1b[0m",
            opt_stats.constants_folded, opt_stats.dead_stmts_eliminated);
    }

    let output_executable = if use_wasm {
        if stem.ends_with(".wasm") {
            stem.clone()
        } else {
            format!("{}.wasm", stem)
        }
    } else if stem.contains('/') {
        stem.clone()
    } else {
        format!("./{}", stem)
    };

    let mut link_flags = Vec::new();
    if !use_wasm {
        link_flags.push("-lpthread");
        // dlopen/dlsym/dlclose require -ldl on Linux (macOS has it in libSystem)
        if std::env::consts::OS != "macos" {
            link_flags.push("-ldl");
        }
        for path in &parsed_files {
            let path_str = path.to_string_lossy();
            if path_str.ends_with("sql.ep") {
                link_flags.push("-DEP_HAS_SQLITE");
                link_flags.push("-lsqlite3");
            }
            if path_str.ends_with("gui.ep") {
                link_flags.push("-lraylib");
            }
            if path_str.ends_with("crypto.ep") {
                link_flags.push("-L/opt/homebrew/opt/openssl/lib");
                link_flags.push("-lcrypto");
            }
        }
    }

    // Determine optimization level and sanitizer flags
    let use_asan = args.iter().any(|a| a == "--asan" || a == "--sanitize");
    let opt_level = if args.iter().any(|a| a == "--release") {
        vec!["-O3", "-DNDEBUG", "-flto"]
    } else if args.iter().any(|a| a == "--debug") {
        vec!["-O0", "-g"]
    } else {
        vec!["-O2"]
    };

    let sanitizer_flags: Vec<&str> = if use_asan {
        vec!["-fsanitize=address", "-fsanitize=undefined", "-fno-omit-frame-pointer", "-g"]
    } else {
        vec![]
    };

    if use_llvm {
        println!("[2/3] Generating LLVM IR...");
        println!("[3/3] Compiling and Linking via Clang (LLVM IR backend)...");
        match llvm_codegen::LLVMCodegen::compile(
            &program,
            &stem,
            is_test_mode,
            &opt_level,
            &sanitizer_flags,
            &link_flags,
        ) {
            Ok(ll_path) => {
                #[cfg(target_os = "macos")]
                {
                    let _ = Command::new("codesign")
                        .arg("--force")
                        .arg("-s")
                        .arg("-")
                        .arg(&stem)
                        .status();
                }
                println!("\nSuccessfully compiled into native binary: {}", output_executable);
                println!("  LLVM IR text file written to: {}", ll_path);
            }
            Err(e) => {
                eprintln!("LLVM IR Compilation Error: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        println!("[2/3] Generating C Code...");

        // Code generation (C backend)
        let mut codegen = codegen::Codegen::new();
        codegen.is_test_mode = is_test_mode;
        let assembly = match codegen.generate(&program) {
            Ok(asm) => asm,
            Err(e) => {
                eprintln!("Code Generation Error: {}", e);
                std::process::exit(1);
            }
        };

        // Write temporary C source file
        let c_path_str = format!("{}_compiled.c", stem);
        let c_path = Path::new(&c_path_str);
        if let Err(e) = fs::write(c_path, &assembly) {
            eprintln!("Error writing compiled C file: {}", e);
            std::process::exit(1);
        }

        println!("[3/3] Compiling and Linking via Clang...");

        let mut clang_cmd = Command::new("clang");
        clang_cmd.arg(&c_path_str)
                 .arg("-o")
                 .arg(&output_executable);
        if use_wasm {
            clang_cmd.arg("--target=wasm32-wasi");
            clang_cmd.arg("-D_WASI_EMULATED_SIGNAL");
            clang_cmd.arg("-lwasi-emulated-signal");
            clang_cmd.arg("-D_WASI_EMULATED_GETPID");
            clang_cmd.arg("-lwasi-emulated-getpid");
            if let Some(sysroot) = find_wasi_sysroot() {
                clang_cmd.arg(format!("--sysroot={}", sysroot));
            } else {
                eprintln!("Warning: wasi-sysroot not found. WebAssembly compilation might fail.");
                eprintln!("Please install it via: brew install wasi-libc");
            }
        }
        for flag in &opt_level {
            clang_cmd.arg(flag);
        }
        for flag in &sanitizer_flags {
            clang_cmd.arg(flag);
        }
        for flag in &link_flags {
            clang_cmd.arg(flag);
        }
        let clang_status = clang_cmd.status();

        match clang_status {
            Ok(status) if status.success() => {
                #[cfg(target_os = "macos")]
                {
                    if !use_wasm {
                        let _ = Command::new("codesign")
                            .arg("--force")
                            .arg("-s")
                            .arg("-")
                            .arg(&stem)
                            .status();
                    }
                }
                if use_wasm {
                    // Generate HTML and JS wrappers
                    let wasm_filename = Path::new(&output_executable)
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or("program.wasm");
                    
                    let html_content = format!(
                        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>ErnosPlain WebAssembly Runner</title>
    <style>
        body {{
            font-family: 'Outfit', 'Inter', sans-serif;
            background: #0f172a;
            color: #f8fafc;
            display: flex;
            flex-direction: column;
            align-items: center;
            padding: 2rem;
            margin: 0;
            min-height: 100vh;
        }}
        h1 {{
            color: #38bdf8;
            margin-bottom: 0.5rem;
        }}
        .container {{
            width: 100%;
            max-width: 800px;
            background: #1e293b;
            border-radius: 12px;
            padding: 1.5rem;
            box-shadow: 0 10px 15px -3px rgba(0, 0, 0, 0.5);
            border: 1px solid #334155;
        }}
        #console {{
            background: #020617;
            color: #10b981;
            font-family: 'Fira Code', 'Courier New', monospace;
            padding: 1rem;
            border-radius: 6px;
            height: 400px;
            overflow-y: auto;
            white-space: pre-wrap;
            border: 1px solid #1e293b;
        }}
        .status {{
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 1rem;
            font-size: 0.9rem;
            color: #94a3b8;
        }}
        .badge {{
            background: #0369a1;
            color: #e0f2fe;
            padding: 0.25rem 0.75rem;
            border-radius: 9999px;
            font-weight: 600;
        }}
    </style>
</head>
<body>
    <h1>ErnosPlain WebAssembly</h1>
    <div class="container">
        <div class="status">
            <span>Status: <strong id="status-text" style="color: #34d399;">Loading...</strong></span>
            <span class="badge">WASM Target</span>
        </div>
        <div id="console"></div>
    </div>

    <script>
        const consoleEl = document.getElementById('console');
        const statusEl = document.getElementById('status-text');

        function print(text) {{
            consoleEl.textContent += text;
            consoleEl.scrollTop = consoleEl.scrollHeight;
        }}

        async function runWasm() {{
            try {{
                const response = await fetch('{wasm_file}');
                const bytes = await response.arrayBuffer();
                
                let memory;
                const wasiMock = {{
                    fd_write: (fd, iovs, iovs_len, nwritten) => {{
                        const view = new DataView(memory.buffer);
                        let total = 0;
                        let text = '';
                        for (let i = 0; i < iovs_len; i++) {{
                            const ptr = view.getUint32(iovs + i * 8, true);
                            const len = view.getUint32(iovs + i * 8 + 4, true);
                            const bytes = new Uint8Array(memory.buffer, ptr, len);
                            text += new TextDecoder('utf-8').decode(bytes);
                            total += len;
                        }}
                        view.setUint32(nwritten, total, true);
                        print(text);
                        return 0;
                    }},
                    proc_exit: (code) => {{
                        statusEl.textContent = `Exited with code ${{code}}`;
                        statusEl.style.color = code === 0 ? '#34d399' : '#f87171';
                        return 0;
                    }},
                    fd_close: () => 0,
                    fd_seek: () => 0,
                    environ_sizes_get: (environ_count, environ_buf_size) => {{
                        const view = new DataView(memory.buffer);
                        view.setUint32(environ_count, 0, true);
                        view.setUint32(environ_buf_size, 0, true);
                        return 0;
                    }},
                    environ_get: () => 0,
                    args_sizes_get: (argc_ptr, argv_buf_size) => {{
                        const view = new DataView(memory.buffer);
                        view.setUint32(argc_ptr, 0, true);
                        view.setUint32(argv_buf_size, 0, true);
                        return 0;
                    }},
                    args_get: () => 0,
                    clock_time_get: (id, precision, time_out) => {{
                        const view = new DataView(memory.buffer);
                        const ms = Date.now();
                        const ns = BigInt(ms) * 1000000n;
                        view.setBigUint64(time_out, ns, true);
                        return 0;
                    }},
                    sched_yield: () => 0,
                    fd_fdstat_get: (fd, stat) => 0,
                    fd_prestat_get: (fd, buf) => 8,
                    fd_prestat_dir_name: () => 8
                }};

                const importObject = {{
                    wasi_snapshot_preview1: wasiMock
                }};

                const {{ instance }} = await WebAssembly.instantiate(bytes, importObject);
                memory = instance.exports.memory;
                statusEl.textContent = 'Running...';
                
                if (instance.exports._start) {{
                    instance.exports._start();
                }} else if (instance.exports.main) {{
                    instance.exports.main();
                }}
                
                if (statusEl.textContent === 'Running...') {{
                    statusEl.textContent = 'Completed (0)';
                }}
            }} catch (err) {{
                statusEl.textContent = 'Error';
                statusEl.style.color = '#f87171';
                print(`\\nError: ${{err.message}}\\n${{err.stack}}`);
            }}
        }}

        runWasm();
    </script>
</body>
</html>"#,
                        wasm_file = wasm_filename
                    );

                    let js_content = format!(
                        r#"const fs = require('fs');
const path = require('path');

let WASI;
try {{
    WASI = require('wasi').WASI;
}} catch (e) {{
    try {{
        const {{ WASI: NodeWASI }} = require('wasi');
        WASI = NodeWASI;
    }} catch (err) {{
        console.error('WASI support not found. Please run with Node.js version >= 12.');
        process.exit(1);
    }}
}}

const wasmPath = path.join(__dirname, '{wasm_file}');
if (!fs.existsSync(wasmPath)) {{
    console.error(`Error: WebAssembly module not found at ${{wasmPath}}`);
    process.exit(1);
}}

const wasi = new WASI({{
    args: process.argv.slice(1),
    env: process.env,
    preopens: {{
        '.': '.'
    }}
}});

const importObject = {{
    wasi_snapshot_preview1: wasi.wasiImport
}};

(async () => {{
    const wasmBuffer = fs.readFileSync(wasmPath);
    const {{ instance }} = await WebAssembly.instantiate(wasmBuffer, importObject);
    wasi.start(instance);
}})();"#,
                        wasm_file = wasm_filename
                    );

                    let html_path = format!("{}.html", stem);
                    let js_path = format!("{}.js", stem);
                    let _ = fs::write(&html_path, html_content);
                    let _ = fs::write(&js_path, js_content);

                    println!("\nSuccessfully compiled into WebAssembly module: {}", output_executable);
                    println!("  HTML runner generated at: {}", html_path);
                    println!("  JS runner generated at: {}", js_path);
                } else {
                    println!("\nSuccessfully compiled into native binary: {}", output_executable);
                }
            }
            Ok(status) => {
                eprintln!("Error: Clang compilation failed with exit code: {}", status);
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("Error invoking Clang: {}", e);
                std::process::exit(1);
            }
        }
    }
}

fn print_diagnostic(file_path: &str, source: &str, message: &str, line: usize, col: usize) {
    let lines: Vec<&str> = source.lines().collect();
    eprintln!("\x1b[1;31mError\x1b[0m: \x1b[1m{}\x1b[0m", message);
    eprintln!("  \x1b[1;34m-->\x1b[0m {}:{}:{}", file_path, line, col);
    eprintln!("   \x1b[1;34m|\x1b[0m");
    if line > 0 && line <= lines.len() {
        let line_content = lines[line - 1];
        eprintln!(" \x1b[1;34m{:3} |\x1b[0m {}", line, line_content);
        let padding = " ".repeat(if col > 0 { col - 1 } else { 0 });
        eprintln!("   \x1b[1;34m|\x1b[0m {}\x1b[1;31m^\x1b[0m", padding);
        if let Some(suggestion) = get_suggestion(message) {
            eprintln!("   \x1b[1;34m|\x1b[0m {}\x1b[1;33mHelp: {}\x1b[0m", padding, suggestion);
        }
    }
    eprintln!("   \x1b[1;34m|\x1b[0m");
}

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

fn find_closest_match<'a>(target: &str, candidates: &[&'a str], max_distance: usize) -> Option<&'a str> {
    let mut best: Option<(&str, usize)> = None;
    for &candidate in candidates {
        let dist = levenshtein_distance(target, candidate);
        if dist <= max_distance && dist > 0 {
            if best.is_none() || dist < best.unwrap().1 {
                best = Some((candidate, dist));
            }
        }
    }
    best.map(|(s, _)| s)
}

fn get_suggestion(message: &str) -> Option<String> {
    // Keep specific suggestions for known patterns
    if message.contains("character: ','") {
        return Some("In Ernos, function arguments are separated by 'and', not commas.".to_string());
    }
    if message.contains("Unexpected statement start: Identifier") {
        return Some("Functions called as statements must be assigned to variables, e.g. 'set ok to func(...)'.".to_string());
    }

    // Try Levenshtein matching against all ErnosPlain keywords
    let keywords: &[&str] = &[
        "set", "to", "define", "with", "return", "display",
        "if", "else", "repeat", "while", "for", "each", "in",
        "is", "equals", "plus", "minus", "multiplied", "divided", "modulo",
        "greater", "less", "than", "not", "and", "also", "or",
        "true", "false", "create", "structure", "field", "choice", "variant",
        "check", "trait", "implement", "spawn", "channel", "send", "receive",
        "async", "await", "borrow", "given", "break", "continue",
        "import", "as", "returning", "of", "try", "from", "range",
    ];

    // Extract potential misspelled words from the error message
    // Look for words in the message that might be misspellings
    let words: Vec<&str> = message.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty() && w.len() >= 2)
        .collect();

    for word in &words {
        let lower = word.to_lowercase();
        let max_dist = if lower.len() <= 4 { 2 } else { 3 };
        if let Some(closest) = find_closest_match(&lower, keywords, max_dist) {
            // Don't suggest if the word is already a keyword
            if lower != closest {
                return Some(format!("Did you mean '{}'?", closest));
            }
        }
    }

    None
}

fn print_usage() {
    eprintln!("\x1b[1;36m┌──────────────────────────────────────────────────┐\x1b[0m");
    eprintln!("\x1b[1;36m│\x1b[0m  \x1b[1mErnos Compiler\x1b[0m — v1.0.0                       \x1b[1;36m│\x1b[0m");
    eprintln!("\x1b[1;36m└──────────────────────────────────────────────────┘\x1b[0m");
    eprintln!();
    eprintln!("\x1b[1mUSAGE:\x1b[0m");
    eprintln!("  epc <filename.ep>               Compile to native binary");
    eprintln!("  epc <filename.ep> --native      Compile via native assembly (no Clang required)");
    eprintln!("  epc <filename.ep> --llvm        Compile via LLVM IR backend (.ll)");
    eprintln!("  epc <filename.ep> --release     Compile with optimizations (O3+LTO)");
    eprintln!("  epc test <filename.ep>          Run as test");
    eprintln!();
    eprintln!("\x1b[1mDEV TOOLS:\x1b[0m");
    eprintln!("  epc doc <filename.ep> [-o dir]  Generate markdown documentation");
    eprintln!("  epc --check <filename.ep>       Syntax check (no compilation)");
    eprintln!("  epc --format <filename.ep>      Auto-format source file");
    eprintln!("  epc --list-builtins             List all built-in functions");
    eprintln!("  epc --version                   Show version info");
    eprintln!("  epc --help                      Show this message");
    eprintln!();
    eprintln!("\x1b[1mEDITOR SUPPORT:\x1b[0m");
    eprintln!("  epc lsp                         Start LSP server (for VS Code / editors)");
    eprintln!();
    eprintln!("\x1b[1mSAFETY:\x1b[0m");
    eprintln!("  epc <filename.ep> --asan         Compile with AddressSanitizer");
    eprintln!("  epc <filename.ep> --debug        Compile with debug symbols");
}

fn format_file(path: &Path) {
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            std::process::exit(1);
        }
    };

    let mut output = String::new();
    let mut prev_blank = false;

    for line in source.lines() {
        let trimmed = line.trim_end();

        // Normalize blank lines (max 1 consecutive)
        if trimmed.is_empty() {
            if !prev_blank {
                output.push('\n');
                prev_blank = true;
            }
            continue;
        }
        prev_blank = false;

        // Count leading spaces in original
        let leading_spaces = line.len() - line.trim_start().len();

        // Normalize indentation: each level = 4 spaces
        // Detect the current indent level (assuming original uses at least 1 space per level)
        let indent_level = if leading_spaces > 0 {
            // If the original uses tabs, convert to 4 spaces
            if line.starts_with('\t') {
                line.chars().take_while(|c| *c == '\t').count()
            } else {
                // Try to detect indent width: common values are 2 or 4
                // We'll just normalize to 4 spaces
                (leading_spaces + 1) / 4
            }
        } else {
            0
        };

        // Re-indent with 4 spaces per level
        for _ in 0..indent_level {
            output.push_str("    ");
        }
        output.push_str(trimmed.trim_start());
        output.push('\n');
    }

    // Remove trailing newline if needed
    while output.ends_with("\n\n") {
        output.pop();
    }
    if !output.ends_with('\n') {
        output.push('\n');
    }

    match fs::write(path, &output) {
        Ok(()) => {
            println!("\x1b[1;32m✓\x1b[0m Formatted: {}", path.display());
        }
        Err(e) => {
            eprintln!("Error writing file: {}", e);
            std::process::exit(1);
        }
    }
}

fn run_repl() {
    use std::io::{self, Write, BufRead};

    println!("\x1b[1;36m╔══════════════════════════════════════════╗\x1b[0m");
    println!("\x1b[1;36m║   ErnosPlain REPL v1.0                   ║\x1b[0m");
    println!("\x1b[1;36m║   Type ErnosPlain code and press Enter   ║\x1b[0m");
    println!("\x1b[1;36m║   Type 'exit' or 'quit' to leave         ║\x1b[0m");
    println!("\x1b[1;36m║   Type ':help' for commands               ║\x1b[0m");
    println!("\x1b[1;36m╚══════════════════════════════════════════╝\x1b[0m");
    println!();

    let stdin = io::stdin();
    let mut history: Vec<String> = Vec::new();
    let mut line_buffer = String::new();
    let mut _in_block = false;
    let mut accumulated_lines: Vec<String> = Vec::new();
    let mut user_functions: Vec<String> = Vec::new();
    let mut block_buffer: Vec<String> = Vec::new();

    loop {
        if _in_block {
            print!("\x1b[33m...  \x1b[0m");
        } else {
            print!("\x1b[1;32mep>\x1b[0m ");
        }
        io::stdout().flush().unwrap();

        line_buffer.clear();
        match stdin.lock().read_line(&mut line_buffer) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                break;
            }
        }

        let trimmed = line_buffer.trim();

        // Handle multi-line block input
        if _in_block {
            if trimmed.is_empty() {
                // Empty line ends the block
                _in_block = false;
                let block_text = block_buffer.join("\n");
                block_buffer.clear();

                // Check if this is a function definition
                if block_text.starts_with("define ") && !block_text.starts_with("define main") {
                    user_functions.push(block_text.clone());
                    history.push(block_text);
                    println!("\x1b[2mFunction defined.\x1b[0m");
                } else {
                    accumulated_lines.push(block_text.clone());
                    history.push(block_text);
                }
                continue;
            } else {
                block_buffer.push(line_buffer.trim_end().to_string());
                continue;
            }
        }

        // Handle meta-commands
        if trimmed == "exit" || trimmed == "quit" {
            println!("\x1b[2mGoodbye!\x1b[0m");
            break;
        }

        if trimmed == ":help" {
            println!("\x1b[1mREPL Commands:\x1b[0m");
            println!("  \x1b[36m:help\x1b[0m      Show this help");
            println!("  \x1b[36m:history\x1b[0m   Show command history");
            println!("  \x1b[36m:clear\x1b[0m     Clear history");
            println!("  \x1b[36m:reset\x1b[0m     Clear all session state");
            println!("  \x1b[36mexit\x1b[0m       Exit the REPL");
            println!();
            println!("\x1b[1mSession State:\x1b[0m");
            println!("  Variables persist across lines (set x to 42, then display x)");
            println!("  Enter a block (define, if, etc.) and end with an empty line");
            println!();
            println!("\x1b[1mExamples:\x1b[0m");
            println!("  \x1b[2mset x to 42\x1b[0m");
            println!("  \x1b[2mdisplay x\x1b[0m");
            println!("  \x1b[2mdisplay concat(\"hello\" and \" world\")\x1b[0m");
            continue;
        }

        if trimmed == ":history" {
            for (i, cmd) in history.iter().enumerate() {
                println!("  \x1b[2m{}\x1b[0m  {}", i + 1, cmd);
            }
            continue;
        }

        if trimmed == ":clear" {
            history.clear();
            println!("History cleared.");
            continue;
        }

        if trimmed == ":reset" {
            accumulated_lines.clear();
            user_functions.clear();
            history.clear();
            println!("Session state and history cleared.");
            continue;
        }

        if trimmed.is_empty() {
            continue;
        }

        // Check if this starts a multi-line block (ends with ':')
        if trimmed.ends_with(':') && (trimmed.starts_with("define ")
            || trimmed.starts_with("if ")
            || trimmed.starts_with("repeat ")
            || trimmed.starts_with("for ")
            || trimmed.starts_with("while "))
        {
            _in_block = true;
            block_buffer.clear();
            block_buffer.push(trimmed.to_string());
            continue;
        }

        // Save to history
        history.push(trimmed.to_string());

        // Check if this is a function definition (single line with body)
        if trimmed.starts_with("define ") && !trimmed.starts_with("define main") {
            user_functions.push(trimmed.to_string());
            println!("\x1b[2mFunction defined.\x1b[0m");
            continue;
        }

        // Add to accumulated lines
        accumulated_lines.push(format!("    {}", trimmed));

        // Build the full source with all accumulated state
        let funcs = user_functions.join("\n\n");
        let body = accumulated_lines.join("\n");
        let source = if funcs.is_empty() {
            format!("define main:\n{}\n    return 0\n", body)
        } else {
            format!("{}\n\ndefine main:\n{}\n    return 0\n", funcs, body)
        };

        // Try to parse and evaluate
        let mut lexer_instance = lexer::Lexer::new(&source);
        let tokens = match lexer_instance.tokenize() {
            Ok(t) => t,
            Err(e) => {
                eprintln!("\x1b[31mLexer error:\x1b[0m {:?}", e);
                // Remove the last line that caused the error
                accumulated_lines.pop();
                continue;
            }
        };

        let mut parser_instance = parser::Parser::new(tokens);
        let mut program = match parser_instance.parse_program() {
            Ok(p) => p,
            Err(errors) => {
                for e in &errors {
                    eprintln!("\x1b[31mParse error:\x1b[0m {}", e.message);
                }
                accumulated_lines.pop();
                continue;
            }
        };

        // Optimize
        optimizer::Optimizer::run(&mut program);

        // Generate C code
        let mut cg = codegen::Codegen::new();
        cg.is_test_mode = false;
        let c_code = match cg.generate(&program) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("\x1b[31mCodegen error:\x1b[0m {}", e);
                accumulated_lines.pop();
                continue;
            }
        };

        // Write temp C file
        let tmp_c = "/tmp/ep_repl.c";
        let tmp_bin = "/tmp/ep_repl";
        if let Err(e) = fs::write(tmp_c, &c_code) {
            eprintln!("\x1b[31mError writing temp file:\x1b[0m {}", e);
            accumulated_lines.pop();
            continue;
        }

        // Compile with cc
        let compile = Command::new("cc")
            .args(&[tmp_c, "-o", tmp_bin, "-lpthread", "-lm"])
            .output();

        match compile {
            Ok(output) => {
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("\x1b[31mCompile error:\x1b[0m {}", stderr);
                    accumulated_lines.pop();
                    continue;
                }
            }
            Err(e) => {
                eprintln!("\x1b[31mFailed to run compiler:\x1b[0m {}", e);
                accumulated_lines.pop();
                continue;
            }
        }

        // Execute
        let run = Command::new(tmp_bin).output();
        match run {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if !stdout.is_empty() {
                    print!("{}", stdout);
                }
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if !stderr.is_empty() {
                        eprint!("\x1b[31m{}\x1b[0m", stderr);
                    }
                }
            }
            Err(e) => {
                eprintln!("\x1b[31mExecution error:\x1b[0m {}", e);
            }
        }

        // Cleanup temp files
        let _ = fs::remove_file(tmp_c);
        let _ = fs::remove_file(tmp_bin);
    }
}
