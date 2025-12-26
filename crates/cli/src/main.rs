//! VPR Command Line Interface
//!
//! This module provides a CLI for interacting with the VPR patient record system.
//! It allows users to initialize patient demographics and clinical records,
//! update demographics, link clinical records to demographics, and list patients.

use clap::{Parser, Subcommand};
use vpr_core::{
    clinical::ClinicalService, demographics::DemographicsService, Author, PatientService,
};
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
/// on patient records, from initialization to updates and queries.
#[derive(Subcommand)]
enum Commands {
    /// List all patients
    List,
    /// Initialise demographics
    InitialiseDemographics {
        /// Author name for Git commit
        name: String,
        /// Author email for Git commit
        email: String,
        /// Author signature (optional)
        #[arg(long)]
        signature: Option<String>,
    },
    /// Initialise clinical
    InitialiseClinical {
        /// Author name for Git commit
        name: String,
        /// Author email for Git commit
        email: String,
        /// Author signature (optional)
        #[arg(long)]
        signature: Option<String>,
    },
    /// Write EHR status
    WriteEhrStatus {
        /// Clinical repository UUID
        clinical_uuid: String,
        /// Demographics UUID
        demographics_uuid: String,
        /// Organisation domain (optional)
        #[arg(long)]
        namespace: Option<String>,
    },
    /// Update demographics
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
    /// Initialise full record (demographics + clinical)
    InitialiseFullRecord {
        /// Author name for Git commit
        name: String,
        /// Author email for Git commit
        email: String,
        /// Author signature (optional)
        #[arg(long)]
        signature: Option<String>,
        /// Given names (comma-separated)
        given_names: String,
        /// Last name
        last_name: String,
        /// Date of birth (YYYY-MM-DD)
        birth_date: String,
        /// Organisation domain (optional)
        #[arg(long)]
        namespace: Option<String>,
    },
}

/// Main entry point for the VPR CLI.
///
/// Parses command line arguments and executes the appropriate command.
/// Handles initialization of demographics and clinical records, updates,
/// linking operations, and patient listing.
///
/// # Returns
///
/// Returns `Ok(())` on successful execution, or an error if something fails.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::List) => {
            let service = DemographicsService;
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
            email,
            signature,
        }) => {
            let author = Author {
                name,
                email,
                signature,
            };
            let demographics_service = DemographicsService;
            match demographics_service.initialise(author) {
                Ok(uuid) => println!("Initialised demographics with UUID: {}", uuid),
                Err(e) => eprintln!("Error initialising demographics: {}", e),
            }
        }
        Some(Commands::InitialiseClinical {
            name,
            email,
            signature,
        }) => {
            let author = Author {
                name,
                email,
                signature,
            };
            let clinical_service = ClinicalService;
            match clinical_service.initialise(author) {
                Ok(uuid) => println!("Initialised clinical with UUID: {}", uuid),
                Err(e) => eprintln!("Error initialising clinical: {}", e),
            }
        }
        Some(Commands::WriteEhrStatus {
            clinical_uuid,
            demographics_uuid,
            namespace,
        }) => {
            let clinical_service = ClinicalService;
            match clinical_service.link_to_demographics(
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
            let demographics_service = DemographicsService;
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
            name,
            email,
            signature,
            given_names,
            last_name,
            birth_date,
            namespace,
        }) => {
            let author = Author {
                name,
                email,
                signature,
            };
            let given_names_vec: Vec<String> = given_names
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
            let service = PatientService::new();
            match service.initialise_full_record(
                author,
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
        None => {
            println!("Use 'vpr --help' for commands");
        }
    }

    Ok(())
}
