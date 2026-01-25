//! VPR Command Line Interface
//!
//! This module provides a CLI for interacting with the VPR patient record system.
//! It allows users to initialise patient demographics and clinical records,
//! update demographics, link clinical records to demographics, and list patients.

#![allow(rustdoc::invalid_html_tags)]

use clap::{Parser, Subcommand};
use fhir::{
    coordination_status::LifecycleState, messaging::SensitivityLevel,
    messaging::ThreadStatus as FhirThreadStatus, AuthorRole, MessageAuthor,
};
use vpr_certificates::Certificate;
use vpr_core::{
    config::rm_system_version_from_env_value,
    constants,
    repositories::clinical::ClinicalService,
    repositories::coordination::{
        CoordinationService, CoordinationStatusUpdate, LedgerUpdate, MessageContent,
    },
    repositories::demographics::DemographicsService,
    repositories::shared::{resolve_clinical_template_dir, validate_template, TemplateDirKind},
    versioned_files::VersionedFileService,
    Author, AuthorRegistration, CoreConfig, PatientService, ShardableUuid, TimestampId,
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
    /// <clinical_uuid> <author_name> <author_email>
    /// --role <author_role>
    /// --care-location <care_location>
    /// [--registration <AUTHORITY> <NUMBER> ...]
    /// [--signature <ecdsa_private_key_pem>]
    InitialiseCoordination {
        /// Clinical record UUID to link coordination record to
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
        /// ECDSA private key PEM for X.509 signing (optional, can be PEM string, base64-encoded PEM, or file path)
        #[arg(long)]
        signature: Option<String>,
    },

    /// Create a messaging thread:
    ///
    /// <coordination_uuid> <author_name> <author_email>
    /// --role <author_role>
    /// --care-location <care_location>
    /// --participant <participant_id> <role> <display_name>
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

    /// Read a communication thread:
    ///
    /// <coordination_uuid> <thread_id>
    ReadCommunication {
        /// Coordination repository UUID
        coordination_uuid: String,
        /// Thread ID
        thread_id: String,
    },

    /// Read a clinical letter:
    ///
    /// <clinical_uuid> <letter_timestamp_id>
    ReadLetter {
        /// Clinical repository UUID
        clinical_uuid: String,
        /// Letter timestamp ID
        letter_timestamp_id: String,
    },

    /// Create a new letter with file attachments:
    ///
    /// <clinical_uuid> <author_name> <author_email>
    /// --role <author_role>
    /// --care-location <care_location>
    /// --attachment-file <file_path> (repeatable)
    /// [--registration <AUTHORITY> <NUMBER> ...]
    /// [--signature <ecdsa_private_key_pem>]
    NewLetterWithAttachments {
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
        /// File paths for attachments (repeatable): --attachment-file <path>
        #[arg(long = "attachment-file", action = clap::ArgAction::Append)]
        attachment_file: Vec<PathBuf>,
        /// ECDSA private key PEM for X.509 signing (optional, can be PEM string, base64-encoded PEM, or file path)
        #[arg(long)]
        signature: Option<String>,
    },

    /// Get letter attachments:
    ///
    /// <clinical_uuid> <letter_timestamp_id>
    GetLetterAttachments {
        /// Clinical repository UUID
        clinical_uuid: String,
        /// Letter timestamp ID
        letter_timestamp_id: String,
    },

    /// Update communication thread ledger:
    ///
    /// <coordination_uuid> <thread_id> <author_name> <author_email>
    /// --role <author_role>
    /// --care-location <care_location>
    /// [--add-participant <UUID> <role> <display_name> ...]
    /// [--remove-participant <UUID> ...]
    /// [--status <open|closed|archived>]
    /// [--sensitivity <standard|confidential|restricted>]
    /// [--restricted <true|false>]
    /// [--allow-patient <true|false>]
    /// [--allow-external <true|false>]
    /// [--registration <AUTHORITY> <NUMBER> ...]
    /// [--signature <ecdsa_private_key_pem>]
    UpdateCommunicationLedger {
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
        /// Add participants (repeatable): --add-participant <UUID> <role> <display_name>
        #[arg(long = "add-participant", value_names = ["UUID", "ROLE", "DISPLAY_NAME"], num_args = 3, action = clap::ArgAction::Append)]
        add_participant: Vec<String>,
        /// Remove participants by UUID (repeatable): --remove-participant <UUID>
        #[arg(long = "remove-participant", action = clap::ArgAction::Append)]
        remove_participant: Vec<String>,
        /// Set thread status
        #[arg(long)]
        status: Option<String>,
        /// Set sensitivity level
        #[arg(long)]
        sensitivity: Option<String>,
        /// Set restricted flag (true/false)
        #[arg(long)]
        restricted: Option<bool>,
        /// Set allow patient participation (true/false)
        #[arg(long)]
        allow_patient: Option<bool>,
        /// Set allow external organisations (true/false)
        #[arg(long)]
        allow_external: Option<bool>,
        /// ECDSA private key PEM for X.509 signing (optional, can be PEM string, base64-encoded PEM, or file path)
        #[arg(long)]
        signature: Option<String>,
    },

    /// Update coordination record status:
    ///
    /// <coordination_uuid> <author_name> <author_email>
    /// --role <author_role>
    /// --care-location <care_location>
    /// [--lifecycle-state <active|suspended|closed>]
    /// [--record-open <true|false>]
    /// [--record-queryable <true|false>]
    /// [--record-modifiable <true|false>]
    /// [--registration <AUTHORITY> <NUMBER> ...]
    /// [--signature <ecdsa_private_key_pem>]
    UpdateCoordinationStatus {
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
        /// Set lifecycle state
        #[arg(long)]
        lifecycle_state: Option<String>,
        /// Set record open flag (true/false)
        #[arg(long)]
        record_open: Option<bool>,
        /// Set record queryable flag (true/false)
        #[arg(long)]
        record_queryable: Option<bool>,
        /// Set record modifiable flag (true/false)
        #[arg(long)]
        record_modifiable: Option<bool>,
        /// ECDSA private key PEM for X.509 signing (optional, can be PEM string, base64-encoded PEM, or file path)
        #[arg(long)]
        signature: Option<String>,
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
                Ok(service) => println!(
                    "Initialised demographics with UUID: {}",
                    service.demographics_id()
                ),
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

            match DemographicsService::with_id(cfg.clone(), &demographics_uuid) {
                Ok(demographics_service) => {
                    match demographics_service.update(given_names_vec, &last_name, &birth_date) {
                        Ok(()) => println!("Updated demographics for UUID: {}", demographics_uuid),
                        Err(e) => eprintln!("Error updating demographics: {}", e),
                    }
                }
                Err(e) => eprintln!("Error creating demographics service: {}", e),
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
                    "Initialised full record - Demographics UUID: {}, Clinical UUID: {}, Coordination UUID: {}",
                    record.demographics_uuid, record.clinical_uuid, record.coordination_uuid
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
            clear_dir_contents(&base_dir.join(constants::COORDINATION_DIR_NAME))?;

            println!(
                "Deleted all patient data under {}",
                base_dir.to_string_lossy()
            );
        }
        Some(Commands::InitialiseCoordination {
            clinical_uuid,
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
            let clinical_id = match uuid::Uuid::parse_str(&clinical_uuid) {
                Ok(id) => id,
                Err(e) => {
                    eprintln!("Error parsing clinical UUID: {}", e);
                    return Ok(());
                }
            };
            let coordination_service = CoordinationService::new(cfg.clone());
            match coordination_service.initialise(author, care_location, clinical_id) {
                Ok(service) => println!(
                    "Initialised coordination with UUID: {}",
                    service.coordination_id()
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

            // Parse participants - each --participant flag captures 3 values: UUID, role, display_name
            let mut participants = Vec::new();
            let mut i = 0;
            while i < participant.len() {
                if i + 2 >= participant.len() {
                    eprintln!("Invalid participant format: needs UUID, role, and display_name");
                    return Ok(());
                }

                let participant_id = match uuid::Uuid::parse_str(&participant[i]) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Invalid participant UUID: {}", e);
                        return Ok(());
                    }
                };

                let role = match participant[i + 1].to_lowercase().as_str() {
                    "clinician" => AuthorRole::Clinician,
                    "careadministrator" | "care_administrator" | "care-administrator" => {
                        AuthorRole::CareAdministrator
                    }
                    "patient" => AuthorRole::Patient,
                    "patientassociate" | "patient_associate" | "patient-associate" => {
                        AuthorRole::PatientAssociate
                    }
                    "system" => AuthorRole::System,
                    _ => {
                        eprintln!("Invalid role: must be clinician, careadministrator, patient, patientassociate, or system");
                        return Ok(());
                    }
                };

                let display_name = participant[i + 2].clone();

                participants.push(MessageAuthor {
                    id: participant_id,
                    name: display_name,
                    role,
                });

                i += 3;
            }

            let initial_msg = initial_message.map(|body| {
                // Find thread author in participants to get their info
                let message_author = participants
                    .iter()
                    .find(|p| p.name == author.name)
                    .cloned()
                    .unwrap_or_else(|| MessageAuthor {
                        id: uuid::Uuid::new_v4(),
                        name: author.name.clone(),
                        role: AuthorRole::System,
                    });

                MessageContent::new(message_author, body, None)
                    .expect("Message body should not be empty")
            });

            let coordination_service =
                CoordinationService::with_id(cfg.clone(), coordination_uuid_parsed);
            match coordination_service.communication_create(
                &author,
                care_location,
                participants,
                initial_msg.unwrap(),
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

            let author_role = match message_type.to_lowercase().as_str() {
                "clinician" => AuthorRole::Clinician,
                "careadministrator" | "care_administrator" | "care-administrator" => {
                    AuthorRole::CareAdministrator
                }
                "patient" => AuthorRole::Patient,
                "patientassociate" | "patient_associate" | "patient-associate" => {
                    AuthorRole::PatientAssociate
                }
                "system" => AuthorRole::System,
                _ => {
                    eprintln!(
                        "Invalid author role: must be clinician, careadministrator, patient, patientassociate, or system"
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

            let message = MessageContent::new(
                MessageAuthor {
                    id: author_id,
                    name: message_author_name,
                    role: author_role,
                },
                message_body,
                corrects_id,
            )
            .expect("Message body should not be empty");

            let thread_id_parsed = match thread_id.parse::<TimestampId>() {
                Ok(id) => id,
                Err(e) => {
                    eprintln!("Invalid thread ID format: {}", e);
                    return Ok(());
                }
            };

            let coordination_service =
                CoordinationService::with_id(cfg.clone(), coordination_uuid_parsed);
            match coordination_service.message_add(
                &author,
                care_location,
                &thread_id_parsed,
                message,
            ) {
                Ok(message_id) => println!("Added message with ID: {}", message_id),
                Err(e) => eprintln!("Error adding message: {}", e),
            }
        }
        Some(Commands::ReadCommunication {
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

            let thread_id_parsed = match thread_id.parse::<TimestampId>() {
                Ok(id) => id,
                Err(e) => {
                    eprintln!("Invalid thread ID format: {}", e);
                    return Ok(());
                }
            };

            let coordination_service =
                CoordinationService::with_id(cfg.clone(), coordination_uuid_parsed);
            match coordination_service.read_communication(&thread_id_parsed) {
                Ok(thread) => {
                    println!("Communication ID: {}", thread.communication_id);
                    println!("Status: {:?}", thread.ledger.status);
                    println!("Created: {}", thread.ledger.created_at);
                    println!("Last Updated: {}", thread.ledger.last_updated_at);
                    println!("\nParticipants:");
                    for p in &thread.ledger.participants {
                        println!("  - {} ({:?}): {}", p.id, p.role, p.name);
                    }
                    println!("\nMessages ({}):", thread.messages.len());
                    for msg in &thread.messages {
                        println!("  ---");
                        println!("  ID: {}", msg.metadata.message_id);
                        println!("  Role: {:?}", msg.metadata.author.role);
                        println!("  Timestamp: {}", msg.metadata.timestamp.to_rfc3339());
                        println!(
                            "  Author: {} ({})",
                            msg.metadata.author.name, msg.metadata.author.id
                        );
                        if let Some(corrects) = msg.corrects {
                            println!("  Corrects: {}", corrects);
                        }
                        println!("  Body: {}", msg.body);
                    }
                }
                Err(e) => eprintln!("Error reading thread: {}", e),
            }
        }
        Some(Commands::ReadLetter {
            clinical_uuid,
            letter_timestamp_id,
        }) => {
            let clinical_uuid_parsed = match ShardableUuid::parse(&clinical_uuid) {
                Ok(uuid) => uuid.uuid(),
                Err(e) => {
                    eprintln!("Error parsing clinical UUID: {}", e);
                    return Ok(());
                }
            };

            let clinical_service = ClinicalService::with_id(cfg.clone(), clinical_uuid_parsed);
            match clinical_service.read_letter(&letter_timestamp_id) {
                Ok(result) => {
                    println!("Letter Timestamp ID: {}", letter_timestamp_id);
                    println!("\n--- Composition Data ---");
                    println!("RM Version: {:?}", result.letter_data.rm_version);
                    println!("Composer: {}", result.letter_data.composer_name);
                    println!("Role: {}", result.letter_data.composer_role);
                    println!("Start Time: {}", result.letter_data.start_time.to_rfc3339());
                    if !result.letter_data.clinical_lists.is_empty() {
                        println!("\nClinical Lists:");
                        for list in &result.letter_data.clinical_lists {
                            println!("  - {}", list.name);
                        }
                    }
                    println!("\n--- Body Content ---");
                    println!("{}", result.body_content);
                }
                Err(e) => eprintln!("Error reading letter: {}", e),
            }
        }
        Some(Commands::NewLetterWithAttachments {
            clinical_uuid,
            author_name,
            author_email,
            role,
            registration,
            care_location,
            attachment_file,
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

            if attachment_file.is_empty() {
                eprintln!("Error: At least one attachment file is required");
                return Ok(());
            }

            let clinical_service = ClinicalService::with_id(cfg.clone(), clinical_uuid_parsed);
            match clinical_service.new_letter_with_attachments(
                &author,
                care_location,
                &attachment_file,
                None,
            ) {
                Ok(timestamp_id) => {
                    println!(
                        "Created new letter with {} attachment(s), timestamp ID: {}",
                        attachment_file.len(),
                        timestamp_id
                    )
                }
                Err(e) => eprintln!("Error creating letter with attachments: {}", e),
            }
        }
        Some(Commands::GetLetterAttachments {
            clinical_uuid,
            letter_timestamp_id,
        }) => {
            let clinical_uuid_parsed = match ShardableUuid::parse(&clinical_uuid) {
                Ok(uuid) => uuid.uuid(),
                Err(e) => {
                    eprintln!("Error parsing clinical UUID: {}", e);
                    return Ok(());
                }
            };

            let clinical_service = ClinicalService::with_id(cfg.clone(), clinical_uuid_parsed);
            match clinical_service.get_letter_attachments(&letter_timestamp_id) {
                Ok(attachments) => {
                    if attachments.is_empty() {
                        println!("No attachments found for letter {}", letter_timestamp_id);
                    } else {
                        println!(
                            "Found {} attachment(s) for letter {}:",
                            attachments.len(),
                            letter_timestamp_id
                        );
                        for attachment in attachments {
                            println!("\n  ---");
                            println!("  Filename: {}", attachment.metadata.filename);
                            println!("  Original: {}", attachment.metadata.original_filename);
                            println!("  Hash: {}", attachment.metadata.hash);
                            println!("  Size: {} bytes", attachment.metadata.size_bytes);
                            println!(
                                "  Media Type: {}",
                                attachment
                                    .metadata
                                    .media_type
                                    .as_deref()
                                    .unwrap_or("unknown")
                            );
                            println!("  Storage Path: {}", attachment.metadata.file_storage_path);
                            println!("  Content Length: {} bytes", attachment.content.len());
                        }
                    }
                }
                Err(e) => eprintln!("Error getting letter attachments: {}", e),
            }
        }
        Some(Commands::UpdateCommunicationLedger {
            coordination_uuid,
            thread_id,
            author_name,
            author_email,
            role,
            registration,
            care_location,
            add_participant,
            remove_participant,
            status,
            sensitivity,
            restricted,
            allow_patient,
            allow_external,
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

            let thread_id_parsed = match thread_id.parse::<TimestampId>() {
                Ok(id) => id,
                Err(e) => {
                    eprintln!("Invalid thread ID format: {}", e);
                    return Ok(());
                }
            };

            // Parse add_participant
            let mut add_participants = Vec::new();
            let mut i = 0;
            while i < add_participant.len() {
                if i + 2 >= add_participant.len() {
                    eprintln!("Invalid add-participant format: needs UUID, role, and display_name");
                    return Ok(());
                }

                let participant_id = match uuid::Uuid::parse_str(&add_participant[i]) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Invalid participant UUID: {}", e);
                        return Ok(());
                    }
                };

                let participant_role = match add_participant[i + 1].to_lowercase().as_str() {
                    "clinician" => AuthorRole::Clinician,
                    "careadministrator" | "care_administrator" | "care-administrator" => {
                        AuthorRole::CareAdministrator
                    }
                    "patient" => AuthorRole::Patient,
                    "patientassociate" | "patient_associate" | "patient-associate" => {
                        AuthorRole::PatientAssociate
                    }
                    "system" => AuthorRole::System,
                    _ => {
                        eprintln!("Invalid role: must be clinician, careadministrator, patient, patientassociate, or system");
                        return Ok(());
                    }
                };

                let display_name = add_participant[i + 2].clone();

                add_participants.push(MessageAuthor {
                    id: participant_id,
                    name: display_name,
                    role: participant_role,
                });

                i += 3;
            }

            // Parse remove_participant
            let mut remove_participants = Vec::new();
            for uuid_str in remove_participant {
                match uuid::Uuid::parse_str(&uuid_str) {
                    Ok(id) => remove_participants.push(id),
                    Err(e) => {
                        eprintln!("Invalid remove-participant UUID: {}", e);
                        return Ok(());
                    }
                }
            }

            // Parse status
            let thread_status = if let Some(s) = status {
                let parsed = match s.to_lowercase().as_str() {
                    "open" => FhirThreadStatus::Open,
                    "closed" => FhirThreadStatus::Closed,
                    "archived" => FhirThreadStatus::Archived,
                    _ => {
                        eprintln!("Invalid status: must be open, closed, or archived");
                        return Ok(());
                    }
                };
                Some(parsed)
            } else {
                None
            };

            // Parse sensitivity
            let sensitivity_level = if let Some(s) = sensitivity {
                let parsed = match s.to_lowercase().as_str() {
                    "standard" => SensitivityLevel::Standard,
                    "confidential" => SensitivityLevel::Confidential,
                    "restricted" => SensitivityLevel::Restricted,
                    _ => {
                        eprintln!(
                            "Invalid sensitivity: must be standard, confidential, or restricted"
                        );
                        return Ok(());
                    }
                };
                Some(parsed)
            } else {
                None
            };

            // Build visibility tuple if sensitivity is provided
            let set_visibility = match (sensitivity_level, restricted) {
                (Some(level), Some(restricted_flag)) => Some((level, restricted_flag)),
                (Some(level), None) => Some((level, false)), // Default to not restricted
                (None, Some(_)) => {
                    eprintln!("Error: --restricted requires --sensitivity to be set");
                    return Ok(());
                }
                (None, None) => None,
            };

            // Build policies tuple if either is provided
            let set_policies = match (allow_patient, allow_external) {
                (Some(ap), Some(ae)) => Some((ap, ae)),
                (Some(ap), None) => Some((ap, true)), // Default external to true
                (None, Some(ae)) => Some((true, ae)), // Default patient to true
                (None, None) => None,
            };

            let ledger_update = LedgerUpdate {
                add_participants: if add_participants.is_empty() {
                    None
                } else {
                    Some(add_participants)
                },
                remove_participants: if remove_participants.is_empty() {
                    None
                } else {
                    Some(remove_participants)
                },
                set_status: thread_status,
                set_visibility,
                set_policies,
            };

            let coordination_service =
                CoordinationService::with_id(cfg.clone(), coordination_uuid_parsed);
            match coordination_service.update_communication_ledger(
                &author,
                care_location,
                &thread_id_parsed,
                ledger_update,
            ) {
                Ok(()) => println!("Successfully updated thread ledger"),
                Err(e) => eprintln!("Error updating thread ledger: {}", e),
            }
        }
        Some(Commands::UpdateCoordinationStatus {
            coordination_uuid,
            author_name,
            author_email,
            role,
            registration,
            care_location,
            lifecycle_state,
            record_open,
            record_queryable,
            record_modifiable,
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

            // Parse lifecycle state
            let parsed_lifecycle_state = if let Some(s) = lifecycle_state {
                let parsed = match s.to_lowercase().as_str() {
                    "active" => LifecycleState::Active,
                    "suspended" => LifecycleState::Suspended,
                    "closed" => LifecycleState::Closed,
                    _ => {
                        eprintln!("Invalid lifecycle-state: must be active, suspended, or closed");
                        return Ok(());
                    }
                };
                Some(parsed)
            } else {
                None
            };

            let status_update = CoordinationStatusUpdate {
                set_lifecycle_state: parsed_lifecycle_state,
                set_record_open: record_open,
                set_record_queryable: record_queryable,
                set_record_modifiable: record_modifiable,
            };

            let coordination_service =
                CoordinationService::with_id(cfg.clone(), coordination_uuid_parsed);
            match coordination_service.update_coordination_status(
                &author,
                care_location,
                status_update,
            ) {
                Ok(()) => println!("Successfully updated coordination status"),
                Err(e) => eprintln!("Error updating coordination status: {}", e),
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
