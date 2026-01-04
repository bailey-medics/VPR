//! Narrative domain model.
//!
//! This component represents free-text clinical narrative captured as opaque Markdown text,
//! plus a small amount of optional, format-agnostic metadata.

use serde::{Deserialize, Serialize};

/// Opaque narrative content.
///
/// The `body` field is treated as an opaque Markdown string; the core crate does not parse or
/// interpret Markdown syntax.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NarrativeComponent {
    /// Optional human-friendly title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// Optional tags for grouping and retrieval.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Opaque Markdown narrative body.
    pub body: String,
}
