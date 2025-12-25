use crate::Author;
use chrono::Utc;
use git2;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use uuid::Uuid;

use crate::{PatientError, PatientResult};

#[derive(Serialize, Deserialize)]
struct Clinical {
    created_at: String,
    // Add other clinical fields as needed, e.g., notes, etc.
}

pub fn initialise_clinical(author: Author) -> PatientResult<String> {
    // Determine storage directory from environment
    let base = std::env::var("PATIENT_DATA_DIR").unwrap_or_else(|_| "/patient_data".into());
    let data_dir = Path::new(&base);
    fs::create_dir_all(data_dir).map_err(PatientError::StorageDirCreation)?;

    // Generate uuid and a 32-hex form without hyphens for directory naming
    let raw_uuid = Uuid::new_v4().to_string();
    let id = raw_uuid.replace('-', "");
    let created_at = Utc::now().to_rfc3339();

    let clinical = Clinical {
        created_at: created_at.clone(),
    };

    // Create the clinical directory
    let clinical_dir = data_dir.join("clinical");
    fs::create_dir_all(&clinical_dir).map_err(PatientError::StorageDirCreation)?;

    // Shard into two-level hex dirs from first 4 chars of the 32-char id within clinical
    let id_lower = id.to_lowercase();
    let s1 = &id_lower[0..2];
    let s2 = &id_lower[2..4];
    let patient_dir = clinical_dir.join(s1).join(s2).join(&id_lower);
    fs::create_dir_all(&patient_dir).map_err(PatientError::PatientDirCreation)?;

    let filename = patient_dir.join("clinical.json");
    let json = serde_json::to_string_pretty(&clinical).map_err(PatientError::Serialization)?;
    fs::write(&filename, json).map_err(PatientError::FileWrite)?;

    // Initialize Git repository for the patient
    let repo = git2::Repository::init(&patient_dir).map_err(PatientError::GitInit)?;

    // Create initial commit with clinical.json
    let mut index = repo.index().map_err(PatientError::GitIndex)?;
    index
        .add_path(std::path::Path::new("clinical.json"))
        .map_err(PatientError::GitAdd)?;

    let tree_id = index.write_tree().map_err(PatientError::GitWriteTree)?;
    let tree = repo.find_tree(tree_id).map_err(PatientError::GitFindTree)?;

    let sig =
        git2::Signature::now(&author.name, &author.email).map_err(PatientError::GitSignature)?;
    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        "Initial clinical record",
        &tree,
        &[],
    )
    .map_err(PatientError::GitCommit)?;

    Ok(id)
}
