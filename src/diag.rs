//! Source locations, diagnostics and pretty printing.
//!
//! Quill takes error messages seriously: every diagnostic points at a span in
//! the original source and, where useful, carries a note explaining the rule or
//! suggesting a fix.

use std::fmt::Write as _;

/// A byte range in a source file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }

    /// Smallest span covering both inputs.
    pub fn join(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

/// A source file plus a line index for fast position lookups.
pub struct SourceFile {
    pub name: String,
    pub text: String,
    line_starts: Vec<usize>,
}

impl SourceFile {
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let mut line_starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }
        SourceFile {
            name: name.into(),
            text,
            line_starts,
        }
    }

    /// Zero-based line and character column for a byte offset.
    pub fn line_col(&self, offset: usize) -> (usize, usize) {
        let offset = offset.min(self.text.len());
        let line = self
            .line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1);
        let line_start = self.line_starts[line];
        let col = self.text[line_start..offset].chars().count();
        (line, col)
    }

    pub fn line_text(&self, line: usize) -> &str {
        let start = self.line_starts[line];
        let end = self
            .line_starts
            .get(line + 1)
            .copied()
            .unwrap_or(self.text.len());
        self.text[start..end].trim_end_matches(['\n', '\r'])
    }
}

/// One compiler message.
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub message: String,
    pub span: Option<Span>,
    pub note: Option<String>,
}

/// A collection of diagnostics produced by one compiler phase.
#[derive(Default)]
pub struct Diags {
    pub items: Vec<Diagnostic>,
    pub error_count: usize,
}

impl Diags {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn error(&mut self, span: Span, message: impl Into<String>) {
        self.items.push(Diagnostic {
            message: message.into(),
            span: Some(span),
            note: None,
        });
        self.error_count += 1;
    }

    pub fn error_note(
        &mut self,
        span: Span,
        message: impl Into<String>,
        note: impl Into<String>,
    ) {
        self.items.push(Diagnostic {
            message: message.into(),
            span: Some(span),
            note: Some(note.into()),
        });
        self.error_count += 1;
    }

    pub fn global_error(&mut self, message: impl Into<String>) {
        self.items.push(Diagnostic {
            message: message.into(),
            span: None,
            note: None,
        });
        self.error_count += 1;
    }

    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }

    /// Render every diagnostic against `file`.
    pub fn render(&self, file: &SourceFile) -> String {
        let mut out = String::new();
        for (i, d) in self.items.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            render_one(&mut out, file, d);
        }
        out
    }
}

fn render_one(out: &mut String, file: &SourceFile, d: &Diagnostic) {
    let _ = writeln!(out, "error: {}", d.message);

    let Some(span) = d.span else {
        if let Some(note) = &d.note {
            let _ = writeln!(out, "  = note: {note}");
        }
        return;
    };

    let (line, col) = file.line_col(span.start);
    let (end_line, end_col) = file.line_col(span.end.max(span.start));
    let line_no = line + 1;
    let gutter = line_no.to_string().len().max(2);

    let _ = writeln!(
        out,
        "{:width$}--> {}:{}:{}",
        "",
        file.name,
        line_no,
        col + 1,
        width = gutter + 1
    );

    let raw_line = file.line_text(line);
    let expanded = expand_tabs(raw_line);
    let display_col = expand_tabs(&raw_line.chars().take(col).collect::<String>())
        .chars()
        .count();
    let _ = writeln!(out, "{:width$} |", "", width = gutter);
    let _ = writeln!(out, "{line_no:>gutter$} | {expanded}");

    // Caret length in characters, clamped to this line.
    let carets = if end_line == line {
        (end_col.saturating_sub(col)).max(1)
    } else {
        raw_line.chars().count().saturating_sub(col).max(1)
    };
    let _ = writeln!(
        out,
        "{:width$} | {:pad$}{:^<carets$}",
        "",
        "",
        "",
        width = gutter,
        pad = display_col,
        carets = carets
    );

    if let Some(note) = &d.note {
        let _ = writeln!(out, "{:width$} |", "", width = gutter);
        let _ = writeln!(out, "{:width$} = note: {note}", "", width = gutter);
    }
}

fn expand_tabs(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '\t' {
            out.push_str("    ");
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_and_column() {
        let file = SourceFile::new("t.q", "let a = 1;\nlet bb = 2;\n");
        assert_eq!(file.line_col(0), (0, 0));
        assert_eq!(file.line_col(11), (1, 0));
        assert_eq!(file.line_col(15), (1, 4));
    }

    #[test]
    fn renders_caret_under_span() {
        let file = SourceFile::new("t.q", "let x = y;\n");
        let mut diags = Diags::new();
        diags.error(Span::new(8, 9), "cannot find value `y`");
        let text = diags.render(&file);
        assert!(text.contains("--> t.q:1:9"));
        assert!(text.contains("let x = y;"));
        assert!(text.contains('^'));
    }

    #[test]
    fn tabs_do_not_misalign_caret() {
        let file = SourceFile::new("t.q", "\tlet x = y;\n");
        let mut diags = Diags::new();
        diags.error(Span::new(9, 10), "boom");
        let text = diags.render(&file);
        let caret_line = text.lines().find(|l| l.contains('^')).unwrap();
        let src_line = text.lines().find(|l| l.contains("let x = y")).unwrap();
        assert_eq!(
            caret_line.find('^').unwrap(),
            src_line.find('y').unwrap(),
            "caret should line up after tab expansion:\n{text}"
        );
    }
}
