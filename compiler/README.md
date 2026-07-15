<p align="center">
  <h1 align="center">Ernos Programming Language</h1>
  <p align="center">A compiled language with plain English syntax, unification-based type inference, garbage-collected memory with ownership safety checks, and C-level performance.</p>
</p>

<p align="center">
  <a href="#"><img src="https://img.shields.io/badge/Version-1.0.0-blue.svg" alt="Version"></a>
  <a href="#"><img src="https://img.shields.io/badge/Tests-51%2F51-brightgreen.svg" alt="Tests"></a>
  <a href="#"><img src="https://img.shields.io/badge/Performance-C--Level-orange.svg" alt="Performance"></a>
  <a href="#"><img src="https://img.shields.io/badge/Platform-macOS%20%7C%20Linux-blueviolet.svg" alt="Platform"></a>
  <a href="#"><img src="https://img.shields.io/badge/Compiler-Self--Hosted-success.svg" alt="Self-Hosted"></a>
</p>

---

## What is Ernos?

Ernos is a **compiled, statically-typed, memory-safe programming language** that reads like plain English. It compiles to optimized native binaries via C with performance equivalent to hand-written C code.

```ernos
define factorial with n as Int returning Int:
    if n < 2:
        return 1
    return n * factorial(n - 1)

define main:
    display "Factorial of 20:"
    display factorial(20)
    return 0
```

**No curly braces. No semicolons. No noise.** Just code that reads like instructions.

---

## Why Ernos?

| Feature | Ernos | Rust | Java | Python |
|---------|-------|------|------|--------|
| **Readability** | ✅ Plain English | ❌ Symbolic | ❌ Verbose | ✅ Clean |
| **Type Safety** | ✅ Inferred + checked | ✅ Full | ✅ Full | ❌ Dynamic |
| **Memory Safety** | ✅ GC + ownership checks | ✅ Ownership | ⚠️ GC only | ❌ GC only |
| **Performance** | ✅ C-level | ✅ C-level | ⚠️ JVM overhead | ❌ Interpreted |
| **Compile Target** | Native binary | Native binary | JVM bytecode | Interpreted |
| **Self-Hosting** | ✅ | ✅ | ❌ | ❌ |

### Performance

Ernos compiles to C, then to a native binary via `clang -O2`. The generated code has no interpreter overhead — it runs at the same speed as equivalent C.

---

## Features

### 🛡️ Compile-Time Safety
- **Type inference with unification** — types are inferred even without annotations (HM-style unification; no let-generalization yet)
- **Enforced type checking** — declared return types, list-element types, and undefined names are hard errors that stop compilation
- **Ownership & borrowing analysis** — use-after-move, move-while-borrowed, modify-while-borrowed, and returning a borrow of a local are rejected (enforced in codegen, backed by the GC)
- **Send/Sync safety** — borrowed references cannot be sent to spawned threads

```ernos
define halve with n as Int returning Int:
    return "half"           # ✗ REJECTED: returns Str, but Int is declared

define main:
    set xs to [1, "two", 3] # ✗ REJECTED: list elements have conflicting types
    display mystery          # ✗ REJECTED: undefined name
    return 0
```

### ⚡ Performance
- Compiles to C, then to native binary via `clang -O2`
- Constant folding and dead code elimination at AST level
- Release mode: `clang -O3 -flto` (via `--release` flag)

### 📦 Standard Library (24 modules)

