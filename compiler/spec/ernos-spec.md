# ErnosPlain Language Specification v1.0

## 1. Lexical Structure

### 1.1 Character Set
ErnosPlain source files are encoded in UTF-8. The language is case-sensitive.

### 1.2 Keywords
```
define, set, to, display, return, if, else, repeat, while,
and, or, not, is, as, returning, plus, minus, times, divided by,
modulo, is less than, is greater than, equals, not equals,
true, false, import, external, borrow, structure, field, create,
choice, variant, check, for, each, in, range, spawn, send,
receive, from, await, async, try, on, trait, implement, break,
continue, channel, given, of, with, and also, or else
```

### 1.3 Operators
```
+  -  *  /  %  ==  !=  <  >  <=  >=  &&  ||
```

### 1.4 Literals
- **Integer**: `42`, `0`, `-1`, `1000000`
- **Float**: `3.14`, `0.5`, `-2.7`
- **String**: `"hello world"`, `"line1\nline2"`, `"tab\there"`
- **F-string**: `f"Hello {name}, you are {age} years old"`
- **Boolean**: `true`, `false`
- **List literal**: `[1, 2, 3]`, `["a", "b", "c"]`

### 1.5 Comments
Lines beginning with `#` are comments.
```
# This is a comment
```

### 1.6 Indentation
ErnosPlain uses indentation (4 spaces) to delimit blocks, similar to Python.

---

## 2. Types

