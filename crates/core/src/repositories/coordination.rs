//! Care Coordination Repository (CCR) Management.
//!
//! This module manages care coordination records, focusing on asynchronous
//! clinical communication and task management between clinicians, patients, and
//! other authorized participants.
//!
//! ## Architecture
//!
//! Like the clinical repository, the coordination repository uses:
//! - **Type-state pattern** for compile-time safety (Uninitialised/Initialised)
//! - **UUID-based sharded storage** for scalability
//! - **Git-based versioning** for all operations
//! - **Immutable append-only records** for audit and legal compliance

use crate::author::Author;
use crate::config::CoreConfig;
use crate::constants::COORDINATION_DIR_NAME;
use crate::error::{PatientError, PatientResult};
use crate::repositories::shared::create_uuid_and_shard_dir;
use crate::versioned_files::{
    CoordinationDomain, FileToWrite, VersionedFileService, VprCommitAction, VprCommitDomain,
    VprCommitMessage,
};
use crate::ShardableUuid;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;
use vpr_uuid::TimestampIdGenerator;
use CoordinationDomain::*;

#[cfg(test)]
use std::collections::HashSet;

#[cfg(test)]
use std::sync::{LazyLock, Mutex};

// ============================================================================
// TYPE-STATE MARKERS
// ============================================================================

/// Marker type: coordination record does not yet exist.
///
/// Used in type-state pattern to prevent operations on non-existent records.
/// Only `initialise()` can be called in this state.
#[derive(Clone, Copy, Debug)]
pub struct Uninitialised;

/// Marker type: coordination record exists.
///
/// Indicates a valid coordination repository with a known UUID.
/// Enables operations like creating threads and adding messages.
#[derive(Clone, Copy, Debug)]
pub struct Initialised {
    coordination_id: Uuid,
}

// ============================================================================
// COORDINATION SERVICE
// ============================================================================

/// Service for managing coordination repository operations.
///
/// Uses type-state pattern to enforce correct usage at compile time.
/// Generic parameter `S` is either `Uninitialised` or `Initialised`.
#[derive(Clone, Debug)]
pub struct CoordinationService<S> {
    cfg: Arc<CoreConfig>,
    state: S,
}

impl CoordinationService<Uninitialised> {
    /// Creates new coordination service in uninitialised state.
    pub fn new(cfg: Arc<CoreConfig>) -> Self {
        Self {
            cfg,
            state: Uninitialised,
        }
    }

    /// Initializes a new coordination repository for a patient.
    ///
    /// Creates:
    /// - UUID and sharded directory structure
    /// - coordination/{shard1}/{shard2}/{uuid}/ directory
    /// - COORDINATION_STATUS.yaml linking to clinical record
    /// - Git repository with initial commit
    ///
    /// Consumes self and returns CoordinationService<Initialised>.
    pub fn initialise(
        self,
        author: Author,
        care_location: String,
        clinical_id: Uuid,
    ) -> PatientResult<CoordinationService<Initialised>> {
        author.validate_commit_author()?;

        let commit_message = VprCommitMessage::new(
            VprCommitDomain::Coordination(Record),
            VprCommitAction::Create,
            "Created coordination record",
            care_location,
        )?;

        let coordination_dir = self.coordination_dir();
        let (coordination_uuid, patient_dir) =
            create_uuid_and_shard_dir(&coordination_dir, ShardableUuid::new)?;

        let result: PatientResult<Uuid> = (|| {
            // Initialize Git repository
            let repo = VersionedFileService::init(&patient_dir)?;

            // Create COORDINATION_STATUS.yaml with link to clinical record
            let status_data = fhir::CoordinationStatusData {
                coordination_id: coordination_uuid.uuid(),
                clinical_id,
                status: fhir::StatusInfo {
                    lifecycle_state: fhir::LifecycleState::Active,
                    record_open: true,
                    record_queryable: true,
                    record_modifiable: true,
                },
            };

            let status_yaml = fhir::CoordinationStatus::render(&status_data)?;
            let status_path = patient_dir.join("COORDINATION_STATUS.yaml");
            fs::write(&status_path, status_yaml).map_err(PatientError::FileWrite)?;

            // Initial commit
            repo.write_and_commit_files(
                &author,
                &commit_message,
                &[FileToWrite {
                    relative_path: Path::new("COORDINATION_STATUS.yaml"),
                    content: &fs::read_to_string(&status_path).map_err(PatientError::FileRead)?,
                    old_content: None,
                }],
            )?;

            Ok(coordination_uuid.uuid())
        })();

        let coordination_id = match result {
            Ok(id) => id,
            Err(e) => {
                // Attempt cleanup
                if let Err(cleanup_err) = remove_patient_dir_all(&patient_dir) {
                    return Err(PatientError::CleanupAfterInitialiseFailed {
                        path: patient_dir,
                        init_error: Box::new(e),
                        cleanup_error: cleanup_err,
                    });
                }
                return Err(e);
            }
        };

        Ok(CoordinationService {
            cfg: self.cfg,
            state: Initialised { coordination_id },
        })
    }
}

