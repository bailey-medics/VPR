//! EHR status file path constants.
//!
//! This module defines the filename for the EHR status YAML file.

/// EHR status filename.
pub struct EhrStatusFile;

impl EhrStatusFile {
    pub const NAME: &'static str = "ehr_status.yaml";
}