### 2.1 Primitive Types
| Type | Description | C Representation |
|------|-------------|-----------------|
| `Int` | 64-bit signed integer | `long long` |
| `Float` | 64-bit IEEE 754 double | `double` (partially supported in codegen) |
| `Bool` | Boolean value | `long long` (0 or 1) |
| `Str` | Static string (immutable) | `const char*` cast to `long long` |
| `DynStr` | Dynamic string (heap) | `char*` (malloc'd) cast to `long long` |
| `Any` | Top type (container returns) | `long long` (can be any of the above) |

> **Note:** All values are represented as `long long` at runtime. The type system is a compile-time overlay. Pointers are cast to `long long` for uniform representation.

### 2.2 Compound Types
| Type | Description |
|------|-------------|
| `List of T` | Dynamic array (backed by `EpList` struct) |
| `Structure` | Named record with typed fields |
| `Choice` (enum) | Tagged union with variants |
| `Fun(params, ret)` | Function type (for closures/HOF) |

### 2.3 Type Annotations
```
define add with a as Int and b as Int returning Int:
    return a plus b

set name as Str to "Alice"
```

### 2.4 Type Inference
When type annotations are omitted, the compiler infers types using
unification-based inference (HM-style, without let-generalization). Declared
return types, list-element types, and undefined names are enforced at compile time.

---

## 3. Declarations

### 3.1 Functions
```
define function_name with param1 and param2:
    # body
    return value
```

### 3.2 Typed Functions
```
define add with a as Int and b as Int returning Int:
    return a plus b
```

### 3.3 Async Functions
```
async define fetch_data with url as Str:
    # async body
    return result
```

### 3.4 External Functions (FFI)
```
external define c_function_name with param1 and param2:
```

### 3.5 Structures
```
define structure Point:
    field x as Int
    field y as Int
```

### 3.6 Enums (Choices)
```
define choice Result:
    variant Ok with value as Int
    variant Error with message as Str
```

### 3.7 Methods
```
define greet on User:
    display self.name
    return 0
```

### 3.8 Traits
```
define trait Printable:
    define to_string with self returning Str

implement Printable for User:
    define to_string:
        return self.name
```

### 3.9 Closures
```
set doubler to given x:
    return x * 2
```

---

## 4. Statements

### 4.1 Variable Assignment
```
set x to 42
set name as Str to "Alice"
```

### 4.2 Display (Print)
```
display x
display "Hello, world!"
display f"The answer is {x}"
```

### 4.3 Return
```
return value
```

### 4.4 Conditionals
```
if x is greater than 10:
    display "big"
else if x is greater than 5:
    display "medium"
else:
    display "small"
```

### 4.5 Loops
```
repeat while x is less than 100:
    set x to x plus 1

while x < 100:
    set x to x + 1

for each item in my_list:
    display item
```

### 4.6 Pattern Matching
```
check result:
    if Ok with value:
        display value
    if Error with message:
        display message
```

### 4.7 Concurrency
```
spawn worker(data and ch)
send value to ch
set result to receive from ch
set result to await async_call()
```

### 4.8 Field Set
```
set obj.field to value
```

---

## 5. Expressions

### 5.1 Arithmetic
```
x plus y          # or: x + y
x minus y         # or: x - y
x times y         # or: x * y
x divided by y    # or: x / y
x modulo y        # or: x % y
```

### 5.2 Comparison
```
x is less than y      # or: x < y
x is greater than y   # or: x > y
x equals y            # or: x == y
x not equals y        # or: x != y
x <= y
x >= y
```

### 5.3 Logical
```
x and also y      # or: x && y
x or else y       # or: x || y
not x
```

> **Note:** `and` alone is context-sensitive: in conditions it acts as logical AND, inside function call parentheses it separates arguments.

### 5.4 Function Calls
```
set result to add(10 and 20)
set msg to concat("hello" and " world")
```

### 5.5 Struct Construction
```
set p to create Point:
    x is 10
    y is 20
```

### 5.6 Field Access
```
display p.x
set p.x to 42
```

### 5.7 Method Calls
```
set result to user.greet()
```

### 5.8 Borrowing
```
set ok to process(borrow data)
```

### 5.9 List Literals
```
set nums to [1, 2, 3]
set names to ["Alice", "Bob"]
```

### 5.10 F-String Interpolation
```
display f"Hello {name}, age {age + 1}"
```

---

## 6. Memory Model

### 6.1 Ownership
Every value has exactly one owner. When the owner goes out of scope,
the value is freed (stack values) or becomes GC-eligible (heap values).

### 6.2 Borrowing
Values can be borrowed with the `borrow` keyword:
- Immutable borrows: multiple readers allowed
- The borrower cannot outlive the owner

### 6.3 Move Semantics
Passing a heap-allocated value to a function without `borrow` transfers
ownership. Using the value after the move is a compile-time error.

### 6.4 Garbage Collection
Heap-allocated values (lists, structs, dynamic strings) are managed by
a mark-and-sweep garbage collector:
- Thread-local GC root stacks (`__thread` storage)
- `ep_gc_push_root` / `ep_gc_pop_roots` for scope tracking
- Stop-the-world collection: all threads pause during mark phase
- Thread registry for cross-thread root walking
- Collection triggered after every N allocations

---

## 7. Concurrency Model

### 7.1 Send/Sync Safety
- **Send**: Types that can be transferred to another thread
  - Value types (Int, Float, Bool) are Send
  - Owned heap types are Send
  - Borrowed references are NOT Send
- **Sync**: Types that can be safely shared
  - Immutable data is Sync
  - Mutable data requires explicit synchronization

### 7.2 Channels
```
set ch to channel
# or: set ch to create_channel()

send 42 to ch
set value to receive from ch
```

### 7.3 Async/Await
```
async define fetch with url:
    # asynchronous operation
    return result

define main:
    set data to await fetch("https://example.com")
```

---

## 8. Imports

### 8.1 Basic Import
```
import "string"
import "fs"
```

Standard library module names resolve to `stdlib/<name>.ep`. Relative file paths also work.

### 8.2 Namespace Import
```
import "math" as m
```

All functions from the imported module become available with the `m_` prefix (e.g., `m_absolute`, `m_gcd`). The original names also remain available.

---

## 9. Standard Library

24 modules: `string`, `collections`, `fs`, `net`, `http`, `json`, `csv`, `datetime`, `crypto`, `regex`, `sync`, `os`, `test`, `log`, `math`, `sort`, `sql`, `gui`, `hash`, `toml`, `static_server`, `websocket`, `select`, `structured`.

---

## 10. Compilation Targets

ErnosPlain compiles to:
1. **C code** (default) — portable, compiled via system C compiler
2. **ARM64 assembly** (`--native` on aarch64) — macOS and Linux
3. **x86_64 assembly** (`--native` on x86_64) — macOS and Linux

---

## 11. Grammar (EBNF)

```ebnf
program     = { import | extern_def | struct_def | enum_def |
                trait_def | trait_impl | method_def | function } ;

import      = "import" STRING [ "as" IDENT ] ;
extern_def  = "external" "define" IDENT { "with" param_list } ":" ;
struct_def  = "define" "structure" IDENT ":" { field_def } ;
enum_def    = "define" "choice" IDENT ":" { variant_def } ;
trait_def   = "define" "trait" IDENT ":" { method_sig } ;
trait_impl  = "implement" IDENT "for" IDENT ":" { function } ;
method_def  = "define" IDENT "on" IDENT { "with" param_list }
              { "returning" type_ann } ":" block ;
function    = ["async"] "define" IDENT { "with" param_list }
              { "returning" type_ann } ":" block ;

param_list  = param { "and" param } ;
param       = ["borrow"] IDENT [ "as" type_ann ] ;
type_ann    = "Int" | "Float" | "Bool" | "Str" | "DynStr" |
              "List" | IDENT [ "of" type_ann { "and" type_ann } ] ;

block       = INDENT { statement } DEDENT ;
statement   = set_stmt | if_stmt | while_stmt | for_stmt |
              return_stmt | display_stmt | match_stmt |
              spawn_stmt | send_stmt | break_stmt |
              continue_stmt | field_set_stmt | expr_stmt ;

set_stmt    = "set" IDENT [ "." IDENT ] [ "as" type_ann ] "to" expr ;
field_set_stmt = "set" IDENT "." IDENT "to" expr ;
if_stmt     = "if" expr ":" block { "else" "if" expr ":" block } [ "else" ":" block ] ;
while_stmt  = ("repeat" "while" | "while") expr ":" block ;
for_stmt    = "for" "each" IDENT "in" expr ":" block ;
return_stmt = "return" expr ;
display_stmt= "display" expr ;
match_stmt  = "check" expr ":" { match_arm } ;
match_arm   = "if" IDENT [ "with" IDENT { "and" IDENT } ] ":" block ;
spawn_stmt  = "spawn" IDENT "(" arg_list ")" ;
send_stmt   = "send" expr "to" expr ;

expr        = logical ;
logical     = comparison { ("&&" | "||" | "and also" | "or else") comparison } ;
comparison  = addition { comp_op addition } ;
addition    = multiplication { ("+" | "-" | "plus" | "minus") multiplication } ;
multiplication = unary { ("*" | "/" | "%" | "times" | "divided" "by" | "modulo") unary } ;
unary       = ["not"] primary ;
primary     = INTEGER | FLOAT | STRING | FSTRING | "true" | "false" |
              IDENT [ "(" arg_list ")" | "." IDENT [ "(" arg_list ")" ] ] |
              "borrow" primary | "receive" "from" primary | "await" primary |
              "try" primary | "create" IDENT ":" field_init_list |
              IDENT "with" arg_list | "(" expr ")" |
              "channel" | "given" IDENT { "and" IDENT } ":" block |
              "[" [ expr { "," expr } ] "]" ;

arg_list    = expr { "and" expr } ;
field_init_list = INDENT { IDENT "is" expr } DEDENT ;

comp_op     = "==" | "!=" | "<" | ">" | "<=" | ">=" |
              "is" "less" "than" | "is" "greater" "than" |
              "equals" | "not" "equals" |
              "is" "equal" "to" | "is" "not" "equal" "to" ;
```