| Module | Description |
|--------|-------------|
| `string` | String functions, StringBuilder, formatting |
| `collections` | HashMap, HashSet, Stack, Queue, PriorityQueue |
| `fs` | File I/O, directories, path utilities |
| `net` / `http` | TCP sockets, HTTP client/server |
| `json` | JSON parsing and generation |
| `csv` | CSV parsing and generation |
| `datetime` | Timestamps, formatting, arithmetic |
| `crypto` | SHA256, MD5, SHA1, base64, UUID, random |
| `regex` | POSIX regex matching, find, replace, split |
| `sync` | Mutex, RWLock, Atomic, Barrier, Semaphore |
| `os` | Environment, process info, system commands |
| `test` | Assertions, test suites, test runner |
| `log` | Structured logging with levels and timestamps |
| `math` | Mathematical functions |
| `sort` | Sorting algorithms |
| `sql` | SQLite database bindings |
| `gui` | GUI via raylib |
| `hash` | Hashing utilities |
| `toml` | TOML config file parsing |
| `static_server` | Static file serving over HTTP |
| `websocket` | WebSocket protocol implementation |
| `select` | I/O multiplexing |
| `structured` | Structured concurrency (task groups, timeouts) |

### 🔌 FFI Bridge Libraries (29 bindings)

Pre-built bindings for C libraries via `ep_dlopen`/`ep_dlsym`/`ep_dlcall`:

`raylib` · `sdl2` · `ncurses` · `cairo` · `libpng` · `stb_image` · `stb_truetype` · `miniaudio` · `libsndfile` · `curl` · `openssl` · `libsodium` · `zlib` · `sqlite` · `jansson` · `expat` · `pcre` · `libgit2` · `libuv` · `lmdb` · `termbox2` · `portmidi` · `freetype` · `lua` · `mongoose` · `mosquitto` · `libnotify` · `libusb` · `chipmunk`

> **Note:** Stdlib modules are written in ErnosPlain and call into the compiler's C runtime. They require the corresponding C runtime functions to be available.

### 🔧 Developer Tools

| Tool | Command | Description |
|------|---------|-------------|
| **Compiler** | `ernos program.ep` | Compile to native binary |
| **REPL** | `ernos --repl` | Interactive evaluation with session state |
| **Formatter** | `ernos format file.ep` | Auto-format source code |
| **Checker** | `ernos check file.ep` | Type/syntax validation without compiling |
| **Test Runner** | `ernos test file.ep` | Run tests |
| **Builtins** | `ernos --list-builtins` | Show all built-in functions |
| **Debug** | `ernos file.ep --debug` | Compile with `-O0 -g` |
| **Release** | `ernos file.ep --release` | Compile with `-O3 -flto` |
| **ASAN** | `ernos file.ep --asan` | Compile with AddressSanitizer |
| **WASM** | `ernos file.ep --wasm` | Compile to WebAssembly |
| **Native** | `ernos file.ep --native` | Compile via native assembly (no Clang) |
| **LSP** | `ernos --lsp` | Language Server Protocol for editor support |
| **Doc Gen** | `ernos doc file.ep -o api.md` | Generate API documentation from doc comments |
| **Bind** | `ernos bind header.h` | Generate .ep bindings from C headers |
| **Transpile** | `ernos transpile file.py` | Translate Python/C/JS/Go/Rust/Ruby/Java/TS to EP |

### 🌍 Platform Support
- **macOS** (ARM64 + x86_64) — primary development platform
- **Linux** (x86_64 + aarch64, GCC or Clang) — supported

> **Note:** Windows has partial C runtime polyfills (`#ifdef _WIN32` blocks) but is not tested or officially supported yet.

---

## Quick Start

### Prerequisites
- A C compiler (`clang` or `gcc`)
- Rust (for building the bootstrap compiler)

### Build
```bash
git clone https://github.com/MettaMazza/Ernos-Programming-Language.git
cd Ernos-Programming-Language
cargo build --release
```

### Hello World
```ernos
# hello.ep
define main:
    display "Hello from Ernos!"
    return 0
```

```bash
./target/release/ernos hello.ep
./hello
# Output: Hello from Ernos!
```

### Typed Functions
```ernos
define add with a as Int and b as Int returning Int:
    return a + b

define greet with name as Str:
    display concat("Hello, " and name)
    return 0

define main:
    display add(10 and 20)      # 30
    set ok to greet("World")    # Hello, World
    return 0
```

