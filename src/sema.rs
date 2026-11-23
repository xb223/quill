//! Semantic analysis: name resolution, type checking and borrow-checker-free
//! mutability rules.
//!
//! The checker annotates every expression with its type (`Expr::ty`) so the
//! code generator never has to guess. It also enforces the rules Quill promises
//! to beginners:
//!
//! * every variable is initialized before use;
//! * `let` bindings cannot be reassigned (`let mut` can);
//! * arithmetic and comparisons are type-correct;
//! * functions, structs and fields actually exist;
//! * functions that return a value do so on every path.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::diag::{Diags, Span};

const BUILTINS: &[&str] = &[
    "print", "println", "len", "push", "str", "int", "float", "abs", "min", "max", "assert",
];

#[derive(Clone)]
struct StructInfo {
    fields: Vec<(String, Type)>,
    span: Span,
}

#[derive(Clone)]
struct FnSig {
    params: Vec<Type>,
    ret: Type,
}

struct Ctx {
    structs: HashMap<String, StructInfo>,
    fns: HashMap<String, FnSig>,
}

struct Binding {
    ty: Type,
    mutable: bool,
}

struct Env {
    scopes: Vec<HashMap<String, Binding>>,
    ret: Type,
    loop_depth: usize,
}

impl Env {
    fn new(ret: Type) -> Self {
        Env {
            scopes: Vec::new(),
            ret,
            loop_depth: 0,
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn bind(&mut self, name: String, ty: Type, mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, Binding { ty, mutable });
        }
    }

    fn lookup(&self, name: &str) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    fn visible_names(&self) -> Vec<String> {
        self.scopes
            .iter()
            .flat_map(|s| s.keys().cloned())
            .collect()
    }
}

/// Type-check a whole program, annotating expressions in place.
pub fn check(program: &mut Program) -> Diags {
    let mut diags = Diags::new();

    let mut struct_names = HashSet::new();
    for s in &program.structs {
        if is_primitive_name(&s.name) {
            diags.error_note(
                s.span,
                format!("`{}` is a built-in type name and cannot be a struct", s.name),
                "choose a different struct name",
            );
        }
        if !struct_names.insert(s.name.clone()) {
            diags.error(s.span, format!("duplicate struct `{}`", s.name));
        }
    }

    let mut fn_sigs: HashMap<String, FnSig> = HashMap::new();
    for f in &program.fns {
        if BUILTINS.contains(&f.name.as_str()) {
            diags.error_note(
                f.span,
                format!("`{}` is a built-in function", f.name),
                "choose a different name",
            );
            continue;
        }
        if struct_names.contains(&f.name) {
            diags.error(
                f.span,
                format!("`{}` is already used as a struct name", f.name),
            );
            continue;
        }
        if fn_sigs.contains_key(&f.name) {
            diags.error(f.span, format!("duplicate function `{}`", f.name));
            continue;
        }

        let mut params = Vec::new();
        for p in &f.params {
            validate_type(&p.ty, &struct_names, false, &mut diags);
            params.push(p.ty.clone());
        }
        validate_type(&f.ret, &struct_names, true, &mut diags);

        fn_sigs.insert(
            f.name.clone(),
            FnSig {
                params,
                ret: f.ret.clone(),
            },
        );
    }

    let mut structs = HashMap::new();
    for s in &program.structs {
        let mut seen = HashSet::new();
        let mut fields = Vec::new();
        for f in &s.fields {
            if !seen.insert(f.name.clone()) {
                diags.error(
                    f.span,
                    format!("duplicate field `{}` in struct `{}`", f.name, s.name),
                );
            }
            validate_type(&f.ty, &struct_names, false, &mut diags);
            fields.push((f.name.clone(), f.ty.clone()));
        }
        structs.insert(
            s.name.clone(),
            StructInfo {
                fields,
                span: s.span,
            },
        );
    }

    check_struct_cycles(&program.structs, &structs, &mut diags);

    match fn_sigs.get("main") {
        None => diags.global_error("missing entry point: no `fn main()` was found"),
        Some(sig) if !sig.params.is_empty() => {
            let span = program
                .fns
                .iter()
                .find(|f| f.name == "main")
                .map(|f| f.span)
                .unwrap_or_default();
            diags.error_note(
                span,
                "`main` must not take any parameters",
                "declare it as `fn main() { ... }`",
            );
        }
        Some(sig) if !sig.ret.is_void() => {
            let span = program
                .fns
                .iter()
                .find(|f| f.name == "main")
                .map(|f| f.span)
                .unwrap_or_default();
            diags.error_note(
                span,
                "`main` must not return a value",
                "use `return;` or end `main` with a statement",
            );
        }
        Some(_) => {}
    }

    if diags.has_errors() {
        return diags;
    }

    let mut checker = Checker {
        ctx: Ctx {
            structs,
            fns: fn_sigs,
        },
        diags,
    };
    for f in &mut program.fns {
        checker.check_fn(f);
    }
    checker.diags
}

fn validate_type(
    ty: &Type,
    struct_names: &HashSet<String>,
    allow_void: bool,
    diags: &mut Diags,
) {
    match &ty.kind {
        TypeKind::Void if !allow_void => {
            diags.error(ty.span, "`void` is not a valid value type here");
        }
        TypeKind::Named(name) if !struct_names.contains(name) => {
            let suggestion = closest(name, struct_names.iter().cloned());
            if let Some(s) = suggestion {
                diags.error_note(
                    ty.span,
                    format!("cannot find type `{name}`"),
                    format!("did you mean `{s}`?"),
                );
            } else {
                diags.error(ty.span, format!("cannot find type `{name}`"));
            }
        }
        TypeKind::Array(inner) => validate_type(inner, struct_names, false, diags),
        TypeKind::Named(_) | TypeKind::Int | TypeKind::Float | TypeKind::Bool | TypeKind::Str
        | TypeKind::Void | TypeKind::Error => {}
    }
}

