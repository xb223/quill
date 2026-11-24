//! End-to-end tests: compile Quill source to a native binary with clang and
//! check what the program actually prints.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use quill::{RUNTIME_C, RUNTIME_H};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Output {
    stdout: String,
    stderr: String,
    code: Option<i32>,
}

fn temp_dir() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("quill-e2e-{}-{nanos}-{n}", std::process::id()))
}

fn cc() -> String {
    std::env::var("CC").unwrap_or_else(|_| "clang".to_string())
}

fn compile_or_panic(source: &str) -> String {
    match quill::compile("test.q", source) {
        Ok(compilation) => compilation.c_source,
        Err((file, diags)) => panic!(
            "expected the program to compile, but got:\n{}",
            diags.render(&file)
        ),
    }
}

fn run(source: &str) -> Output {
    run_with_input(source, None)
}

fn run_with_input(source: &str, input: Option<&str>) -> Output {
    let c_source = compile_or_panic(source);
    let dir = temp_dir();
    fs::create_dir_all(&dir).expect("create temp dir");
    fs::write(dir.join("main.c"), &c_source).expect("write main.c");
    fs::write(dir.join("quill_rt.c"), RUNTIME_C).expect("write runtime");
    fs::write(dir.join("quill_rt.h"), RUNTIME_H).expect("write header");

    let binary = dir.join("program");
    let build = Command::new(cc())
        .arg("-std=gnu11")
        .arg("-O1")
        .arg("-Wno-parentheses-equality")
        .arg("-o")
        .arg(&binary)
        .arg(dir.join("main.c"))
        .arg(dir.join("quill_rt.c"))
        .arg("-lm")
        .output()
        .expect("run C compiler");

    if !build.status.success() {
        let stderr = String::from_utf8_lossy(&build.stderr).to_string();
        let _ = fs::remove_dir_all(&dir);
        panic!("C compilation failed:\n{stderr}\n--- generated C ---\n{c_source}");
    }

    let mut command = Command::new(&binary);
    command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().expect("run program");
    if let Some(input) = input {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .expect("stdin")
            .write_all(input.as_bytes())
            .expect("write stdin");
    }
    drop(child.stdin.take());

    let output = child.wait_with_output().expect("wait for program");
    let _ = fs::remove_dir_all(&dir);

    Output {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        code: output.status.code(),
    }
}

fn expect_stdout(source: &str, expected: &str) {
    let out = run(source);
    assert_eq!(
        out.stdout, expected,
        "stdout mismatch (exit {:?})\nstderr:\n{}",
        out.code, out.stderr
    );
    assert_eq!(out.code, Some(0), "program failed: {}", out.stderr);
}

fn expect_compile_error(source: &str) -> String {
    match quill::compile("test.q", source) {
        Ok(_) => panic!("expected a compile error"),
        Err((file, diags)) => diags.render(&file),
    }
}

#[test]
fn hello_world() {
    expect_stdout(
        r#"fn main() { println("Hello, Quill!"); }"#,
        "Hello, Quill!\n",
    );
}

#[test]
fn arithmetic_and_precedence() {
    expect_stdout(
        r#"fn main() {
            println(1 + 2 * 3);
            println((1 + 2) * 3);
            println(7 / 2);
            println(7 % 2);
            println(-5 + 2);
        }"#,
        "7\n9\n3\n1\n-3\n",
    );
}

#[test]
fn overflow_panics_instead_of_corrupting() {
    let out = run(
        r#"fn main() {
            let mut x = 9223372036854775807;
            x = x + 1;
        }"#,
    );
    assert_eq!(out.code, Some(101), "expected a panic exit code");
    assert!(out.stderr.contains("integer overflow in `+`"), "{}", out.stderr);
}

#[test]
fn bounds_are_checked() {
    let out = run(
        r#"fn main() {
            let xs = [10, 20, 30];
            println(xs[1]);
            println(xs[3]);
        }"#,
    );
    assert_eq!(out.stdout, "20\n");
    assert_eq!(out.code, Some(101));
    assert!(out.stderr.contains("out of bounds"), "{}", out.stderr);
}

#[test]
fn division_by_zero_panics() {
    let out = run(r#"fn main() { println(1 / 0); }"#);
    assert_eq!(out.code, Some(101));
    assert!(out.stderr.contains("division by zero"), "{}", out.stderr);
}

#[test]
fn strings_interpolation_and_comparison() {
    expect_stdout(
        r#"fn main() {
            let name = "world";
            println("hello, {name}!");
            println("a" + "b" + "c");
            println("abc" == "abc");
            println("abc" != "abd");
            println("abc" < "abd");
            println(len("héllo"));
        }"#,
        "hello, world!\nabc\ntrue\ntrue\ntrue\n6\n",
    );
}

