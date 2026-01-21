//! FHIR-aligned coordination status wire models and translation helpers.
//!
//! This module provides both domain-level types and wire models for coordination status,
//! which tracks the lifecycle state and permissions of a coordination record.
//!
//! Responsibilities:
//! - Define public domain-level types for external API use
//! - Define a strict wire model for serialisation/deserialisation
//! - Provide translation helpers between domain primitives and the wire model
//! - Validate coordination status structure and enforce required fields
//!
//! Notes:
//! - This status file is mutable and overwriteable
//! - Changes should be git-audited where appropriate

use crate::FhirError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================================================
// Public domain-level types
// ============================================================================

/// Domain-level carrier for coordination status data.
///
/// This struct represents the lifecycle state and permissions for a coordination record
/// in a format that is independent of specific wire formats.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoordinationStatusData {
    /// Unique identifier for this coordination record (UUID).
    pub coordination_id: Uuid,

    /// Reference to the associated clinical record (UUID).
    pub clinical_id: Uuid,

    /// Current lifecycle state of the coordination record.
    pub lifecycle_state: LifecycleState,

    /// Whether the record is currently open for new entries.
    pub record_open: bool,

    /// Whether the record can be queried/read.
    pub record_queryable: bool,

    /// Whether the record can be modified.
    pub record_modifiable: bool,
}

/// Lifecycle state enumeration.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LifecycleState {
    /// Record is active and operational.
    Active,
    /// Record is temporarily suspended.
    Suspended,
    /// Record is closed.
    Closed,
}

// ============================================================================
// Public CoordinationStatus operations
// ============================================================================

/// Coordination status operations.
///
/// This is a zero-sized type used for namespacing coordination status-related operations.
/// All methods are associated functions.
pub struct CoordinationStatus;

impl CoordinationStatus {
    /// Parse a coordination status from YAML text.
    ///
    /// This uses `serde_path_to_error` to surface a best-effort "path" (e.g. `status.lifecycle_state`)
    /// to the failing field when the YAML does not match the wire schema.
    ///
    /// # Arguments
    ///
    /// * `yaml_text` - YAML text expected to represent a coordination status mapping.
    ///
    /// # Returns
    ///
    /// Returns a [`CoordinationStatusData`] with domain-level fields extracted from the status.
    ///
    /// # Errors
    ///
    /// Returns [`FhirError`] if:
    /// - the YAML does not represent a valid coordination status,
    /// - any field has an unexpected type,
    /// - any unknown keys are present (due to `#[serde(deny_unknown_fields)]`),
    /// - coordination_id or clinical_id are not valid UUIDs.
    pub fn parse(yaml_text: &str) -> Result<CoordinationStatusData, FhirError> {
        let deserializer = serde_yaml::Deserializer::from_str(yaml_text);

        let wire = match serde_path_to_error::deserialize::<_, CoordinationStatusWire>(deserializer)
        {
            Ok(parsed) => parsed,
            Err(err) => {
                let path = err.path().to_string();
                let source = err.into_inner();
                let path = if path.is_empty() {
                    "<root>"
                } else {
                    path.as_str()
                };
                return Err(FhirError::Translation(format!(
                    "Coordination status schema mismatch at {path}: {source}"
                )));
            }
        };

        // Convert wire format to domain types
        wire_to_domain(wire)
    }

    /// Render a coordination status as YAML text.
    ///
    /// This converts domain-level [`CoordinationStatusData`] into wire format and serializes to YAML.
    ///
    /// # Arguments
    ///
    /// * `data` - Coordination status data containing all fields.
    ///
    /// # Returns
    ///
    /// Returns a YAML string representation of the coordination status.
    ///
    /// # Errors
    ///
    /// Returns [`FhirError`] if serialization fails.
    pub fn render(data: &CoordinationStatusData) -> Result<String, FhirError> {
        let wire: CoordinationStatusWire = domain_to_wire(data);
        serde_yaml::to_string(&wire).map_err(|e| {
            FhirError::Translation(format!("Failed to serialize coordination status: {e}"))
        })
    }
}

