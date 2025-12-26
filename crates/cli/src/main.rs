use clap::{Parser, Subcommand};
use vpr_core::{
    clinical::initialise_clinical, clinical::write_ehr_status,
    demographics::initialise_demographics, demographics::update_demographics, Author,
    PatientService,
};

#[derive(Parser)]
#[command(name = "vpr")]
#[command(about = "VPR patient record system CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Say hi
    Hi,
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
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Hi) => {
            println!("hi");
        }
        Some(Commands::List) => {
            let service = PatientService::new();
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
            match initialise_demographics(author) {
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
            match initialise_clinical(author) {
                Ok(uuid) => println!("Initialised clinical with UUID: {}", uuid),
                Err(e) => eprintln!("Error initialising clinical: {}", e),
            }
        }
        Some(Commands::WriteEhrStatus {
            clinical_uuid,
            demographics_uuid,
            namespace,
        }) => match write_ehr_status(&clinical_uuid, &demographics_uuid, namespace) {
            Ok(()) => println!("Wrote EHR status for clinical UUID: {}", clinical_uuid),
            Err(e) => eprintln!("Error writing EHR status: {}", e),
        },
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
            match update_demographics(&demographics_uuid, given_names_vec, &last_name, &birth_date)
            {
                Ok(()) => println!("Updated demographics for UUID: {}", demographics_uuid),
                Err(e) => eprintln!("Error updating demographics: {}", e),
            }
        }
        None => {
            println!("Use 'vpr --help' for commands");
        }
    }

    Ok(())
}
