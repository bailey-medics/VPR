//! Constants used throughout the VPR core crate.
//!
//! This module contains all path and filename constants to ensure
//! consistency across the codebase and make maintenance easier.

/// Directory name for clinical records storage.
pub const CLINICAL_DIR_NAME: &str = "clinical";

/// Default directory for patient data storage when no explicit directory is configured.
pub const DEFAULT_PATIENT_DATA_DIR: &str = "patient_data";

/// Directory name for clinical templates.
pub const CLINICAL_TEMPLATE_DIR: &str = "crates/core/templates/clinical";

/// Directory name for demographics records storage.
pub const DEMOGRAPHICS_DIR_NAME: &str = "demographics";

/// Directory name for coordination records storage.
pub const COORDINATION_DIR_NAME: &str = "coordination";

/// Latest supported openEHR RM module version.
pub const LATEST_RM: openehr::RmVersion = openehr::RmVersion::rm_1_1_0;
/// Filename for patient JSON files.
pub const PATIENT_JSON_FILENAME: &str = "patient.json";

/// Filename for coordination thread (message collection).
pub const THREAD_FILENAME: &str = "thread.md";

/// Filename for coordination thread ledger.
pub const THREAD_LEDGER_FILENAME: &str = "ledger.yaml";

/// Filename for coordination status linking to clinical record.
pub const COORDINATION_STATUS_FILENAME: &str = "COORDINATION_STATUS.yaml";