fn check_struct_cycles(
    defs: &[StructDef],
    structs: &HashMap<String, StructInfo>,
    diags: &mut Diags,
) {
    fn visit(
        name: &str,
        structs: &HashMap<String, StructInfo>,
        state: &mut HashMap<String, u8>,
        stack: &mut Vec<String>,
        diags: &mut Diags,
    ) {
        match state.get(name).copied().unwrap_or(0) {
            2 => return,
            1 => {
                let span = structs
                    .get(name)
                    .map(|s| s.span)
                    .unwrap_or_default();
                let chain = stack.join(" -> ");
                diags.error_note(
                    span,
                    format!("struct `{name}` contains itself by value"),
                    format!("cycle: {chain}; store the value in an array or restructure the types"),
                );
                return;
            }
            _ => {}
        }

        state.insert(name.to_string(), 1);
        stack.push(name.to_string());
        if let Some(info) = structs.get(name) {
            for (_, field_ty) in &info.fields {
                if let TypeKind::Named(target) = &field_ty.kind {
                    visit(target, structs, state, stack, diags);
                }
            }
        }
        stack.pop();
        state.insert(name.to_string(), 2);
    }

    let mut state = HashMap::new();
    let mut stack = Vec::new();
    for def in defs {
        if state.get(&def.name).copied().unwrap_or(0) == 0 {
            visit(&def.name, structs, &mut state, &mut stack, diags);
        }
    }
}

struct Checker {
    ctx: Ctx,
    diags: Diags,
}

impl Checker {
    // -------------------------------------------------------------- functions

    fn check_fn(&mut self, f: &mut FnDef) {
        let mut env = Env::new(f.ret.clone());
        env.push_scope();
        for p in &f.params {
            env.bind(p.name.clone(), p.ty.clone(), false);
        }

        let expected = if f.ret.is_void() {
            None
        } else {
            Some(f.ret.clone())
        };
        let body_ty = self.check_block(&mut f.body, expected.as_ref(), &mut env);
        env.pop_scope();

        if f.ret.is_void() {
            if !body_ty.is_void() && !body_ty.is_error() {
                let span = f
                    .body
                    .value
                    .as_ref()
                    .map(|v| v.span)
                    .unwrap_or(f.body.span);
                self.diags.error_note(
                    span,
                    format!(
                        "this function returns nothing, but its final expression has type `{body_ty}`"
                    ),
                    "add a `;` to discard the value, or declare a return type",
                );
            }
        } else if f.body.value.is_none() && !block_diverges(&f.body) {
            let span = f.body.span;
            self.diags.error_note(
                span,
                format!("not all paths in `{}` return a value of type `{}`", f.name, f.ret),
                format!("make the last expression a `{}`, or `return` on every path", f.ret),
            );
        }
    }

    fn check_block(
        &mut self,
        block: &mut Block,
        expected: Option<&Type>,
        env: &mut Env,
    ) -> Type {
        env.push_scope();
        for stmt in &mut block.stmts {
            self.check_stmt(stmt, env);
        }
        let ty = match &mut block.value {
            Some(value) => match expected {
                Some(exp) => self.check_expr_expected(value, exp, env),
                None => self.check_expr(value, env),
            },
            None => Type::void(block.span),
        };
        env.pop_scope();
        ty
    }

    fn check_stmt(&mut self, stmt: &mut Stmt, env: &mut Env) {
        match stmt {
            Stmt::Let {
                name,
                mutable,
                ty,
                value,
                span,
            } => {
                let bound_ty = match ty {
                    Some(ann) => {
                        if ann.is_void() {
                            self.diags.error(
                                ann.span,
                                "a variable cannot have type `void`",
                            );
                        }
                        self.check_expr_expected(value, ann, env);
                        ann.clone()
                    }
                    None => {
                        let inferred = self.check_expr(value, env);
                        if inferred.is_void() {
                            self.diags.error_note(
                                value.span,
                                "this expression produces no value",
                                format!("`{name}` needs a value; did you mean to call a function that returns something?"),
                            );
                        }
                        inferred
                    }
                };
                let _ = span;
                env.bind(name.clone(), bound_ty, *mutable);
            }
            Stmt::Assign {
                target,
                value,
                span,
            } => {
                if !is_lvalue(target) {
                    self.diags.error_note(
                        target.span,
                        "cannot assign to this expression",
                        "assign to a variable, a field, or an array element",
                    );
                    self.check_expr(value, env);
                    return;
                }

                if let ExprKind::Ident(name) = &target.kind {
                    if let Some(binding) = env.lookup(name) {
                        if !binding.mutable {
                            self.diags.error_note(
                                target.span,
                                format!("cannot reassign `{name}` because it was declared with `let`"),
                                format!("declare it with `let mut {name} = ...` to allow reassignment"),
                            );
                        }
                    }
                }

                let target_ty = self.check_expr(target, env);
                if target_ty.is_void() || target_ty.is_error() {
                    self.check_expr(value, env);
                    return;
                }
                self.check_expr_expected(value, &target_ty, env);
                let _ = span;
            }
            Stmt::Expr(expr) => {
                self.check_expr(expr, env);
            }
            Stmt::Return { value, span } => {
                if env.ret.is_void() {
                    if value.is_some() {
                        self.diags.error_note(
                            *span,
                            "this function does not return a value",
                            "remove the expression after `return`",
                        );
                        if let Some(v) = value {
                            self.check_expr(v, env);
                        }
                    }
                } else {
                    match value {
                        Some(v) => {
                            let ret = env.ret.clone();
                            self.check_expr_expected(v, &ret, env);
                        }
                        None => {
                            self.diags.error(
                                *span,
                                format!("expected a value of type `{}` after `return`", env.ret),
                            );
                        }
                    }
                }
            }
            Stmt::Break(span) => {
                if env.loop_depth == 0 {
                    self.diags.error_note(
                        *span,
                        "`break` can only be used inside a loop",
                        "this statement is not inside a `while` or `for` loop",
                    );
                }
            }
            Stmt::Continue(span) => {
                if env.loop_depth == 0 {
                    self.diags.error_note(
                        *span,
                        "`continue` can only be used inside a loop",
                        "this statement is not inside a `while` or `for` loop",
                    );
                }
            }
        }
    }