impl CoordinationService<Initialised> {
    /// Creates coordination service for existing record.
    pub fn with_id(cfg: Arc<CoreConfig>, coordination_id: Uuid) -> Self {
        Self {
            cfg,
            state: Initialised { coordination_id },
        }
    }

    /// Returns the coordination UUID.
    pub fn coordination_id(&self) -> Uuid {
        self.state.coordination_id
    }
}

// ============================================================================
// MESSAGING OPERATIONS
// ============================================================================

impl CoordinationService<Initialised> {
    /// Creates a new messaging thread.
    ///
    /// Creates:
    /// - communications/{thread_id}/ directory
    /// - messages.md with header/metadata
    /// - ledger.yaml with participants, status, policies
    ///
    /// Thread ID format: YYYYMMDDTHHMMSS.sssZ-UUID
    ///
    /// Returns the thread_id string.
    pub fn create_thread(
        &self,
        author: &Author,
        care_location: String,
        participants: Vec<ThreadParticipant>,
        initial_message: Option<MessageContent>,
    ) -> PatientResult<String> {
        author.validate_commit_author()?;

        let msg = VprCommitMessage::new(
            VprCommitDomain::Coordination(Messaging),
            VprCommitAction::Create,
            "Created messaging thread",
            care_location,
        )?;

        let thread_id = generate_thread_id()?;
        let coordination_uuid = ShardableUuid::parse(&self.coordination_id().simple().to_string())?;
        let patient_dir = self.coordination_patient_dir(&coordination_uuid);
        let thread_dir = thread_dir(&patient_dir, &thread_id);

        // Create thread directory
        fs::create_dir_all(&thread_dir).map_err(PatientError::PatientDirCreation)?;

        // Create initial messages.md
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let mut messages_content = "# Messages\n\n".to_string();

        // Add initial message if provided
        if let Some(msg_content) = initial_message {
            let message_id = generate_message_id();
            messages_content.push_str(&format_message(&msg_content, message_id, &now));
        }

        // Create ledger.yaml
        let ledger = ThreadLedger {
            thread_id: thread_id.clone(),
            status: ThreadStatus::Open,
            created_at: now.clone(),
            last_updated_at: now,
            participants,
            visibility: Visibility {
                sensitivity: SensitivityLevel::Standard,
                restricted: false,
            },
            policies: Policies {
                allow_patient_participation: true,
                allow_external_organisations: true,
            },
        };
        let ledger_content = serialize_ledger(&ledger)?;

        // Write files and commit
        let messages_path = thread_dir.join("messages.md");
        let ledger_path = thread_dir.join("ledger.yaml");

        let messages_relative = messages_path
            .strip_prefix(&patient_dir)
            .map_err(|_| PatientError::InvalidInput("Invalid path prefix".to_string()))?;
        let ledger_relative = ledger_path
            .strip_prefix(&patient_dir)
            .map_err(|_| PatientError::InvalidInput("Invalid path prefix".to_string()))?;

        let files_to_write = [
            FileToWrite {
                relative_path: messages_relative,
                content: &messages_content,
                old_content: None,
            },
            FileToWrite {
                relative_path: ledger_relative,
                content: &ledger_content,
                old_content: None,
            },
        ];

        let repo = VersionedFileService::open(&patient_dir)?;
        repo.write_and_commit_files(author, &msg, &files_to_write)?;

        Ok(thread_id)
    }

