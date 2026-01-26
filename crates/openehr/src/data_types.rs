//! OpenEHR RM data types.
//!
//! This module provides wire representations of openEHR Reference Model (RM) data types
//! used across multiple RM structures. It includes simplified wrappers for common types
//! and validated representations of complex identifiers such as archetype IDs.
//!
//! Key types:
//! - [`DvText`]: Simple text value wrapper for RM `DV_TEXT`.
//! - [`ArchetypeId`]: Parsed and validated openEHR archetype identifier with strict constraints.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use vpr_types::NonEmptyText;

use crate::OpenEhrError;

/// Simplified representation of the openEHR `DV_TEXT` data type.
///
/// In the openEHR Reference Model, `DV_TEXT` is a fundamental data value type used to
/// represent human-readable text content. The full specification includes optional fields
/// for language, character encoding, text formatting, and terminology mappings.
///
/// This implementation provides a minimal wire representation containing only the text
/// `value` itself, which is sufficient for VPR's current use cases where plain text
/// content is needed without internationalisation or formatting metadata.
///
/// # OpenEHR Context
///
/// `DV_TEXT` appears throughout the openEHR RM structures:
/// - As narrative text in compositions and entries
/// - For coded term descriptions and display text
/// - In protocol and workflow descriptions
/// - For free-text clinical observations
///
/// # Implementation Notes
///
/// - The `#[serde(deny_unknown_fields)]` attribute ensures strict deserialisation,
///   rejecting any JSON/YAML with unexpected fields.
/// - Only the `value` field is implemented; language, encoding, hyperlink, mappings,
///   and formatting fields from the full specification are not currently supported.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DvText {
    /// The plain text content.
    pub value: NonEmptyText,
}

/// Parsed and validated representation of an openEHR archetype identifier.
///
/// In the openEHR architecture, archetypes are formal constraint definitions that specialise
/// Reference Model (RM) classes to create domain-specific clinical models. Each archetype is
/// uniquely identified by an archetype ID, which encodes the authority, RM target, semantic
/// concept, and version in a structured string format.
///
/// # Canonical Form
///
/// `openEHR-EHR-<RM_CLASS>.<concept>.v<version>`
///
/// Example: `openEHR-EHR-COMPOSITION.correspondence.v1`
///
/// # OpenEHR Context
///
/// Archetype IDs serve several critical purposes in openEHR systems:
/// - **Type identification**: Distinguishes which archetype constrains a given RM instance.
/// - **Semantic meaning**: The concept name conveys clinical intent (e.g., "correspondence", "snapshot").
/// - **Versioning**: Multiple versions of the same archetype can coexist (e.g., v1, v2).
/// - **Governance**: The authority indicates who published/maintains the archetype.
///
/// In VPR, archetype IDs are used to identify templates and validate that clinical documents
/// conform to expected structures. For example, EHR_STATUS objects reference the `ehr_status`
/// archetype, while clinical letters reference the `correspondence` archetype.
///
/// # VPR Constraints
///
/// VPR enforces strict validation on archetype IDs to ensure only supported archetypes are used:
/// - **Authority**: Must be `"openEHR"` (case-sensitive).
/// - **RM Package**: Must be `"EHR"` (case-sensitive).
/// - **RM Class**: Must be one of: `STATUS`, `COMPOSITION`, `SECTION`, or `EVALUATION`.
/// - **Concept**: Must be one of: `ehr_status`, `correspondence`, or `snapshot`.
/// - **Version**: Must be `1`.
///
/// These constraints reflect VPR's current supported archetype set and prevent invalid or
/// unsupported archetypes from being processed.
///
/// # Implementation Notes
///
/// - The type implements custom `Serialize`/`Deserialize` to handle string conversion transparently.
/// - When serialized, the archetype ID is represented as a single string in canonical form.
/// - When deserializing, the string is parsed and validated, rejecting malformed or unsupported IDs.
/// - The `Display` trait provides `.to_string()` for converting back to the canonical form.
/// - All validation is performed via the private `validate_components()` helper to ensure consistency.
///
/// # Examples
///
/// ```rust
/// # use openehr::data_types::ArchetypeId;
/// // Parse from string
/// let id = ArchetypeId::parse("openEHR-EHR-COMPOSITION.correspondence.v1")?;
///
/// // Create from components
/// let id = ArchetypeId::new("openEHR", "EHR", "COMPOSITION", "correspondence", 1)?;
///
/// // Convert back to string
/// let canonical = id.to_string(); // "openEHR-EHR-COMPOSITION.correspondence.v1"
///
/// // Check RM class
/// assert!(id.is_composition());
/// # Ok::<(), openehr::OpenEhrError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchetypeId {
    /// Archetype authority (e.g. "openEHR")
    pub authority: String,

    /// Reference Model package (e.g. "EHR")
    pub rm_package: String,

    /// Reference Model class (e.g. "COMPOSITION", "EVALUATION")
    pub rm_class: String,

    /// Archetype concept (e.g. "correspondence", "snapshot")
    pub concept: String,

    /// Archetype version number (e.g. 1)
    pub version: u32,
}

