//! Public domain-level letter data types.
//!
//! This module provides RM-agnostic data carriers for letter compositions,
//! allowing external code to work with domain concepts without coupling to
//! specific RM wire formats.

use crate::{RmVersion, TimestampId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Domain-level carrier for letter composition data.
///
/// This struct represents the essential fields of a clinical letter composition
/// in a format that is independent of specific RM versions and wire formats.
///
/// This type is symmetric with both parsing and rendering:
/// - `composition_parse()` extracts domain fields into this struct
/// - `composition_render()` builds wire format from this struct
///
/// # Letter Content
///
/// A letter can have:
/// - A body (markdown text in body.md) via `has_body` flag
/// - Attachments (external files) via `attachments` vector
/// - Both body AND attachments
/// - But never neither (at least one must be present)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LetterData {
    /// RM version for this letter.
    pub rm_version: RmVersion,

    /// Unique identifier for this composition.
    pub uid: TimestampId,

    /// Name of the composer (author) of the letter.
    pub composer_name: String,

    /// Role of the composer (for example "Consultant Physician").
    pub composer_role: String,

    /// Start time of the clinical context.
    pub start_time: DateTime<Utc>,

    /// Optional clinical lists (snapshot evaluations) to include.
    pub clinical_lists: Vec<ClinicalList>,

    /// Whether this letter has a body.md file.
    /// When true, generates an external_text narrative pointing to ./body.md
    pub has_body: bool,

    /// Attachment references. When present, these generate
    /// external_media narratives pointing to attachment metadata files.
    pub attachments: Vec<AttachmentReference>,
}

/// Reference to an attachment file in a letter composition.
///
/// This represents a pointer to an attachment YAML file that contains
/// metadata about the actual file stored in the files repository.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentReference {
    /// Relative path to the attachment metadata file
    /// (e.g., "./attachments/attachment_1.yaml")
    pub path: String,
}

/// A clinical list representing a collection of related clinical items.
///
/// This is an RM-agnostic carrier type intended for public API use.
/// It maps internally to the snapshot EVALUATION archetype (openEHR-EHR-EVALUATION.snapshot.v1).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClinicalList {
    /// Human-readable name for this list (for example "Diagnoses (snapshot)").
    pub name: String,

    /// Semantic kind identifying what this list represents (for example "diagnoses", "medications").
    pub kind: String,

    /// Items in this clinical list.
    pub items: Vec<ClinicalListItem>,
}

/// An item within a clinical list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClinicalListItem {
    /// Human-readable text for this item.
    pub text: String,

    /// Optional coded concept associated with this item.
    pub code: Option<CodedConcept>,
}

// TODO: Replace with a proper coded concept implementation.

/// A coded concept with terminology and code value.
///
/// NOTE: This is a simplified representation. A proper implementation of coded concepts
/// would require significantly more work and validation, including terminology binding,
/// code system validation, versioning, and conformance to standards like FHIR CodeableConcept
/// or openEHR DV_CODED_TEXT constraints.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodedConcept {
    /// Terminology system (for example "SNOMED-CT", "ICD-10").
    pub terminology: String,

    /// Code value within the terminology system.
    pub value: String,
}
