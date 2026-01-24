//! Patient file path constants.
//!
//! This module defines the filename for the patient YAML file.

/// Patient YAML filename.
pub struct PatientFile;

impl PatientFile {
    pub const NAME: &'static str = "patient.yaml";
}