    /// Adds a message to an existing thread.
    ///
    /// Appends to messages.md:
    /// - Message metadata (id, timestamp, author, type)
    /// - Message content (markdown body)
    /// - Correction reference if applicable
    ///
    /// Atomic operation: write + Git commit.
    pub fn add_message(
        &self,
        author: &Author,
        care_location: String,
        thread_id: &str,
        message: MessageContent,
    ) -> PatientResult<String> {
        author.validate_commit_author()?;
        validate_thread_id(thread_id)?;

        let msg = VprCommitMessage::new(
            VprCommitDomain::Coordination(Messaging),
            VprCommitAction::Update,
            "Added message to thread",
            care_location,
        )?;

        let coordination_uuid = ShardableUuid::parse(&self.coordination_id().simple().to_string())?;
        let patient_dir = self.coordination_patient_dir(&coordination_uuid);
        let thread_dir = thread_dir(&patient_dir, thread_id);
        let messages_path = thread_dir.join("messages.md");

        if !messages_path.exists() {
            return Err(PatientError::InvalidInput(format!(
                "Thread does not exist: {}",
                thread_id
            )));
        }

        // Generate message ID and timestamp
        let message_id = generate_message_id();
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        // Format message
        let formatted_message = format_message(&message, message_id, &now);

        // Read existing content and append
        let existing_content =
            fs::read_to_string(&messages_path).map_err(PatientError::FileRead)?;
        let new_content = format!("{}{}", existing_content, formatted_message);

        // Update ledger last_updated_at
        let ledger_path = thread_dir.join("ledger.yaml");
        let ledger_content = fs::read_to_string(&ledger_path).map_err(PatientError::FileRead)?;
        let mut ledger = deserialize_ledger(&ledger_content)?;
        ledger.last_updated_at = now.clone();
        let updated_ledger_content = serialize_ledger(&ledger)?;

        // Write and commit
        let messages_relative = messages_path
            .strip_prefix(&patient_dir)
            .map_err(|_| PatientError::InvalidInput("Invalid path prefix".to_string()))?;
        let ledger_relative = ledger_path
            .strip_prefix(&patient_dir)
            .map_err(|_| PatientError::InvalidInput("Invalid path prefix".to_string()))?;

        let files_to_write = [
            FileToWrite {
                relative_path: messages_relative,
                content: &new_content,
                old_content: Some(&existing_content),
            },
            FileToWrite {
                relative_path: ledger_relative,
                content: &updated_ledger_content,
                old_content: Some(&ledger_content),
            },
        ];

        let repo = VersionedFileService::open(&patient_dir)?;
        repo.write_and_commit_files(author, &msg, &files_to_write)?;

        Ok(message_id.to_string())
    }

    /// Reads an entire thread (messages.md + ledger.yaml).
    ///
    /// Returns structured data containing:
    /// - Thread metadata from ledger.yaml
    /// - All messages parsed from messages.md
    /// - Correction relationships resolved
    pub fn read_thread(&self, thread_id: &str) -> PatientResult<Thread> {
        validate_thread_id(thread_id)?;

        let coordination_uuid = ShardableUuid::parse(&self.coordination_id().simple().to_string())?;
        let patient_dir = self.coordination_patient_dir(&coordination_uuid);
        let thread_dir = thread_dir(&patient_dir, thread_id);
        let messages_path = thread_dir.join("messages.md");
        let ledger_path = thread_dir.join("ledger.yaml");

        if !messages_path.exists() || !ledger_path.exists() {
            return Err(PatientError::InvalidInput(format!(
                "Thread does not exist: {}",
                thread_id
            )));
        }

        // Read and parse files
        let messages_content =
            fs::read_to_string(&messages_path).map_err(PatientError::FileRead)?;
        let ledger_content = fs::read_to_string(&ledger_path).map_err(PatientError::FileRead)?;

        let messages = parse_messages_md(&messages_content)?;
        let ledger = deserialize_ledger(&ledger_content)?;

        Ok(Thread {
            thread_id: thread_id.to_string(),
            ledger,
            messages,
        })
    }