    // ------------------------------------------------------------ expressions

    fn check_expr(&mut self, expr: &mut Expr, env: &mut Env) -> Type {
        let ty = self.check_expr_inner(expr, env);
        expr.ty = Some(ty.clone());
        ty
    }

    /// Check an expression against a known type. This enables context-directed
    /// inference, so `let xs: [float] = [1, 2];` works without literals like
    /// `1.0`.
    fn check_expr_expected(&mut self, expr: &mut Expr, expected: &Type, env: &mut Env) -> Type {
        if expected.is_error() {
            return self.check_expr(expr, env);
        }

        match (&mut expr.kind, &expected.kind) {
            (ExprKind::Array(items), TypeKind::Array(elem)) => {
                for item in items {
                    self.check_expr_expected(item, elem, env);
                }
                expr.ty = Some(expected.clone());
                return expected.clone();
            }
            (
                ExprKind::If {
                    cond,
                    then_block,
                    else_branch: Some(else_branch),
                },
                _,
            ) if !expected.is_void() => {
                let cond_ty = self.check_expr(cond, env);
                self.expect_bool(&cond_ty, cond.span, "an `if` condition");
                self.check_block(then_block, Some(expected), env);
                self.check_expr_expected(else_branch, expected, env);
                expr.ty = Some(expected.clone());
                return expected.clone();
            }
            (ExprKind::Block(block), _) if !expected.is_void() => {
                let ty = self.check_block(block, Some(expected), env);
                expr.ty = Some(expected.clone());
                return ty;
            }
            _ => {}
        }

        let actual = self.check_expr(expr, env);
        if !actual.is_error() && !assignable(&actual, expected) {
            self.mismatch(expr.span, expected, &actual);
        }
        actual
    }

