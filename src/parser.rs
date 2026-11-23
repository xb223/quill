//! Recursive-descent parser for Quill.
//!
//! The grammar is expression-oriented: `if`, blocks and plain `{ ... }` all
//! produce values. Statements are `let`, assignment, `return`, `break`,
//! `continue`, and expression statements.

use crate::ast::*;
use crate::diag::{Diags, Span};
use crate::lexer::{self, StrPart as LexStrPart, Token, TokenKind};

pub fn parse(src: &str) -> (Option<Program>, Diags) {
    let (tokens, diags) = lexer::lex(src);
    let mut parser = Parser {
        tokens,
        pos: 0,
        diags,
        depth: 0,
    };
    let program = parser.parse_program();
    (program, parser.diags)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    diags: Diags,
    depth: usize,
}

impl Parser {
    // ---------------------------------------------------------------- helpers

    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn kind_at(&self, offset: usize) -> &TokenKind {
        let i = (self.pos + offset).min(self.tokens.len() - 1);
        &self.tokens[i].kind
    }

    fn at(&self, kind: &TokenKind) -> bool {
        self.peek().kind == *kind
    }

    fn at_eof(&self) -> bool {
        self.at(&TokenKind::Eof)
    }

    fn bump(&mut self) -> Token {
        let token = self.peek().clone();
        if !matches!(token.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        token
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(&kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind, what: &str) -> Option<Span> {
        if self.at(&kind) {
            Some(self.bump().span)
        } else {
            self.err_here(format!("expected {what}, found {}", self.peek().kind.describe()));
            None
        }
    }

    fn expect_ident(&mut self, what: &str) -> Option<(String, Span)> {
        if let TokenKind::Ident(name) = &self.peek().kind {
            let name = name.clone();
            let span = self.bump().span;
            Some((name, span))
        } else {
            self.err_here(format!("expected {what}, found {}", self.peek().kind.describe()));
            None
        }
    }

    fn err(&mut self, span: Span, message: impl Into<String>) {
        self.diags.error(span, message);
    }

    fn err_here(&mut self, message: impl Into<String>) {
        let span = self.peek().span;
        self.err(span, message);
    }

    fn err_note_here(&mut self, message: impl Into<String>, note: impl Into<String>) {
        let span = self.peek().span;
        self.diags.error_note(span, message, note);
    }

    // --------------------------------------------------------------- program

    fn parse_program(&mut self) -> Option<Program> {
        let mut structs = Vec::new();
        let mut fns = Vec::new();

        while !self.at_eof() {
            if self.at(&TokenKind::Struct) {
                structs.push(self.parse_struct()?);
            } else if self.at(&TokenKind::Fn) {
                fns.push(self.parse_fn()?);
            } else {
                self.err_here(format!(
                    "expected `fn` or `struct` at the top level, found {}",
                    self.peek().kind.describe()
                ));
                self.bump();
            }
        }

        Some(Program { structs, fns })
    }

    fn parse_struct(&mut self) -> Option<StructDef> {
        let start = self.expect(TokenKind::Struct, "`struct`")?;
        let (name, _) = self.expect_ident("a struct name")?;
        self.expect(TokenKind::LBrace, "`{`")?;

        let mut fields = Vec::new();
        while !self.at(&TokenKind::RBrace) {
            if self.at_eof() {
                self.err(start, "unclosed `struct`: missing `}`");
                return None;
            }
            let (field_name, field_span) = self.expect_ident("a field name")?;
            self.expect(TokenKind::Colon, "`:` after field name")?;
            let ty = self.parse_type()?;
            let span = field_span.join(ty.span);
            fields.push(Field {
                name: field_name,
                ty,
                span,
            });
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        let close = self.expect(TokenKind::RBrace, "`}` to close the struct")?;
        Some(StructDef {
            name,
            fields,
            span: start.join(close),
        })
    }

    fn parse_fn(&mut self) -> Option<FnDef> {
        let start = self.expect(TokenKind::Fn, "`fn`")?;
        let (name, name_span) = self.expect_ident("a function name")?;
        self.expect(TokenKind::LParen, "`(` after function name")?;

        let mut params = Vec::new();
        while !self.at(&TokenKind::RParen) {
            if self.at_eof() {
                self.err(start, "unclosed parameter list: missing `)`");
                return None;
            }
            let (param_name, param_span) = self.expect_ident("a parameter name")?;
            self.expect(TokenKind::Colon, "`:` after parameter name")?;
            let ty = self.parse_type()?;
            let span = param_span.join(ty.span);
            params.push(Param {
                name: param_name,
                ty,
                span,
            });
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RParen, "`)` to close the parameter list")?;

        let ret = if self.eat(TokenKind::Arrow) {
            self.parse_type()?
        } else {
            Type::void(name_span)
        };

        let body = self.parse_block()?;
        Some(FnDef {
            name,
            params,
            ret,
            span: start.join(body.span),
            body,
        })
    }

    // ------------------------------------------------------------------ types

    fn parse_type(&mut self) -> Option<Type> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::LBracket => {
                self.bump();
                let inner = self.parse_type()?;
                let close = self.expect(TokenKind::RBracket, "`]` to close the array type")?;
                Some(Type::new(
                    TypeKind::Array(Box::new(inner)),
                    token.span.join(close),
                ))
            }
            TokenKind::Ident(name) => {
                self.bump();
                let kind = match name.as_str() {
                    "int" => TypeKind::Int,
                    "float" => TypeKind::Float,
                    "bool" => TypeKind::Bool,
                    "str" => TypeKind::Str,
                    "void" => TypeKind::Void,
                    _ => TypeKind::Named(name),
                };
                Some(Type::new(kind, token.span))
            }
            _ => {
                self.err_here(format!(
                    "expected a type (such as `int`, `float`, `bool`, `str`, `[T]` or a struct name), found {}",
                    token.kind.describe()
                ));
                None
            }
        }
    }

    // ----------------------------------------------------------------- blocks

    fn parse_block(&mut self) -> Option<Block> {
        let open = self.expect(TokenKind::LBrace, "`{`")?;
        let mut stmts = Vec::new();
        let mut value = None;

        loop {
            if self.at(&TokenKind::RBrace) {
                let close = self.bump().span;
                return Some(Block {
                    stmts,
                    value,
                    span: open.join(close),
                });
            }
            if self.at_eof() {
                self.err(open, "unclosed block: missing `}`");
                return None;
            }

            match self.peek().kind.clone() {
                TokenKind::Let => {
                    stmts.push(self.parse_let()?);
                }
                TokenKind::Return => {
                    stmts.push(self.parse_return()?);
                }
                TokenKind::Break | TokenKind::Continue => {
                    let is_break = self.at(&TokenKind::Break);
                    let keyword = self.bump();
                    self.statement_terminator();
                    stmts.push(if is_break {
                        Stmt::Break(keyword.span)
                    } else {
                        Stmt::Continue(keyword.span)
                    });
                }
                _ => {
                    let expr = self.parse_expr()?;

                    let assignment = match &self.peek().kind {
                        TokenKind::Assign => Some(None),
                        TokenKind::PlusEq => Some(Some(BinOp::Add)),
                        TokenKind::MinusEq => Some(Some(BinOp::Sub)),
                        TokenKind::StarEq => Some(Some(BinOp::Mul)),
                        TokenKind::SlashEq => Some(Some(BinOp::Div)),
                        TokenKind::PercentEq => Some(Some(BinOp::Rem)),
                        _ => None,
                    };
                    if let Some(op) = assignment {
                        self.bump();
                        let value = self.parse_expr()?;
                        let span = expr.span.join(value.span);
                        self.expect(TokenKind::Semi, "`;` after assignment")?;
                        stmts.push(Stmt::Assign {
                            target: expr,
                            value,
                            op,
                            span,
                        });
                        continue;
                    }

                    if self.eat(TokenKind::Semi) {
                        stmts.push(Stmt::Expr(expr));
                        continue;
                    }
                    if self.at(&TokenKind::RBrace) {
                        value = Some(Box::new(expr));
                        continue;
                    }
                    if is_block_like(&expr) {
                        stmts.push(Stmt::Expr(expr));
                        continue;
                    }

                    self.err_note_here(
                        "expected `;` after this expression",
                        "Quill uses `;` between statements; a block's final expression is its value",
                    );
                    return None;
                }
            }
        }
    }

    fn statement_terminator(&mut self) {
        if self.eat(TokenKind::Semi) {
            return;
        }
        if self.at(&TokenKind::RBrace) {
            return;
        }
        self.err_note_here(
            "expected `;` after this statement",
            "the last statement in a block may omit the semicolon",
        );
    }

    fn parse_let(&mut self) -> Option<Stmt> {
        let start = self.expect(TokenKind::Let, "`let`")?;
        let mutable = self.eat(TokenKind::Mut);
        let (name, _) = self.expect_ident("a variable name")?;

        let ty = if self.eat(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        if self.eat(TokenKind::Assign) {
            let value = self.parse_expr()?;
            let semi = self.expect(TokenKind::Semi, "`;` after `let`")?;
            Some(Stmt::Let {
                name,
                mutable,
                ty,
                value,
                span: start.join(semi),
            })
        } else if self.at(&TokenKind::Semi) {
            self.err_note_here(
                format!("`{name}` needs an initial value"),
                format!("write `let {name} = ...;` (Quill never leaves a variable uninitialized)"),
            );
            None
        } else {
            self.err_here(format!(
                "expected `=` or `:` after `{name}`, found {}",
                self.peek().kind.describe()
            ));
            None
        }
    }

    fn parse_return(&mut self) -> Option<Stmt> {
        let start = self.expect(TokenKind::Return, "`return`")?;
        let value = if self.at(&TokenKind::Semi) || self.at(&TokenKind::RBrace) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.statement_terminator();
        Some(Stmt::Return {
            value,
            span: start,
        })
    }

    // ------------------------------------------------------------ expressions

    fn parse_expr(&mut self) -> Option<Expr> {
        self.depth += 1;
        if self.depth > 256 {
            self.err_here("expression is too deeply nested");
            self.depth -= 1;
            return None;
        }
        let result = self.parse_binary(0);
        self.depth -= 1;
        result
    }

    fn parse_binary(&mut self, min_bp: u8) -> Option<Expr> {
        let mut lhs = self.parse_unary()?;

        loop {
            let Some((op, bp)) = infix_op(&self.peek().kind) else {
                break;
            };
            if bp < min_bp {
                break;
            }
            self.bump();
            let rhs = self.parse_binary(bp + 1)?;
            let span = lhs.span.join(rhs.span);
            lhs = Expr::new(
                ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }

        Some(lhs)
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        let token = self.peek().clone();
        let op = match token.kind {
            TokenKind::Minus => Some(UnOp::Neg),
            TokenKind::Bang => Some(UnOp::Not),
            _ => None,
        };
        if let Some(op) = op {
            self.bump();
            let expr = self.parse_unary()?;
            let span = token.span.join(expr.span);
            return Some(Expr::new(
                ExprKind::Unary {
                    op,
                    expr: Box::new(expr),
                },
                span,
            ));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Option<Expr> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.eat(TokenKind::LParen) {
                let ExprKind::Ident(name) = &expr.kind else {
                    self.err_note_here(
                        "only named functions can be called",
                        "methods are planned; use a free function for now, e.g. `len(items)`",
                    );
                    return None;
                };
                let name = name.clone();
                let mut args = Vec::new();
                while !self.at(&TokenKind::RParen) {
                    args.push(self.parse_expr()?);
                    if !self.eat(TokenKind::Comma) {
                        break;
                    }
                }
                let close = self.expect(TokenKind::RParen, "`)` to close the call")?;
                let span = expr.span.join(close);
                expr = Expr::new(ExprKind::Call { name, args }, span);
            } else if self.eat(TokenKind::Dot) {
                let (name, name_span) = self.expect_ident("a field name after `.`")?;
                if self.at(&TokenKind::LParen) {
                    self.err_note_here(
                        format!("method calls like `.{name}(...)` are not available yet"),
                        "call a free function instead, e.g. `len(value)`",
                    );
                    return None;
                }
                let span = expr.span.join(name_span);
                expr = Expr::new(
                    ExprKind::Field {
                        object: Box::new(expr),
                        name,
                    },
                    span,
                );
            } else if self.eat(TokenKind::LBracket) {
                let index = self.parse_expr()?;
                let close = self.expect(TokenKind::RBracket, "`]` to close the index")?;
                let span = expr.span.join(close);
                expr = Expr::new(
                    ExprKind::Index {
                        object: Box::new(expr),
                        index: Box::new(index),
                    },
                    span,
                );
            } else {
                break;
            }
        }

        Some(expr)
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Int(value) => {
                self.bump();
                Some(Expr::new(ExprKind::Int(value), token.span))
            }
            TokenKind::Float(value) => {
                self.bump();
                Some(Expr::new(ExprKind::Float(value), token.span))
            }
            TokenKind::True => {
                self.bump();
                Some(Expr::new(ExprKind::Bool(true), token.span))
            }
            TokenKind::False => {
                self.bump();
                Some(Expr::new(ExprKind::Bool(false), token.span))
            }
            TokenKind::Str(parts) => {
                self.bump();
                self.parse_string(parts, token.span)
            }
            TokenKind::Ident(name) => {
                if self.looks_like_struct_literal() {
                    self.parse_struct_literal()
                } else {
                    self.bump();
                    Some(Expr::new(ExprKind::Ident(name), token.span))
                }
            }
            TokenKind::LParen => {
                self.bump();
                let mut expr = self.parse_expr()?;
                let close = self.expect(TokenKind::RParen, "`)`")?;
                expr.span = token.span.join(close);
                Some(expr)
            }
            TokenKind::LBracket => self.parse_array_literal(),
            TokenKind::LBrace => {
                let block = self.parse_block()?;
                let span = block.span;
                Some(Expr::new(ExprKind::Block(block), span))
            }
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::For => self.parse_for(),
            TokenKind::Eof => {
                self.err_here("expected an expression, found end of file");
                None
            }
            _ => {
                self.err_here(format!(
                    "expected an expression, found {}",
                    token.kind.describe()
                ));
                None
            }
        }
    }

    fn parse_if(&mut self) -> Option<Expr> {
        let start = self.expect(TokenKind::If, "`if`")?;
        let cond = self.parse_expr()?;
        let then_block = self.parse_block()?;

        let else_branch = if self.eat(TokenKind::Else) {
            if self.at(&TokenKind::If) {
                Some(Box::new(self.parse_if()?))
            } else {
                let block = self.parse_block()?;
                let span = block.span;
                Some(Box::new(Expr::new(ExprKind::Block(block), span)))
            }
        } else {
            None
        };

        let end = else_branch
            .as_ref()
            .map(|e| e.span)
            .unwrap_or(then_block.span);
        Some(Expr::new(
            ExprKind::If {
                cond: Box::new(cond),
                then_block,
                else_branch,
            },
            start.join(end),
        ))
    }

    fn parse_while(&mut self) -> Option<Expr> {
        let start = self.expect(TokenKind::While, "`while`")?;
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;
        let span = start.join(body.span);
        Some(Expr::new(
            ExprKind::While {
                cond: Box::new(cond),
                body,
            },
            span,
        ))
    }

    fn parse_for(&mut self) -> Option<Expr> {
        let start = self.expect(TokenKind::For, "`for`")?;
        let (var, _) = self.expect_ident("a loop variable name")?;
        self.expect(TokenKind::In, "`in` after the loop variable")?;
        let first = self.parse_expr()?;

        let iter = if self.eat(TokenKind::DotDot) {
            let end = self.parse_expr()?;
            ForIter::Range {
                start: Box::new(first),
                end: Box::new(end),
            }
        } else {
            ForIter::Array(Box::new(first))
        };

        let body = self.parse_block()?;
        let span = start.join(body.span);
        Some(Expr::new(ExprKind::For { var, iter, body }, span))
    }

    fn parse_array_literal(&mut self) -> Option<Expr> {
        let open = self.expect(TokenKind::LBracket, "`[`")?;
        let mut items = Vec::new();
        while !self.at(&TokenKind::RBracket) {
            if self.at_eof() {
                self.err(open, "unclosed array literal: missing `]`");
                return None;
            }
            items.push(self.parse_expr()?);
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        let close = self.expect(TokenKind::RBracket, "`]` to close the array")?;
        Some(Expr::new(ExprKind::Array(items), open.join(close)))
    }

    fn looks_like_struct_literal(&self) -> bool {
        matches!(self.kind_at(1), TokenKind::LBrace)
            && matches!(self.kind_at(2), TokenKind::Ident(_))
            && matches!(self.kind_at(3), TokenKind::Colon)
    }

    fn parse_struct_literal(&mut self) -> Option<Expr> {
        let (name, name_span) = self.expect_ident("a struct name")?;
        self.expect(TokenKind::LBrace, "`{`")?;

        let mut fields = Vec::new();
        while !self.at(&TokenKind::RBrace) {
            let (field_name, _) = self.expect_ident("a field name")?;
            self.expect(TokenKind::Colon, "`:` after field name")?;
            let value = self.parse_expr()?;
            fields.push((field_name, value));
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }

        let close = self.expect(TokenKind::RBrace, "`}` to close the struct literal")?;
        Some(Expr::new(
            ExprKind::StructLit { name, fields },
            name_span.join(close),
        ))
    }

    fn parse_string(&mut self, parts: Vec<LexStrPart>, span: Span) -> Option<Expr> {
        let mut out = Vec::new();
        for part in parts {
            match part {
                LexStrPart::Text(text) => out.push(StrPart::Text(text)),
                LexStrPart::Expr { src, offset } => {
                    let expr = self.parse_interpolation(&src, offset, span)?;
                    out.push(StrPart::Expr(expr));
                }
            }
        }
        Some(Expr::new(ExprKind::Str(out), span))
    }

    fn parse_interpolation(&mut self, src: &str, offset: usize, outer: Span) -> Option<Expr> {
        let (mut tokens, lex_diags) = lexer::lex(src);
        for token in &mut tokens {
            token.span.start += offset;
            token.span.end += offset;
        }
        self.merge_diags(shift_diags(lex_diags, offset));

        let mut sub = Parser {
            tokens,
            pos: 0,
            diags: Diags::new(),
            depth: self.depth,
        };
        let expr = sub.parse_expr();
        if expr.is_some() && !sub.at_eof() {
            sub.err_here("expected the end of this interpolated expression");
        }
        let failed = sub.diags.has_errors() || expr.is_none();
        self.merge_diags(sub.diags);

        if failed {
            self.err(outer, "invalid expression inside string interpolation");
            return None;
        }
        expr
    }

    fn merge_diags(&mut self, other: Diags) {
        self.diags.error_count += other.error_count;
        self.diags.items.extend(other.items);
    }
}

fn shift_diags(mut diags: Diags, offset: usize) -> Diags {
    for item in &mut diags.items {
        if let Some(span) = &mut item.span {
            span.start += offset;
            span.end += offset;
        }
    }
    diags
}

fn infix_op(kind: &TokenKind) -> Option<(BinOp, u8)> {
    Some(match kind {
        TokenKind::OrOr => (BinOp::Or, 1),
        TokenKind::AndAnd => (BinOp::And, 2),
        TokenKind::EqEq => (BinOp::Eq, 3),
        TokenKind::NotEq => (BinOp::Ne, 3),
        TokenKind::Lt => (BinOp::Lt, 4),
        TokenKind::LtEq => (BinOp::Le, 4),
        TokenKind::Gt => (BinOp::Gt, 4),
        TokenKind::GtEq => (BinOp::Ge, 4),
        TokenKind::Plus => (BinOp::Add, 5),
        TokenKind::Minus => (BinOp::Sub, 5),
        TokenKind::Star => (BinOp::Mul, 6),
        TokenKind::Slash => (BinOp::Div, 6),
        TokenKind::Percent => (BinOp::Rem, 6),
        _ => return None,
    })
}

fn is_block_like(expr: &Expr) -> bool {
    matches!(
        expr.kind,
        ExprKind::If { .. } | ExprKind::Block(_) | ExprKind::While { .. } | ExprKind::For { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(src: &str) -> Program {
        let (program, diags) = parse(src);
        assert!(
            !diags.has_errors(),
            "unexpected errors: {:?}",
            diags.items
        );
        program.unwrap()
    }

    #[test]
    fn parses_function() {
        let program = parse_ok("fn add(a: int, b: int) -> int { a + b }");
        assert_eq!(program.fns.len(), 1);
        let f = &program.fns[0];
        assert_eq!(f.name, "add");
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.ret.kind, TypeKind::Int);
        assert!(f.body.value.is_some());
    }

    #[test]
    fn precedence_is_standard() {
        let program = parse_ok("fn main() { let x = 1 + 2 * 3 == 7; }");
        let Stmt::Let { value, .. } = &program.fns[0].body.stmts[0] else {
            panic!("expected let");
        };
        let ExprKind::Binary { op, lhs, rhs } = &value.kind else {
            panic!("expected binary");
        };
        assert_eq!(*op, BinOp::Eq);
        assert!(matches!(rhs.kind, ExprKind::Int(7)));
        let ExprKind::Binary { op, rhs, .. } = &lhs.kind else {
            panic!("expected binary lhs");
        };
        assert_eq!(*op, BinOp::Add);
        let ExprKind::Binary { op, .. } = &rhs.kind else {
            panic!("expected binary rhs");
        };
        assert_eq!(*op, BinOp::Mul);
    }

    #[test]
    fn parses_struct_and_literal() {
        let program = parse_ok(
            "struct Point { x: int, y: int }\nfn main() { let p = Point { x: 1, y: 2 }; }",
        );
        assert_eq!(program.structs.len(), 1);
        let Stmt::Let { value, .. } = &program.fns[0].body.stmts[0] else {
            panic!("expected let");
        };
        let ExprKind::StructLit { name, fields } = &value.kind else {
            panic!("expected struct literal");
        };
        assert_eq!(name, "Point");
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn if_else_chain_is_expression() {
        let program = parse_ok("fn main() { let x = if a { 1 } else if b { 2 } else { 3 }; }");
        let Stmt::Let { value, .. } = &program.fns[0].body.stmts[0] else {
            panic!("expected let");
        };
        let ExprKind::If { else_branch, .. } = &value.kind else {
            panic!("expected if");
        };
        let else_expr = else_branch.as_ref().unwrap();
        assert!(matches!(else_expr.kind, ExprKind::If { .. }));
    }

    #[test]
    fn postfix_chain() {
        let program = parse_ok("fn main() { let x = foo(1).bar[2]; }");
        let Stmt::Let { value, .. } = &program.fns[0].body.stmts[0] else {
            panic!("expected let");
        };
        let ExprKind::Index { object, .. } = &value.kind else {
            panic!("expected index");
        };
        let ExprKind::Field { .. } = &object.kind else {
            panic!("expected field");
        };
    }

    #[test]
    fn interpolation_contains_expression() {
        let (program, diags) = parse("fn main() { println(\"n = {n + 1}\"); }");
        assert!(!diags.has_errors(), "{:?}", diags.items);
        let program = program.unwrap();
        let Stmt::Expr(call) = &program.fns[0].body.stmts[0] else {
            panic!("expected call");
        };
        let ExprKind::Call { args, .. } = &call.kind else {
            panic!("expected call");
        };
        let ExprKind::Str(parts) = &args[0].kind else {
            panic!("expected string");
        };
        assert_eq!(parts.len(), 2);
        let StrPart::Expr(e) = &parts[1] else {
            panic!("expected interpolation");
        };
        assert!(matches!(e.kind, ExprKind::Binary { .. }));
        assert_eq!(e.span.start, 26);
    }

    #[test]
    fn loops_parse() {
        let program = parse_ok(
            "fn main() { for i in 0..10 { while i > 0 { break; } } for x in items { println(x); } }",
        );
        let body = &program.fns[0].body;
        let Stmt::Expr(for_expr) = &body.stmts[0] else {
            panic!("expected for");
        };
        let ExprKind::For { iter, .. } = &for_expr.kind else {
            panic!("expected for");
        };
        assert!(matches!(iter, ForIter::Range { .. }));
    }

    #[test]
    fn block_like_statements_need_no_semicolon() {
        let program = parse_ok("fn main() { if true { } let x = 1; }");
        assert_eq!(program.fns[0].body.stmts.len(), 2);
    }

    #[test]
    fn missing_semicolon_is_reported() {
        let (_, diags) = parse("fn main() { let x = 1 let y = 2; }");
        assert!(diags.has_errors());
        assert!(diags.items.iter().any(|d| d.message.contains("expected `;`")));
    }

    #[test]
    fn unclosed_block_is_reported() {
        let (_, diags) = parse("fn main() { let x = 1;");
        assert!(diags.has_errors());
        assert!(diags.items.iter().any(|d| d.message.contains("unclosed block")));
    }

    #[test]
    fn bad_interpolation_span_is_shifted() {
        let (_, diags) = parse("fn main() { println(\"hi {1 +}\"); }");
        assert!(diags.has_errors());
        let span = diags.items[0].span.unwrap();
        assert!(span.start >= 23, "span should point inside the string: {span:?}");
    }
}