    /// Updates thread ledger (participants, status, policies).
    ///
    /// Rewrites ledger.yaml with updated metadata.
    /// Does NOT modify messages.md.
    ///
    /// Atomic operation: write + Git commit.
    pub fn update_thread_ledger(
        &self,
        author: &Author,
        care_location: String,
        thread_id: &str,
        ledger_update: LedgerUpdate,
    ) -> PatientResult<()> {
        author.validate_commit_author()?;
        validate_thread_id(thread_id)?;

        let msg = VprCommitMessage::new(
            VprCommitDomain::Coordination(Messaging),
            VprCommitAction::Update,
            "Updated thread ledger",
            care_location,
        )?;

        let coordination_uuid = ShardableUuid::parse(&self.coordination_id().simple().to_string())?;
        let patient_dir = self.coordination_patient_dir(&coordination_uuid);
        let thread_dir = thread_dir(&patient_dir, thread_id);
        let ledger_path = thread_dir.join("ledger.yaml");

        if !ledger_path.exists() {
            return Err(PatientError::InvalidInput(format!(
                "Thread does not exist: {}",
                thread_id
            )));
        }

        // Read existing ledger
        let ledger_content = fs::read_to_string(&ledger_path).map_err(PatientError::FileRead)?;
        let mut ledger = deserialize_ledger(&ledger_content)?;

        // Apply updates
        if let Some(add_participants) = ledger_update.add_participants {
            ledger.participants.extend(add_participants);
        }
        if let Some(remove_ids) = ledger_update.remove_participants {
            ledger
                .participants
                .retain(|p| !remove_ids.contains(&p.participant_id));
        }
        if let Some(status) = ledger_update.set_status {
            ledger.status = status;
        }
        if let Some(visibility) = ledger_update.set_visibility {
            ledger.visibility = visibility;
        }
        if let Some(policies) = ledger_update.set_policies {
            ledger.policies = policies;
        }

        // Update timestamp
        ledger.last_updated_at =
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        // Serialize and write
        let new_content = serialize_ledger(&ledger)?;

        let ledger_relative = ledger_path
            .strip_prefix(&patient_dir)
            .map_err(|_| PatientError::InvalidInput("Invalid path prefix".to_string()))?;

        let files_to_write = [FileToWrite {
            relative_path: ledger_relative,
            content: &new_content,
            old_content: Some(&ledger_content),
        }];

        let repo = VersionedFileService::open(&patient_dir)?;
        repo.write_and_commit_files(author, &msg, &files_to_write)?;

        Ok(())
    }
}

// ============================================================================
// DATA STRUCTURES
// ============================================================================

/// Represents a participant in a messaging thread.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ThreadParticipant {
    pub participant_id: Uuid,
    pub display_name: String,
    pub role: ParticipantRole,
}

/// Role of a participant in care coordination.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ParticipantRole {
    Clinician,
    CareAdministrator,
    Patient,
    PatientAssociate,
    System,
}

/// Content of a message to be added to a thread.
#[derive(Clone, Debug)]
pub struct MessageContent {
    pub message_type: MessageType,
    pub author_id: Uuid,
    pub author_display_name: String,
    pub body: String,           // Markdown content
    pub corrects: Option<Uuid>, // For correction messages
}

/// Type of message in the thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageType {
    Clinician,
    Patient,
    System,
    Correction,
}

/// Complete thread data (messages + ledger).
#[derive(Clone, Debug)]
pub struct Thread {
    pub thread_id: String,
    pub ledger: ThreadLedger,
    pub messages: Vec<Message>,
}

/// Parsed message from messages.md.
#[derive(Clone, Debug)]
pub struct Message {
    pub message_id: Uuid,
    pub timestamp: String, // ISO 8601
    pub message_type: MessageType,
    pub author_id: Uuid,
    pub author_display_name: String,
    pub body: String, // Markdown content
    pub corrects: Option<Uuid>,
}

/// Thread context and policy metadata from ledger.yaml.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ThreadLedger {
    pub thread_id: String,
    pub status: ThreadStatus,
    pub created_at: String,
    pub last_updated_at: String,
    pub participants: Vec<ThreadParticipant>,
    pub visibility: Visibility,
    pub policies: Policies,
}

/// Status of a messaging thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ThreadStatus {
    Open,
    Closed,
    Archived,
}

/// Visibility and sensitivity settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Visibility {
    pub sensitivity: SensitivityLevel,
    pub restricted: bool,
}

/// Sensitivity classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SensitivityLevel {
    Standard,
    Confidential,
    Restricted,
}

/// Thread access and participation policies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Policies {
    pub allow_patient_participation: bool,
    pub allow_external_organisations: bool,
}

/// Update to apply to a thread ledger.
#[derive(Clone, Debug, Default)]
pub struct LedgerUpdate {
    pub add_participants: Option<Vec<ThreadParticipant>>,
    pub remove_participants: Option<Vec<Uuid>>,
    pub set_status: Option<ThreadStatus>,
    pub set_visibility: Option<Visibility>,
    pub set_policies: Option<Policies>,
}

