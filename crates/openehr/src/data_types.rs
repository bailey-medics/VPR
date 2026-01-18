//! openEHR RM data types.
//!
//! This module contains wire representations of openEHR RM data types used across
//! multiple RM structures.

use serde::{Deserialize, Serialize};

/// RM `DV_TEXT` (simplified to a `value` string wrapper).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DvText {
    pub value: String,
}
