use crate::diagnostics::registry::{lookup, Severity};
use std::path::PathBuf;

/// A diagnostic emitted by the codegen for a resource file.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Diagnostic {
    pub code: &'static str,
    pub error: String,
    pub fix: String,
    pub example: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,

    pub severity: Severity,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_url: Option<String>,
}

/// Source position for a diagnostic.
///
/// `line` and `col` are 1-indexed. `col` is a 1-indexed UTF-8 byte column
/// (not a grapheme or character index). `end_line` / `end_col` describe an
/// **exclusive** end — i.e. a single-character span at line 3 col 5 is
/// `(line: 3, col: 5, end_line: 3, end_col: 6)`. When the parser cannot
/// determine an end position, set `end_line == line` and `end_col == col`
/// (a zero-width span pointing at the start).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Span {
    pub file: PathBuf,
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

impl Diagnostic {
    /// Construct a diagnostic. Severity and doc_url come from the registry.
    /// In debug builds, panics if `code` is not in the registry — keeps the
    /// registry authoritative. In release builds, falls back to Severity::Error.
    pub fn error(
        code: &'static str,
        error: impl Into<String>,
        fix: impl Into<String>,
        example: impl Into<String>,
    ) -> Self {
        let severity = lookup(code).map(|e| e.severity).unwrap_or_else(|| {
            debug_assert!(false, "diagnostic code {code} not in registry");
            Severity::Error
        });
        Self {
            code,
            error: error.into(),
            fix: fix.into(),
            example: example.into(),
            span: None,
            severity,
            doc_url: Some(format!("https://shaperail.io/errors/{code}.html")),
        }
    }

    /// Attach a source position to this diagnostic.
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
}