### Concurrency
```ernos
define worker with id as Int and ch:
    send id * 10 to ch
    return 0

define main:
    set ch to channel
    spawn worker(1 and ch)
    spawn worker(2 and ch)

    set a to receive from ch
    set b to receive from ch
    display f"Total: {a + b}"
    return 0
```

### Structs & Methods
```ernos
define structure User:
    field name as Str
    field age as Int

define greet on User:
    display concat("Hi, I'm " and self.name)
    return 0

define main:
    set user to create User:
        name is "Alice"
        age is 30
    set ok to user.greet()
    return 0
```

### Enums & Pattern Matching
```ernos
define choice Shape:
    variant Circle with radius as Int
    variant Rect with width as Int and height as Int

define area with s as Shape returning Int:
    check s:
        if Circle with r:
            return r * r * 3
        if Rect with w and h:
            return w * h

define main:
    set c to Circle with 5
    display f"Area: {area(c)}"
    return 0
```

### Namespace Imports
```ernos
import "math" as m

define main:
    set result to m_absolute(-42)
    display result    # 42
    return 0
```

### Dynamic Library Loading (FFI)
```ernos
define main:
    set lib to ep_dlopen("libm.dylib")
    set abs_fn to ep_dlsym(lib and "abs")
    set result to ep_dlcall1(abs_fn and -42)
    display result    # 42
    set _ to ep_dlclose(lib)
    return 0
```

### Cross-Language Transpilation
```bash
# Translate existing code from other languages into ErnosPlain
ernos transpile script.py -o script.ep     # Python → ErnosPlain
ernos transpile program.c -o program.ep    # C → ErnosPlain
ernos transpile app.js -o app.ep           # JavaScript → ErnosPlain
ernos transpile main.go -o main.ep         # Go → ErnosPlain
ernos transpile lib.rs -o lib.ep           # Rust → ErnosPlain
ernos transpile App.java -o App.ep         # Java → ErnosPlain
ernos transpile app.ts -o app.ep           # TypeScript → ErnosPlain
ernos transpile script.rb -o script.ep     # Ruby → ErnosPlain

# Generate .ep bindings from C headers
ernos bind /usr/include/math.h -o math_bindings.ep
```

---

## Architecture

```
Source (.ep)
    ↓
  Lexer → Tokens
    ↓
  Parser → AST
    ↓
  Type Checker (unification-based inference) — hard errors
    ↓
  Borrow Checker (ownership analysis) — hard errors
    ↓
  Optimizer (constant folding, dead code elimination)
    ↓
  Codegen → C source (includes ownership safety checks)
    ↓
  Clang -O2 → Native binary
```

> **Note:** The codegen phase performs additional ownership checks (use-after-move, borrow violations) as a safety net alongside the dedicated borrow checker. Both must pass for compilation to succeed.

### Compiler Modules

