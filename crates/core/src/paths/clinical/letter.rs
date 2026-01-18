//! Clinical letter on-disk paths.
//!
//! This module defines the relative filesystem structure for clinical letters
//! stored within a VPR patient repository.
//!
//! It contains **no I/O**, **no Git logic**, and **no clinical semantics**.
//! Its sole responsibility is to provide typed, canonical paths so that
//! path invariants are defined in exactly one place.
//!
//! # Path Structure
//!
//! Each letter is stored under:
//! ```text
//! correspondence/
//!     letter/
//!         <timestamp-id>/
//!             composition.yaml
//!             body.md
//!             attachments/
//! ```
//!
//! Where `<timestamp-id>` is a [`TimestampId`] in the format:
//! `YYYYMMDDTHHMMSS.mmmZ-<uuid>`
//!
//! # Usage
//!
//! Use [`LetterPaths::new`] to create relative paths, then resolve them
//! against a patient repository root before performing filesystem operations.

use std::path::{Path, PathBuf};

use crate::TimestampId;

/// Top-level clinical correspondence directory.
///
/// This is a fixed path invariant relative to the patient repository root.
#[derive(Debug, Clone, Copy)]
pub struct CorrespondenceDir;

impl CorrespondenceDir {
    pub const NAME: &'static str = "correspondence";
}

/// Letter-based correspondence subdirectory.
///
/// This is intentionally a type (not a string literal) so that:
/// - the name is defined once,
/// - it cannot be misspelled,
/// - it carries meaning without behaviour.
#[derive(Debug, Clone, Copy)]
pub struct LetterDir;

impl LetterDir {
    pub const NAME: &'static str = "letter";
}

/// OpenEHR composition metadata file.
///
/// Contains the OpenEHR COMPOSITION envelope for the letter,
/// including identity, authorship, time context, and structured snapshots.
#[derive(Debug, Clone, Copy)]
pub struct CompositionYaml;

impl CompositionYaml {
    pub const NAME: &'static str = "composition.yaml";
}

/// Canonical clinical letter content file.
///
/// Contains the human-readable Markdown letter content.
#[derive(Debug, Clone, Copy)]
pub struct BodyMd;

impl BodyMd {
    pub const NAME: &'static str = "body.md";
}

/// Letter attachments subdirectory.
///
/// Large binary artefacts (PDFs, images, scans) are stored here
/// using Git LFS when appropriate.
#[derive(Debug, Clone, Copy)]
pub struct AttachmentsDir;

impl AttachmentsDir {
    pub const NAME: &'static str = "attachments";
}

/// Relative on-disk paths for a single clinical letter.
///
/// This represents **where a letter lives**, not what a letter *is*.
///
/// The paths are relative to the patient repository root and must be
/// resolved by repository-level code before filesystem access.
///
/// The directory name is derived from a [`TimestampId`], which provides:
/// - global uniqueness,
/// - per-patient chronological ordering,
/// - human-readable audit semantics.
#[derive(Debug, Clone)]
pub struct LetterPaths {
    relative_root: PathBuf,
}

impl LetterPaths {
    /// Creates a new relative path set for a letter with the given timestamp ID.
    ///
    /// The resulting paths are **relative** and must be joined to a patient
    /// repository root before filesystem access.
    ///
    /// # Arguments
    ///
    /// * `letter_id` - The timestamp identifier for this letter
    pub fn new(letter_id: &TimestampId) -> Self {
        Self {
            relative_root: PathBuf::from(CorrespondenceDir::NAME)
                .join(LetterDir::NAME)
                .join(letter_id.to_string()),
        }
    }

    /// Returns the relative path to the letter directory.
    pub fn dir(&self) -> &Path {
        &self.relative_root
    }

    /// Returns the relative path to `composition.yaml`.
    pub fn composition_yaml(&self) -> PathBuf {
        self.relative_root.join(CompositionYaml::NAME)
    }

    /// Returns the relative path to `body.md`.
    pub fn body_md(&self) -> PathBuf {
        self.relative_root.join(BodyMd::NAME)
    }

    /// Returns the relative path to the attachments directory.
    pub fn attachments_dir(&self) -> PathBuf {
        self.relative_root.join(AttachmentsDir::NAME)
    }