    fn check_expr_inner(&mut self, expr: &mut Expr, env: &mut Env) -> Type {
        let span = expr.span;
        match &mut expr.kind {
            ExprKind::Int(_) => Type::int(span),
            ExprKind::Float(_) => Type::float(span),
            ExprKind::Bool(_) => Type::bool(span),
            ExprKind::Str(parts) => {
                for part in parts {
                    if let StrPart::Expr(e) = part {
                        let ty = self.check_expr(e, env);
                        if ty.is_void() {
                            self.diags.error(
                                e.span,
                                "cannot interpolate an expression that produces no value",
                            );
                        } else if !ty.is_error() && !interpolatable(&ty) {
                            self.diags.error_note(
                                e.span,
                                format!("cannot put a value of type `{ty}` inside a string"),
                                "interpolation supports `int`, `float`, `bool` and `str`",
                            );
                        }
                    }
                }
                Type::str(span)
            }
            ExprKind::Ident(name) => match env.lookup(name) {
                Some(binding) => binding.ty.clone(),
                None => {
                    let suggestion = closest(name, env.visible_names());
                    if let Some(s) = suggestion {
                        self.diags.error_note(
                            span,
                            format!("cannot find a value named `{name}`"),
                            format!("did you mean `{s}`?"),
                        );
                    } else {
                        self.diags.error(
                            span,
                            format!("cannot find a value named `{name}` in this scope"),
                        );
                    }
                    Type::error(span)
                }
            },
            ExprKind::Array(items) => {
                if items.is_empty() {
                    self.diags.error_note(
                        span,
                        "cannot infer the type of an empty array",
                        "add a type annotation, for example `let xs: [int] = [];`",
                    );
                    return Type::error(span);
                }
                let first_ty = self.check_expr(&mut items[0], env);
                if first_ty.is_error() {
                    for item in &mut items[1..] {
                        self.check_expr(item, env);
                    }
                    return Type::error(span);
                }
                for item in &mut items[1..] {
                    self.check_expr_expected(item, &first_ty, env);
                }
                Type::new(TypeKind::Array(Box::new(first_ty)), span)
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let lt = self.check_expr(lhs, env);
                let rt = self.check_expr(rhs, env);
                if lt.is_error() || rt.is_error() {
                    return Type::error(span);
                }
                self.check_binary(*op, &lt, &rt, span)
            }
            ExprKind::Unary { op, expr: inner } => {
                let ty = self.check_expr(inner, env);
                if ty.is_error() {
                    return Type::error(span);
                }
                match op {
                    UnOp::Neg => {
                        if is_numeric(&ty) {
                            ty
                        } else {
                            self.diags.error_note(
                                span,
                                format!("cannot negate a value of type `{ty}`"),
                                "unary `-` only works on `int` and `float`",
                            );
                            Type::error(span)
                        }
                    }
                    UnOp::Not => {
                        if ty.kind == TypeKind::Bool {
                            Type::bool(span)
                        } else {
                            self.diags.error_note(
                                span,
                                format!("cannot apply `!` to a value of type `{ty}`"),
                                "`!` only works on `bool`",
                            );
                            Type::error(span)
                        }
                    }
                }
            }
            ExprKind::Call { name, args } => self.check_call(name, args, span, env),
            ExprKind::Field { object, name } => {
                let obj_ty = self.check_expr(object, env);
                if obj_ty.is_error() {
                    return Type::error(span);
                }
                match &obj_ty.kind {
                    TypeKind::Named(struct_name) => {
                        let fields = self
                            .ctx
                            .structs
                            .get(struct_name)
                            .map(|s| s.fields.clone())
                            .unwrap_or_default();
                        if let Some((_, ty)) = fields.iter().find(|(n, _)| n == name) {
                            ty.clone()
                        } else {
                            let names: Vec<String> =
                                fields.iter().map(|(n, _)| n.clone()).collect();
                            let suggestion = closest(name, names.clone());
                            let note = if let Some(s) = suggestion {
                                format!("did you mean `{s}`?")
                            } else if names.is_empty() {
                                format!("`{struct_name}` has no fields")
                            } else {
                                format!("available fields: {}", names.join(", "))
                            };
                            self.diags.error_note(
                                span,
                                format!("struct `{struct_name}` has no field `{name}`"),
                                note,
                            );
                            Type::error(span)
                        }
                    }
                    _ => {
                        self.diags.error_note(
                            span,
                            format!("type `{obj_ty}` has no fields"),
                            "only struct values support field access",
                        );
                        Type::error(span)
                    }
                }
            }
            ExprKind::Index { object, index } => {
                let obj_ty = self.check_expr(object, env);
                let idx_ty = self.check_expr(index, env);
                if obj_ty.is_error() {
                    return Type::error(span);
                }
                if !idx_ty.is_error() && idx_ty.kind != TypeKind::Int {
                    self.mismatch(index.span, &Type::int(index.span), &idx_ty);
                }
                match &obj_ty.kind {
                    TypeKind::Array(elem) => (**elem).clone(),
                    _ => {
                        self.diags.error_note(
                            span,
                            format!("cannot index a value of type `{obj_ty}`"),
                            "only arrays support `value[index]`; strings are not indexable in v0.1",
                        );
                        Type::error(span)
                    }
                }
            }
            ExprKind::StructLit { name, fields } => {
                self.check_struct_literal(name, fields, span, env)
            }
            ExprKind::If {
                cond,
                then_block,
                else_branch,
            } => {
                let cond_ty = self.check_expr(cond, env);
                self.expect_bool(&cond_ty, cond.span, "an `if` condition");
                let then_ty = self.check_block(then_block, None, env);
                match else_branch {
                    None => Type::void(span),
                    Some(else_expr) => {
                        let else_ty = self.check_expr(else_expr, env);
                        if then_ty.is_error() || else_ty.is_error() {
                            return Type::error(span);
                        }
                        match unify(&then_ty, &else_ty) {
                            Some(ty) => ty,
                            None => {
                                self.diags.error_note(
                                    span,
                                    format!(
                                        "`if` and `else` produce different types: `{then_ty}` and `{else_ty}`"
                                    ),
                                    "both branches of an `if` expression must have the same type",
                                );
                                Type::error(span)
                            }
                        }
                    }
                }
            }
            ExprKind::Block(block) => self.check_block(block, None, env),
            ExprKind::While { cond, body } => {
                let cond_ty = self.check_expr(cond, env);
                self.expect_bool(&cond_ty, cond.span, "a `while` condition");
                env.loop_depth += 1;
                self.check_block(body, None, env);
                env.loop_depth -= 1;
                Type::void(span)
            }
            ExprKind::For { var, iter, body } => {
                let elem_ty = match iter {
                    ForIter::Range { start, end } => {
                        let start_ty = self.check_expr(start, env);
                        let end_ty = self.check_expr(end, env);
                        if !start_ty.is_error() && start_ty.kind != TypeKind::Int {
                            self.mismatch(start.span, &Type::int(start.span), &start_ty);
                        }
                        if !end_ty.is_error() && end_ty.kind != TypeKind::Int {
                            self.mismatch(end.span, &Type::int(end.span), &end_ty);
                        }
                        Type::int(span)
                    }
                    ForIter::Array(array_expr) => {
                        let array_ty = self.check_expr(array_expr, env);
                        match &array_ty.kind {
                            TypeKind::Array(elem) => (**elem).clone(),
                            TypeKind::Error => Type::error(span),
                            _ => {
                                self.diags.error_note(
                                    array_expr.span,
                                    format!(
                                        "cannot loop over a value of type `{array_ty}`"
                                    ),
                                    "`for x in ...` accepts a range (`0..n`) or an array",
                                );
                                Type::error(span)
                            }
                        }
                    }
                };

                env.push_scope();
                env.bind(var.clone(), elem_ty, false);
                env.loop_depth += 1;
                self.check_block(body, None, env);
                env.loop_depth -= 1;
                env.pop_scope();
                Type::void(span)
            }
        }
    }

