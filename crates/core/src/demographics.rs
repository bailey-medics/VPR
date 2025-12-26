use crate::Author;
use git2;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use uuid::Uuid;

use crate::{PatientError, PatientResult};

#[derive(Serialize, Deserialize)]
struct Patient {
    resource_type: String,
    id: String,
    name: Vec<Name>,
    birth_date: String,
}

#[derive(Serialize, Deserialize)]
struct Name {
    #[serde(rename = "use")]
    use_: String,
    family: String,
    given: Vec<String>,
}

pub fn initialise_demographics(author: Author) -> PatientResult<String> {
    // Determine storage directory from environment
    let base = std::env::var("PATIENT_DATA_DIR").unwrap_or_else(|_| "/patient_data".into());
    let data_dir = Path::new(&base);
    fs::create_dir_all(data_dir).map_err(PatientError::StorageDirCreation)?;

    // Generate uuid and a 32-hex form without hyphens for directory naming
    let raw_uuid = Uuid::new_v4().to_string();
    let id = raw_uuid.replace('-', "");

    // Create initial patient JSON
    let patient = Patient {
        resource_type: "Patient".to_string(),
        id: id.clone(),
        name: vec![],               // Empty initially
        birth_date: "".to_string(), // Empty initially
    };
    let json = serde_json::to_string_pretty(&patient).map_err(PatientError::Serialization)?;

    // Create the demographics directory
    let demographics_dir = data_dir.join("demographics");
    fs::create_dir_all(&demographics_dir).map_err(PatientError::StorageDirCreation)?;

    // Shard into two-level hex dirs from first 4 chars of the 32-char id within demographics
    let id_lower = id.to_lowercase();
    let s1 = &id_lower[0..2];
    let s2 = &id_lower[2..4];
    let patient_dir = demographics_dir.join(s1).join(s2).join(&id_lower);
    fs::create_dir_all(&patient_dir).map_err(PatientError::PatientDirCreation)?;

    let filename = patient_dir.join("patient.json");
    fs::write(&filename, json).map_err(PatientError::FileWrite)?;

    // Initialize Git repository for the patient
    let repo = git2::Repository::init(&patient_dir).map_err(PatientError::GitInit)?;

    // Create initial commit with demographics.json
    let mut index = repo.index().map_err(PatientError::GitIndex)?;
    index
        .add_path(std::path::Path::new("patient.json"))
        .map_err(PatientError::GitAdd)?;

    let tree_id = index.write_tree().map_err(PatientError::GitWriteTree)?;
    let tree = repo.find_tree(tree_id).map_err(PatientError::GitFindTree)?;

    let sig =
        git2::Signature::now(&author.name, &author.email).map_err(PatientError::GitSignature)?;
    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        "Initial patient record",
        &tree,
        &[],
    )
    .map_err(PatientError::GitCommit)?;

    Ok(id)
}

pub fn update_demographics(
    demographics_uuid: &str,
    given_names: Vec<String>,
    last_name: &str,
    birth_date: &str,
) -> PatientResult<()> {
    let base = std::env::var("PATIENT_DATA_DIR").unwrap_or_else(|_| "/patient_data".into());
    let data_dir = Path::new(&base);
    let demographics_dir = data_dir.join("demographics");

    let id_lower = demographics_uuid.to_lowercase();
    let s1 = &id_lower[0..2];
    let s2 = &id_lower[2..4];
    let patient_dir = demographics_dir.join(s1).join(s2).join(&id_lower);
    let filename = patient_dir.join("patient.json");

    // Read existing patient.json
    let existing_json = fs::read_to_string(&filename).map_err(PatientError::FileRead)?;
    let mut patient: Patient =
        serde_json::from_str(&existing_json).map_err(PatientError::Deserialization)?;

    // Update only the specified fields
    patient.name = vec![Name {
        use_: "official".to_string(),
        family: last_name.to_string(),
        given: given_names,
    }];
    patient.birth_date = birth_date.to_string();

    // Write back the updated JSON
    let json = serde_json::to_string_pretty(&patient).map_err(PatientError::Serialization)?;
    fs::write(&filename, json).map_err(PatientError::FileWrite)?;

    Ok(())
}