impl<S> CoordinationService<S> {
    /// Returns the path to the coordination records directory.
    fn coordination_dir(&self) -> PathBuf {
        let data_dir = self.cfg.patient_data_dir().to_path_buf();
        data_dir.join(COORDINATION_DIR_NAME)
    }

    /// Returns the path to a specific patient's coordination record directory.
    fn coordination_patient_dir(&self, coordination_uuid: &ShardableUuid) -> PathBuf {
        let coordination_dir = self.coordination_dir();
        coordination_uuid.sharded_dir(&coordination_dir)
    }
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Generates a new thread ID using TimestampIdGenerator.
///
/// Format: YYYYMMDDTHHMMSS.sssZ-UUID
fn generate_thread_id() -> PatientResult<String> {
    let timestamp_id = TimestampIdGenerator::generate(None)?;
    Ok(timestamp_id.to_string())
}

/// Generates a new message ID (UUID v4).
fn generate_message_id() -> Uuid {
    Uuid::new_v4()
}

/// Validates thread ID format.
fn validate_thread_id(thread_id: &str) -> PatientResult<()> {
    // Validate using TimestampId parsing
    use std::str::FromStr;
    let _timestamp_id = vpr_uuid::TimestampId::from_str(thread_id)?;
    Ok(())
}

/// Returns path to a specific thread directory.
fn thread_dir(coordination_patient_dir: &Path, thread_id: &str) -> PathBuf {
    coordination_patient_dir
        .join("communications")
        .join(thread_id)
}

/// Formats a message for appending to messages.md.
fn format_message(message: &MessageContent, message_id: Uuid, timestamp: &str) -> String {
    let message_type_str = match message.message_type {
        MessageType::Clinician => "clinician",
        MessageType::Patient => "patient",
        MessageType::System => "system",
        MessageType::Correction => "correction",
    };

    let mut output = format!(
        "**Message ID:** `{}`  \n**Timestamp:** {}  \n**Author ID:** `{}`  \n**Author:** {}  \n**Role:** {}  \n",
        message_id,
        timestamp,
        message.author_id,
        message.author_display_name,
        message_type_str
    );

    if let Some(corrects_id) = message.corrects {
        output.push_str(&format!("**Corrects:** `{}`  \n", corrects_id));
    }

    output.push_str(&format!("\n{}\n\n---\n", message.body));
    output
}

/// Parses messages.md into structured Message objects.
fn parse_messages_md(content: &str) -> PatientResult<Vec<Message>> {
    let mut messages = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        // Look for message metadata (starts with **Message ID:**)
        if lines[i].starts_with("**Message ID:**") {
            // Parse metadata
            let mut message_id: Option<Uuid> = None;
            let mut message_type: Option<MessageType> = None;
            let mut timestamp: Option<String> = None;
            let mut author_id: Option<Uuid> = None;
            let mut author_display_name: Option<String> = None;
            let mut corrects: Option<Uuid> = None;

            while i < lines.len() && lines[i].starts_with("**") {
                let line = lines[i];
                if line.starts_with("**Message ID:**") {
                    let id_str = line
                        .trim_start_matches("**Message ID:** `")
                        .trim_end_matches("`  ");
                    message_id =
                        Some(Uuid::parse_str(id_str).map_err(|e| {
                            PatientError::InvalidInput(format!("Invalid UUID: {}", e))
                        })?);
                } else if line.starts_with("**Role:**") {
                    let type_str = line.trim_start_matches("**Role:** ").trim();
                    message_type = Some(match type_str {
                        "clinician" => MessageType::Clinician,
                        "patient" => MessageType::Patient,
                        "system" => MessageType::System,
                        "correction" => MessageType::Correction,
                        _ => {
                            return Err(PatientError::InvalidInput(format!(
                                "Invalid message type: {}",
                                type_str
                            )))
                        }
                    });
                } else if line.starts_with("**Timestamp:**") {
                    timestamp = Some(
                        line.trim_start_matches("**Timestamp:** ")
                            .trim()
                            .to_string(),
                    );
                } else if line.starts_with("**Author ID:**") {
                    let id_str = line
                        .trim_start_matches("**Author ID:** `")
                        .trim_end_matches("`  ");
                    author_id =
                        Some(Uuid::parse_str(id_str).map_err(|e| {
                            PatientError::InvalidInput(format!("Invalid UUID: {}", e))
                        })?);
                } else if line.starts_with("**Author:**") {
                    author_display_name =
                        Some(line.trim_start_matches("**Author:** ").trim().to_string());
                } else if line.starts_with("**Corrects:**") {
                    let id_str = line
                        .trim_start_matches("**Corrects:** `")
                        .trim_end_matches("`  ");
                    corrects =
                        Some(Uuid::parse_str(id_str).map_err(|e| {
                            PatientError::InvalidInput(format!("Invalid UUID: {}", e))
                        })?);
                }
                i += 1;
            }

            // Skip blank line before body
            if i < lines.len() && lines[i].is_empty() {
                i += 1;
            }

            // Read body until separator
            let mut body_lines = Vec::new();
            while i < lines.len() && lines[i] != "---" {
                body_lines.push(lines[i]);
                i += 1;
            }

            let body = body_lines.join("\n").trim().to_string();

            // Construct message
            if let (Some(msg_id), Some(msg_type), Some(ts), Some(auth_id), Some(auth_name)) = (
                message_id,
                message_type,
                timestamp,
                author_id,
                author_display_name,
            ) {
                messages.push(Message {
                    message_id: msg_id,
                    timestamp: ts,
                    message_type: msg_type,
                    author_id: auth_id,
                    author_display_name: auth_name,
                    body,
                    corrects,
                });
            }
        }
        i += 1;
    }

