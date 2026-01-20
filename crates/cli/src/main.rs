//! VPR Command Line Interface
//!
//! This module provides a CLI for interacting with the VPR patient record system.
//! It allows users to initialise patient demographics and clinical records,
//! update demographics, link clinical records to demographics, and list patients.

#![allow(rustdoc::invalid_html_tags)]

use clap::{Parser, Subcommand};
use vpr_certificates::Certificate;
use vpr_core::{
    config::rm_system_version_from_env_value,
    constants,
    repositories::clinical::ClinicalService,
    repositories::coordination::{
        CoordinationService, MessageContent, MessageType, ParticipantRole, ThreadParticipant,
    },
    repositories::demographics::DemographicsService,
    repositories::shared::{resolve_clinical_template_dir, validate_template, TemplateDirKind},
    versioned_files::VersionedFileService,
    Author, AuthorRegistration, CoreConfig, PatientService, ShardableUuid,
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

    /// Create a new letter:
    ///
    /// <clinical_uuid> <author_name> <author_email>
    /// --role <author_role>
    /// --care-location <care_location>
    /// --content <letter_content>
    /// [--registration <AUTHORITY> <NUMBER> ...]
    /// [--signature <ecdsa_private_key_pem>]
    NewLetter {
        /// Clinical repository UUID
        clinical_uuid: String,
        /// Author name for Git commit
        author_name: String,
        /// Author email for Git commit
        author_email: String,
        /// Mandatory author role for commit metadata
        #[arg(long)]
        role: String,
        /// Declared professional registrations (repeatable): --registration <AUTHORITY> <NUMBER>
        #[arg(long, value_names = ["AUTHORITY", "NUMBER"], num_args = 2, action = clap::ArgAction::Append)]
        registration: Vec<String>,
        /// Mandatory organisational location for the commit (e.g. hospital name, GP surgery)
        #[arg(long)]
        care_location: String,
        /// Letter content (markdown text)
        #[arg(long)]
        content: String,
        /// ECDSA private key PEM for X.509 signing (optional, can be PEM string, base64-encoded PEM, or file path)
        #[arg(long)]
        signature: Option<String>,
    },

    /// Delete ALL patient data under patient_data (DEV only).
    ///
    /// Deletes both:
    /// - `<PATIENT_DATA_DIR>/clinical`
    /// - `<PATIENT_DATA_DIR>/demographics`
    ///
    /// Refuses to run unless `DEV_ENV=true`.
    DeleteAllData,

    /// Initialise coordination record:
    ///
    /// <author_name> <author_email>
    /// --role <author_role>
    /// --care-location <care_location>
    /// [--registration <AUTHORITY> <NUMBER> ...]
    /// [--signature <ecdsa_private_key_pem>]
    InitialiseCoordination {
        /// Author name for Git commit
        author_name: String,
        /// Author email for Git commit
        author_email: String,
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
    },

    /// Create a messaging thread:
    ///
    /// <coordination_uuid> <author_name> <author_email>
    /// --role <author_role>
    /// --care-location <care_location>
    /// --participant <participant_id> <role> <display_name> [organisation]
    /// [--initial-message <message_content>]
    /// [--registration <AUTHORITY> <NUMBER> ...]
    /// [--signature <ecdsa_private_key_pem>]
    CreateThread {
        /// Coordination repository UUID
        coordination_uuid: String,
        /// Author name for Git commit
        author_name: String,
        /// Author email for Git commit
        author_email: String,
        /// Mandatory author role for commit metadata
        #[arg(long)]
        role: String,
        /// Declared professional registrations (repeatable): --registration <AUTHORITY> <NUMBER>
        #[arg(long, value_names = ["AUTHORITY", "NUMBER"], num_args = 2, action = clap::ArgAction::Append)]
        registration: Vec<String>,
        /// Mandatory organisational location for the commit (e.g. hospital name, GP surgery)
        #[arg(long)]
        care_location: String,
        /// Thread participants (repeatable): --participant <UUID> <clinician|patient|system> <display_name> [organisation]
        #[arg(long, value_names = ["UUID", "ROLE", "DISPLAY_NAME", "ORGANISATION"], num_args = 3..=4, action = clap::ArgAction::Append)]
        participant: Vec<String>,
        /// Initial message content (markdown)
        #[arg(long)]
        initial_message: Option<String>,
        /// ECDSA private key PEM for X.509 signing (optional, can be PEM string, base64-encoded PEM, or file path)
        #[arg(long)]
        signature: Option<String>,
    },

    /// Add a message to a thread:
    ///
    /// <coordination_uuid> <thread_id> <author_name> <author_email>
    /// --role <author_role>
    /// --care-location <care_location>
    /// --message-type <clinician|patient|system|correction>
    /// --message-body <content>
    /// --message-author-id <UUID>
    /// --message-author-name <display_name>
    /// [--corrects <message_id>]
    /// [--registration <AUTHORITY> <NUMBER> ...]
    /// [--signature <ecdsa_private_key_pem>]
    AddMessage {
        /// Coordination repository UUID
        coordination_uuid: String,
        /// Thread ID
        thread_id: String,
        /// Author name for Git commit
        author_name: String,
        /// Author email for Git commit
        author_email: String,
        /// Mandatory author role for commit metadata
        #[arg(long)]
        role: String,
        /// Declared professional registrations (repeatable): --registration <AUTHORITY> <NUMBER>
        #[arg(long, value_names = ["AUTHORITY", "NUMBER"], num_args = 2, action = clap::ArgAction::Append)]
        registration: Vec<String>,
        /// Mandatory organisational location for the commit (e.g. hospital name, GP surgery)
        #[arg(long)]
        care_location: String,
        /// Type of message (clinician, patient, system, correction)
        #[arg(long)]
        message_type: String,
        /// Message content (markdown)
        #[arg(long)]
        message_body: String,
        /// Author ID for the message
        #[arg(long)]
        message_author_id: String,
        /// Author display name for the message
        #[arg(long)]
        message_author_name: String,
        /// Message ID being corrected (for correction messages)
        #[arg(long)]
        corrects: Option<String>,
        /// ECDSA private key PEM for X.509 signing (optional, can be PEM string, base64-encoded PEM, or file path)
        #[arg(long)]
        signature: Option<String>,
    },

    /// Read a messaging thread:
    ///
    /// <coordination_uuid> <thread_id>
    ReadThread {
        /// Coordination repository UUID
        coordination_uuid: String,
        /// Thread ID
        thread_id: String,
    },
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
                Ok(service) => println!(
                    "Initialised clinical with UUID: {}",
                    service.clinical_id().simple()
                ),
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
            let clinical_uuid_parsed = match ShardableUuid::parse(&clinical_uuid) {
                Ok(uuid) => uuid.uuid(),
                Err(e) => {
                    eprintln!("Error parsing clinical UUID: {}", e);
                    return Ok(());
                }
            };
            let clinical_service = ClinicalService::with_id(cfg.clone(), clinical_uuid_parsed);
            match clinical_service.link_to_demographics(
                &author,
                care_location,
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

            let clinical_dir = cfg.patient_data_dir().join(constants::CLINICAL_DIR_NAME);
            match VersionedFileService::verify_commit_signature(
                &clinical_dir,
                &clinical_uuid,
                &public_key_pem,
            ) {
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
        Some(Commands::NewLetter {
            clinical_uuid,
            author_name,
            author_email,
            role,
            registration,
            care_location,
            content,
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
                name: author_name,
                role,
                email: author_email,
                registrations,
                signature,
                certificate: None,
            };

            let clinical_uuid_parsed = match ShardableUuid::parse(&clinical_uuid) {
                Ok(uuid) => uuid.uuid(),
                Err(e) => {
                    eprintln!("Error parsing clinical UUID: {}", e);
                    return Ok(());
                }
            };

            let clinical_service = ClinicalService::with_id(cfg.clone(), clinical_uuid_parsed);
            match clinical_service.new_letter(&author, care_location, content, None) {
                Ok(timestamp_id) => {
                    println!("Created new letter with timestamp ID: {}", timestamp_id)
                }
                Err(e) => eprintln!("Error creating letter: {}", e),
            }
        }
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
        Some(Commands::InitialiseCoordination {
            author_name,
            author_email,
            role,
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
                name: author_name,
                role,
                email: author_email,
                registrations,
                signature,
                certificate: None,
            };
            let coordination_service = CoordinationService::new(cfg.clone());
            match coordination_service.initialise(author, care_location) {
                Ok(service) => println!(
                    "Initialised coordination with UUID: {}",
                    service.coordination_id().simple()
                ),
                Err(e) => eprintln!("Error initialising coordination: {}", e),
            }
        }
        Some(Commands::CreateThread {
            coordination_uuid,
            author_name,
            author_email,
            role,
            registration,
            care_location,
            participant,
            initial_message,
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
                name: author_name,
                role,
                email: author_email,
                registrations,
                signature,
                certificate: None,
            };

            let coordination_uuid_parsed = match ShardableUuid::parse(&coordination_uuid) {
                Ok(uuid) => uuid.uuid(),
                Err(e) => {
                    eprintln!("Error parsing coordination UUID: {}", e);
                    return Ok(());
                }
            };

            // Parse participants
            let mut participants = Vec::new();
            for chunk in participant.chunks(3) {
                if chunk.len() < 3 {
                    eprintln!("Invalid participant format: needs UUID, role, display_name, and optional organisation");
                    return Ok(());
                }
                let participant_id = match uuid::Uuid::parse_str(&chunk[0]) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Invalid participant UUID: {}", e);
                        return Ok(());
                    }
                };
                let role = match chunk[1].to_lowercase().as_str() {
                    "clinician" => ParticipantRole::Clinician,
                    "patient" => ParticipantRole::Patient,
                    "system" => ParticipantRole::System,
                    _ => {
                        eprintln!("Invalid role: must be clinician, patient, or system");
                        return Ok(());
                    }
                };
                let display_name = chunk[2].clone();
                let organisation = chunk.get(3).cloned();

                participants.push(ThreadParticipant {
                    participant_id,
                    role,
                    display_name,
                    organisation,
                });
            }

            let initial_msg = initial_message.map(|body| {
                // Use first participant as message author if available
                let (author_id, author_name) = if let Some(p) = participants.first() {
                    (p.participant_id, p.display_name.clone())
                } else {
                    (uuid::Uuid::new_v4(), "System".to_string())
                };

                MessageContent {
                    message_type: MessageType::System,
                    author_id,
                    author_display_name: author_name,
                    body,
                    corrects: None,
                }
            });

            let coordination_service =
                CoordinationService::with_id(cfg.clone(), coordination_uuid_parsed);
            match coordination_service.create_thread(
                &author,
                care_location,
                participants,
                initial_msg,
            ) {
                Ok(thread_id) => println!("Created thread with ID: {}", thread_id),
                Err(e) => eprintln!("Error creating thread: {}", e),
            }
        }
        Some(Commands::AddMessage {
            coordination_uuid,
            thread_id,
            author_name,
            author_email,
            role,
            registration,
            care_location,
            message_type,
            message_body,
            message_author_id,
            message_author_name,
            corrects,
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
                name: author_name,
                role,
                email: author_email,
                registrations,
                signature,
                certificate: None,
            };

            let coordination_uuid_parsed = match ShardableUuid::parse(&coordination_uuid) {
                Ok(uuid) => uuid.uuid(),
                Err(e) => {
                    eprintln!("Error parsing coordination UUID: {}", e);
                    return Ok(());
                }
            };

            let msg_type = match message_type.to_lowercase().as_str() {
                "clinician" => MessageType::Clinician,
                "patient" => MessageType::Patient,
                "system" => MessageType::System,
                "correction" => MessageType::Correction,
                _ => {
                    eprintln!(
                        "Invalid message type: must be clinician, patient, system, or correction"
                    );
                    return Ok(());
                }
            };

            let author_id = match uuid::Uuid::parse_str(&message_author_id) {
                Ok(id) => id,
                Err(e) => {
                    eprintln!("Invalid message author UUID: {}", e);
                    return Ok(());
                }
            };

            let corrects_id = if let Some(c) = corrects {
                match uuid::Uuid::parse_str(&c) {
                    Ok(id) => Some(id),
                    Err(e) => {
                        eprintln!("Invalid corrects UUID: {}", e);
                        return Ok(());
                    }
                }
            } else {
                None
            };

            let message = MessageContent {
                message_type: msg_type,
                author_id,
                author_display_name: message_author_name,
                body: message_body,
                corrects: corrects_id,
            };

            let coordination_service =
                CoordinationService::with_id(cfg.clone(), coordination_uuid_parsed);
            match coordination_service.add_message(&author, care_location, &thread_id, message) {
                Ok(message_id) => println!("Added message with ID: {}", message_id),
                Err(e) => eprintln!("Error adding message: {}", e),
            }
        }
        Some(Commands::ReadThread {
            coordination_uuid,
            thread_id,
        }) => {
            let coordination_uuid_parsed = match ShardableUuid::parse(&coordination_uuid) {
                Ok(uuid) => uuid.uuid(),
                Err(e) => {
                    eprintln!("Error parsing coordination UUID: {}", e);
                    return Ok(());
                }
            };

            let coordination_service =
                CoordinationService::with_id(cfg.clone(), coordination_uuid_parsed);
            match coordination_service.read_thread(&thread_id) {
                Ok(thread) => {
                    println!("Thread ID: {}", thread.thread_id);
                    println!("Status: {:?}", thread.ledger.status);
                    println!("Created: {}", thread.ledger.created_at);
                    println!("Last Updated: {}", thread.ledger.last_updated_at);
                    println!("\nParticipants:");
                    for p in &thread.ledger.participants {
                        println!(
                            "  - {} ({:?}): {}",
                            p.participant_id, p.role, p.display_name
                        );
                        if let Some(org) = &p.organisation {
                            println!("    Organisation: {}", org);
                        }
                    }
                    println!("\nMessages ({}):", thread.messages.len());
                    for msg in &thread.messages {
                        println!("  ---");
                        println!("  ID: {}", msg.message_id);
                        println!("  Type: {:?}", msg.message_type);
                        println!("  Timestamp: {}", msg.timestamp);
                        println!("  Author: {} ({})", msg.author_display_name, msg.author_id);
                        if let Some(corrects) = msg.corrects {
                            println!("  Corrects: {}", corrects);
                        }
                        println!("  Body: {}", msg.body);
                    }
                }
                Err(e) => eprintln!("Error reading thread: {}", e),
            }
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

    let template_override = std::env::var("VPR_CLINICAL_TEMPLATE_DIR")
        .ok()
        .map(PathBuf::from);
    let clinical_template_dir = resolve_clinical_template_dir(template_override)?;
    validate_template(&TemplateDirKind::Clinical, &clinical_template_dir)?;

    let rm_system_version =
        rm_system_version_from_env_value(std::env::var("RM_SYSTEM_VERSION").ok())?;
    let vpr_namespace = std::env::var("VPR_NAMESPACE").unwrap_or_else(|_| "vpr.dev.1".into());

    Ok(Arc::new(CoreConfig::new(
        patient_data_path.to_path_buf(),
        clinical_template_dir,
        rm_system_version,
        vpr_namespace,
    )?))
}
