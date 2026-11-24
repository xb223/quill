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

/// The bundled terminal library, used by `#add <term.h>`.
pub const TERM_C: &str = include_str!("../runtime/term.c");
pub const TERM_H: &str = include_str!("../runtime/term.h");

/// Libraries that ship with Quill itself, as (header, source, extra link flags).
pub fn bundled_source(library: &str) -> Option<&'static str> {
    match library {
        "term.h" => Some(TERM_C),
        _ => None,
    }
}

/// Extra C compiler flags needed for a system library header.
pub fn library_link_flags(library: &str) -> Vec<String> {
    match library {
        // Bundled libraries are compiled from source; no -l needed.
        "term.h" => Vec::new(),
        // A header such as `math.h` or `ncurses.h` maps to `-lm` / `-lncurses`.
        other => {
            let stem = other.strip_suffix(".h").unwrap_or(other);
            // Headers without a matching system library, or whose library has
            // a different name than the header (`math.h` -> `-lm`, already
            // always linked).
            const NO_LIB: &[&str] = &[
                "stdio", "stdlib", "string", "stdint", "stdbool", "stddef", "time",
                "unistd", "fcntl", "math", "errno", "ctype", "assert",
            ];
            if NO_LIB.contains(&stem) {
                Vec::new()
            } else {
                vec![format!("-l{stem}")]
            }
        }
    }
}

pub struct Compilation {
    pub file: SourceFile,
    pub c_source: String,
    pub adds: Vec<String>,
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
    let adds = program.adds.iter().map(|a| a.library.clone()).collect();
    Ok(Compilation { file, c_source, adds })
}
