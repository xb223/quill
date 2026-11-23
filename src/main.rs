use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::time::{SystemTime, UNIX_EPOCH};

use quill::{Compilation, RUNTIME_C, RUNTIME_H};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        usage();
        process::exit(2);
    }

    match args[0].as_str() {
        "--version" | "-V" | "version" => {
            println!("quillc {}", quill::VERSION);
        }
        "help" | "--help" | "-h" => usage(),
        "check" => {
            let path = require_file(&args, 1, "check");
            let source = read_source(&path);
            match quill::compile(&path.display().to_string(), &source) {
                Ok(_) => {}
                Err((file, diags)) => fail(&file, &diags),
            }
        }
        "emit-c" => {
            let path = require_file(&args, 1, "emit-c");
            let source = read_source(&path);
            match quill::compile(&path.display().to_string(), &source) {
                Ok(compilation) => print!("{}", compilation.c_source),
                Err((file, diags)) => fail(&file, &diags),
            }
        }
        "build" => {
            let path = require_file(&args, 1, "build");
            let output = match args.iter().position(|a| a == "-o" || a == "--output") {
                Some(i) => {
                    let Some(value) = args.get(i + 1) else {
                        eprintln!("error: `{}` needs a path after it", args[i]);
                        process::exit(2);
                    };
                    PathBuf::from(value)
                }
                None => default_output(&path),
            };
            let source = read_source(&path);
            let compilation = compile_or_exit(&path, &source);
            match build_native(&compilation, &output) {
                Ok(()) => {}
                Err(message) => {
                    eprintln!("{message}");
                    process::exit(1);
                }
            }
        }
        "run" => {
            let path = require_file(&args, 1, "run");
            let source = read_source(&path);
            let compilation = compile_or_exit(&path, &source);
            run_native(&compilation);
        }
        other if other.ends_with(".q") => {
            let path = PathBuf::from(other);
            let source = read_source(&path);
            let compilation = compile_or_exit(&path, &source);
            run_native(&compilation);
        }
        other => {
            eprintln!("error: unknown command `{other}`");
            usage();
            process::exit(2);
        }
    }
}

fn usage() {
    println!(
        "quillc {} — the Quill compiler

usage:
  quillc run   <file.q>             compile and run a program
  quillc build <file.q> [-o path]   compile to a native binary
  quillc check <file.q>             type-check without compiling
  quillc emit-c <file.q>            print the generated C
  quillc --version

environment:
  CC    C compiler to use (default: clang, then cc)",
        quill::VERSION
    );
}

fn require_file(args: &[String], index: usize, command: &str) -> PathBuf {
    match args.get(index) {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("error: `quillc {command}` needs a .q file");
            process::exit(2);
        }
    }
}

fn read_source(path: &Path) -> String {
    match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("error: cannot read `{}`: {error}", path.display());
            process::exit(1);
        }
    }
}

fn compile_or_exit(path: &Path, source: &str) -> Compilation {
    match quill::compile(&path.display().to_string(), source) {
        Ok(compilation) => compilation,
        Err((file, diags)) => fail(&file, &diags),
    }
}

fn fail(file: &quill::diag::SourceFile, diags: &quill::diag::Diags) -> ! {
    eprint!("{}", diags.render(file));
    process::exit(1);
}

fn default_output(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "a.out".to_string());
    #[cfg(windows)]
    {
        PathBuf::from(format!("{stem}.exe"))
    }
    #[cfg(not(windows))]
    {
        PathBuf::from(stem)
    }
}

fn temp_dir() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    env::temp_dir().join(format!("quillc-{}-{nanos}", process::id()))
}

fn c_compiler() -> String {
    env::var("CC").unwrap_or_else(|_| "clang".to_string())
}

/// Write the generated C and runtime to a temporary directory.
fn write_project(dir: &Path, compilation: &Compilation) {
    if let Err(error) = fs::create_dir_all(dir) {
        eprintln!("error: cannot create temp directory: {error}");
        process::exit(1);
    }
    let files = [
        ("main.c", compilation.c_source.as_str()),
        ("quill_rt.c", RUNTIME_C),
        ("quill_rt.h", RUNTIME_H),
    ];
    for (name, contents) in files {
        if let Err(error) = fs::write(dir.join(name), contents) {
            eprintln!("error: cannot write `{name}`: {error}");
            process::exit(1);
        }
    }
}

fn build_native(compilation: &Compilation, output: &Path) -> Result<(), String> {
    let dir = temp_dir();
    write_project(&dir, compilation);
    let main_c = dir.join("main.c");
    let runtime_c = dir.join("quill_rt.c");

    let status = Command::new(c_compiler())
        .arg("-std=gnu11")
        .arg("-O2")
        .arg("-Wno-parentheses-equality")
        .arg("-o")
        .arg(output)
        .arg(&main_c)
        .arg(&runtime_c)
        .arg("-lm")
        .status();

    let result = match status {
        Ok(status) if status.success() => Ok(()),
        Ok(status) => Err(format!(
            "error: the C compiler exited with {status}\ngenerated C kept in {}",
            dir.display()
        )),
        Err(error) => Err(format!(
            "error: cannot run the C compiler (`{}`): {error}\nset $CC to a working C compiler",
            c_compiler()
        )),
    };

    if result.is_ok() {
        let _ = fs::remove_dir_all(&dir);
    }
    result
}

fn run_native(compilation: &Compilation) {
    let dir = temp_dir();
    write_project(&dir, compilation);
    let binary = dir.join("program");

    let compile = Command::new(c_compiler())
        .arg("-std=gnu11")
        .arg("-O2")
        .arg("-Wno-parentheses-equality")
        .arg("-o")
        .arg(&binary)
        .arg(dir.join("main.c"))
        .arg(dir.join("quill_rt.c"))
        .arg("-lm")
        .status();

    match compile {
        Ok(status) if status.success() => {}
        Ok(status) => {
            eprintln!(
                "error: the C compiler exited with {status}\ngenerated C kept in {}",
                dir.display()
            );
            process::exit(1);
        }
        Err(error) => {
            eprintln!(
                "error: cannot run the C compiler (`{}`): {error}\nset $CC to a working C compiler",
                c_compiler()
            );
            process::exit(1);
        }
    }

    let status = Command::new(&binary).status();
    let _ = fs::remove_dir_all(&dir);

    match status {
        Ok(status) => process::exit(status.code().unwrap_or(1)),
        Err(error) => {
            eprintln!("error: cannot run the compiled program: {error}");
            process::exit(1);
        }
    }
}
