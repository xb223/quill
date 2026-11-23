//! The Quill tokenizer.
//!
//! The lexer is deliberately forgiving about whitespace and comments, and
//! deliberately strict about everything that could hide a bug. It also splits
//! interpolated strings (`"x = {x}"`) into literal and expression parts so the
//! parser can treat interpolation as ordinary expressions.

use crate::diag::{Diags, Span};

#[derive(Clone, Debug, PartialEq)]
pub enum StrPart {
    /// Already-unescaped literal text.
    Text(String),
    /// Source text of an interpolated expression, plus its byte offset in the
    /// enclosing file so spans point at the right place.
    Expr { src: String, offset: usize },
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    // Literals and names.
    Int(i64),
    Float(f64),
    Str(Vec<StrPart>),
    Ident(String),

    // Keywords.
    Fn,
    Let,
    Mut,
    If,
    Else,
    While,
    For,
    In,
    Return,
    Break,
    Continue,
    Struct,
    True,
    False,

    // Delimiters.
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semi,
    Colon,
    Dot,
    DotDot,
    Arrow,

    // Operators.
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    Assign,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    AndAnd,
    OrOr,
    Bang,

    Eof,
}

impl TokenKind {
    /// Human-readable spelling for diagnostics.
    pub fn describe(&self) -> String {
        match self {
            TokenKind::Int(n) => format!("integer `{n}`"),
            TokenKind::Float(n) => format!("float `{n}`"),
            TokenKind::Str(_) => "string literal".to_string(),
            TokenKind::Ident(name) => format!("`{name}`"),
            TokenKind::Fn => "`fn`".into(),
            TokenKind::Let => "`let`".into(),
            TokenKind::Mut => "`mut`".into(),
            TokenKind::If => "`if`".into(),
            TokenKind::Else => "`else`".into(),
            TokenKind::While => "`while`".into(),
            TokenKind::For => "`for`".into(),
            TokenKind::In => "`in`".into(),
            TokenKind::Return => "`return`".into(),
            TokenKind::Break => "`break`".into(),
            TokenKind::Continue => "`continue`".into(),
            TokenKind::Struct => "`struct`".into(),
            TokenKind::True => "`true`".into(),
            TokenKind::False => "`false`".into(),
            TokenKind::LParen => "`(`".into(),
            TokenKind::RParen => "`)`".into(),
            TokenKind::LBrace => "`{`".into(),
            TokenKind::RBrace => "`}`".into(),
            TokenKind::LBracket => "`[`".into(),
            TokenKind::RBracket => "`]`".into(),
            TokenKind::Comma => "`,`".into(),
            TokenKind::Semi => "`;`".into(),
            TokenKind::Colon => "`:`".into(),
            TokenKind::Dot => "`.`".into(),
            TokenKind::DotDot => "`..`".into(),
            TokenKind::Arrow => "`->`".into(),
            TokenKind::Plus => "`+`".into(),
            TokenKind::Minus => "`-`".into(),
            TokenKind::Star => "`*`".into(),
            TokenKind::Slash => "`/`".into(),
            TokenKind::Percent => "`%`".into(),
            TokenKind::PlusEq => "`+=`".into(),
            TokenKind::MinusEq => "`-=`".into(),
            TokenKind::StarEq => "`*=`".into(),
            TokenKind::SlashEq => "`/=`".into(),
            TokenKind::PercentEq => "`%=`".into(),
            TokenKind::Assign => "`=`".into(),
            TokenKind::EqEq => "`==`".into(),
            TokenKind::NotEq => "`!=`".into(),
            TokenKind::Lt => "`<`".into(),
            TokenKind::LtEq => "`<=`".into(),
            TokenKind::Gt => "`>`".into(),
            TokenKind::GtEq => "`>=`".into(),
            TokenKind::AndAnd => "`&&`".into(),
            TokenKind::OrOr => "`||`".into(),
            TokenKind::Bang => "`!`".into(),
            TokenKind::Eof => "end of file".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub fn lex(src: &str) -> (Vec<Token>, Diags) {
    let mut lexer = Lexer {
        src,
        pos: 0,
        tokens: Vec::new(),
    };
    let mut diags = Diags::new();
    lexer.run(&mut diags);
    (lexer.tokens, diags)
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    tokens: Vec<Token>,
}

impl<'a> Lexer<'a> {
    fn run(&mut self, diags: &mut Diags) {
        loop {
            self.skip_trivia(diags);
            let start = self.pos;
            let Some(c) = self.peek() else {
                self.tokens.push(Token {
                    kind: TokenKind::Eof,
                    span: Span::new(start, start),
                });
                break;
            };

            let kind = match c {
                c if c.is_alphabetic() || c == '_' => self.ident_or_keyword(),
                '0'..='9' => self.number(diags),
                '"' => {
                    let kind = self.string(diags);
                    self.push(start, kind);
                    continue;
                }
                '(' => self.single(TokenKind::LParen),
                ')' => self.single(TokenKind::RParen),
                '{' => self.single(TokenKind::LBrace),
                '}' => self.single(TokenKind::RBrace),
                '[' => self.single(TokenKind::LBracket),
                ']' => self.single(TokenKind::RBracket),
                ',' => self.single(TokenKind::Comma),
                ';' => self.single(TokenKind::Semi),
                ':' => self.single(TokenKind::Colon),
                '.' => {
                    self.bump();
                    if self.peek() == Some('.') {
                        self.bump();
                        TokenKind::DotDot
                    } else {
                        TokenKind::Dot
                    }
                }
                '+' => {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        TokenKind::PlusEq
                    } else {
                        TokenKind::Plus
                    }
                }
                '-' => {
                    self.bump();
                    if self.peek() == Some('>') {
                        self.bump();
                        TokenKind::Arrow
                    } else if self.peek() == Some('=') {
                        self.bump();
                        TokenKind::MinusEq
                    } else {
                        TokenKind::Minus
                    }
                }
                '*' => {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        TokenKind::StarEq
                    } else {
                        TokenKind::Star
                    }
                }
                '/' => {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        TokenKind::SlashEq
                    } else {
                        TokenKind::Slash
                    }
                }
                '%' => {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        TokenKind::PercentEq
                    } else {
                        TokenKind::Percent
                    }
                }
                '=' => {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        TokenKind::EqEq
                    } else {
                        TokenKind::Assign
                    }
                }
                '!' => {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        TokenKind::NotEq
                    } else {
                        TokenKind::Bang
                    }
                }
                '<' => {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        TokenKind::LtEq
                    } else {
                        TokenKind::Lt
                    }
                }
                '>' => {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        TokenKind::GtEq
                    } else {
                        TokenKind::Gt
                    }
                }
                '&' => {
                    let span = self.span_from(start);
                    self.bump();
                    if self.peek() == Some('&') {
                        self.bump();
                        TokenKind::AndAnd
                    } else {
                        diags.error_note(
                            span,
                            "Quill has no references, so `&` is not valid here",
                            "values are passed by value; use `&&` for logical and",
                        );
                        TokenKind::Eof
                    }
                }
                '|' => {
                    let span = self.span_from(start);
                    self.bump();
                    if self.peek() == Some('|') {
                        self.bump();
                        TokenKind::OrOr
                    } else {
                        diags.error_note(
                            span,
                            "unexpected `|`",
                            "use `||` for logical or",
                        );
                        TokenKind::Eof
                    }
                }
                other => {
                    let span = self.span_from(start);
                    self.bump();
                    diags.error(span, format!("unexpected character `{other}`"));
                    TokenKind::Eof
                }
            };
            self.push(start, kind);
        }
    }

    fn push(&mut self, start: usize, kind: TokenKind) {
        let span = self.span_from(start);
        self.tokens.push(Token { kind, span });
    }

    fn span_from(&self, start: usize) -> Span {
        Span::new(start, self.pos)
    }

    fn single(&mut self, kind: TokenKind) -> TokenKind {
        self.bump();
        kind
    }

    fn peek(&self) -> Option<char> {
        self.src[self.pos..].chars().next()
    }

    fn peek_after(&self, n: usize) -> Option<char> {
        self.src[self.pos..].chars().nth(n)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn skip_trivia(&mut self, diags: &mut Diags) {
        loop {
            match self.peek() {
                Some(c) if c.is_whitespace() => {
                    self.bump();
                }
                Some('/') if self.peek_after(1) == Some('/') => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                Some('/') if self.peek_after(1) == Some('*') => {
                    let start = self.pos;
                    self.bump();
                    self.bump();
                    let mut closed = false;
                    while let Some(c) = self.bump() {
                        if c == '*' && self.peek() == Some('/') {
                            self.bump();
                            closed = true;
                            break;
                        }
                    }
                    if !closed {
                        diags.error(
                            Span::new(start, self.pos),
                            "unterminated block comment",
                        );
                    }
                }
                _ => break,
            }
        }
    }

    fn ident_or_keyword(&mut self) -> TokenKind {
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_alphanumeric() || c == '_') {
            self.bump();
        }
        let text = &self.src[start..self.pos];
        match text {
            "fn" => TokenKind::Fn,
            "let" => TokenKind::Let,
            "mut" => TokenKind::Mut,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "return" => TokenKind::Return,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "struct" => TokenKind::Struct,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            _ => TokenKind::Ident(text.to_string()),
        }
    }

    fn number(&mut self, diags: &mut Diags) -> TokenKind {
        let start = self.pos;
        while matches!(self.peek(), Some('0'..='9' | '_')) {
            self.bump();
        }

        let mut is_float = false;

        // A dot is only part of the literal when a digit follows, so `0..10`
        // stays a range and `1.foo` stays field access.
        if self.peek() == Some('.') && matches!(self.peek_after(1), Some('0'..='9')) {
            is_float = true;
            self.bump();
            while matches!(self.peek(), Some('0'..='9' | '_')) {
                self.bump();
            }
        }

        // Optional exponent: 1e9, 2.5e-3.
        if matches!(self.peek(), Some('e' | 'E')) {
            let after = self.peek_after(1);
            let after2 = self.peek_after(2);
            let has_exp = match after {
                Some('0'..='9') => true,
                Some('+' | '-') => matches!(after2, Some('0'..='9')),
                _ => false,
            };
            if has_exp {
                is_float = true;
                self.bump();
                if matches!(self.peek(), Some('+' | '-')) {
                    self.bump();
                }
                while matches!(self.peek(), Some('0'..='9' | '_')) {
                    self.bump();
                }
            }
        }

        let text = self.src[start..self.pos].replace('_', "");
        let span = self.span_from(start);

        if is_float {
            match text.parse::<f64>() {
                Ok(value) => TokenKind::Float(value),
                Err(_) => {
                    diags.error(span, format!("invalid float literal `{text}`"));
                    TokenKind::Float(0.0)
                }
            }
        } else {
            match text.parse::<i64>() {
                Ok(value) => TokenKind::Int(value),
                Err(_) => {
                    diags.error_note(
                        span,
                        format!("integer literal `{text}` is too large"),
                        "`int` is a signed 64-bit integer (max 9,223,372,036,854,775,807)",
                    );
                    TokenKind::Int(0)
                }
            }
        }
    }

    fn string(&mut self, diags: &mut Diags) -> TokenKind {
        let open = self.pos;
        self.bump(); // opening quote

        let mut parts: Vec<StrPart> = Vec::new();
        let mut text = String::new();
        let mut closed = false;

        while let Some(c) = self.peek() {
            match c {
                '"' => {
                    self.bump();
                    closed = true;
                    break;
                }
                '\\' => {
                    let esc_start = self.pos;
                    self.bump();
                    match self.bump() {
                        Some('n') => text.push('\n'),
                        Some('t') => text.push('\t'),
                        Some('r') => text.push('\r'),
                        Some('0') => text.push('\0'),
                        Some('\\') => text.push('\\'),
                        Some('"') => text.push('"'),
                        Some('\'') => text.push('\''),
                        Some('{') => text.push('{'),
                        Some('}') => text.push('}'),
                        Some(other) => {
                            diags.error(
                                Span::new(esc_start, self.pos),
                                format!("unknown escape `\\{other}`"),
                            );
                            text.push(other);
                        }
                        None => {
                            diags.error(
                                Span::new(esc_start, self.pos),
                                "unterminated escape sequence",
                            );
                        }
                    }
                }
                '{' => {
                    self.bump();
                    if self.peek() == Some('{') {
                        self.bump();
                        text.push('{');
                        continue;
                    }
                    // Flush accumulated literal text.
                    if !text.is_empty() {
                        parts.push(StrPart::Text(std::mem::take(&mut text)));
                    }

                    let expr_start = self.pos;
                    let mut depth = 1usize;
                    while let Some(inner) = self.peek() {
                        match inner {
                            '{' => {
                                depth += 1;
                                self.bump();
                            }
                            '}' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                                self.bump();
                            }
                            '"' => self.skip_nested_string(diags),
                            _ => {
                                self.bump();
                            }
                        }
                    }

                    if depth != 0 {
                        diags.error(
                            Span::new(open, self.pos),
                            "unterminated interpolation: missing `}` in string",
                        );
                        break;
                    }

                    let src = self.src[expr_start..self.pos].to_string();
                    if src.trim().is_empty() {
                        diags.error(
                            Span::new(expr_start, self.pos),
                            "empty interpolation `{}`",
                        );
                    } else {
                        parts.push(StrPart::Expr {
                            src,
                            offset: expr_start,
                        });
                    }
                    self.bump(); // closing `}`
                }
                '}' => {
                    self.bump();
                    if self.peek() == Some('}') {
                        self.bump();
                        text.push('}');
                    } else {
                        diags.error(
                            Span::new(self.pos - 1, self.pos),
                            "unmatched `}` in string",
                        );
                    }
                }
                _ => {
                    self.bump();
                    text.push(c);
                }
            }
        }

        if !closed {
            diags.error(
                Span::new(open, self.pos),
                "unterminated string literal",
            );
        }

        if !text.is_empty() {
            parts.push(StrPart::Text(text));
        }
        TokenKind::Str(parts)
    }

    /// Skip a string literal nested inside an interpolation so braces inside it
    /// do not confuse interpolation scanning.
    fn skip_nested_string(&mut self, diags: &mut Diags) {
        let start = self.pos;
        self.bump(); // opening quote
        while let Some(c) = self.peek() {
            match c {
                '"' => {
                    self.bump();
                    return;
                }
                '\\' => {
                    self.bump();
                    self.bump();
                }
                _ => {
                    self.bump();
                }
            }
        }
        diags.error(
            Span::new(start, self.pos),
            "unterminated string inside interpolation",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        let (tokens, diags) = lex(src);
        assert!(!diags.has_errors(), "unexpected lex errors: {:?}", diags.items);
        tokens.into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn keywords_and_idents() {
        assert_eq!(
            kinds("fn main let mut struct"),
            vec![
                TokenKind::Fn,
                TokenKind::Ident("main".into()),
                TokenKind::Let,
                TokenKind::Mut,
                TokenKind::Struct,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn range_is_not_a_float() {
        assert_eq!(
            kinds("0..10"),
            vec![
                TokenKind::Int(0),
                TokenKind::DotDot,
                TokenKind::Int(10),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn numbers() {
        assert_eq!(
            kinds("1_000 2.5 1e3 2.5e-2"),
            vec![
                TokenKind::Int(1000),
                TokenKind::Float(2.5),
                TokenKind::Float(1000.0),
                TokenKind::Float(0.025),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn string_interpolation_is_split() {
        let tokens = kinds(r#""hi {name}, you are {age + 1}""#);
        let TokenKind::Str(parts) = &tokens[0] else {
            panic!("expected string");
        };
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], StrPart::Text("hi ".into()));
        assert_eq!(
            parts[1],
            StrPart::Expr {
                src: "name".into(),
                offset: 5
            }
        );
        assert_eq!(parts[2], StrPart::Text(", you are ".into()));
        assert!(matches!(parts[3], StrPart::Expr { .. }));
    }

    #[test]
    fn string_escapes_and_braces() {
        assert_eq!(
            kinds(r#""a\nb {{literal}}" "\\""#),
            vec![
                TokenKind::Str(vec![StrPart::Text("a\nb {literal}".into())]),
                TokenKind::Str(vec![StrPart::Text("\\".into())]),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn comments_are_skipped() {
        assert_eq!(
            kinds("// hello\nlet /* inline */ x = 1;"),
            vec![
                TokenKind::Let,
                TokenKind::Ident("x".into()),
                TokenKind::Assign,
                TokenKind::Int(1),
                TokenKind::Semi,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn stray_ampersand_has_a_kind_error() {
        let (_, diags) = lex("let x = &y;");
        assert!(diags.has_errors());
        assert!(diags.items[0].message.contains("no references"));
    }

    #[test]
    fn compound_operators_lex() {
        let (tokens, diags) = lex("+= -= *= /= %=");
        assert!(!diags.has_errors());
        let kinds: Vec<TokenKind> = tokens.into_iter().map(|t| t.kind).collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::PlusEq,
                TokenKind::MinusEq,
                TokenKind::StarEq,
                TokenKind::SlashEq,
                TokenKind::PercentEq,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn unterminated_string_is_reported() {
        let (_, diags) = lex("\"oops");
        assert!(diags.has_errors());
        assert!(diags.items[0].message.contains("unterminated string"));
    }

    #[test]
    fn spans_are_byte_accurate() {
        let (tokens, _) = lex("let café = 1;");
        assert_eq!(tokens[1].kind, TokenKind::Ident("café".into()));
        assert_eq!(tokens[1].span, Span::new(4, 9));
        assert_eq!(tokens[2].span, Span::new(10, 11));
    }
}
