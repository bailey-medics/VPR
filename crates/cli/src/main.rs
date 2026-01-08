//! VPR Command Line Interface
//!
//! This module provides a CLI for interacting with the VPR patient record system.
//! It allows users to initialise patient demographics and clinical records,
//! update demographics, link clinical records to demographics, and list patients.

use clap::{Parser, Subcommand};
use vpr_certificates::Certificate;
use vpr_core::{
    clinical::ClinicalService,
    config::{
        resolve_ehr_template_dir, rm_system_version_from_env_value,
        validate_ehr_template_dir_safe_to_copy,
    },
    constants,
    demographics::DemographicsService,
    Author, AuthorRegistration, CoreConfig, PatientService,
};

use base64::{engine::general_purpose, Engine as _};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
///
/// This struct defines the CLI structure using clap for parsing command line arguments.
#[derive(Parser)]
#[command(name = "vpr")]
#[command(about = "VPR patient record system CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

/// Available CLI commands for the VPR system.
///
/// Each variant represents a different operation that can be performed
/// on patient records, from initialisation to updates and queries.
#[derive(Subcommand)]
enum Commands {
    /// List all patients
    List,
    /// Initialise demographics: <name> <email> --role <role> --care-location <care_location> [--signature <ecdsa_private_key_pem>]
    InitialiseDemographics {
        /// Author name for Git commit
        name: String,
        /// Mandatory author role for commit metadata
        #[arg(long)]
        role: String,
        /// Author email for Git commit
        email: String,
        /// Declared professional registrations (repeatable): --registration <AUTHORITY> <NUMBER>
        #[arg(long, value_names = ["AUTHORITY", "NUMBER"], num_args = 2, action = clap::ArgAction::Append)]
        registration: Vec<String>,
        /// Mandatory organisational location for the commit (e.g. hospital name, GP surgery)
        #[arg(long)]
        care_location: String,
        /// ECDSA private key PEM for X.509 signing (optional, can be PEM string, base64-encoded PEM, or file path)
        #[arg(long)]
        signature: Option<String>,
    },
    /// Initialise clinical: <name> <email> --role <role> --care-location <care_location> [--signature <ecdsa_private_key_pem>]
    InitialiseClinical {
        /// Author name for Git commit
        name: String,
        /// Mandatory author role for commit metadata
        #[arg(long)]
        role: String,
        /// Author email for Git commit
        email: String,
        /// Declared professional registrations (repeatable): --registration <AUTHORITY> <NUMBER>
        #[arg(long, value_names = ["AUTHORITY", "NUMBER"], num_args = 2, action = clap::ArgAction::Append)]
        registration: Vec<String>,
        /// Mandatory organisational location for the commit (e.g. hospital name, GP surgery)
        #[arg(long)]
        care_location: String,
        /// ECDSA private key PEM for X.509 signing (optional, can be PEM string, base64-encoded PEM, or file path)
        #[arg(long)]
        signature: Option<String>,
    },
    /// Write EHR status (and commit):
    /// <clinical_uuid> <demographics_uuid> <name> <email> --role <role> --care-location <care_location>
    /// [--signature <ecdsa_private_key_pem>] [--namespace <namespace>]
    WriteEhrStatus {
        /// Clinical repository UUID
        clinical_uuid: String,
        /// Demographics UUID
        demographics_uuid: String,
        /// Author name for Git commit
        name: String,
        /// Author email for Git commit
        email: String,
        /// Mandatory author role for commit metadata
        #[arg(long)]
        role: String,
        /// Declared professional registrations (repeatable): --registration <AUTHORITY> <NUMBER>
        #[arg(long, value_names = ["AUTHORITY", "NUMBER"], num_args = 2, action = clap::ArgAction::Append)]
        registration: Vec<String>,
        /// Mandatory organisational location for the commit (e.g. hospital name, GP surgery)
        #[arg(long)]
        care_location: String,
        /// ECDSA private key PEM for X.509 signing (optional, can be PEM string, base64-encoded PEM, or file path)
        #[arg(long)]
        signature: Option<String>,
        /// Organisation domain (optional)
        #[arg(long)]
        namespace: Option<String>,
    },
    /// Update demographics: <demographics_uuid> <given_names> <last_name> <birth_date>
    UpdateDemographics {
        /// Demographics UUID
        demographics_uuid: String,
        /// Given names (comma-separated)
        given_names: String,
        /// Last name
        last_name: String,
        /// Date of birth (YYYY-MM-DD)
        birth_date: String,
    },
    /// Initialise full record:
    ///
    /// <given_names> <last_name> <birth_date>
    /// <author_name> <author_email>
    /// <author_role>
    /// <care_location>
    /// [--author-registration "AUTHORITY NUMBER" ...]
    /// [--signature <ecdsa_private_key_pem>]
    /// [--namespace <namespace>]
    InitialiseFullRecord {
        /// Given names (comma-separated)
        given_names: String,
        /// Last name
        last_name: String,
        /// Date of birth (YYYY-MM-DD)
        birth_date: String,
        /// Author name for Git commit
        author_name: String,
        /// Author email for Git commit
        author_email: String,
        /// Mandatory author role for commit metadata
        author_role: String,
        /// Mandatory organisational location for the commit (e.g. hospital name, GP surgery)
        care_location: String,
        /// Declared professional registrations (repeatable): --author-registration "AUTHORITY NUMBER"
        #[arg(
            long = "author-registration",
            action = clap::ArgAction::Append,
            value_name = "AUTHORITY NUMBER",
        )]
        author_registration: Vec<String>,
        /// ECDSA private key PEM for X.509 signing (optional, can be PEM string, base64-encoded PEM, or file path)
        #[arg(long)]
        signature: Option<String>,
        /// Organisation domain (optional)
        #[arg(long)]
        namespace: Option<String>,
    },
    /// Verify the signature on the latest clinical commit: <clinical_uuid> <public_key>
    ///
    /// The public key can be:
    /// - PEM text (contains "-----BEGIN")
    /// - a file path to a PEM file
    /// - base64-encoded PEM
    VerifyClinicalCommitSignature {
        /// Clinical repository UUID
        clinical_uuid: String,
        /// ECDSA P-256 public key (PEM string, base64-encoded PEM, or file path)
        public_key: String,
    },
    /// Create a professional registration certificate: <name> <registration_authority> <registration_number> [--cert-out <cert_file>] [--key-out <key_file>]
    ///
    /// The generated X.509 Subject includes:
    /// - `CN` = name
    /// - `O` = registration authority
    /// - `serialNumber` = registration number
    ///
    /// A `subjectAltName` URI is also added: `vpr://<authority>/<number>`.
    CreateCertificate {
        /// Full name of the person
        name: String,
        /// Registration authority (e.g., GMC, NMC). Populates X.509 Subject `O`.
        registration_authority: String,
        /// Registration number. Populates X.509 Subject `serialNumber`.
        registration_number: String,
        /// Output file for the certificate (optional, prints to stdout if not specified)
        #[arg(long)]
        cert_out: Option<String>,
        /// Output file for the private key (optional, prints to stdout if not specified)
        #[arg(long)]
        key_out: Option<String>,
    },

    /// Delete ALL patient data under patient_data (DEV only).
    ///
    /// Deletes both:
    /// - `<PATIENT_DATA_DIR>/clinical`
    /// - `<PATIENT_DATA_DIR>/demographics`
    ///
    /// Refuses to run unless `DEV_ENV=true`.
    DeleteAllData,
}