    fn check_binary(&mut self, op: BinOp, lt: &Type, rt: &Type, span: Span) -> Type {
        use BinOp::*;
        match op {
            Add => {
                if lt.kind == TypeKind::Str && rt.kind == TypeKind::Str {
                    return Type::str(span);
                }
                if is_numeric(lt) && is_numeric(rt) {
                    return if is_floaty(lt) || is_floaty(rt) {
                        Type::float(span)
                    } else {
                        Type::int(span)
                    };
                }
                let note = if lt.kind == TypeKind::Str || rt.kind == TypeKind::Str {
                    "convert the non-string side with `str(value)` to concatenate"
                } else {
                    "`+` works on numbers and on two `str` values"
                };
                self.bin_error(op, lt, rt, span, note);
                Type::error(span)
            }
            Sub | Mul | Div | Rem => {
                if is_numeric(lt) && is_numeric(rt) {
                    if is_floaty(lt) || is_floaty(rt) {
                        Type::float(span)
                    } else {
                        Type::int(span)
                    }
                } else {
                    self.bin_error(op, lt, rt, span, "this operator only works on `int` and `float`");
                    Type::error(span)
                }
            }
            Eq | Ne => {
                if is_numeric(lt) && is_numeric(rt) {
                    return Type::bool(span);
                }
                if lt == rt
                    && matches!(
                        lt.kind,
                        TypeKind::Str | TypeKind::Bool
                    )
                {
                    return Type::bool(span);
                }
                let note = match &lt.kind {
                    TypeKind::Named(_) => "compare individual fields instead of whole structs",
                    TypeKind::Array(_) => "compare lengths and elements instead of whole arrays",
                    _ => "both sides of a comparison must have the same type",
                };
                self.bin_error(op, lt, rt, span, note);
                Type::error(span)
            }
            Lt | Le | Gt | Ge => {
                if is_numeric(lt) && is_numeric(rt) {
                    return Type::bool(span);
                }
                if lt.kind == TypeKind::Str && rt.kind == TypeKind::Str {
                    return Type::bool(span);
                }
                self.bin_error(op, lt, rt, span, "ordering works on numbers and on two `str` values");
                Type::error(span)
            }
            And | Or => {
                if lt.kind == TypeKind::Bool && rt.kind == TypeKind::Bool {
                    Type::bool(span)
                } else {
                    self.bin_error(op, lt, rt, span, "`&&` and `||` only work on `bool` values");
                    Type::error(span)
                }
            }
        }
    }

    fn bin_error(&mut self, op: BinOp, lt: &Type, rt: &Type, span: Span, note: &str) {
        self.diags.error_note(
            span,
            format!("cannot apply `{}` to `{lt}` and `{rt}`", op.symbol()),
            note,
        );
    }

    fn check_call(
        &mut self,
        name: &str,
        args: &mut [Expr],
        span: Span,
        env: &mut Env,
    ) -> Type {
        if BUILTINS.contains(&name) {
            return self.check_builtin(name, args, span, env);
        }

        let Some(sig) = self.ctx.fns.get(name).cloned() else {
            let mut candidates: Vec<String> = self.ctx.fns.keys().cloned().collect();
            candidates.extend(BUILTINS.iter().map(|s| s.to_string()));
            let suggestion = closest(name, candidates);
            let note = suggestion
                .map(|s| format!("did you mean `{s}()`?"))
                .unwrap_or_else(|| "functions must be declared at the top level".to_string());
            for arg in args {
                self.check_expr(arg, env);
            }
            self.diags
                .error_note(span, format!("cannot find function `{name}`"), note);
            return Type::error(span);
        };

        if args.len() != sig.params.len() {
            self.diags.error_note(
                span,
                format!(
                    "`{name}` takes {} argument{} but {} {} supplied",
                    sig.params.len(),
                    if sig.params.len() == 1 { "" } else { "s" },
                    args.len(),
                    if args.len() == 1 { "was" } else { "were" }
                ),
                if sig.params.is_empty() {
                    format!("`{name}` takes no arguments")
                } else {
                    format!("expected: {}", describe_params(&sig.params))
                },
            );
            for arg in args {
                self.check_expr(arg, env);
            }
            return sig.ret;
        }

        for (arg, param) in args.iter_mut().zip(sig.params.iter()) {
            self.check_expr_expected(arg, param, env);
        }
        sig.ret
    }

