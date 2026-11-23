# Quill design notes

This document explains *why* Quill looks the way it does, what it guarantees,
and where it is going.

## The one-sentence goal

> Make the power of a modern systems language available to someone in their
> first week of programming, without giving up native performance or memory
> safety.

Rust proved that a language can be safe without a garbage collector. It also
proved that proving safety to a compiler can be harder than proving it to a
human. Quill takes a different trade: **safety by construction and runtime
checks, not by borrow analysis.**

## Flaws Quill targets

### 1. The borrow checker and lifetimes

In Rust, every reference must be proven valid at compile time. This eliminates
use-after-free and data races, but it also eliminates a beginner's ability to
write a linked list, return a slice, or share a value between two structures
without learning a new mental model.

Quill has no references and no lifetimes. Values are values: passing a value
copies it (or copies a cheap handle). Mutation is explicit through `let mut`.
Memory is reclaimed automatically, so there is nothing to prove.

### 2. Two string types

Rust exposes `String`, `&str`, `&'static str`, `Cow<str>`, `OsStr`, `CStr` and
conversions between them. Quill has exactly one string type, `str`. It is
immutable; building a new string allocates a new one.

### 3. `async` function coloring

An `async fn` can only be called from another `async fn`, which spreads through
every caller and forces runtime choices on library authors. Quill has no
`async` keyword. A single structured concurrency model is planned instead.

### 4. Macros

Rust's two macro systems are powerful and famously hard to learn, debug and
teach. Quill v0.1 has no macros. String interpolation covers the most common
macro-like need (`println!("{}", x)` becomes `println("x = {x}")`). Features
that would normally need macros (`derive`, `vec!`) are built into the language
or the standard library.

### 5. Traits, coherence and orphan rules

Trait resolution conflicts produce some of Rust's most intimidating errors.
Quill uses plain functions and (planned) simple interfaces. There are no
orphan-rule puzzles because there is no trait implementation search across
crates.

### 6. Hidden undefined behavior

C and C++ make signed overflow, out-of-bounds access and null dereference
undefined. Rust makes most of these panics in debug builds but silent in
release. Quill checks integer overflow and array bounds **always**, in every
build profile. Programs may abort with a clear message; they may not corrupt
memory.

### 7. Verbose error handling

`Result<T, E>`, `?`, `From` conversions, `Box<dyn Error>` and `.map_err(..)`
are a lot of machinery for "this might fail". Quill v0.1 keeps errors simple
(see roadmap): a `?` operator on a single built-in result type with readable
messages, no conversion traits.

### 8. Compile-time and tooling weight

The reference Rust compiler is a large, LLVM-based system. Quill is a small
hand-written compiler that emits readable C. Debug builds are seconds, not
minutes, and `quillc emit-c` makes the lowering transparent.

## Language principles

1. **One obvious way.** No redundant syntax for the same idea.
2. **Immutable bindings, mutable values.** `let` is immutable; `let mut` opts in.
   This catches accidental reassignment without infecting function signatures.
3. **Values, not references.** No `&`/`*` in user code. Handles to strings and
   arrays are copied freely.
4. **Safety is not optional.** Bounds checks, overflow checks, no null, no
   uninitialized reads. There is no `unsafe` in v0.1.
5. **Errors are a conversation.** Every diagnostic points at source, explains
   the rule, and suggests a fix.
6. **Small core, clear lowering.** If a feature cannot be explained in a page,
   it waits.

## Semantics (v0.1)

- **Types**: `int` (64-bit signed), `float` (64-bit IEEE-754), `bool`, `str`,
  `[T]` (growable array), `void`, and user `struct`s (value types).
- **Bindings**: `let` is immutable, `let mut` is mutable. Shadowing is allowed.
- **Functions**: explicit parameter and return types; the body's final
  expression is its value; `return` also works.
- **Control flow**: expressions everywhere. `if`/`else` and blocks produce
  values. `while`, `for i in 0..n`, `for x in array`, `break`, `continue`.
- **Structs**: named fields, literals, field access. No inheritance.
- **Strings**: immutable UTF-8 byte strings; `+` concatenates; `{}` interpolation;
  `len` counts bytes.
- **Arrays**: homogeneous, growable, zero-indexed, bounds checked. Literals infer
  their element type; empty literals need an annotation.
- **Integer arithmetic**: checked (`+`, `-`, `*`, `/`, `%`, unary `-`); overflow
  aborts with a diagnostic message.
- **Conversions**: `int(x)`, `float(x)`, `str(x)`; implicit `int` → `float`
  widening where a float is expected.
- **Builtins**: `print`, `println`, `len`, `push`, `assert`, `abs`, `min`, `max`.
- **Entry point**: `fn main()` with no parameters.

Deliberately absent from v0.1: generics, traits, enums, pattern matching,
closures, modules, threads, FFI, macros. Each will be added only with a design
that keeps the learning curve flat.

## Memory model

v0.1 uses a **process-lifetime arena**: every string, array and temporary is
allocated from a bump allocator that is released when the program exits. This is
safe (no dangling pointers, no use-after-free) and predictable, but it does not
reuse memory during execution.

The next milestone replaces the arena with precise reclamation while keeping the
language surface identical. The plan is a non-moving mark-and-sweep collector
with a shadow stack maintained by generated code, so user programs never see
ownership annotations. Because the language has no raw pointers, the collector
can be precise rather than conservative.

This is an explicit trade: v0.1 ships a working, safe language quickly; memory
reuse is an implementation detail that can improve without breaking programs.

## Backend

Quill emits C11 (plus the widely supported GNU statement-expression extension,
available in clang and gcc). Benefits:

- native code without depending on LLVM's C++ API;
- generated code is inspectable and debuggable;
- the compiler stays small enough to read in an afternoon.

`-O2` is used for `build` and `run`. Checked arithmetic and bounds checks are in
the runtime and cannot be optimized away.

## Roadmap

- [x] Lexer, parser, type checker, C backend, CLI
- [x] Strings, interpolation, arrays, structs, checked arithmetic
- [ ] Methods (`fn Point.sum(self) -> int`) declared next to the struct
- [ ] Enums + exhaustive `match`
- [ ] `T?` optionals with `if let` and `??`
- [ ] Error values + `?` (no trait machinery)
- [ ] Simple interfaces (type classes without coherence puzzles)
- [ ] Precise mark-and-sweep GC (same language surface)
- [ ] Modules and a tiny package manager
- [ ] Structured concurrency
- [ ] C FFI (explicit, audited boundary)
- [ ] Better diagnostics: suggestions, machine-readable JSON
- [ ] Self-hosting exploration (a compiled language should be able to compile
      itself eventually)

## Non-goals

- Winning every benchmark. Quill aims to be within a small constant factor of
  Rust for ordinary code, not to beat it.
- Replacing Rust for kernel or embedded work in v1.
- A macro system.
- Backwards compatibility before 1.0. The language will change as it learns.
