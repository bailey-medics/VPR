//! Coordination status file path constants.
//!
//! This module defines the filename for the coordination status YAML file.

/// Coordination status filename.
pub struct CoordinationStatusFile;

impl CoordinationStatusFile {
    pub const NAME: &'static str = "COORDINATION_STATUS.yaml";
}