// ============================================================================
// Wire types (internal)
// ============================================================================

/// Wire representation of a coordination status for on-disk YAML.
///
/// This is the exact structure that will be serialized to/from YAML.
/// All fields use `#[serde(deny_unknown_fields)]` for strict validation.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct CoordinationStatusWire {
    pub coordination_id: String,
    pub clinical_id: String,
    pub status: StatusWire,
}

/// Wire representation of status information.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StatusWire {
    pub lifecycle_state: LifecycleState,
    pub record_open: bool,
    pub record_queryable: bool,
    pub record_modifiable: bool,
}

// ============================================================================
// Helper functions (internal)
// ============================================================================

/// Convert wire format coordination status to domain types.
///
/// This performs validation and conversion of string identifiers to proper types.
fn wire_to_domain(wire: CoordinationStatusWire) -> Result<CoordinationStatusData, FhirError> {
    // Parse coordination_id as UUID
    let coordination_id = Uuid::parse_str(&wire.coordination_id).map_err(|_| {
        FhirError::InvalidUuid(format!(
            "Invalid UUID in coordination_id: {}",
            wire.coordination_id
        ))
    })?;

    // Parse clinical_id as UUID
    let clinical_id = Uuid::parse_str(&wire.clinical_id).map_err(|_| {
        FhirError::InvalidUuid(format!("Invalid UUID in clinical_id: {}", wire.clinical_id))
    })?;

    Ok(CoordinationStatusData {
        coordination_id,
        clinical_id,
        lifecycle_state: wire.status.lifecycle_state,
        record_open: wire.status.record_open,
        record_queryable: wire.status.record_queryable,
        record_modifiable: wire.status.record_modifiable,
    })
}