#[derive(Debug)]
struct CliError(String);

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for CliError {}

fn is_dev_env() -> bool {
    let value = match std::env::var("DEV_ENV") {
        Ok(v) => v,
        Err(_) => return false,
    };

    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "true" | "1" | "yes" | "y"
    )
}

fn patient_data_dir() -> PathBuf {
    std::env::var("PATIENT_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(constants::DEFAULT_PATIENT_DATA_DIR))
}

fn confirm_delete_all_data(base_dir: &Path) -> Result<bool, io::Error> {
    eprintln!("WARNING: This will permanently delete ALL patient data.");
    eprintln!("It will remove all contents within:");
    eprintln!(
        "- {}",
        base_dir.join(constants::CLINICAL_DIR_NAME).display()
    );
    eprintln!(
        "- {}",
        base_dir.join(constants::DEMOGRAPHICS_DIR_NAME).display()
    );
    eprint!("Are you sure you wish to proceed? (y/N): ");
    io::stderr().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let answer = input.trim().to_ascii_lowercase();
    Ok(answer == "y" || answer == "yes")
}

fn clear_dir_contents(path: &Path) -> Result<(), io::Error> {
    std::fs::create_dir_all(path)?;

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            std::fs::remove_dir_all(&entry_path)?;
        } else {
            std::fs::remove_file(&entry_path)?;
        }
    }

    Ok(())
}