#[test]
fn structs_nested_by_value() {
    // Outer is declared before Inner on purpose: codegen must order typedefs.
    expect_stdout(
        r#"struct Outer {
            a: int,
            inner: Inner,
        }
        struct Inner {
            value: int,
        }
        fn make() -> Outer {
            Outer { a: 1, inner: Inner { value: 9 } }
        }
        fn main() {
            let o = make();
            println(o.a + o.inner.value);
            o.a = 5;
            println(o.a);
        }"#,
        "10\n5\n",
    );
}

#[test]
fn arrays_push_index_len_and_iterate() {
    expect_stdout(
        r#"fn main() {
            let xs: [int] = [];
            for i in 0..4 {
                push(xs, i);
            }
            println(xs);
            println(len(xs));
            let mut sum = 0;
            for x in xs {
                sum = sum + x;
            }
            println(sum);
            xs[0] = 10;
            println(xs[0]);
        }"#,
        "[0, 1, 2, 3]\n4\n6\n10\n",
    );
}

#[test]
fn recursion() {
    expect_stdout(
        r#"fn fib(n: int) -> int {
            if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
        }
        fn main() { println(fib(10)); }"#,
        "55\n",
    );
}

#[test]
fn if_and_block_expressions_produce_values() {
    expect_stdout(
        r#"fn main() {
            let x = if 2 > 1 { 10 } else { 20 };
            let y = { let a = 5; a * 2 };
            let z = if false { 1 } else if true { 2 } else { 3 };
            println(x + y + z);
        }"#,
        "22\n",
    );
}

#[test]
fn loops_break_and_continue() {
    expect_stdout(
        r#"fn main() {
            let mut i = 0;
            while true {
                i = i + 1;
                if i == 3 { break; }
            }
            let mut evens = 0;
            for n in [1, 2, 3, 4, 5, 6] {
                if n % 2 != 0 { continue; }
                evens = evens + 1;
            }
            println(i);
            println(evens);
        }"#,
        "3\n3\n",
    );
}

#[test]
fn conversions_and_builtins() {
    expect_stdout(
        r#"fn main() {
            println(int("42") + 1);
            println(float("2.5") * 2.0);
            println(int(2.9));
            println(abs(-5));
            println(min(3, 7));
            println(max(2.5, 1.0));
            println(str(12) + "!");
        }"#,
        "43\n5\n2\n5\n3\n2.5\n12!\n",
    );
}

#[test]
fn shadowing_and_mutable_bindings() {
    expect_stdout(
        r#"fn main() {
            let x = 1;
            {
                let x = 2;
                println(x);
            }
            println(x);
            let mut y = 1;
            y = y + 1;
            println(y);
        }"#,
        "2\n1\n2\n",
    );
}

#[test]
fn float_formatting_is_shortest_round_trip() {
    expect_stdout(
        r#"fn main() {
            println(1.0);
            println(0.1);
            println(1.25);
            println(1e9);
            println(1.0 / 3.0);
        }"#,
        "1\n0.1\n1.25\n1000000000\n0.3333333333333333\n",
    );
}

#[test]
fn unicode_identifiers_and_strings() {
    expect_stdout(
        r#"fn main() {
            let café = "naïve";
            println("☕ {café}");
        }"#,
        "☕ naïve\n",
    );
}

#[test]
fn assertions_pass() {
    expect_stdout(
        r#"fn main() { assert(1 + 1 == 2); println("ok"); }"#,
        "ok\n",
    );
}

#[test]
fn failing_assertion_reports_the_line() {
    let out = run("fn main() {\n    assert(false);\n}\n");
    assert_eq!(out.code, Some(101));
    assert!(
        out.stderr.contains("test.q:2: assertion failed"),
        "{}",
        out.stderr
    );
}

#[test]
fn early_returns() {
    expect_stdout(
        r#"fn sign(n: int) -> str {
            if n > 0 { return "positive"; }
            if n < 0 { return "negative"; }
            "zero"
        }
        fn main() {
            println(sign(1));
            println(sign(-1));
            println(sign(0));
        }"#,
        "positive\nnegative\nzero\n",
    );
}

