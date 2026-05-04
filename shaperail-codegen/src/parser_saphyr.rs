//! Saphyr-based YAML parser that produces both a [`ResourceDefinition`] and a
//! [`SpanMap`] with per-field source positions.
//!
//! This module is available only when the `saphyr-spans` cargo feature is
//! enabled. It does **not** replace the default `serde_yaml`-based parser;
//! it adds an alternative entry point that also returns position information.
//!
//! # Usage
//! ```rust,ignore
//! let (rd, spans) = shaperail_codegen::parser_saphyr::parse_with_spans(yaml)?;
//! if let Some(span) = spans.lookup("schema.email.type") {
//!     println!("field type declared at line {}", span.line);
//! }
//! ```

use std::path::PathBuf;

use saphyr::{LoadableYamlNode, MarkedYaml, Scalar, YamlData};
use shaperail_core::ResourceDefinition;

use crate::span::SpanMap;

/// Errors that can occur in the saphyr-based parser.
#[derive(Debug, thiserror::Error)]
pub enum SaphyrParseError {
    /// The YAML source was syntactically invalid.
    #[error("yaml scan error: {0}")]
    Scan(#[from] saphyr::ScanError),

    /// The YAML was valid but could not be deserialized into a
    /// [`ResourceDefinition`]. This mirrors the structural validation done by
    /// `serde_yaml`.
    #[error("deserialization error: {0}")]
    Deserialize(#[from] serde_yaml::Error),

    /// The document was empty or had no top-level mapping.
    #[error("empty or non-mapping YAML document")]
    EmptyDocument,
}

/// Parse a YAML string into a [`ResourceDefinition`] plus a [`SpanMap`].
///
/// The file path stored in the returned spans will be empty (`PathBuf::new()`).
/// Use [`parse_with_spans_in_file`] when you have a real file path.
///
/// # Errors
/// Returns [`SaphyrParseError`] if the YAML is syntactically invalid or
/// cannot be deserialized into a [`ResourceDefinition`].
pub fn parse_with_spans(yaml: &str) -> Result<(ResourceDefinition, SpanMap), SaphyrParseError> {
    parse_with_spans_in_file(yaml, PathBuf::new())
}

/// Parse a YAML string into a [`ResourceDefinition`] plus a [`SpanMap`].
///
/// `file` is stored inside every [`Span`](crate::diagnostics::Span) in the
/// returned map. Use `PathBuf::new()` if no file path is available.
///
/// # Errors
/// Returns [`SaphyrParseError`] if the YAML is syntactically invalid or
/// cannot be deserialized into a [`ResourceDefinition`].
pub fn parse_with_spans_in_file(
    yaml: &str,
    file: PathBuf,
) -> Result<(ResourceDefinition, SpanMap), SaphyrParseError> {
    // Load with span information via the MarkedYaml node type.
    let docs = MarkedYaml::load_from_str(yaml)?;
    let doc = docs
        .into_iter()
        .next()
        .ok_or(SaphyrParseError::EmptyDocument)?;

    let mut span_map = SpanMap::new(file);

    // Walk the AST to build (a) a serde_yaml::Value tree for deserialization
    // and (b) the SpanMap keyed by dotted path.
    let serde_value = walk_node(&doc, "", &mut span_map);

    // Deserialize the reconstructed serde_yaml::Value into ResourceDefinition.
    let rd: ResourceDefinition = serde_yaml::from_value(serde_value)?;

    Ok((rd, span_map))
}

/// Convert a saphyr `Scalar` to a `serde_yaml::Value`.
fn scalar_to_serde(scalar: &Scalar<'_>) -> serde_yaml::Value {
    match scalar {
        Scalar::Null => serde_yaml::Value::Null,
        Scalar::Boolean(b) => serde_yaml::Value::Bool(*b),
        Scalar::Integer(i) => serde_yaml::Value::Number(serde_yaml::Number::from(*i)),
        Scalar::FloatingPoint(f) => {
            // ordered_float::OrderedFloat<f64> derefs to f64
            serde_yaml::Value::Number(serde_yaml::Number::from(f64::from(*f)))
        }
        Scalar::String(s) => serde_yaml::Value::String(s.as_ref().to_owned()),
    }
}

/// Recursively walk a [`MarkedYaml`] node, recording spans into `map` and
/// returning the equivalent [`serde_yaml::Value`].
///
/// `path` is the dotted path to the current node (empty string for the root
/// document node).
fn walk_node(node: &MarkedYaml<'_>, path: &str, map: &mut SpanMap) -> serde_yaml::Value {
    // Record span for this node. The saphyr Marker uses:
    //   line  — 1-indexed (matches our convention)
    //   col   — 0-indexed (we add 1 to match our 1-indexed convention)
    let start = node.span.start;
    let end = node.span.end;
    let line = start.line() as u32;
    // col() returns 0-indexed; convert to 1-indexed.
    let col = start.col() as u32 + 1;
    let end_line = end.line() as u32;
    let end_col = if end.line() == 0 && end.col() == 0 {
        // Default (zero) marker — zero-width span at start.
        col
    } else {
        end.col() as u32 + 1
    };

    if !path.is_empty() {
        map.insert(path, line, col, end_line, end_col);
    }

    match &node.data {
        YamlData::Value(scalar) => scalar_to_serde(scalar),

        YamlData::Sequence(seq) => {
            let items = seq
                .iter()
                .enumerate()
                .map(|(i, child)| {
                    let child_path = if path.is_empty() {
                        format!("[{i}]")
                    } else {
                        format!("{path}[{i}]")
                    };
                    walk_node(child, &child_path, map)
                })
                .collect();
            serde_yaml::Value::Sequence(items)
        }

        YamlData::Mapping(mapping) => {
            let mut out = serde_yaml::Mapping::new();
            for (k, v) in mapping {
                // Extract the key string from the key node.
                let key_str = match &k.data {
                    YamlData::Value(Scalar::String(s)) => s.as_ref().to_owned(),
                    YamlData::Value(scalar) => format!("{scalar:?}"),
                    _ => continue,
                };

                // Record key node span.
                let k_start = k.span.start;
                let k_end = k.span.end;
                let k_line = k_start.line() as u32;
                let k_col = k_start.col() as u32 + 1;
                let k_end_line = k_end.line() as u32;
                let k_end_col = if k_end.line() == 0 && k_end.col() == 0 {
                    k_col
                } else {
                    k_end.col() as u32 + 1
                };

                let key_path = if path.is_empty() {
                    key_str.clone()
                } else {
                    format!("{path}.{key_str}")
                };

                // Insert key span under <path> (e.g. "resource", "schema.email").
                map.insert(&key_path, k_line, k_col, k_end_line, k_end_col);

                // Walk value node under <path>.<key> (e.g. "resource.value",
                // "schema.email.type"). We append ".value" only for scalars to
                // avoid path collisions; for collections the path itself is the
                // container.
                let value_path = match &v.data {
                    YamlData::Mapping(_) | YamlData::Sequence(_) => key_path.clone(),
                    _ => format!("{key_path}.__value"),
                };
                let serde_val = walk_node(v, &value_path, map);

                out.insert(serde_yaml::Value::String(key_str), serde_val);
            }
            serde_yaml::Value::Mapping(out)
        }

        // Tagged nodes: unwrap and recurse.
        YamlData::Tagged(_, inner) => walk_node(inner, path, map),

        // Alias / BadValue / Representation — return null and skip span.
        YamlData::Alias(_) | YamlData::BadValue => serde_yaml::Value::Null,

        // Representation (unparsed scalar string) — treat as string.
        YamlData::Representation(s, _, _) => serde_yaml::Value::String(s.as_ref().to_owned()),
    }
}
