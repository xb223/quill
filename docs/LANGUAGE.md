# The Quill language guide

Quill is a small, expression-oriented systems language. This guide covers the
whole language as implemented in v0.1 — if something is not here, it is not in
the language yet.

**Contents**

1. [Hello, Quill](#1-hello-quill)
2. [Comments](#2-comments)
3. [Values and types](#3-values-and-types)
4. [Variables and mutability](#4-variables-and-mutability)
5. [Operators](#5-operators)
6. [Strings](#6-strings)
7. [Arrays](#7-arrays)
8. [Structs](#8-structs)
9. [Functions](#9-functions)
10. [Control flow](#10-control-flow)
11. [Built-in functions](#11-built-in-functions)
12. [Errors and panics](#12-errors-and-panics)
13. [C libraries: `#add` and `extern`](#13-c-libraries-add-and-extern)
14. [Grammar summary](#14-grammar-summary)
15. [Not in the language yet](#15-not-in-the-language-yet)

---

## 1. Hello, Quill

```quill
fn main() {
    println("Hello, Quill!");
}
```

Every program needs a `fn main()` with no parameters and no return value. Save
the file as `hello.q`, then:

```console
$ quillc run hello.q
Hello, Quill!
```

## 2. Comments

```quill
// A line comment.

/* A block comment.
   It can span lines. */
```

## 3. Values and types

| Type | Meaning | Examples |
| --- | --- | --- |
| `int` | signed 64-bit integer | `0`, `42`, `-7`, `1_000_000` |
| `float` | 64-bit IEEE-754 number | `1.5`, `-0.25`, `1e9` |
| `bool` | `true` or `false` | `true`, `false` |
| `str` | immutable UTF-8 string | `"hello"` |
| `[T]` | growable array of `T` | `[1, 2, 3]`, `["a", "b"]` |
| `void` | "no value" | the result of `println` |
| struct | named product type | `Point { x: 1, y: 2 }` |

There is **no null** and no uninitialized value. Every variable is initialized
when it is declared.

Numbers support `_` separators (`1_000_000`) and floats support exponents
(`2.5e-3`). Integer literals must fit in 64 bits.

## 4. Variables and mutability

```quill
let name = "ada";        // immutable binding, type inferred
let mut count = 0;       // mutable binding
count += 1;              // ok: count is mutable

let ratio: float = 0.5;  // explicit type
let xs: [int] = [];      // empty arrays need an annotation
```

* `let` bindings cannot be **reassigned**. Use `let mut` when you need to
  rebind the name.
* Shadowing is allowed — the new binding simply takes over:

```quill
let x = 1;
let x = x + 1;   // the right-hand x is the old one
println(x);      // 2
```

* Quill does not freeze data. `let p = Point { ... }; p.x = 3;` is allowed,
  because `let` governs the *name* `p`, not everything reachable from it. This
  is deliberate: beginners should not need a deep-mutability model to update a
  field.
* Arrays and strings are **handles**: assigning an array to a second variable
  shares the same underlying array. Pushing through one name is visible through
  the other. Strings are immutable, so they can be shared freely.

```quill
let a = [1, 2];
let b = a;
push(b, 3);
println(a);   // [1, 2, 3]
```

## 5. Operators

From lowest to highest precedence:

| Operators | Meaning |
| --- | --- |
| `\|\|` | logical or (short-circuits) |
| `&&` | logical and (short-circuits) |
| `==` `!=` | equality |
| `<` `<=` `>` `>=` | ordering |
| `+` `-` | addition, subtraction, string concatenation |
| `*` `/` `%` | multiplication, division, remainder |
| `!` `-` | unary not, unary negation |

```quill
println(1 + 2 * 3);      // 7
println((1 + 2) * 3);    // 9
println(7 / 2);          // 3  (truncates toward zero)
println(-7 / 2);         // -3
println(7 % 3);          // 1
println("a" + "b");      // ab
```

Rules:

* `+` works on two numbers or two strings. Mixing a string and a number needs
  `str(...)`: `"n = " + str(n)`.
* Arithmetic comparisons require two numbers; `==`/`!=` also work on `bool` and
  `str`. Ordering works on numbers and strings.
* `&&` and `||` require `bool`. Quill does not treat `0` as false.
* **Integer arithmetic is checked.** Overflow, division by zero and
  `% 0` stop the program with a clear panic instead of wrapping or invoking
  undefined behavior.

Compound assignment:

```quill
let mut total = 0;
total += 10;
total -= 2;
total *= 3;
total /= 2;
total %= 5;

let mut greeting = "hello";
greeting += ", world";     // string concatenation
```

## 6. Strings

```quill
let who = "world";
println("hello, {who}!");          // interpolation
println("2 + 2 = {2 + 2}");        // expressions are allowed
println("literal braces: {{ }}");  // escapes for { and }
```

Escapes: `\n`, `\t`, `\r`, `\0`, `\\`, `\"`, `\'`, `\{`, `\}`.

Interpolation accepts `int`, `float`, `bool` and `str` expressions. Arrays and
structs are not interpolated; print them directly (`println(xs)`) or print
their fields.

Operations:

| Expression | Result |
| --- | --- |
| `a + b` | new concatenated string |
| `a == b`, `a != b` | byte-wise equality |
| `a < b`, `a <= b`, ... | byte-wise lexicographic order |
| `len(s)` | length in bytes |
| `str(x)` | `int`/`float`/`bool` → printable string |
| `int(s)`, `float(s)` | parse, panicking on invalid input |

Strings are UTF-8 bytes. `len` counts bytes, not characters (Unicode-aware
lengths are planned).

## 7. Arrays

```quill
let xs = [1, 2, 3];        // [int]
let names = ["ada", "grace"];
let scores: [float] = [1, 2, 3];   // integer literals widen to float
let empty: [int] = [];

push(xs, 4);               // grow
println(xs);               // [1, 2, 3, 4]
println(xs[0]);            // 1
println(len(xs));          // 4
xs[0] = 10;                // update an element

for x in xs {
    println(x);
}
```

* Arrays are homogeneous. An empty literal needs a type annotation.
* Indexing is bounds checked; an invalid index panics with the index and length.
* You can print arrays of `int`, `float`, `bool` or `str`.
* Iterating with `for x in xs` copies each element. For structs that means a
  copy; mutate the array element itself (`xs[i].field = ...`) to change it.
* Arrays are not comparable as a whole; compare lengths and elements.

## 8. Structs

```quill
struct Point {
    x: int,
    y: int,
}

fn main() {
    let p = Point { x: 3, y: 4 };
    println(p.x + p.y);   // 7

    p.x = 10;             // fields can be updated
    let copy = p;         // structs are value types: this copies
    copy.x = 99;
    println(p.x);         // 10 — the original is unchanged
}
```

* Struct literals must initialize every field, in any order.
* Structs can contain other structs by value. Recursive-by-value definitions
  (`struct Node { next: Node }`) are rejected; use an array to build a
  collection.
* Field access uses `value.field`; assignment to a field is allowed.
* Structs cannot be compared with `==` yet; compare fields. They cannot be
  printed as a whole yet; print fields.

## 9. Functions

```quill
fn add(a: int, b: int) -> int {
    a + b          // the last expression is the return value
}

fn shout(message: str) {
    println(message + "!");
}

fn sign(n: int) -> str {
    if n > 0 {
        return "positive";   // early return also works
    }
    if n < 0 {
        return "negative";
    }
    "zero"
}

fn main() {
    println(add(2, 3));   // 5
    shout("hello");
    println(sign(-4));    // negative
}
```

* Parameter and return types are required. Omit `-> T` when nothing is returned.
* A function body is a block, and a block's value is its final expression (with
  no semicolon). `return` works anywhere.
* A function with a return type must return a value on every path; the compiler
  checks this.
* Functions can call themselves (recursion) and each other regardless of
  declaration order.
* Parameters are immutable bindings.
* `main` takes no parameters, returns nothing, and cannot be called directly.

## 10. Control flow

### if / else

`if` is an expression:

```quill
let label = if score >= 90 {
    "great"
} else if score >= 60 {
    "ok"
} else {
    "retry"
};
```

Conditions must be `bool`. Both branches of a value-producing `if` must have the
same type (`int` widens to `float`).

### while

```quill
let mut i = 0;
while i < 3 {
    println(i);
    i += 1;
}
```

### for

```quill
for i in 0..5 {          // 0, 1, 2, 3, 4 (end is exclusive)
    println(i);
}

for name in ["ada", "grace"] {
    println(name);
}
```

### break and continue

```quill
for n in 0..10 {
    if n == 3 { continue; }
    if n == 6 { break; }
    println(n);
}
```

`break` and `continue` must be inside a loop. A range whose end is not greater
than its start performs zero iterations.

## 11. Built-in functions

| Function | Description |
| --- | --- |
| `print(x)` / `println(x)` | write a value to stdout (optionally with a newline); no argument prints just the newline |
| `len(s)` / `len(xs)` | byte length of a string, element count of an array |
| `push(xs, value)` | append to an array |
| `str(x)` | convert `int`, `float` or `bool` to `str` |
| `int(x)` | convert `int`/`float`/`bool`/`str` to `int` |
| `float(x)` | convert `int`/`float`/`str` to `float` |
| `abs(x)` | absolute value of an `int` or `float` |
| `min(a, b)` / `max(a, b)` | smaller/larger of two numbers |
| `assert(cond)` | panic with file:line if `cond` is false |
| `read_line()` | read one line from stdin (without the newline); `""` at EOF |
| `read_int()` | read a line and parse it as `int` |
| `read_float()` | read a line and parse it as `float` |

Conversions are explicit. The only implicit conversion is `int` → `float` in a
position that expects a float:

```quill
let f: float = 3;          // becomes 3.0
let g = f + 1;             // 4.0
println(int(2.9));         // 2 (truncates toward zero)
```

Example with input:

```quill
fn main() {
    println("What is your name?");
    let name = read_line();
    println("How old are you?");
    let age = read_int();
    println("Hi {name}! Next year you will be {age + 1}.");
}
```

## 12. Errors and panics

Compile-time errors point at the source line, underline the problem, and often
suggest a fix:

```console
error: cannot reassign `x` because it was declared with `let`
   --> hello.q:3:5
   |
 3 |     x = 6;
   |     ^
   |
   = note: declare it with `let mut x = ...` to allow reassignment
```

At runtime, safety checks produce a panic on stderr and exit code 101:

```console
panic: array index 3 is out of bounds (length 3)
panic: integer overflow in `+`
panic: division by zero
panic: hello.q:12: assertion failed
```

Panics are preferable to continuing with corrupted memory. There are no
undefined-behavior traps in safe Quill code: no null dereference, no
uninitialized reads, no silent overflow, no out-of-bounds access.

## 13. C libraries: `#add` and `extern`

Quill calls into C libraries with a directive and a declaration. `#add` brings in
a header (like `#include`, but it is part of the language and the compiler also
knows how to link the matching library):

```quill
#add <math.h>

extern fn sqrt(x: float) -> float;

fn main() {
    println(sqrt(16.0));   // 4
}
```

`#add <header.h>` does three things: it includes the header in the generated C,
it links the matching library (`<math.h>` → `-lm`, `<ncurses.h>` → `-lncurses`),
and it lets `extern fn` declarations type-check against real signatures. You
still declare each function you call with `extern fn`, which is what gives you
friendly Quill types and type errors at compile time.

**The bundled `term` library.** Quill ships a small ncurses-like library for
real-time terminal programs (games, dashboards, TUIs). Add it with `#add
<term.h>`:

```quill
#add <term.h>

extern fn term_init() -> int;
extern fn term_cleanup() -> void;
extern fn term_move(row: int, col: int) -> void;
extern fn term_print(s: str) -> void;
extern fn term_fg(color: int) -> void;
extern fn term_reset_colors() -> void;
extern fn term_getch_timeout(ms: int) -> int;

fn main() {
    term_init();
    term_move(2, 4);
    term_fg(2);                 // green
    term_print("press a key");
    term_reset_colors();
    term_getch_timeout(5000);   // wait up to 5s
    term_cleanup();
}
```

| `term` function | What it does |
| --- | --- |
| `term_init()` / `term_cleanup()` | enter/leave raw mode, alternate screen |
| `term_clear()` | clear the screen |
| `term_move(row, col)` | move the cursor (0-indexed) |
| `term_print(s)` | print at the cursor |
| `term_fg(n)` / `term_bg(n)` | set color 0–15 |
| `term_reset_colors()` | back to default colors |
| `term_getch()` | block until a key, return its code |
| `term_getch_timeout(ms)` | wait up to `ms`; `0` means "poll" |
| `term_now_ms()` | monotonic time in milliseconds |
| `term_sleep_ms(ms)` | sleep |
| `term_hide_cursor()` / `term_show_cursor()` | cursor visibility |
| `term_mouse_on()` / `term_mouse_off()` / `term_mouse_event(...)` | mouse input |

Key codes are plain integers (`'q'` is `113`). Special keys such as arrows
return negative values declared in `term.h` (`TERM_KEY_UP`, `TERM_KEY_DOWN`,
and so on). See [`examples/pong.q`](../examples/pong.q) for a complete real-time
game, and [`examples/term_demo.q`](../examples/term_demo.q) for a tour.

A `str` crossing into C is passed as a NUL-terminated C string automatically;
`int`, `float` and `bool` map to `int64_t`, `double` and `bool`.

## 14. Grammar summary

```text
program    = { addDirective | externDecl | structDef | fnDef }
addDirective = "#add" "<" name ">"
externDecl = "extern" "fn" ident "(" [ param { "," param } ] ")" [ "->" type ] ";"
structDef  = "struct" ident "{" [ field { "," field } [","] ] "}"
field      = ident ":" type
fnDef      = "fn" ident "(" [ param { "," param } ] ")" [ "->" type ] block
param      = ident ":" type
type       = "int" | "float" | "bool" | "str" | "void" | "[" type "]" | ident

block      = "{" { stmt } [ expr ] "}"
stmt       = letStmt | assignStmt | "return" [ expr ] ";"
           | "break" ";" | "continue" ";" | expr ";"
letStmt    = "let" [ "mut" ] ident [ ":" type ] "=" expr ";"
assignStmt = expr ( "=" | "+=" | "-=" | "*=" | "/=" | "%=" ) expr ";"

expr       = unary { binop unary }
unary      = ( "!" | "-" ) unary | postfix
postfix    = primary { "(" args ")" | "." ident | "[" expr "]" }
primary    = int | float | "true" | "false" | string | ident
           | "[" [ expr { "," expr } [","] ] "]"
           | ident "{" ident ":" expr { "," ident ":" expr } [","] "}"
           | "(" expr ")" | block
           | "if" expr block [ "else" ( ifExpr | block ) ]
           | "while" expr block
           | "for" ident "in" ( expr ".." expr | expr ) block
```

## 15. Not in the language yet

These are deliberately deferred until they can be added without steepening the
learning curve:

* methods attached to structs (`fn Point.sum(self) -> int`)
* enums and exhaustive `match`
* optionals (`T?`, `if let`, `??`) and error values (`?`)
* generics and simple interfaces
* `impl` blocks, traits, macros — probably never in Rust's form
* modules and packages
* threads / async
* richer C FFI (structs and callbacks across the boundary; scalars, strings and
  pointers-to-opaque handles already work)
* automatic memory reclamation (v0.1 uses a process-lifetime arena; see
  [DESIGN.md](DESIGN.md))
* Unicode-aware string indexing and length
* array/struct printing beyond the supported element types

See the roadmap in [DESIGN.md](DESIGN.md) for the planned order.