    fn check_builtin(
        &mut self,
        name: &str,
        args: &mut [Expr],
        span: Span,
        env: &mut Env,
    ) -> Type {
        match name {
            "print" | "println" => {
                if args.len() > 1 {
                    for arg in args.iter_mut() {
                        self.check_expr(arg, env);
                    }
                    self.diags.error(
                        span,
                        format!("`{name}` takes at most one argument"),
                    );
                    return Type::void(span);
                }
                if let Some(arg) = args.first_mut() {
                    let ty = self.check_expr(arg, env);
                    if ty.is_void() {
                        self.diags.error(
                            arg.span,
                            format!("`{name}` cannot print a value that produces nothing"),
                        );
                    } else if !ty.is_error() && !printable(&ty) {
                        self.diags.error_note(
                            arg.span,
                            format!("cannot print a value of type `{ty}`"),
                            "print its fields or elements instead; supported: `int`, `float`, `bool`, `str`, and arrays of those",
                        );
                    }
                }
                Type::void(span)
            }
            "len" => {
                if self.wrong_arity(name, args, 1, span, env) {
                    return Type::int(span);
                }
                let ty = self.check_expr(&mut args[0], env);
                if !ty.is_error() && !matches!(ty.kind, TypeKind::Str | TypeKind::Array(_)) {
                    self.diags.error_note(
                        args[0].span,
                        format!("`len` does not accept `{ty}`"),
                        "`len` works on strings and arrays",
                    );
                }
                Type::int(span)
            }
            "push" => {
                if self.wrong_arity(name, args, 2, span, env) {
                    return Type::void(span);
                }
                let array_ty = self.check_expr(&mut args[0], env);
                match &array_ty.kind {
                    TypeKind::Array(elem) => {
                        let elem = (**elem).clone();
                        let value = &mut args[1];
                        self.check_expr_expected(value, &elem, env);
                    }
                    TypeKind::Error => {}
                    _ => {
                        self.diags.error_note(
                            args[0].span,
                            format!("`push` expects an array, found `{array_ty}`"),
                            "the first argument must be an array, e.g. `push(items, 3)`",
                        );
                        self.check_expr(&mut args[1], env);
                    }
                }
                Type::void(span)
            }
            "str" => {
                if self.wrong_arity(name, args, 1, span, env) {
                    return Type::str(span);
                }
                let ty = self.check_expr(&mut args[0], env);
                if !ty.is_error() && !interpolatable(&ty) {
                    self.diags.error_note(
                        args[0].span,
                        format!("cannot convert `{ty}` to `str`"),
                        "`str(...)` converts `int`, `float` and `bool`",
                    );
                }
                Type::str(span)
            }
            "int" => {
                if self.wrong_arity(name, args, 1, span, env) {
                    return Type::int(span);
                }
                let ty = self.check_expr(&mut args[0], env);
                if !ty.is_error()
                    && !matches!(
                        ty.kind,
                        TypeKind::Int | TypeKind::Float | TypeKind::Bool | TypeKind::Str
                    )
                {
                    self.diags.error(
                        args[0].span,
                        format!("cannot convert `{ty}` to `int`"),
                    );
                }
                Type::int(span)
            }
            "float" => {
                if self.wrong_arity(name, args, 1, span, env) {
                    return Type::float(span);
                }
                let ty = self.check_expr(&mut args[0], env);
                if !ty.is_error()
                    && !matches!(ty.kind, TypeKind::Int | TypeKind::Float | TypeKind::Str)
                {
                    self.diags.error(
                        args[0].span,
                        format!("cannot convert `{ty}` to `float`"),
                    );
                }
                Type::float(span)
            }
            "abs" => {
                if self.wrong_arity(name, args, 1, span, env) {
                    return Type::error(span);
                }
                let ty = self.check_expr(&mut args[0], env);
                if ty.is_error() {
                    return Type::error(span);
                }
                if !is_numeric(&ty) {
                    self.diags.error_note(
                        args[0].span,
                        format!("`abs` expects a number, found `{ty}`"),
                        "`abs` works on `int` and `float`",
                    );
                    return Type::error(span);
                }
                ty
            }
            "min" | "max" => {
                if self.wrong_arity(name, args, 2, span, env) {
                    return Type::error(span);
                }
                let a = self.check_expr(&mut args[0], env);
                let b = self.check_expr(&mut args[1], env);
                if a.is_error() || b.is_error() {
                    return Type::error(span);
                }
                if !is_numeric(&a) || !is_numeric(&b) {
                    self.diags.error(
                        span,
                        format!("`{name}` expects two numbers, found `{a}` and `{b}`"),
                    );
                    return Type::error(span);
                }
                if is_floaty(&a) || is_floaty(&b) {
                    Type::float(span)
                } else {
                    Type::int(span)
                }
            }
            "assert" => {
                if self.wrong_arity(name, args, 1, span, env) {
                    return Type::void(span);
                }
                let ty = self.check_expr(&mut args[0], env);
                if !ty.is_error() && ty.kind != TypeKind::Bool {
                    self.diags.error_note(
                        args[0].span,
                        format!("`assert` expects a `bool`, found `{ty}`"),
                        "write a comparison, e.g. `assert(x > 0)`",
                    );
                }
                Type::void(span)
            }
            _ => unreachable!("unknown builtin"),
        }
    }

    fn wrong_arity(
        &mut self,
        name: &str,
        args: &mut [Expr],
        n: usize,
        span: Span,
        env: &mut Env,
    ) -> bool {
        if args.len() == n {
            return false;
        }
        for arg in args.iter_mut() {
            self.check_expr(arg, env);
        }
        self.diags.error(
            span,
            format!(
                "`{name}` takes {n} argument{} but {} {} supplied",
                if n == 1 { "" } else { "s" },
                args.len(),
                if args.len() == 1 { "was" } else { "were" }
            ),
        );
        true
    }

