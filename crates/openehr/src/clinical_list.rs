//! Public RM-agnostic clinical list types for external use.
//!
//! This module provides domain-friendly types that can be used by callers (like clinical.rs)
//! without coupling to specific RM versions. The openehr crate handles mapping these to the
//! appropriate RM structures internally.

use serde::{Deserialize, Serialize};

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

/// A coded concept with terminology and code value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodedConcept {
    /// Terminology system (for example "SNOMED-CT", "ICD-10").
    pub terminology: String,

    /// Code value within the terminology system.
    pub value: String,
}
