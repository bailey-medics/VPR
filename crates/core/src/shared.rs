//! Shared utilities and types for the VPR core.
//!
//! This module contains common functionality that spans multiple domains,
//! such as full record initialization combining demographics and clinical data.

use crate::Author;
use crate::PatientResult;

/// Represents a complete patient record with both demographics and clinical components.
#[derive(Debug)]
pub struct FullRecord {
    /// The UUID of the demographics record.
    pub demographics_uuid: String,
    /// The UUID of the clinical record.
    pub clinical_uuid: String,
}

/// Initializes a complete patient record with demographics and clinical components.
///
/// This function creates both a demographics repository and a clinical repository,
/// links them together, and populates the demographics with the provided patient information.
///
/// # Arguments
///
/// * `author` - The author information for Git commits.
/// * `given_names` - A vector of the patient's given names.
/// * `last_name` - The patient's family/last name.
/// * `birth_date` - The patient's date of birth as a string (e.g., "YYYY-MM-DD").
/// * `namespace` - Optional namespace for the clinical-demographics link.
///
/// # Returns
///
/// Returns a `FullRecord` containing both UUIDs on success.
///
/// # Errors
///
/// Returns a `PatientError` if any step in the initialization fails.
pub fn initialise_full_record(
    author: Author,
    given_names: Vec<String>,
    last_name: String,
    birth_date: String,
    namespace: Option<String>,
) -> PatientResult<FullRecord> {
    // Initialize demographics
    let demographics_uuid = crate::demographics::initialise_demographics(author.clone())?;

    // Update demographics with patient information
    crate::demographics::update_demographics(
        &demographics_uuid,
        given_names,
        &last_name,
        &birth_date,
    )?;

    // Initialize clinical
    let clinical_uuid = crate::clinical::initialise_clinical(author)?;

    // Link clinical to demographics
    crate::clinical::link_to_demographics(&clinical_uuid, &demographics_uuid, namespace)?;

    Ok(FullRecord {
        demographics_uuid,
        clinical_uuid,
    })
}