#[test]
fn string_to_int_rejects_garbage() {
    let out = run(r#"fn main() { println(int("nope")); }"#);
    assert_eq!(out.code, Some(101));
    assert!(out.stderr.contains("cannot convert string"), "{}", out.stderr);
}

#[test]
fn friendly_compile_errors() {
    let text = expect_compile_error(
        r#"fn main() {
            let x = 1;
            x = 2;
            println(missing);
        }"#,
    );
    assert!(text.contains("cannot reassign `x`"), "{text}");
    assert!(text.contains("cannot find a value named `missing`"), "{text}");
    assert!(text.contains("--> test.q:"), "{text}");
    assert!(text.contains('^'), "{text}");
}

#[test]
fn type_errors_explain_conversions() {
    let text = expect_compile_error(
        r#"fn main() {
            let s: str = 42;
            let n: int = "42";
        }"#,
    );
    assert!(text.contains("expected `str`, found `int`"), "{text}");
    assert!(text.contains("str(value)"), "{text}");
    assert!(text.contains("expected `int`, found `str`"), "{text}");
}

#[test]
fn compound_assignment() {
    expect_stdout(
        r#"fn main() {
            let mut x = 10;
            x += 5;
            x -= 3;
            x *= 2;
            x /= 4;
            x %= 4;
            println(x);

            let mut s = "a";
            s += "b";
            println(s);

            let xs = [1, 2, 3];
            xs[1] += 10;
            println(xs[1]);

            let mut f: float = 1.0;
            f += 2;
            println(f);
        }"#,
        "2\nab\n12\n3\n",
    );
}

#[test]
fn reads_lines_and_numbers_from_stdin() {
    let out = run_with_input(
        r#"fn main() {
            let name = read_line();
            let n = read_int();
            let f = read_float();
            println("hi {name}, {n + 1}, {f * 2.0}");
        }"#,
        Some("ada\n41\n1.25\n"),
    );
    assert_eq!(out.code, Some(0), "{}", out.stderr);
    assert_eq!(out.stdout, "hi ada, 42, 2.5\n");
}

#[test]
fn compound_assignment_rejects_bool() {
    let text = expect_compile_error(
        r#"fn main() {
            let mut ok = true;
            ok += false;
        }"#,
    );
    assert!(text.contains("cannot apply `+=`"), "{text}");
}

#[test]
fn calling_main_is_rejected() {
    let text = expect_compile_error("fn main() { main(); }");
    assert!(text.contains("`main` cannot be called directly"), "{text}");
}

#[test]
fn missing_return_path_is_rejected() {
    let text = expect_compile_error(
        r#"fn f(x: int) -> int {
            if x > 0 { return 1; }
        }
        fn main() { }"#,
    );
    assert!(
        text.contains("not all paths") || text.contains("expected `int`"),
        "{text}"
    );
}

#[test]
fn extern_c_functions_from_system_header() {
    expect_stdout(
        r#"#add <math.h>
        extern fn sqrt(x: float) -> float;
        extern fn pow(base: float, exp: float) -> float;
        fn main() {
            println(sqrt(16.0));
            println(pow(2.0, 10.0));
        }"#,
        "4\n1024\n",
    );
}

#[test]
fn term_header_is_bundled_and_included() {
    let c = compile_or_panic(
        r#"#add <term.h>
        extern fn term_move(row: int, col: int) -> void;
        extern fn term_print(s: str) -> void;
        fn main() {
            term_move(1, 1);
            term_print("hi");
        }"#,
    );
    assert!(c.contains("#include \"term.h\""), "{c}");
    assert!(c.contains("term_move((1LL), (1LL))"), "{c}");
    // Real strings cross the FFI boundary as NUL-terminated C strings.
    assert!(c.contains("term_print(q_str_cstr("), "{c}");
}

#[test]
fn extern_calls_pass_strings_as_c_strings() {
    let c = compile_or_panic(
        r#"#add <stdio.h>
        extern fn puts(s: str) -> int;
        fn main() {
            puts("hello from C");
        }"#,
    );
    assert!(c.contains("puts(q_str_cstr("), "{c}");
}

#[test]
fn extern_function_arg_types_are_checked() {
    let text = expect_compile_error(
        r#"#add <math.h>
        extern fn sqrt(x: float) -> float;
        fn main() { println(sqrt("nope")); }"#,
    );
    assert!(text.contains("expected `float`, found `str`"), "{text}");
}
