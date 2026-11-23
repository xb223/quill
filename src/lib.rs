//! Quill — a friendly systems language.
//!
//! The compiler pipeline lives here so that it can be driven both by the
//! `quillc` command line tool and by the test suite.
//!
//! ```text
//! source -> lexer -> parser -> checker -> C codegen -> clang -> native binary
//! ```

pub mod ast;
pub mod codegen;
pub mod diag;
pub mod lexer;
pub mod parser;
pub mod sema;

use diag::{Diags, SourceFile};

/// Compiler version shown by `quillc --version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The C runtime is embedded so the installed `quillc` binary is self-contained.
pub const RUNTIME_C: &str = include_str!("../runtime/quill_rt.c");
pub const RUNTIME_H: &str = include_str!("../runtime/quill_rt.h");

pub struct Compilation {
    pub file: SourceFile,
    pub c_source: String,
}

/// Compile Quill source to C. On failure, returns the source file together with
/// fully-rendered-ready diagnostics.
pub fn compile(name: &str, source: &str) -> Result<Compilation, (SourceFile, Diags)> {
    let file = SourceFile::new(name, source);

    let (program, parse_diags) = parser::parse(source);
    if parse_diags.has_errors() {
        return Err((file, parse_diags));
    }
    let Some(mut program) = program else {
        let mut diags = parse_diags;
        diags.global_error("the program could not be parsed");
        return Err((file, diags));
    };

    let check_diags = sema::check(&mut program);
    if check_diags.has_errors() {
        return Err((file, check_diags));
    }

    let c_source = codegen::generate(&program, &file);
    Ok(Compilation { file, c_source })
}
