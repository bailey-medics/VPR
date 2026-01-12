//! openEHR Reference Model (RM) 1.1.0 wire support.
//!
//! This module implements RM 1.1.0 specific wire structs and translation logic for Git-native
//! clinical files on disk.

use crate::RmVersion;

/// The RM version implemented by this module.
pub const CURRENT_RM_VERSION: RmVersion = RmVersion::rm_1_1_0;

pub mod constants;
pub mod ehr_status;
pub mod letter;