/// Main entry point for the VPR CLI.
///
/// Parses command line arguments and executes the appropriate command.
/// Handles initialisation of demographics and clinical records, updates,
/// linking operations, and patient listing.
///
/// # Returns
///
/// Returns `Ok(())` on successful execution, or an error if something fails.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let cfg = build_core_config_from_env()?;

    match cli.command {
        Some(Commands::List) => {
            let service = DemographicsService::new(cfg.clone());
            let patients = service.list_patients();
            if patients.is_empty() {
                println!("No patients found.");
            } else {
                for patient in patients {
                    println!(
                        "ID: {}, Name: {} {}, Created: {}",
                        patient.id, patient.first_name, patient.last_name, patient.created_at
                    );
                }
            }
        }
        Some(Commands::InitialiseDemographics {
            name,
            role,
            email,
            registration,
            care_location,
            signature,
        }) => {
            let registrations: Vec<AuthorRegistration> = registration
                .chunks(2)
                .map(|chunk| AuthorRegistration {
                    authority: chunk.first().cloned().unwrap_or_default(),
                    number: chunk.get(1).cloned().unwrap_or_default(),
                })
                .collect();
            let author = Author {
                name,
                role,
                email,
                registrations,
                signature,
                certificate: None,
            };
            let demographics_service = DemographicsService::new(cfg.clone());
            match demographics_service.initialise(author, care_location) {
                Ok(uuid) => println!("Initialised demographics with UUID: {}", uuid),
                Err(e) => eprintln!("Error initialising demographics: {}", e),
            }
        }
        Some(Commands::InitialiseClinical {
            name,
            role,
            email,
            registration,
            care_location,
            signature,
        }) => {
            let registrations: Vec<AuthorRegistration> = registration
                .chunks(2)
                .map(|chunk| AuthorRegistration {
                    authority: chunk.first().cloned().unwrap_or_default(),
                    number: chunk.get(1).cloned().unwrap_or_default(),
                })
                .collect();
            let author = Author {
                name,
                role,
                email,
                registrations,
                signature,
                certificate: None,
            };
            let clinical_service = ClinicalService::new(cfg.clone());
            match clinical_service.initialise(author, care_location) {
                Ok(uuid) => println!("Initialised clinical with UUID: {}", uuid.simple()),
                Err(e) => eprintln!("Error initialising clinical: {}", e),
            }
        }
        Some(Commands::WriteEhrStatus {
            clinical_uuid,
            demographics_uuid,
            name,
            email,
            role,
            registration,
            care_location,
            signature,
            namespace,
        }) => {
            let clinical_service = ClinicalService::new(cfg.clone());
            let registrations: Vec<AuthorRegistration> = registration
                .chunks(2)
                .map(|chunk| AuthorRegistration {
                    authority: chunk.first().cloned().unwrap_or_default(),
                    number: chunk.get(1).cloned().unwrap_or_default(),
                })
                .collect();
            let author = Author {
                name,
                role,
                email,
                registrations,
                signature,
                certificate: None,
            };
            match clinical_service.link_to_demographics(
                &author,
                care_location,
                &clinical_uuid,
                &demographics_uuid,
                namespace,
            ) {
                Ok(()) => println!("Wrote EHR status for clinical UUID: {}", clinical_uuid),
                Err(e) => eprintln!("Error writing EHR status: {}", e),
            }
        }
        Some(Commands::UpdateDemographics {
            demographics_uuid,
            given_names,
            last_name,
            birth_date,
        }) => {
            let given_names_vec: Vec<String> = given_names
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
            let demographics_service = DemographicsService::new(cfg.clone());
            match demographics_service.update(
                &demographics_uuid,
                given_names_vec,
                &last_name,
                &birth_date,
            ) {
                Ok(()) => println!("Updated demographics for UUID: {}", demographics_uuid),
                Err(e) => eprintln!("Error updating demographics: {}", e),
            }
        }
        Some(Commands::InitialiseFullRecord {
            given_names,
            last_name,
            birth_date,
            author_name,
            author_email,
            author_role,
            care_location,
            author_registration,
            signature,
            namespace,
        }) => {
            let mut registrations: Vec<AuthorRegistration> = Vec::new();
            for reg in author_registration {
                let mut parts = reg.split_whitespace();
                let authority = parts.next().unwrap_or("");
                let number = parts.next().unwrap_or("");

                if authority.is_empty() || number.is_empty() || parts.next().is_some() {
                    return Err(format!(
                        "Invalid --author-registration value (expected \"AUTHORITY NUMBER\"): {}",
                        reg
                    )
                    .into());
                }

                registrations.push(AuthorRegistration {
                    authority: authority.to_string(),
                    number: number.to_string(),
                });
            }

            let author = Author {
                name: author_name,
                role: author_role,
                email: author_email,
                registrations,
                signature,
                certificate: None,
            };
            let given_names_vec: Vec<String> = given_names
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
            let service = PatientService::new(cfg.clone());
            match service.initialise_full_record(
                author,
                care_location,
                given_names_vec,
                last_name,
                birth_date,
                namespace,
            ) {
                Ok(record) => println!(
                    "Initialised full record - Demographics UUID: {}, Clinical UUID: {}",
                    record.demographics_uuid, record.clinical_uuid
                ),
                Err(e) => eprintln!("Error initialising full record: {}", e),
            }
        }
        Some(Commands::VerifyClinicalCommitSignature {
            clinical_uuid,
            public_key,
        }) => {
            let public_key_pem = if public_key.contains("-----BEGIN") {
                public_key
            } else if std::path::Path::new(&public_key).exists() {
                match std::fs::read_to_string(&public_key) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Error reading public key file {}: {}", public_key, e);
                        return Ok(());
                    }
                }
            } else {
                match general_purpose::STANDARD.decode(&public_key) {
                    Ok(bytes) => match String::from_utf8(bytes) {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("Error decoding base64 public key to UTF-8: {}", e);
                            return Ok(());
                        }
                    },
                    Err(e) => {
                        eprintln!(
                            "Public key must be PEM, a readable file path, or base64-encoded PEM: {}",
                            e
                        );
                        return Ok(());
                    }
                }
            };

            let clinical_service = ClinicalService::new(cfg.clone());
            match clinical_service.verify_commit_signature(&clinical_uuid, &public_key_pem) {
                Ok(true) => println!("Signature VALID for clinical UUID: {}", clinical_uuid),
                Ok(false) => println!("Signature INVALID for clinical UUID: {}", clinical_uuid),
                Err(e) => eprintln!("Error verifying signature: {}", e),
            }
        }
        Some(Commands::CreateCertificate {
            name,
            registration_authority,
            registration_number,
            cert_out,
            key_out,
        }) => match Certificate::create(&name, &registration_authority, &registration_number) {
            Ok((cert_pem, key_pem)) => {
                if let Some(cert_file) = cert_out {
                    if let Err(e) = std::fs::write(&cert_file, &cert_pem) {
                        eprintln!("Error writing certificate to {}: {}", cert_file, e);
                        return Ok(());
                    }
                    println!("Certificate written to {}", cert_file);
                } else {
                    println!("--- Certificate ---");
                    println!("{}", cert_pem);
                }

                if let Some(key_file) = key_out {
                    if let Err(e) = std::fs::write(&key_file, &key_pem) {
                        eprintln!("Error writing private key to {}: {}", key_file, e);
                        return Ok(());
                    }
                    println!("Private key written to {}", key_file);
                } else {
                    println!("--- Private Key ---");
                    println!("{}", key_pem);
                }
            }
            Err(e) => eprintln!("Error creating certificate: {}", e),
        },
        Some(Commands::DeleteAllData) => {
            if !is_dev_env() {
                return Err(Box::new(CliError(
                    "Refusing to delete data: DEV_ENV=true is required".to_string(),
                )));
            }

            let base_dir = patient_data_dir();
            if !confirm_delete_all_data(&base_dir)? {
                eprintln!("Aborted.");
                return Ok(());
            }

            clear_dir_contents(&base_dir.join(constants::CLINICAL_DIR_NAME))?;
            clear_dir_contents(&base_dir.join(constants::DEMOGRAPHICS_DIR_NAME))?;

            println!(
                "Deleted all patient data under {}",
                base_dir.to_string_lossy()
            );
        }
        None => {
            println!("Use 'vpr --help' for commands");
        }
    }

    Ok(())
}

fn build_core_config_from_env() -> Result<Arc<CoreConfig>, Box<dyn std::error::Error>> {
    let patient_data_dir = std::env::var("PATIENT_DATA_DIR")
        .unwrap_or_else(|_| vpr_core::DEFAULT_PATIENT_DATA_DIR.into());
    let patient_data_path = Path::new(&patient_data_dir);
    if !patient_data_path.exists() {
        return Err(format!(
            "Patient data directory does not exist: {}",
            patient_data_path.display()
        )
        .into());
    }

    let template_override = std::env::var("VPR_EHR_TEMPLATE_DIR")
        .ok()
        .map(PathBuf::from);
    let ehr_template_dir = resolve_ehr_template_dir(template_override)?;
    validate_ehr_template_dir_safe_to_copy(&ehr_template_dir)?;

    let rm_system_version =
        rm_system_version_from_env_value(std::env::var("RM_SYSTEM_VERSION").ok())?;
    let vpr_namespace = std::env::var("VPR_NAMESPACE").unwrap_or_else(|_| "vpr.dev.1".into());

    Ok(Arc::new(CoreConfig::new(
        patient_data_path.to_path_buf(),
        ehr_template_dir,
        rm_system_version,
        vpr_namespace,
    )?))
}