    /// Returns the relative path to a specific attachment file.
    ///
    /// This does not validate filenames and performs no I/O.
    ///
    /// # Arguments
    ///
    /// * `filename` - The name of the attachment file
    pub fn attachment(&self, filename: &str) -> PathBuf {
        self.attachments_dir().join(filename)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_directory_constants() {
        assert_eq!(CorrespondenceDir::NAME, "correspondence");
        assert_eq!(LetterDir::NAME, "letter");
        assert_eq!(CompositionYaml::NAME, "composition.yaml");
        assert_eq!(BodyMd::NAME, "body.md");
        assert_eq!(AttachmentsDir::NAME, "attachments");
    }

    #[test]
    fn test_letter_paths_relative_paths() {
        let timestamp_id =
            TimestampId::from_str("20260114T143522.045Z-550e8400-e29b-41d4-a716-446655440000")
                .expect("valid timestamp id");

        let paths = LetterPaths::new(&timestamp_id);

        assert_eq!(
            paths.dir(),
            Path::new(
                "correspondence/letter/20260114T143522.045Z-550e8400-e29b-41d4-a716-446655440000"
            )
        );

        assert_eq!(
            paths.composition_yaml(),
            PathBuf::from("correspondence/letter/20260114T143522.045Z-550e8400-e29b-41d4-a716-446655440000/composition.yaml")
        );

        assert_eq!(
            paths.body_md(),
            PathBuf::from("correspondence/letter/20260114T143522.045Z-550e8400-e29b-41d4-a716-446655440000/body.md")
        );

        assert_eq!(
            paths.attachments_dir(),
            PathBuf::from("correspondence/letter/20260114T143522.045Z-550e8400-e29b-41d4-a716-446655440000/attachments")
        );
    }

    #[test]
    fn test_letter_paths_attachment_path() {
        let timestamp_id =
            TimestampId::from_str("20260114T143522.045Z-550e8400-e29b-41d4-a716-446655440000")
                .expect("valid timestamp id");

        let paths = LetterPaths::new(&timestamp_id);

        assert_eq!(
            paths.attachment("letter.pdf"),
            PathBuf::from("correspondence/letter/20260114T143522.045Z-550e8400-e29b-41d4-a716-446655440000/attachments/letter.pdf")
        );

        assert_eq!(
            paths.attachment("scan.png"),
            PathBuf::from("correspondence/letter/20260114T143522.045Z-550e8400-e29b-41d4-a716-446655440000/attachments/scan.png")
        );
    }

    #[test]
    fn test_different_timestamp_ids_produce_different_paths() {
        let timestamp_id1 =
            TimestampId::from_str("20260114T143522.045Z-550e8400-e29b-41d4-a716-446655440000")
                .expect("valid timestamp id");
        let timestamp_id2 =
            TimestampId::from_str("20260115T093015.123Z-661f9511-f3ac-52e5-b827-557766551111")
                .expect("valid timestamp id");

        let paths1 = LetterPaths::new(&timestamp_id1);
        let paths2 = LetterPaths::new(&timestamp_id2);

        assert_ne!(paths1.dir(), paths2.dir());
        assert_ne!(paths1.composition_yaml(), paths2.composition_yaml());
        assert_ne!(paths1.body_md(), paths2.body_md());
    }

    #[test]
    fn test_paths_is_relative_not_absolute() {
        let timestamp_id =
            TimestampId::from_str("20260114T143522.045Z-550e8400-e29b-41d4-a716-446655440000")
                .expect("valid timestamp id");

        let paths = LetterPaths::new(&timestamp_id);

        // Relative paths should not start with '/'
        assert!(!paths.dir().to_str().unwrap().starts_with('/'));
        assert!(!paths.composition_yaml().to_str().unwrap().starts_with('/'));
        assert!(!paths.body_md().to_str().unwrap().starts_with('/'));
        assert!(!paths.attachments_dir().to_str().unwrap().starts_with('/'));
    }

    #[test]
    fn test_caller_can_join_with_patient_root() {
        let timestamp_id =
            TimestampId::from_str("20260114T143522.045Z-550e8400-e29b-41d4-a716-446655440000")
                .expect("valid timestamp id");

        let paths = LetterPaths::new(&timestamp_id);
        let patient_root =
            Path::new("/patient_data/clinical/55/0e/550e8400e29b41d4a716446655440000");

        // Callers join paths themselves
        assert_eq!(
            patient_root.join(paths.dir()),
            PathBuf::from("/patient_data/clinical/55/0e/550e8400e29b41d4a716446655440000/correspondence/letter/20260114T143522.045Z-550e8400-e29b-41d4-a716-446655440000")
        );

        assert_eq!(
            patient_root.join(paths.composition_yaml()),
            PathBuf::from("/patient_data/clinical/55/0e/550e8400e29b41d4a716446655440000/correspondence/letter/20260114T143522.045Z-550e8400-e29b-41d4-a716-446655440000/composition.yaml")
        );
    }
}