| File | Lines | Description |
|------|-------|-------------|
| `src/lexer.rs` | ~900 | Tokenizer with indentation tracking |
| `src/parser.rs` | ~1,640 | Recursive descent parser with Pratt precedence |
| `src/type_check.rs` | ~1,900 | Type inference via unification (HM-style; no let-generalization) |
| `src/borrow_check.rs` | ~830 | Ownership, borrowing, Send/Sync analysis |
| `src/optimizer.rs` | ~1,450 | Constant folding, DCE, CSE, LICM, inlining, loop unrolling |
| `src/codegen.rs` | ~7,700 | C code generation with full runtime |
| `src/llvm_codegen.rs` | ~80 | LLVM IR backend (via clang -emit-llvm) |
| `src/lsp.rs` | ~1,200 | Language Server Protocol implementation |
| `src/diagnostics.rs` | ~380 | Rich error reporting with ANSI colors |
| `src/native_codegen.rs` | ~660 | ARM64 native assembly backend (macOS + Linux) |
| `src/x86_64_codegen.rs` | ~620 | x86_64 native assembly backend (macOS + Linux) |
| `src/bind_c.rs` | ~1,440 | C header binding generator (zero-dependency) |
| `src/main.rs` | ~2,090 | CLI, imports, REPL, compilation pipeline |
| `src/transpile_py.rs` | ~1,880 | Python → ErnosPlain transpiler |
| `src/transpile_c.rs` | ~1,380 | C → ErnosPlain transpiler |
| `src/transpile_js.rs` | ~1,240 | JavaScript → ErnosPlain transpiler |
| `src/transpile_go.rs` | ~1,360 | Go → ErnosPlain transpiler |
| `src/transpile_rs.rs` | ~1,210 | Rust → ErnosPlain transpiler |
| `src/transpile_rb.rs` | ~1,080 | Ruby → ErnosPlain transpiler |
| `src/transpile_java.rs` | ~730 | Java → ErnosPlain transpiler |
| `src/transpile_ts.rs` | ~730 | TypeScript → ErnosPlain transpiler |
| `src/emit_c.rs` | ~570 | ErnosPlain → C emitter |
| `src/emit_js.rs` | ~590 | ErnosPlain → JavaScript emitter |
| `src/emit_python.rs` | ~640 | ErnosPlain → Python emitter |
| **Total** | **~32,750** | |

---

## Self-Hosting

Ernos compiles its own compiler. The self-hosted compiler modules:

- `ep_lexer.ep` — Lexer (540 lines)
- `ep_parser.ep` — Parser (1,396 lines)
- `ep_codegen.ep` — Code generator (3,622 lines)
- `epc.ep` — Compiler driver (266 lines)
- **Total: 5,824 lines of ErnosPlain**

### Bootstrap
```bash
# The Rust compiler compiles the self-hosted compiler
./target/release/ernos epc.ep

# The self-hosted compiler can compile programs
./epc hello.ep
./hello
```

---

## Syntax Quick Reference

| Concept | Syntax |
|---------|--------|
| Variable | `set x to 42` |
| String | `"hello"` or `f"value: {x}"` |
| Function | `define foo with a and b:` |
| Typed param | `with a as Int and b as Str` |
| Return type | `define foo returning Int:` |
| If/else | `if cond:` ... `else:` |
| While loop | `repeat while cond:` or `while cond:` |
| For-each | `for each item in list:` |
| Comparison | `equals`, `is not equal to`, `<`, `>`, `<=`, `>=`, `==`, `!=` |
| Logical | `&&`, `\|\|`, `not`, `and also`, `or else` |
| Struct | `define structure Name:` with `field x as Type` |
| Enum | `define choice Name:` with `variant X` |
| Match | `check expr:` with `if Pattern:` |
| Method | `define foo on StructName:` |
| Struct create | `create StructName:` (block with `field is value`) |
| Channel | `set ch to channel` |
| Send | `send value to ch` |
| Receive | `set v to receive from ch` |
| Spawn | `spawn function(args)` |
| Try | `try expression` |
| List literal | `[1, 2, 3]` or `["a", "b"]` |
| Import | `import "module"` or `import "module" as alias` |
| Closure | `set f to given x: return x * 2` |
| Comment | `# this is a comment` |


---

## Language Specification

A formal specification is available in [`spec/ernos-spec.md`](spec/ernos-spec.md), including:
- Complete EBNF grammar
- Type system rules
- Memory model (ownership, borrowing, GC)
- Concurrency model (Send/Sync)
- Standard library contracts

Conformance tests are in the [`conformance/`](conformance/) directory.

---

## VS Code Syntax Highlighting

```bash
cp -R ernosplain-syntax ~/.vscode/extensions/
# Restart VS Code — all .ep files will have syntax highlighting
```

---

<p align="center">
  <b>Ernos</b> — Code that reads like English. Runs like C.
</p>