    Ok(messages)
}

/// Serializes ThreadLedger to YAML for ledger.yaml.
fn serialize_ledger(ledger: &ThreadLedger) -> PatientResult<String> {
    // Convert ThreadLedger to fhir::LedgerData
    let ledger_data = fhir::LedgerData {
        thread_id: ledger
            .thread_id
            .parse()
            .map_err(|e| PatientError::InvalidInput(format!("Invalid thread_id format: {}", e)))?,
        status: match ledger.status {
            ThreadStatus::Open => fhir::ThreadStatus::Open,
            ThreadStatus::Closed => fhir::ThreadStatus::Closed,
            ThreadStatus::Archived => fhir::ThreadStatus::Archived,
        },
        created_at: chrono::DateTime::parse_from_rfc3339(&ledger.created_at)
            .map_err(|e| PatientError::InvalidInput(format!("Invalid created_at: {}", e)))?
            .with_timezone(&chrono::Utc),
        last_updated_at: chrono::DateTime::parse_from_rfc3339(&ledger.last_updated_at)
            .map_err(|e| PatientError::InvalidInput(format!("Invalid last_updated_at: {}", e)))?
            .with_timezone(&chrono::Utc),
        participants: ledger
            .participants
            .iter()
            .map(|p| fhir::LedgerParticipant {
                participant_id: p.participant_id,
                display_name: p.display_name.clone(),
                role: match p.role {
                    ParticipantRole::Clinician => fhir::ParticipantRole::Clinician,
                    ParticipantRole::CareAdministrator => fhir::ParticipantRole::CareAdministrator,
                    ParticipantRole::Patient => fhir::ParticipantRole::Patient,
                    ParticipantRole::PatientAssociate => fhir::ParticipantRole::PatientAssociate,
                    ParticipantRole::System => fhir::ParticipantRole::System,
                },
                organisation: None,
            })
            .collect(),
        visibility: fhir::LedgerVisibility {
            sensitivity: match ledger.visibility.sensitivity {
                SensitivityLevel::Standard => "standard".to_string(),
                SensitivityLevel::Confidential => "confidential".to_string(),
                SensitivityLevel::Restricted => "restricted".to_string(),
            },
            restricted: ledger.visibility.restricted,
        },
        policies: fhir::LedgerPolicies {
            allow_patient_participation: ledger.policies.allow_patient_participation,
            allow_external_organisations: ledger.policies.allow_external_organisations,
        },
    };

    fhir::Messaging::ledger_render(&ledger_data)
        .map_err(|e| PatientError::InvalidInput(format!("Failed to serialize ledger: {}", e)))
}

