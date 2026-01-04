//! Constants used throughout the VPR core crate.
//!
//! This module contains all path and filename constants to ensure
//! consistency across the codebase and make maintenance easier.

/// Directory name for clinical records storage.
pub const CLINICAL_DIR_NAME: &str = "clinical";

/// Default directory for patient data storage if PATIENT_DATA_DIR is not set.
pub const DEFAULT_PATIENT_DATA_DIR: &str = "patient_data";

/// Filename for EHR status YAML files.
pub const EHR_STATUS_FILENAME: &str = "ehr_status.yaml";

/// Directory name for EHR templates.
pub const EHR_TEMPLATE_DIR: &str = "ehr-template";

/// Directory name for demographics records storage.
pub const DEMOGRAPHICS_DIR_NAME: &str = "demographics";

/// Latest supported openEHR RM module version.
pub const LATEST_RM: openehr::RmVersion = openehr::RmVersion::rm_1_1_0;
/// Filename for patient JSON files.
pub const PATIENT_JSON_FILENAME: &str = "patient.json";
