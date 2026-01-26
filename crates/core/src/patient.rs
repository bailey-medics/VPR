//! Patient service and related types.
//!
//! This module provides the main service for patient operations,
//! including initialising full patient records.

use crate::{
    author::Author, error::PatientResult, repositories::clinical::ClinicalService,
    repositories::coordination::CoordinationService,
    repositories::demographics::DemographicsService, NonEmptyText,
};

/// Represents a complete patient record with both demographics and clinical components.
#[derive(Debug)]
pub struct FullRecord {
    /// The UUID of the demographics record.
    pub demographics_uuid: String,
    /// The UUID of the clinical record.
    pub clinical_uuid: String,
    /// The UUID of the coordination record.
    pub coordination_uuid: String,
}

/// Pure patient data operations - no API concerns
#[derive(Clone)]
pub struct PatientService {
    cfg: std::sync::Arc<crate::CoreConfig>,
}

impl PatientService {
    /// Creates a new instance of PatientService.
    ///
    /// # Returns
    /// A new `PatientService` instance ready to handle patient operations.
    pub fn new(cfg: std::sync::Arc<crate::CoreConfig>) -> Self {
        Self { cfg }
    }

    /// Initialises a complete patient record with demographics and clinical components.
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
    /// Returns a `PatientError` if:
    /// - demographics initialisation or update fails,
    /// - clinical initialisation fails,
    /// - linking clinical to demographics fails.
    pub fn initialise_full_record(
        &self,
        author: Author,
        care_location: NonEmptyText,
        given_names: Vec<String>,
        last_name: String,
        birth_date: String,
        namespace: Option<String>,
    ) -> PatientResult<FullRecord> {
        let demographics_service = DemographicsService::new(self.cfg.clone());
        // Initialise demographics
        let demographics_service =
            demographics_service.initialise(author.clone(), care_location.clone())?;

        // Get the UUID for later use
        let demographics_uuid = demographics_service.demographics_id().to_string();

        // Update demographics with patient information
        demographics_service.update(given_names, &last_name, &birth_date)?;

        // Initialise clinical
        let clinical_service = ClinicalService::new(self.cfg.clone());
        let clinical_service =
            clinical_service.initialise(author.clone(), care_location.clone())?;
        let clinical_uuid = clinical_service.clinical_id();

        // Link clinical to demographics
        clinical_service.link_to_demographics(
            &author,
            care_location.clone(),
            &demographics_uuid,
            namespace,
        )?;

        // Initialise coordination record linked to clinical
        let coordination_service = CoordinationService::new(self.cfg.clone());
        let coordination_service = coordination_service.initialise(
            author,
            care_location.as_str().to_string(),
            clinical_uuid,
        )?;
        let coordination_uuid = coordination_service.coordination_id();

        Ok(FullRecord {
            demographics_uuid,
            clinical_uuid: clinical_uuid.simple().to_string(),
            coordination_uuid: coordination_uuid.to_string(),
        })
    }
}