impl ArchetypeId {
    /// Validates archetype ID components against permitted values.
    ///
    /// This is a private helper function that enforces strict constraints on each component
    /// of an archetype identifier to ensure only valid archetypes are constructed.
    ///
    /// # Arguments
    ///
    /// - `authority`: The archetype authority (must be `"openEHR"`).
    /// - `rm_package`: The Reference Model package (must be `"EHR"`).
    /// - `rm_class`: The Reference Model class (must be `STATUS`, `COMPOSITION`, `SECTION`, or `EVALUATION`).
    /// - `concept`: The archetype concept (must be `ehr_status`, `correspondence`, `snapshot`, or `clinical_correspondence`).
    /// - `version`: The archetype version number (must be `1`).
    ///
    /// # Errors
    ///
    /// Returns [`OpenEhrError::InvalidArchetypeId`] if:
    /// - Authority is not exactly `"openEHR"`.
    /// - RM package is not exactly `"EHR"`.
    /// - RM class is not one of the permitted values (`STATUS`, `COMPOSITION`, `SECTION`, `EVALUATION`).
    /// - Concept is not one of the permitted values (`ehr_status`, `correspondence`, `snapshot`, `clinical_correspondence`).
    /// - Version is not exactly `1`.
    fn validate_components(
        authority: &str,
        rm_package: &str,
        rm_class: &str,
        concept: &str,
        version: u32,
    ) -> Result<(), OpenEhrError> {
        // Validate authority must be "openEHR"
        if authority != "openEHR" {
            return Err(OpenEhrError::InvalidArchetypeId(format!(
                "authority must be 'openEHR', got '{}'",
                authority
            )));
        }

        // Validate rm_package must be "EHR"
        if rm_package != "EHR" {
            return Err(OpenEhrError::InvalidArchetypeId(format!(
                "rm_package must be 'EHR', got '{}'",
                rm_package
            )));
        }

        // Validate rm_class must be one of: STATUS, COMPOSITION, SECTION, EVALUATION
        if !matches!(
            rm_class,
            "STATUS" | "COMPOSITION" | "SECTION" | "EVALUATION"
        ) {
            return Err(OpenEhrError::InvalidArchetypeId(format!(
                "rm_class must be STATUS, COMPOSITION, SECTION, or EVALUATION, got '{}'",
                rm_class
            )));
        }

        // Validate concept must be one of: ehr_status, correspondence, snapshot, clinical_correspondence
        if !matches!(
            concept,
            "ehr_status" | "correspondence" | "snapshot" | "clinical_correspondence"
        ) {
            return Err(OpenEhrError::InvalidArchetypeId(format!(
                "concept must be ehr_status, correspondence, snapshot, or clinical_correspondence, got '{}'",
                concept
            )));
        }

        // Validate version must be 1
        if version != 1 {
            return Err(OpenEhrError::InvalidArchetypeId(format!(
                "version must be 1, got {}",
                version
            )));
        }

        Ok(())
    }

