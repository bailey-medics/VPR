//! Public domain-level types for external API use.
//!
//! This module provides RM-agnostic data types that external code can use
//! without coupling to specific RM wire formats or version details.

pub mod letter;

pub use letter::{AttachmentReference, ClinicalList, ClinicalListItem, CodedConcept, LetterData};
