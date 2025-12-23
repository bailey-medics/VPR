use clap::{Parser, Subcommand};
use vpr_core::{pb, PatientService};

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
        None => {
            println!("Use 'vpr --help' for commands");
        }
    }

    Ok(())
}