    /// Creates a new `ArchetypeId` from individual components with validation.
    ///
    /// # Arguments
    ///
    /// - `authority`: Must be exactly `"openEHR"`.
    /// - `rm_package`: Must be exactly `"EHR"`.
    /// - `rm_class`: Must be one of: `STATUS`, `COMPOSITION`, `SECTION`, or `EVALUATION`.
    /// - `concept`: Must be one of: `ehr_status`, `correspondence`, `snapshot`, or `clinical_correspondence`.
    /// - `version`: Must be `1`.
    ///
    /// # Returns
    ///
    /// A validated `ArchetypeId` if all constraints are met.
    ///
    /// # Errors
    ///
    /// Returns [`OpenEhrError::InvalidArchetypeId`] if any parameter fails validation.
    pub fn new(
        authority: &str,
        rm_package: &str,
        rm_class: &str,
        concept: &str,
        version: u32,
    ) -> Result<Self, OpenEhrError> {
        Self::validate_components(authority, rm_package, rm_class, concept, version)?;

        Ok(Self {
            authority: authority.to_string(),
            rm_package: rm_package.to_string(),
            rm_class: rm_class.to_string(),
            concept: concept.to_string(),
            version,
        })
    }

    /// Parses and validates an openEHR archetype identifier string.
    ///
    /// # Arguments
    ///
    /// - `raw`: A string in the canonical form `openEHR-EHR-<RM_CLASS>.<concept>.v<version>`.
    ///
    /// # Returns
    ///
    /// A validated `ArchetypeId` if the string matches the expected format and all components
    /// pass validation.
    ///
    /// # Errors
    ///
    /// Returns [`OpenEhrError::InvalidArchetypeId`] if:
    /// - The string format is invalid (missing delimiters or incorrect structure).
    /// - Authority is not `"openEHR"`.
    /// - RM package is not `"EHR"`.
    /// - RM class is not one of: `STATUS`, `COMPOSITION`, `SECTION`, or `EVALUATION`.
    /// - Concept is not one of: `ehr_status`, `correspondence`, or `snapshot`.
    /// - Version is not `1` or cannot be parsed as a number.
    pub fn parse(raw: &str) -> Result<Self, OpenEhrError> {
        // Split authority and remainder
        let (authority, rest) = raw
            .split_once('-')
            .ok_or_else(|| OpenEhrError::InvalidArchetypeId(raw.to_string()))?;

        // Expect EHR-<RM_CLASS>.<concept>.v<version>
        let (rm_package, remainder) = rest
            .split_once('-')
            .ok_or_else(|| OpenEhrError::InvalidArchetypeId(raw.to_string()))?;

        let (rm_class, remainder) = remainder
            .split_once('.')
            .ok_or_else(|| OpenEhrError::InvalidArchetypeId(raw.to_string()))?;

        let (concept, version_part) = remainder
            .rsplit_once(".v")
            .ok_or_else(|| OpenEhrError::InvalidArchetypeId(raw.to_string()))?;

        let version = version_part
            .parse::<u32>()
            .map_err(|_| OpenEhrError::InvalidArchetypeId(raw.to_string()))?;

        Self::validate_components(authority, rm_package, rm_class, concept, version)?;

        Ok(Self {
            authority: authority.to_string(),
            rm_package: rm_package.to_string(),
            rm_class: rm_class.to_string(),
            concept: concept.to_string(),
            version,
        })
    }

    /// Returns `true` if this archetype targets the `COMPOSITION` RM class.
    pub fn is_composition(&self) -> bool {
        self.rm_class == "COMPOSITION"
    }

    /// Returns `true` if this archetype targets the `EVALUATION` RM class.
    pub fn is_evaluation(&self) -> bool {
        self.rm_class == "EVALUATION"
    }
}

impl fmt::Display for ArchetypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}-{}.{}.v{}",
            self.authority, self.rm_package, self.rm_class, self.concept, self.version
        )
    }
}

impl Serialize for ArchetypeId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ArchetypeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(serde::de::Error::custom)
    }
}
