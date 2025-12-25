use clap::{Parser, Subcommand};
use vpr_core::{
    clinical::initialise_clinical, demographics::initialise_demographics, Author, PatientService,
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
        #[arg(long)]
        name: String,
        /// Author email for Git commit
        #[arg(long)]
        email: String,
        /// Author signature (optional)
        #[arg(long)]
        signature: Option<String>,
    },
    /// Initialise clinical
    InitialiseClinical {
        /// Author name for Git commit
        #[arg(long)]
        name: String,
        /// Author email for Git commit
        #[arg(long)]
        email: String,
        /// Author signature (optional)
        #[arg(long)]
        signature: Option<String>,
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
        None => {
            println!("Use 'vpr --help' for commands");
        }
    }

    Ok(())
}
