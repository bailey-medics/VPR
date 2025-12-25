use crate::Author;
use chrono::Utc;
use git2;
use std::fs;
use std::path::Path;
use uuid::Uuid;

use crate::{PatientError, PatientResult};

pub fn initialise_demographics(author: Author) -> PatientResult<String> {
    // Determine storage directory from environment
    let base = std::env::var("PATIENT_DATA_DIR").unwrap_or_else(|_| "/patient_data".into());
    let data_dir = Path::new(&base);
    fs::create_dir_all(data_dir).map_err(PatientError::StorageDirCreation)?;

    // Generate uuid and a 32-hex form without hyphens for directory naming
    let raw_uuid = Uuid::new_v4().to_string();
    let id = raw_uuid.replace('-', "");
    let created_at = Utc::now().to_rfc3339();

    // Create JSON directly without a struct
    let json = format!(
        r#"{{
  "first_name": "",
  "last_name": "",
  "created_at": "{}",
  "national_id": null
}}"#,
        created_at
    );

    // Create the demographics directory
    let demographics_dir = data_dir.join("demographics");
    fs::create_dir_all(&demographics_dir).map_err(PatientError::StorageDirCreation)?;

    // Shard into two-level hex dirs from first 4 chars of the 32-char id within demographics
    let id_lower = id.to_lowercase();
    let s1 = &id_lower[0..2];
    let s2 = &id_lower[2..4];
    let patient_dir = demographics_dir.join(s1).join(s2).join(&id_lower);
    fs::create_dir_all(&patient_dir).map_err(PatientError::PatientDirCreation)?;

    let filename = patient_dir.join("demographics.json");
    fs::write(&filename, json).map_err(PatientError::FileWrite)?;

    // Initialize Git repository for the patient
    let repo = git2::Repository::init(&patient_dir).map_err(PatientError::GitInit)?;

    // Create initial commit with demographics.json
    let mut index = repo.index().map_err(PatientError::GitIndex)?;
    index
        .add_path(std::path::Path::new("demographics.json"))
        .map_err(PatientError::GitAdd)?;

    let tree_id = index.write_tree().map_err(PatientError::GitWriteTree)?;
    let tree = repo.find_tree(tree_id).map_err(PatientError::GitFindTree)?;

    let sig =
        git2::Signature::now(&author.name, &author.email).map_err(PatientError::GitSignature)?;
    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        "Initial demographics record",
        &tree,
        &[],
    )
    .map_err(PatientError::GitCommit)?;

    Ok(id)
}
