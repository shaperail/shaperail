//! Path-indexed spans. Field paths use dot notation (`schema.email.type`)
//! mirroring how diagnostics name fields in their `error` strings.

use crate::diagnostics::Span;
use std::collections::HashMap;
use std::path::PathBuf;

/// Map from dotted field path to source span. Built by the saphyr parser;
/// consumed by `diagnose_resource_with_spans` (Task 4) to attach positions
/// to diagnostics.
#[derive(Debug, Default, Clone)]
pub struct SpanMap {
    file: PathBuf,
    inner: HashMap<String, (u32, u32, u32, u32)>,
}

impl SpanMap {
    /// Create a new `SpanMap` associated with the given source file path.
    pub fn new(file: PathBuf) -> Self {
        Self {
            file,
            inner: HashMap::new(),
        }
    }

    /// Insert a span for the given dotted path.
    ///
    /// Lines and columns are 1-indexed; `end_col` is exclusive.
    pub fn insert(
        &mut self,
        path: impl Into<String>,
        line: u32,
        col: u32,
        end_line: u32,
        end_col: u32,
    ) {
        self.inner
            .insert(path.into(), (line, col, end_line, end_col));
    }

    /// Look up the span for the given dotted path, returning a [`Span`] if found.
    pub fn lookup(&self, path: &str) -> Option<Span> {
        self.inner
            .get(path)
            .map(|&(line, col, end_line, end_col)| Span {
                file: self.file.clone(),
                line,
                col,
                end_line,
                end_col,
            })
    }

    /// Return `true` if no spans have been inserted.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Return the number of spans in this map.
    pub fn len(&self) -> usize {
        self.inner.len()
    }
}