/// Deserializes ledger.yaml into ThreadLedger.
fn deserialize_ledger(content: &str) -> PatientResult<ThreadLedger> {
    // Parse using fhir::Messaging
    let ledger_data = fhir::Messaging::ledger_parse(content)
        .map_err(|e| PatientError::InvalidInput(format!("Failed to deserialize ledger: {}", e)))?;

    // Convert fhir::LedgerData to ThreadLedger
    Ok(ThreadLedger {
        thread_id: ledger_data.thread_id.to_string(),
        status: match ledger_data.status {
            fhir::ThreadStatus::Open => ThreadStatus::Open,
            fhir::ThreadStatus::Closed => ThreadStatus::Closed,
            fhir::ThreadStatus::Archived => ThreadStatus::Archived,
        },
        created_at: ledger_data
            .created_at
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        last_updated_at: ledger_data
            .last_updated_at
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        participants: ledger_data
            .participants
            .iter()
            .map(|p| ThreadParticipant {
                participant_id: p.participant_id,
                display_name: p.display_name.clone(),
                role: match p.role {
                    fhir::ParticipantRole::Clinician => ParticipantRole::Clinician,
                    fhir::ParticipantRole::CareAdministrator => ParticipantRole::CareAdministrator,
                    fhir::ParticipantRole::Patient => ParticipantRole::Patient,
                    fhir::ParticipantRole::PatientAssociate => ParticipantRole::PatientAssociate,
                    fhir::ParticipantRole::System => ParticipantRole::System,
                },
            })
            .collect(),
        visibility: Visibility {
            sensitivity: match ledger_data.visibility.sensitivity.as_str() {
                "confidential" => SensitivityLevel::Confidential,
                "restricted" => SensitivityLevel::Restricted,
                _ => SensitivityLevel::Standard,
            },
            restricted: ledger_data.visibility.restricted,
        },
        policies: Policies {
            allow_patient_participation: ledger_data.policies.allow_patient_participation,
            allow_external_organisations: ledger_data.policies.allow_external_organisations,
        },
    })
}
#[cfg(test)]
static FORCE_CLEANUP_ERROR_FOR_THREADS: LazyLock<Mutex<HashSet<std::thread::ThreadId>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

#[cfg(test)]
#[allow(dead_code)]
fn force_cleanup_error_for_current_thread() {
    let mut guard = FORCE_CLEANUP_ERROR_FOR_THREADS
        .lock()
        .expect("FORCE_CLEANUP_ERROR_FOR_THREADS mutex poisoned");
    guard.insert(std::thread::current().id());
}

/// Removes a patient directory and all its contents.
///
/// This is a wrapper around [`std::fs::remove_dir_all`] with test instrumentation.
fn remove_patient_dir_all(patient_dir: &Path) -> io::Result<()> {
    #[cfg(test)]
    {
        let current_id = std::thread::current().id();
        let mut guard = FORCE_CLEANUP_ERROR_FOR_THREADS
            .lock()
            .expect("FORCE_CLEANUP_ERROR_FOR_THREADS mutex poisoned");

        if guard.remove(&current_id) {
            return Err(io::Error::other("forced cleanup error"));
        }
    }

    fs::remove_dir_all(patient_dir)
}
// ============================================================================
// TESTS
// ============================================================================

// #[cfg(test)]
// mod tests {
//     // Test initialise() creates coordination repo without template
//     // Test initialise() creates Git repo and initial commit
//     // Test initialise() validates author and care_location
//     // Test initialise() cleans up on failure
//     //
//     // Test create_thread() creates directory structure
//     // Test create_thread() generates valid thread_id
//     // Test create_thread() writes messages.md and ledger.yaml
//     // Test create_thread() commits to Git
//     // Test create_thread() with initial message
//     // Test create_thread() without initial message
//     //
//     // Test add_message() appends to messages.md
//     // Test add_message() generates unique message_id
//     // Test add_message() commits to Git
//     // Test add_message() for clinician message
//     // Test add_message() for patient message
//     // Test add_message() for system message
//     // Test add_message() for correction message with corrects field
//     //
//     // Test read_thread() parses messages.md correctly
//     // Test read_thread() parses ledger.yaml correctly
//     // Test read_thread() fails gracefully if thread doesn't exist
//     // Test read_thread() handles empty threads (no messages)
//     //
//     // Test update_thread_ledger() adds participants
//     // Test update_thread_ledger() removes participants
//     // Test update_thread_ledger() changes status
//     // Test update_thread_ledger() updates visibility
//     // Test update_thread_ledger() updates policies
//     // Test update_thread_ledger() commits to Git
//     //
//     // Test message immutability (cannot edit/delete)
//     // Test thread_id format validation
//     // Test message_id uniqueness
//     // Test correction message references
//     // Test concurrent message additions
//     // Test cryptographic signing of commits
// }