/// Convert domain types to wire format coordination status.
fn domain_to_wire(data: &CoordinationStatusData) -> CoordinationStatusWire {
    CoordinationStatusWire {
        coordination_id: data.coordination_id.to_string(),
        clinical_id: data.clinical_id.to_string(),
        status: StatusWire {
            lifecycle_state: data.lifecycle_state.clone(),
            record_open: data.record_open,
            record_queryable: data.record_queryable,
            record_modifiable: data.record_modifiable,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_sample_yaml() {
        let input = r#"coordination_id: "7f4c2e9d-4b0a-4f3a-9a2c-0e9a6b5d1c88"
clinical_id: "a4f91c6d-3b2e-4c5f-9d7a-1e8b6c0a9f12"
status:
  lifecycle_state: active
  record_open: true
  record_queryable: true
  record_modifiable: true
"#;

        let status_data = CoordinationStatus::parse(input).expect("parse yaml");
        let output = CoordinationStatus::render(&status_data).expect("render status");
        let reparsed = CoordinationStatus::parse(&output).expect("reparse yaml");
        assert_eq!(status_data, reparsed);
    }

    #[test]
    fn strict_validation_rejects_unknown_keys() {
        let input = r#"coordination_id: "7f4c2e9d-4b0a-4f3a-9a2c-0e9a6b5d1c88"
clinical_id: "a4f91c6d-3b2e-4c5f-9d7a-1e8b6c0a9f12"
status:
  lifecycle_state: active
  record_open: true
  record_queryable: true
  record_modifiable: true
unexpected_key: should_fail
"#;

        let err = CoordinationStatus::parse(input).expect_err("should reject unknown key");
        match err {
            FhirError::Translation(msg) => {
                assert!(msg.contains("unexpected_key"));
            }
            other => panic!("expected Translation error, got {other:?}"),
        }
    }

    #[test]
    fn strict_validation_rejects_wrong_types() {
        let input = r#"coordination_id: "7f4c2e9d-4b0a-4f3a-9a2c-0e9a6b5d1c88"
clinical_id: "a4f91c6d-3b2e-4c5f-9d7a-1e8b6c0a9f12"
status:
  lifecycle_state: active
  record_open: "not_a_boolean"
  record_queryable: true
  record_modifiable: true
"#;

        let err = CoordinationStatus::parse(input).expect_err("should reject wrong type");
        match err {
            FhirError::Translation(msg) => {
                assert!(msg.contains("record_open"));
            }
            other => panic!("expected Translation error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_coordination_id() {
        let input = r#"coordination_id: "not-a-valid-uuid"
clinical_id: "a4f91c6d-3b2e-4c5f-9d7a-1e8b6c0a9f12"
status:
  lifecycle_state: active
  record_open: true
  record_queryable: true
  record_modifiable: true
"#;

        let err =
            CoordinationStatus::parse(input).expect_err("should reject invalid coordination_id");
        match err {
            FhirError::InvalidUuid(msg) => {
                assert!(msg.contains("coordination_id"));
            }
            other => panic!("expected InvalidUuid error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_clinical_id() {
        let input = r#"coordination_id: "7f4c2e9d-4b0a-4f3a-9a2c-0e9a6b5d1c88"
clinical_id: "not-a-valid-uuid"
status:
  lifecycle_state: active
  record_open: true
  record_queryable: true
  record_modifiable: true
"#;

        let err = CoordinationStatus::parse(input).expect_err("should reject invalid clinical_id");
        match err {
            FhirError::InvalidUuid(msg) => {
                assert!(msg.contains("clinical_id"));
            }
            other => panic!("expected InvalidUuid error, got {other:?}"),
        }
    }

    #[test]
    fn handles_all_lifecycle_states() {
        let base = r#"coordination_id: "7f4c2e9d-4b0a-4f3a-9a2c-0e9a6b5d1c88"
clinical_id: "a4f91c6d-3b2e-4c5f-9d7a-1e8b6c0a9f12"
status:
  lifecycle_state: active
  record_open: true
  record_queryable: true
  record_modifiable: true
"#;

        let active = CoordinationStatus::parse(base).expect("should parse active");
        assert_eq!(active.lifecycle_state, LifecycleState::Active);

        let suspended = base.replace("lifecycle_state: active", "lifecycle_state: suspended");
        let result = CoordinationStatus::parse(&suspended).expect("should parse suspended");
        assert_eq!(result.lifecycle_state, LifecycleState::Suspended);

        let closed = base.replace("lifecycle_state: active", "lifecycle_state: closed");
        let result = CoordinationStatus::parse(&closed).expect("should parse closed");
        assert_eq!(result.lifecycle_state, LifecycleState::Closed);
    }

    #[test]
    fn handles_false_permissions() {
        let input = r#"coordination_id: "7f4c2e9d-4b0a-4f3a-9a2c-0e9a6b5d1c88"
clinical_id: "a4f91c6d-3b2e-4c5f-9d7a-1e8b6c0a9f12"
status:
  lifecycle_state: suspended
  record_open: false
  record_queryable: true
  record_modifiable: false
"#;

        let result = CoordinationStatus::parse(input).expect("should parse mixed permissions");
        assert_eq!(result.lifecycle_state, LifecycleState::Suspended);
        assert!(!result.record_open);
        assert!(result.record_queryable);
        assert!(!result.record_modifiable);
    }

    #[test]
    fn parses_minimal_valid_status() {
        let input = r#"coordination_id: "7f4c2e9d-4b0a-4f3a-9a2c-0e9a6b5d1c88"
clinical_id: "a4f91c6d-3b2e-4c5f-9d7a-1e8b6c0a9f12"
status:
  lifecycle_state: active
  record_open: true
  record_queryable: true
  record_modifiable: true
"#;

        let result = CoordinationStatus::parse(input).expect("should parse minimal status");
        assert_eq!(
            result.coordination_id.to_string(),
            "7f4c2e9d-4b0a-4f3a-9a2c-0e9a6b5d1c88"
        );
        assert_eq!(
            result.clinical_id.to_string(),
            "a4f91c6d-3b2e-4c5f-9d7a-1e8b6c0a9f12"
        );
        assert_eq!(result.lifecycle_state, LifecycleState::Active);
        assert!(result.record_open);
        assert!(result.record_queryable);
        assert!(result.record_modifiable);
    }
}