    fn check_struct_literal(
        &mut self,
        name: &str,
        fields: &mut [(String, Expr)],
        span: Span,
        env: &mut Env,
    ) -> Type {
        let Some(info) = self.ctx.structs.get(name).cloned() else {
            for (_, value) in fields.iter_mut() {
                self.check_expr(value, env);
            }
            let suggestion = closest(name, self.ctx.structs.keys().cloned());
            let note = suggestion
                .map(|s| format!("did you mean `{s}`?"))
                .unwrap_or_else(|| "structs must be declared at the top level".to_string());
            self.diags.error_note(
                span,
                format!("cannot find struct `{name}`"),
                note,
            );
            return Type::error(span);
        };

        let mut seen = HashSet::new();
        for (field_name, value) in fields.iter_mut() {
            if !seen.insert(field_name.clone()) {
                self.diags.error(
                    value.span,
                    format!("field `{field_name}` is initialized more than once"),
                );
            }
            if let Some((_, field_ty)) = info.fields.iter().find(|(n, _)| n == field_name) {
                let field_ty = field_ty.clone();
                self.check_expr_expected(value, &field_ty, env);
            } else {
                let names: Vec<String> = info.fields.iter().map(|(n, _)| n.clone()).collect();
                let suggestion = closest(field_name, names.clone());
                let note = suggestion
                    .map(|s| format!("did you mean `{s}`?"))
                    .unwrap_or_else(|| {
                        if names.is_empty() {
                            format!("`{name}` has no fields")
                        } else {
                            format!("available fields: {}", names.join(", "))
                        }
                    });
                self.diags.error_note(
                    value.span,
                    format!("struct `{name}` has no field `{field_name}`"),
                    note,
                );
                self.check_expr(value, env);
            }
        }

        let missing: Vec<String> = info
            .fields
            .iter()
            .filter(|(n, _)| !seen.contains(n))
            .map(|(n, _)| n.clone())
            .collect();
        if !missing.is_empty() {
            self.diags.error_note(
                span,
                format!(
                    "missing field{} {} in `{name}` initializer",
                    if missing.len() == 1 { "" } else { "s" },
                    missing
                        .iter()
                        .map(|m| format!("`{m}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                "every field must be initialized",
            );
        }

        Type::new(TypeKind::Named(name.to_string()), span)
    }

    fn expect_bool(&mut self, ty: &Type, span: Span, what: &str) {
        if ty.is_error() || ty.kind == TypeKind::Bool {
            return;
        }
        self.diags.error_note(
            span,
            format!("{what} must be `bool`, found `{ty}`"),
            "Quill does not treat integers as truthy; write `x != 0`",
        );
    }

    fn mismatch(&mut self, span: Span, expected: &Type, actual: &Type) {
        self.diags.error_note(
            span,
            format!("expected `{expected}`, found `{actual}`"),
            mismatch_hint(expected, actual),
        );
    }
}

fn mismatch_hint(expected: &Type, actual: &Type) -> &'static str {
    match (&expected.kind, &actual.kind) {
        (TypeKind::Str, TypeKind::Int | TypeKind::Float | TypeKind::Bool) => {
            "convert the value with `str(value)`"
        }
        (TypeKind::Int, TypeKind::Str) => "parse the string with `int(value)`",
        (TypeKind::Float, TypeKind::Int) => "`int` widens to `float` automatically in this position",
        (TypeKind::Float, TypeKind::Str) => "parse the string with `float(value)`",
        (TypeKind::Array(_), _) => "use an array literal, e.g. `[1, 2, 3]`",
        _ => "these types are not interchangeable",
    }
}

fn assignable(from: &Type, to: &Type) -> bool {
    if from.is_error() || to.is_error() {
        return true;
    }
    if from == to {
        return true;
    }
    matches!(
        (&from.kind, &to.kind),
        (TypeKind::Int, TypeKind::Float)
    )
}

fn unify(a: &Type, b: &Type) -> Option<Type> {
    if a.is_error() {
        return Some(b.clone());
    }
    if b.is_error() {
        return Some(a.clone());
    }
    if a == b {
        return Some(a.clone());
    }
    if is_numeric(a) && is_numeric(b) {
        if is_floaty(a) || is_floaty(b) {
            return Some(Type::float(a.span.join(b.span)));
        }
        return Some(Type::int(a.span.join(b.span)));
    }
    None
}

fn is_numeric(ty: &Type) -> bool {
    matches!(ty.kind, TypeKind::Int | TypeKind::Float)
}

fn is_floaty(ty: &Type) -> bool {
    ty.kind == TypeKind::Float
}

fn printable(ty: &Type) -> bool {
    match &ty.kind {
        TypeKind::Int | TypeKind::Float | TypeKind::Bool | TypeKind::Str => true,
        TypeKind::Array(inner) => matches!(
            inner.kind,
            TypeKind::Int | TypeKind::Float | TypeKind::Bool | TypeKind::Str
        ),
        _ => false,
    }
}

fn interpolatable(ty: &Type) -> bool {
    matches!(
        ty.kind,
        TypeKind::Int | TypeKind::Float | TypeKind::Bool | TypeKind::Str
    )
}

fn is_primitive_name(name: &str) -> bool {
    matches!(name, "int" | "float" | "bool" | "str" | "void")
}

fn describe_params(params: &[Type]) -> String {
    params
        .iter()
        .map(|p| format!("`{p}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn is_lvalue(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::Ident(_) | ExprKind::Field { .. } | ExprKind::Index { .. }
    )
}

fn block_diverges(block: &Block) -> bool {
    if let Some(value) = &block.value {
        return expr_diverges(value);
    }
    match block.stmts.last() {
        Some(Stmt::Return { .. }) => true,
        Some(Stmt::Expr(expr)) => expr_diverges(expr),
        _ => false,
    }
}

fn expr_diverges(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::If {
            then_block,
            else_branch: Some(else_branch),
            ..
        } => block_diverges(then_block) && expr_diverges(else_branch),
        ExprKind::Block(block) => block_diverges(block),
        ExprKind::While { cond, body } => {
            matches!(cond.kind, ExprKind::Bool(true)) && !block_breaks(body)
        }
        _ => false,
    }
}

/// Does this block contain a `break` belonging to the enclosing loop?
fn block_breaks(block: &Block) -> bool {
    block.stmts.iter().any(|s| match s {
        Stmt::Break(_) => true,
        Stmt::Expr(e) => expr_breaks(e),
        _ => false,
    }) || block.value.as_ref().map(|v| expr_breaks(v)).unwrap_or(false)
}

fn expr_breaks(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::If {
            then_block,
            else_branch,
            ..
        } => {
            block_breaks(then_block)
                || else_branch.as_ref().map(|e| expr_breaks(e)).unwrap_or(false)
        }
        ExprKind::Block(b) => block_breaks(b),
        // A break inside a nested loop belongs to that loop.
        ExprKind::While { .. } | ExprKind::For { .. } => false,
        _ => false,
    }
}

fn closest(name: &str, candidates: impl IntoIterator<Item = String>) -> Option<String> {
    candidates
        .into_iter()
        .map(|c| (edit_distance(name, &c), c))
        .filter(|(d, _)| *d <= 2)
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| c)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1)
                .min(cur[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    fn check_ok(src: &str) -> Program {
        let (mut program, parse_diags) = parser::parse(src);
        assert!(
            !parse_diags.has_errors(),
            "parse errors: {:?}",
            parse_diags.items
        );
        let mut program = program.take().unwrap();
        let diags = check(&mut program);
        assert!(
            !diags.has_errors(),
            "check errors: {:?}",
            diags.items.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
        program
    }

    fn check_err(src: &str) -> Vec<String> {
        let (mut program, parse_diags) = parser::parse(src);
        assert!(
            !parse_diags.has_errors(),
            "parse errors: {:?}",
            parse_diags.items
        );
        let mut program = program.take().unwrap();
        let diags = check(&mut program);
        assert!(diags.has_errors(), "expected a type error");
        diags.items.into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn checks_a_simple_program() {
        check_ok(
            "struct Point { x: int, y: int }
             fn sum(p: Point) -> int { p.x + p.y }
             fn main() { let p = Point { x: 1, y: 2 }; let s = sum(p); println(s); }",
        );
    }

    #[test]
    fn unknown_name_suggests_a_fix() {
        let errors = check_err("fn main() { let value = 1; println(valu); }");
        assert!(errors[0].contains("cannot find a value named `valu`"));
    }

    #[test]
    fn immutable_assignment_is_rejected() {
        let errors = check_err("fn main() { let x = 1; x = 2; }");
        assert!(errors.iter().any(|e| e.contains("cannot reassign `x`")));
    }

    #[test]
    fn mutable_assignment_is_allowed() {
        check_ok("fn main() { let mut x = 1; x = 2; x = x + 1; }");
    }

    #[test]
    fn field_mutation_through_let_is_allowed() {
        check_ok(
            "struct P { x: int }
             fn main() { let p = P { x: 1 }; p.x = 2; }",
        );
    }

    #[test]
    fn type_mismatch() {
        let errors = check_err("fn main() { let x: int = \"hi\"; }");
        assert!(errors.iter().any(|e| e.contains("expected `int`, found `str`")));
    }

    #[test]
    fn int_widens_to_float() {
        check_ok("fn main() { let x: float = 1; let y = x + 2; }");
    }

    #[test]
    fn float_does_not_narrow_to_int() {
        let errors = check_err("fn main() { let x: int = 1.5; }");
        assert!(errors.iter().any(|e| e.contains("expected `int`, found `float`")));
    }

    #[test]
    fn if_branches_must_agree() {
        let errors = check_err("fn main() { let x = if true { 1 } else { \"no\" }; }");
        assert!(errors
            .iter()
            .any(|e| e.contains("`if` and `else` produce different types")));
    }

    #[test]
    fn if_expression_widens() {
        check_ok("fn main() { let x: float = if true { 1 } else { 2.5 }; }");
    }

    #[test]
    fn unknown_field_lists_options() {
        let errors = check_err(
            "struct Point { x: int, y: int }
             fn main() { let p = Point { x: 1, y: 2 }; println(p.z); }",
        );
        assert!(errors.iter().any(|e| e.contains("has no field `z`")));
    }

    #[test]
    fn missing_struct_field_is_reported() {
        let errors = check_err(
            "struct Point { x: int, y: int }
             fn main() { let p = Point { x: 1 }; }",
        );
        assert!(errors.iter().any(|e| e.contains("missing field `y`")));
    }

    #[test]
    fn wrong_argument_count() {
        let errors = check_err(
            "fn add(a: int, b: int) -> int { a + b }
             fn main() { let x = add(1); }",
        );
        assert!(errors.iter().any(|e| e.contains("takes 2 arguments")));
    }

    #[test]
    fn void_result_cannot_be_bound() {
        let errors = check_err("fn main() { let x = println(\"hi\"); }");
        assert!(errors.iter().any(|e| e.contains("produces no value")));
    }

    #[test]
    fn missing_return_path_is_reported() {
        let errors = check_err(
            "fn f(x: int) -> int { if x > 0 { return 1; } }
             fn main() { println(f(1)); }",
        );
        assert!(errors
            .iter()
            .any(|e| e.contains("not all paths") || e.contains("expected `int`")));
    }

    #[test]
    fn diverging_if_is_accepted() {
        check_ok(
            "fn f(x: int) -> int { if x > 0 { return 1; } else { return 2; } }
             fn main() { println(f(1)); }",
        );
    }

    #[test]
    fn empty_array_needs_annotation() {
        let errors = check_err("fn main() { let xs = []; }");
        assert!(errors.iter().any(|e| e.contains("empty array")));
        check_ok("fn main() { let xs: [int] = []; }");
    }

    #[test]
    fn for_over_non_array_is_rejected() {
        let errors = check_err("fn main() { for x in 5 { println(x); } }");
        assert!(errors.iter().any(|e| e.contains("cannot loop over")));
    }

    #[test]
    fn break_outside_loop_is_rejected() {
        let errors = check_err("fn main() { break; }");
        assert!(errors.iter().any(|e| e.contains("only be used inside a loop")));
    }

    #[test]
    fn duplicate_definitions_are_rejected() {
        let errors = check_err("fn main() { } fn main() { }");
        assert!(errors.iter().any(|e| e.contains("duplicate function")));
    }

    #[test]
    fn recursive_struct_by_value_is_rejected() {
        let errors = check_err("struct Node { next: Node } fn main() { }");
        assert!(errors.iter().any(|e| e.contains("contains itself by value")));
    }

    #[test]
    fn main_is_required() {
        let errors = check_err("fn helper() { }");
        assert!(errors.iter().any(|e| e.contains("missing entry point")));
    }

    #[test]
    fn string_plus_int_suggests_conversion() {
        let errors = check_err("fn main() { let s = \"n=\" + 1; }");
        assert!(errors.iter().any(|e| e.contains("cannot apply `+`")));
    }

    #[test]
    fn interpolation_rejects_structs() {
        let errors = check_err(
            "struct P { x: int }
             fn main() { let p = P { x: 1 }; println(\"p = {p}\"); }",
        );
        assert!(errors.iter().any(|e| e.contains("inside a string")));
    }

    #[test]
    fn annotation_drives_array_inference() {
        check_ok("fn main() { let xs: [float] = [1, 2, 3]; let y = xs[0] + 0.5; }");
    }

    #[test]
    fn int_literal_types_are_annotated() {
        let program = check_ok("fn main() { let x = 1 + 2; }");
        let Stmt::Let { value, .. } = &program.fns[0].body.stmts[0] else {
            panic!("expected let");
        };
        assert_eq!(value.ty.as_ref().unwrap().kind, TypeKind::Int);
    }
}
