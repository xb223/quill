//! Quill — a friendly systems language.
//!
//! The compiler pipeline lives here so that it can be driven both by the
//! `quillc` command line tool and by the test suite.
//!
//! ```text
//! source -> lexer -> parser -> checker -> C codegen -> clang -> native binary
//! ```

pub mod codegen;
pub mod diag;
pub mod lexer;
pub mod parser;
pub mod sema;

/// Compiler version shown by `quillc --version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
