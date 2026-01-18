//! Common clinical path components.
//!
//! This module contains path components shared across multiple clinical record types.

/// Top-level clinical correspondence directory.
///
/// This is a fixed path invariant relative to the patient repository root.
#[derive(Debug, Clone, Copy)]
pub struct CorrespondenceDir;

impl CorrespondenceDir {
    pub const NAME: &'static str = "correspondence";
}
