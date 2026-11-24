# Quill

**A friendly systems language: Rust-like safety, without the borrow checker.**

Quill compiles to native code through C and clang. It is designed for people who
want fast, memory-safe programs without spending weeks fighting lifetimes,
aliasing rules or `async` coloring.

```quill
struct Point {
    x: int,
    y: int,
}

fn distance_squared(a: Point, b: Point) -> int {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

fn main() {
    let origin = Point { x: 0, y: 0 };
    let p = Point { x: 3, y: 4 };
    println("distance² = {distance_squared(origin, p)}");
}
```

```console
$ quillc run examples/tour.q
distance² = 25
```

Calling C libraries is a directive and a declaration:

```quill
#add <math.h>
extern fn sqrt(x: float) -> float;

fn main() {
    println(sqrt(16.0));   // 4
}
```

The bundled `term` library gives you cursor control, colors, non-blocking keys
and timing — enough for real-time terminal games:

```console
$ quillc run examples/pong.q
```

## Why another language?

Quill keeps the good parts of Rust — value semantics, immutable bindings,
expressive `struct`s, exhaustive errors, no null, native speed — and removes the
parts that make the first month miserable:

| Rust pain point | Quill's answer |
| --- | --- |
| Borrow checker, lifetimes, aliasing rules | Plain values; automatically managed strings and arrays. No `&`, `&mut`, or lifetime annotations. |
| `String` vs `&str`, `.clone()` sprinkling | One `str` type. Copying is cheap and invisible. |
| `async fn` colors the whole call graph | No async coloring. Concurrency will be structured (planned). |
| Macro soup (`!`, proc macros, `#[derive]`) | No macros. String interpolation and simple language features instead. |
| Trait coherence, orphan rules, `impl` blocks | Methods belong to the type, declared inline (planned); simple interfaces. |
| Silent integer overflow in release builds | Arithmetic is checked by default, everywhere. |
| `unsafe`, null pointers, use-after-free | Safe by default; no null; bounds and overflow checks. |
| Slow debug builds and huge generics trees | A small compiler with a C backend and no monomorphization in v0.1. |
| Cryptic compiler errors | Human-first diagnostics with source spans and fix hints. |

## Status

Quill is an early-stage compiler (v0.1). It already compiles real programs:
functions, `let`/`mut`, `if`/`else`, `while`, `for`, `struct`s, strings with
interpolation, arrays, stdin input, compound assignment, and checked arithmetic.
It calls C libraries with `#add <header.h>` and `extern fn`, and ships a small
ncurses-like `term` library for real-time terminal programs — see
[`examples/pong.q`](examples/pong.q). The optimizer is clang's.

See [docs/LANGUAGE.md](docs/LANGUAGE.md) for the language guide and
[docs/DESIGN.md](docs/DESIGN.md) for the design rationale and roadmap.

## Build

Requirements: Rust (stable), clang (or any C11 compiler in `$CC`).

```console
$ cargo build --release
$ ./target/release/quillc run examples/hello.q
```

Or install the compiler into your Cargo bin directory:

```console
$ cargo install --path .
$ quillc run examples/hello.q
```

## Commands

```
quillc run   <file.q>        compile and run a program
quillc build <file.q> [-o]   compile to a native binary
quillc check <file.q>        type-check only
quillc emit-c <file.q>       print the generated C
quillc --version
```

## How it works

```
source .q
   │
   ▼
 lexer ──► parser ──► type checker ──► C code generator ──► clang ──► native binary
                                          │
                                          └── small C runtime (strings, arrays, checks)
```

The generated C is intentionally readable: `quillc emit-c hello.q` is a good
way to see exactly what your program means. The C backend keeps the compiler
small, portable and fast to build; the language itself is independent of the
backend.

## License

MIT
