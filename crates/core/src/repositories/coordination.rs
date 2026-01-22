//! Care Coordination Repository (CCR) Management.
//!
//! This module manages care coordination records, focusing on asynchronous
//! clinical communication and task management between clinicians, patients, and
//! other authorised participants.
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
use crate::markdown::{MarkdownService, MessageMetadata};
use crate::repositories::shared::create_uuid_and_shard_dir;
use crate::versioned_files::{
    CoordinationDomain, FileToWrite, VersionedFileService, VprCommitAction, VprCommitDomain,
    VprCommitMessage,
};
use crate::ShardableUuid;
use chrono::Utc;
use fhir::{
    AuthorRole, CoordinationStatus, CoordinationStatusData, LedgerData, LifecycleState,
    MessageAuthor, Messaging as FhirMessaging, SensitivityLevel, ThreadStatus as FhirThreadStatus,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;
use vpr_uuid::{TimestampId, TimestampIdGenerator};
use CoordinationDomain::*;

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
#[derive(Clone, Debug)]
pub struct Initialised {
    coordination_id: ShardableUuid,
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

    /// Initialises a new coordination repository for a patient.
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

        let coordination_root_dir = self.coordination_root_dir();
        let (coordination_uuid, patient_dir) = create_uuid_and_shard_dir(&coordination_root_dir)?;

        // Create COORDINATION_STATUS.yaml contents with link to clinical record
        let status_data = CoordinationStatusData {
            coordination_id: coordination_uuid.uuid(),
            clinical_id,
            lifecycle_state: LifecycleState::Active,
            record_open: true,
            record_queryable: true,
            record_modifiable: true,
        };

        let status_yaml = CoordinationStatus::render(&status_data)?;

        let status_file = FileToWrite {
            relative_path: Path::new("COORDINATION_STATUS.yaml"),
            content: &status_yaml,
            old_content: None,
        };

        VersionedFileService::init_and_commit_with_cleanup(
            &patient_dir,
            &author,
            &commit_message,
            &[status_file],
        )?;

        Ok(CoordinationService {
            cfg: self.cfg,
            state: Initialised {
                coordination_id: coordination_uuid,
            },
        })
    }
}

impl CoordinationService<Initialised> {
    /// Creates coordination service for existing record.
    pub fn with_id(cfg: Arc<CoreConfig>, coordination_id: Uuid) -> Self {
        Self {
            cfg,
            state: Initialised {
                coordination_id: ShardableUuid::from_uuid(coordination_id),
            },
        }
    }

    /// Returns the coordination UUID.
    pub fn coordination_id(&self) -> &ShardableUuid {
        &self.state.coordination_id
    }
}

// ============================================================================
// MESSAGING OPERATIONS
// ============================================================================

impl CoordinationService<Initialised> {
    /// Creates a new messaging thread in the coordination repository.
    ///
    /// Creates a new communication thread for asynchronous messaging between care team
    /// participants. The thread is initialised with metadata (participants, policies,
    /// visibility) and optionally an initial message.
    ///
    /// Creates:
    /// - `communications/{thread_id}/` directory
    /// - `ledger.yaml` with participants, status, policies, and visibility settings
    /// - `messages.md` with optional initial message
    /// - Git commit with thread creation
    ///
    /// Thread ID format: `YYYYMMDDTHHMMSS.sssZ-UUID` (timestamp-based for ordering).
    ///
    /// # Arguments
    ///
    /// * `author` - The author creating the thread (validated for commit permissions)
    /// * `care_location` - The care location context for the Git commit message
    /// * `authors` - Initial list of thread participants with roles
    /// * `initial_message` - Initial message to include when creating the thread (required)
    ///
    /// # Returns
    ///
    /// The generated thread ID on success.
    ///
    /// # Errors
    ///
    /// Returns `PatientError` if:
    /// - Author validation fails - [`PatientError::InvalidInput`], [`PatientError::MissingCommitAuthor`]
    /// - Thread ID generation fails - [`PatientError::TimestampIdError`]
    /// - Directory creation fails - [`PatientError::PatientDirCreation`]
    /// - Ledger serialization fails - [`PatientError::InvalidInput`]
    /// - File write or Git commit fails - [`PatientError::FileWrite`], various Git errors
    /// - Initial message body is empty - [`PatientError::InvalidInput`]
    pub fn thread_create(
        &self,
        author: &Author,
        care_location: String,
        message_authors: Vec<MessageAuthor>,
        initial_message: MessageContent,
    ) -> PatientResult<TimestampId> {
        author.validate_commit_author()?;

        let msg = VprCommitMessage::new(
            VprCommitDomain::Coordination(Messaging),
            VprCommitAction::Create,
            "Created messaging thread",
            care_location,
        )?;

        validate_message_authors(&message_authors)?;

        let thread_id = TimestampIdGenerator::generate(None)?;
        let coordination_dir = self.coordination_dir(self.coordination_id());

        let now = Utc::now();
        let message_id = generate_message_id();

        let metadata = MessageMetadata {
            message_id,
            timestamp: now,
            author: initial_message.author().clone(),
        };

        let markdown_service = MarkdownService::new();
        let messages_content =
            markdown_service.new_thread_render("Messages", &metadata, initial_message.body())?;

        let ledger = LedgerData {
            thread_id,
            status: FhirThreadStatus::Open,
            created_at: now,
            last_updated_at: now,
            participants: message_authors.clone(),
            sensitivity: SensitivityLevel::Standard,
            restricted: false,
            allow_patient_participation: true,
            allow_external_organisations: true,
        };

        let ledger_content = FhirMessaging::ledger_render(&ledger)?;

        // Construct relative paths directly
        let thread_relative_dir = Path::new("communications").join(ledger.thread_id.to_string());
        let messages_relative = thread_relative_dir.join("messages.md");
        let ledger_relative = thread_relative_dir.join("ledger.yaml");

        let files_to_write = [
            FileToWrite {
                relative_path: &messages_relative,
                content: &messages_content,
                old_content: None,
            },
            FileToWrite {
                relative_path: &ledger_relative,
                content: &ledger_content,
                old_content: None,
            },
        ];

        VersionedFileService::write_and_commit_files(
            &coordination_dir,
            author,
            &msg,
            &files_to_write,
        )?;

        Ok(ledger.thread_id)
    }

    /// Adds a message to an existing thread.
    ///
    /// Appends to messages.md:
    /// - Message metadata (id, timestamp, author, type)
    /// - Message content (markdown body)
    /// - Correction reference if applicable
    ///
    /// Atomic operation: write + Git commit.
    pub fn message_add(
        &self,
        author: &Author,
        care_location: String,
        thread_id: &TimestampId,
        new_message: MessageContent,
    ) -> PatientResult<String> {
        author.validate_commit_author()?;

        let msg = VprCommitMessage::new(
            VprCommitDomain::Coordination(Messaging),
            VprCommitAction::Update,
            "Added message to thread",
            care_location,
        )?;

        let thread_dir = self.thread_dir(thread_id);
        let messages_path = thread_dir.join("messages.md");

        if !messages_path.exists() {
            return Err(PatientError::InvalidInput(format!(
                "Thread does not exist: {}",
                thread_id
            )));
        }

        // Generate message ID and timestamp
        let message_id = generate_message_id();
        let now = Utc::now();

        // Format message using MarkdownService
        let metadata = MessageMetadata {
            message_id,
            timestamp: now,
            author: new_message.author().clone(),
        };

        let markdown_service = MarkdownService::new();
        let new_message_rendered = markdown_service.message_render(
            &metadata,
            new_message.body(),
            new_message.corrects(),
        )?;

        // Read existing content and append
        let existing_content =
            fs::read_to_string(&messages_path).map_err(PatientError::FileRead)?;
        let messages_updated = format!("{}{}", existing_content, new_message_rendered);

        // Update ledger last_updated_at
        let ledger_path = thread_dir.join("ledger.yaml");
        let ledger_content = fs::read_to_string(&ledger_path).map_err(PatientError::FileRead)?;
        let mut ledger_data = FhirMessaging::ledger_parse(&ledger_content)?;
        ledger_data.last_updated_at = now;

        let updated_ledger_content = FhirMessaging::ledger_render(&ledger_data)?;

        // Write and commit
        let coordination_dir = self.coordination_dir(self.coordination_id());
        let thread_relative_dir = Path::new("communications").join(thread_id.to_string());
        let messages_relative = thread_relative_dir.join("messages.md");
        let ledger_relative = thread_relative_dir.join("ledger.yaml");

        let files_to_write = [
            FileToWrite {
                relative_path: &messages_relative,
                content: &messages_updated,
                old_content: Some(&existing_content),
            },
            FileToWrite {
                relative_path: &ledger_relative,
                content: &updated_ledger_content,
                old_content: Some(&ledger_content),
            },
        ];

        VersionedFileService::write_and_commit_files(
            &coordination_dir,
            author,
            &msg,
            &files_to_write,
        )?;

        Ok(message_id.to_string())
    }

    /// Reads an entire thread (messages.md + ledger.yaml).
    ///
    /// Returns structured data containing:
    /// - Thread metadata from ledger.yaml
    /// - All messages parsed from messages.md
    /// - Correction relationships resolved
    pub fn read_thread(&self, thread_id: &TimestampId) -> PatientResult<Thread> {
        let thread_dir = self.thread_dir(thread_id);
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
        let ledger = FhirMessaging::ledger_parse(&ledger_content)?;

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
        thread_id: &TimestampId,
        ledger_update: LedgerUpdate,
    ) -> PatientResult<()> {
        author.validate_commit_author()?;

        let msg = VprCommitMessage::new(
            VprCommitDomain::Coordination(Messaging),
            VprCommitAction::Update,
            "Updated thread ledger",
            care_location,
        )?;

        let thread_dir = self.thread_dir(thread_id);
        let ledger_path = thread_dir.join("ledger.yaml");

        if !ledger_path.exists() {
            return Err(PatientError::InvalidInput(format!(
                "Thread does not exist: {}",
                thread_id
            )));
        }

        // Read existing ledger
        let ledger_content = fs::read_to_string(&ledger_path).map_err(PatientError::FileRead)?;
        let mut ledger_data = FhirMessaging::ledger_parse(&ledger_content)?;

        // Apply updates
        if let Some(add_participants) = ledger_update.add_participants {
            ledger_data.participants.extend(add_participants);
        }
        if let Some(remove_ids) = ledger_update.remove_participants {
            ledger_data
                .participants
                .retain(|p| !remove_ids.contains(&p.id));
        }
        if let Some(status) = ledger_update.set_status {
            ledger_data.status = status;
        }
        if let Some((sensitivity, restricted)) = ledger_update.set_visibility {
            ledger_data.sensitivity = sensitivity;
            ledger_data.restricted = restricted;
        }
        if let Some((allow_patient, allow_external)) = ledger_update.set_policies {
            ledger_data.allow_patient_participation = allow_patient;
            ledger_data.allow_external_organisations = allow_external;
        }

        // Update timestamp
        ledger_data.last_updated_at = Utc::now();

        let new_content = FhirMessaging::ledger_render(&ledger_data)?;

        let coordination_dir = self.coordination_dir(self.coordination_id());
        let thread_relative_dir = Path::new("communications").join(thread_id.to_string());
        let ledger_relative = thread_relative_dir.join("ledger.yaml");

        let files_to_write = [FileToWrite {
            relative_path: &ledger_relative,
            content: &new_content,
            old_content: Some(&ledger_content),
        }];

        VersionedFileService::write_and_commit_files(
            &coordination_dir,
            author,
            &msg,
            &files_to_write,
        )?;

        Ok(())
    }
}

// ============================================================================
// DATA STRUCTURES
// ============================================================================

/// Content of a message to be added to a thread.
#[derive(Clone, Debug)]
pub struct MessageContent {
    author: MessageAuthor,
    body: String,
    corrects: Option<Uuid>, // For correction messages
}

impl MessageContent {
    /// Creates a new message with validated content.
    ///
    /// # Arguments
    ///
    /// * `author` - The author of the message
    /// * `body` - The message body (must not be empty after trimming)
    /// * `corrects` - Optional UUID of a message this corrects
    ///
    /// # Errors
    ///
    /// Returns `PatientError::InvalidInput` if the body is empty or only whitespace.
    pub fn new(author: MessageAuthor, body: String, corrects: Option<Uuid>) -> PatientResult<Self> {
        if body.trim().is_empty() {
            return Err(PatientError::InvalidInput(
                "Message body must not be empty".to_string(),
            ));
        }
        Ok(Self {
            author,
            body,
            corrects,
        })
    }

    /// Returns a reference to the message author.
    pub fn author(&self) -> &MessageAuthor {
        &self.author
    }

    /// Returns the message body as a string slice.
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Returns the UUID of the message this corrects, if any.
    pub fn corrects(&self) -> Option<Uuid> {
        self.corrects
    }
}

/// Complete thread data (messages + ledger).
#[derive(Clone, Debug)]
pub struct Thread {
    pub thread_id: String,
    pub ledger: LedgerData,
    pub messages: Vec<Message>,
}

/// Parsed message from messages.md.
#[derive(Clone, Debug)]
pub struct Message {
    pub message_id: Uuid,
    pub timestamp: String, // ISO 8601
    pub author: MessageAuthor,
    pub body: String, // Markdown content
    pub corrects: Option<Uuid>,
}

/// Update to apply to a thread ledger.
#[derive(Clone, Debug, Default)]
pub struct LedgerUpdate {
    pub add_participants: Option<Vec<MessageAuthor>>,
    pub remove_participants: Option<Vec<Uuid>>,
    pub set_status: Option<FhirThreadStatus>,
    pub set_visibility: Option<(SensitivityLevel, bool)>,
    pub set_policies: Option<(bool, bool)>,
}

impl<S> CoordinationService<S> {
    /// Returns the path to the coordination records directory.
    fn coordination_root_dir(&self) -> PathBuf {
        let data_dir = self.cfg.patient_data_dir().to_path_buf();
        data_dir.join(COORDINATION_DIR_NAME)
    }

    /// Returns the path to a specific patient's coordination record directory.
    fn coordination_dir(&self, coordination_id: &ShardableUuid) -> PathBuf {
        let coordination_root_dir = self.coordination_root_dir();
        coordination_id.sharded_dir(&coordination_root_dir)
    }
}

impl CoordinationService<Initialised> {
    /// Returns the path to a specific thread directory.
    ///
    /// Combines coordination directory lookup with thread path construction.
    fn thread_dir(&self, thread_id: &TimestampId) -> PathBuf {
        let coordination_dir = self.coordination_dir(self.coordination_id());
        coordination_dir
            .join("communications")
            .join(thread_id.to_string())
    }
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Generates a new message ID (UUID v4).
fn generate_message_id() -> Uuid {
    Uuid::new_v4()
}

/// Validates that authors list is not empty and all author names contain content.
///
/// # Arguments
///
/// * `authors` - List of thread participants to validate
///
/// # Errors
///
/// Returns `PatientError::InvalidInput` if:
/// - Authors list is empty
/// - Any author name is empty or whitespace-only
fn validate_message_authors(authors: &[MessageAuthor]) -> PatientResult<()> {
    if authors.is_empty() {
        return Err(PatientError::InvalidInput(
            "Authors list must not be empty".to_string(),
        ));
    }

    for author in authors {
        if author.name.trim().is_empty() {
            return Err(PatientError::InvalidInput(
                "Author name must not be empty".to_string(),
            ));
        }
    }

    Ok(())
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
            let mut author_role: Option<AuthorRole> = None;
            let mut timestamp: Option<String> = None;
            let mut author_id: Option<Uuid> = None;
            let mut author_name: Option<String> = None;
            let mut corrects: Option<Uuid> = None;

            while i < lines.len() && lines[i].starts_with("**") {
                let line = lines[i];
                if line.starts_with("**Message ID:**") {
                    let id_str = line.trim_start_matches("**Message ID:** ").trim();
                    message_id =
                        Some(Uuid::parse_str(id_str).map_err(|e| {
                            PatientError::InvalidInput(format!("Invalid UUID: {}", e))
                        })?);
                } else if line.starts_with("**Author role:**") {
                    let role_str = line.trim_start_matches("**Author role:** ").trim();
                    author_role = Some(
                        fhir::AuthorRole::parse(role_str)
                            .map_err(|e| PatientError::InvalidInput(e.to_string()))?,
                    );
                } else if line.starts_with("**Timestamp:**") {
                    timestamp = Some(
                        line.trim_start_matches("**Timestamp:** ")
                            .trim()
                            .to_string(),
                    );
                } else if line.starts_with("**Author ID:**") {
                    let id_str = line.trim_start_matches("**Author ID:** ").trim();
                    author_id =
                        Some(Uuid::parse_str(id_str).map_err(|e| {
                            PatientError::InvalidInput(format!("Invalid UUID: {}", e))
                        })?);
                } else if line.starts_with("**Author name:**") {
                    author_name = Some(
                        line.trim_start_matches("**Author name:** ")
                            .trim()
                            .to_string(),
                    );
                } else if line.starts_with("**Corrects:**") {
                    let id_str = line.trim_start_matches("**Corrects:** ").trim();
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
            if let (Some(msg_id), Some(role), Some(ts), Some(auth_id), Some(auth_name)) =
                (message_id, author_role, timestamp, author_id, author_name)
            {
                messages.push(Message {
                    message_id: msg_id,
                    timestamp: ts,
                    author: MessageAuthor {
                        id: auth_id,
                        name: auth_name,
                        role,
                    },
                    body,
                    corrects,
                });
            }
        }
        i += 1;
    }

    Ok(messages)
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_env() -> (TempDir, TempDir, Arc<CoreConfig>, Author) {
        let temp_dir = TempDir::new().unwrap();
        let template_dir = TempDir::new().unwrap();

        let cfg = Arc::new(
            CoreConfig::new(
                temp_dir.path().to_path_buf(),
                template_dir.path().to_path_buf(),
                openehr::RmVersion::rm_1_1_0,
                "test-namespace".to_string(),
            )
            .unwrap(),
        );

        let author = Author {
            name: "Dr. Test".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        (temp_dir, template_dir, cfg, author)
    }

    fn create_test_participants() -> Vec<MessageAuthor> {
        vec![
            MessageAuthor {
                id: Uuid::new_v4(),
                name: "Dr. Smith".to_string(),
                role: fhir::AuthorRole::Clinician,
            },
            MessageAuthor {
                id: Uuid::new_v4(),
                name: "Patient John".to_string(),
                role: fhir::AuthorRole::Patient,
            },
        ]
    }

    #[test]
    fn test_initialise_creates_coordination_repo() {
        let (_temp, _template, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let result = CoordinationService::new(cfg.clone()).initialise(
            author,
            "Test Location".to_string(),
            clinical_id,
        );

        assert!(result.is_ok());
        let service = result.unwrap();
        let coord_dir = service.coordination_dir(service.coordination_id());
        assert!(coord_dir.exists());
        assert!(coord_dir.join(".git").exists());
    }

    #[test]
    fn test_initialise_validates_author() {
        let (_temp, _template, cfg, _author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let invalid_author = Author {
            name: "".to_string(),
            role: "Clinician".to_string(),
            email: "test@example.com".to_string(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        let result = CoordinationService::new(cfg.clone()).initialise(
            invalid_author,
            "Test Location".to_string(),
            clinical_id,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_thread_create_with_initial_message() {
        let (_temp, _template, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(author.clone(), "Test Location".to_string(), clinical_id)
            .unwrap();

        let participants = create_test_participants();
        let initial_message = MessageContent::new(
            participants[0].clone(),
            "Initial thread message".to_string(),
            None,
        )
        .unwrap();

        let result = service.thread_create(
            &author,
            "Test Location".to_string(),
            participants.clone(),
            initial_message,
        );

        assert!(result.is_ok());
        let thread_id = result.unwrap();

        // Verify thread directory exists
        let coord_dir = service.coordination_dir(service.coordination_id());
        let thread_dir = coord_dir.join("communications").join(thread_id.to_string());
        assert!(thread_dir.exists());
        assert!(thread_dir.join("messages.md").exists());
        assert!(thread_dir.join("ledger.yaml").exists());

        // Verify messages.md contains initial message
        let messages_content = fs::read_to_string(thread_dir.join("messages.md")).unwrap();
        assert!(messages_content.contains("Initial thread message"));
    }

    #[test]
    fn test_thread_create_validates_empty_body() {
        let (_temp, _template, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let _service = CoordinationService::new(cfg.clone())
            .initialise(author.clone(), "Test Location".to_string(), clinical_id)
            .unwrap();

        let participants = create_test_participants();
        let result = MessageContent::new(
            participants[0].clone(),
            "   ".to_string(), // Empty after trim
            None,
        );

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PatientError::InvalidInput(_)));
    }

    #[test]
    fn test_validate_message_authors_empty_list() {
        let result = validate_message_authors(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_message_authors_empty_name() {
        let authors = vec![MessageAuthor {
            id: Uuid::new_v4(),
            name: "   ".to_string(),
            role: fhir::AuthorRole::Clinician,
        }];

        let result = validate_message_authors(&authors);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_message_authors_valid() {
        let authors = create_test_participants();
        let result = validate_message_authors(&authors);
        assert!(result.is_ok());
    }

    #[test]
    fn test_message_add_appends_to_thread() {
        let (_temp, _template, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(author.clone(), "Test Location".to_string(), clinical_id)
            .unwrap();

        let participants = create_test_participants();
        let initial_message =
            MessageContent::new(participants[0].clone(), "First message".to_string(), None)
                .unwrap();

        let thread_id = service
            .thread_create(
                &author,
                "Test Location".to_string(),
                participants.clone(),
                initial_message,
            )
            .unwrap();

        // Add second message
        let second_message = MessageContent::new(
            participants[1].clone(),
            "Second message from patient".to_string(),
            None,
        )
        .unwrap();

        let result = service.message_add(
            &author,
            "Test Location".to_string(),
            &thread_id,
            second_message,
        );

        assert!(result.is_ok());

        // Read thread and verify both messages
        let thread = service.read_thread(&thread_id).unwrap();
        assert_eq!(thread.messages.len(), 2);
        assert_eq!(thread.messages[0].body, "First message");
        assert_eq!(thread.messages[1].body, "Second message from patient");
    }

    #[test]
    fn test_message_add_with_correction() {
        let (_temp, _template, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(author.clone(), "Test Location".to_string(), clinical_id)
            .unwrap();

        let participants = create_test_participants();
        let initial_message = MessageContent::new(
            participants[0].clone(),
            "Original message with typo".to_string(),
            None,
        )
        .unwrap();

        let thread_id = service
            .thread_create(
                &author,
                "Test Location".to_string(),
                participants.clone(),
                initial_message,
            )
            .unwrap();

        let thread = service.read_thread(&thread_id).unwrap();
        let original_msg_id = thread.messages[0].message_id;

        // Add correction message
        let correction = MessageContent::new(
            participants[0].clone(),
            "Corrected message without typo".to_string(),
            Some(original_msg_id),
        )
        .unwrap();

        let result =
            service.message_add(&author, "Test Location".to_string(), &thread_id, correction);
        assert!(result.is_ok());

        // Verify correction is recorded
        let thread = service.read_thread(&thread_id).unwrap();
        assert_eq!(thread.messages.len(), 2);
        assert_eq!(thread.messages[1].corrects, Some(original_msg_id));
    }

    #[test]
    fn test_message_add_to_nonexistent_thread() {
        let (_temp, _template, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(author.clone(), "Test Location".to_string(), clinical_id)
            .unwrap();

        let fake_thread_id = TimestampIdGenerator::generate(None).unwrap();
        let participants = create_test_participants();
        let message = MessageContent::new(
            participants[0].clone(),
            "Message to nowhere".to_string(),
            None,
        )
        .unwrap();

        let result = service.message_add(
            &author,
            "Test Location".to_string(),
            &fake_thread_id,
            message,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_read_thread_returns_complete_data() {
        let (_temp, _template, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(author.clone(), "Test Location".to_string(), clinical_id)
            .unwrap();

        let participants = create_test_participants();
        let initial_message =
            MessageContent::new(participants[0].clone(), "Test message".to_string(), None).unwrap();

        let thread_id = service
            .thread_create(
                &author,
                "Test Location".to_string(),
                participants.clone(),
                initial_message,
            )
            .unwrap();

        let thread = service.read_thread(&thread_id).unwrap();

        assert_eq!(thread.thread_id, thread_id.to_string());
        assert_eq!(thread.ledger.participants.len(), 2);
        assert_eq!(thread.messages.len(), 1);
        assert_eq!(thread.ledger.status, FhirThreadStatus::Open);
    }

    #[test]
    fn test_read_thread_nonexistent() {
        let (_temp, _template, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(author.clone(), "Test Location".to_string(), clinical_id)
            .unwrap();

        let fake_thread_id = TimestampIdGenerator::generate(None).unwrap();
        let result = service.read_thread(&fake_thread_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_thread_ledger_add_participants() {
        let (_temp, _template, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(author.clone(), "Test Location".to_string(), clinical_id)
            .unwrap();

        let participants = create_test_participants();
        let initial_message =
            MessageContent::new(participants[0].clone(), "Test".to_string(), None).unwrap();

        let thread_id = service
            .thread_create(
                &author,
                "Test Location".to_string(),
                participants,
                initial_message,
            )
            .unwrap();

        // Add new participant
        let new_participant = MessageAuthor {
            id: Uuid::new_v4(),
            name: "Nurse Jane".to_string(),
            role: fhir::AuthorRole::Clinician,
        };

        let update = LedgerUpdate {
            add_participants: Some(vec![new_participant.clone()]),
            ..Default::default()
        };

        let result =
            service.update_thread_ledger(&author, "Test Location".to_string(), &thread_id, update);
        assert!(result.is_ok());

        // Verify participant was added
        let thread = service.read_thread(&thread_id).unwrap();
        assert_eq!(thread.ledger.participants.len(), 3);
        assert!(thread
            .ledger
            .participants
            .iter()
            .any(|p| p.name == "Nurse Jane"));
    }

    #[test]
    fn test_update_thread_ledger_remove_participants() {
        let (_temp, _template, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(author.clone(), "Test Location".to_string(), clinical_id)
            .unwrap();

        let participants = create_test_participants();
        let remove_id = participants[1].id;
        let initial_message =
            MessageContent::new(participants[0].clone(), "Test".to_string(), None).unwrap();

        let thread_id = service
            .thread_create(
                &author,
                "Test Location".to_string(),
                participants,
                initial_message,
            )
            .unwrap();

        // Remove participant
        let update = LedgerUpdate {
            remove_participants: Some(vec![remove_id]),
            ..Default::default()
        };

        let result =
            service.update_thread_ledger(&author, "Test Location".to_string(), &thread_id, update);
        assert!(result.is_ok());

        // Verify participant was removed
        let thread = service.read_thread(&thread_id).unwrap();
        assert_eq!(thread.ledger.participants.len(), 1);
        assert!(!thread.ledger.participants.iter().any(|p| p.id == remove_id));
    }

    #[test]
    fn test_update_thread_ledger_change_status() {
        let (_temp, _template, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(author.clone(), "Test Location".to_string(), clinical_id)
            .unwrap();

        let participants = create_test_participants();
        let initial_message =
            MessageContent::new(participants[0].clone(), "Test".to_string(), None).unwrap();

        let thread_id = service
            .thread_create(
                &author,
                "Test Location".to_string(),
                participants,
                initial_message,
            )
            .unwrap();

        // Close the thread
        let update = LedgerUpdate {
            set_status: Some(FhirThreadStatus::Closed),
            ..Default::default()
        };

        let result =
            service.update_thread_ledger(&author, "Test Location".to_string(), &thread_id, update);
        assert!(result.is_ok());

        // Verify status changed
        let thread = service.read_thread(&thread_id).unwrap();
        assert_eq!(thread.ledger.status, FhirThreadStatus::Closed);
    }

    #[test]
    fn test_update_thread_ledger_change_visibility() {
        let (_temp, _template, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(author.clone(), "Test Location".to_string(), clinical_id)
            .unwrap();

        let participants = create_test_participants();
        let initial_message =
            MessageContent::new(participants[0].clone(), "Test".to_string(), None).unwrap();

        let thread_id = service
            .thread_create(
                &author,
                "Test Location".to_string(),
                participants,
                initial_message,
            )
            .unwrap();

        // Change visibility
        let update = LedgerUpdate {
            set_visibility: Some((SensitivityLevel::Confidential, true)),
            ..Default::default()
        };

        let result =
            service.update_thread_ledger(&author, "Test Location".to_string(), &thread_id, update);
        assert!(result.is_ok());

        // Verify visibility changed
        let thread = service.read_thread(&thread_id).unwrap();
        assert_eq!(thread.ledger.sensitivity, SensitivityLevel::Confidential);
        assert!(thread.ledger.restricted);
    }

    #[test]
    fn test_update_thread_ledger_change_policies() {
        let (_temp, _template, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(author.clone(), "Test Location".to_string(), clinical_id)
            .unwrap();

        let participants = create_test_participants();
        let initial_message =
            MessageContent::new(participants[0].clone(), "Test".to_string(), None).unwrap();

        let thread_id = service
            .thread_create(
                &author,
                "Test Location".to_string(),
                participants,
                initial_message,
            )
            .unwrap();

        // Change policies
        let update = LedgerUpdate {
            set_policies: Some((false, false)),
            ..Default::default()
        };

        let result =
            service.update_thread_ledger(&author, "Test Location".to_string(), &thread_id, update);
        assert!(result.is_ok());

        // Verify policies changed
        let thread = service.read_thread(&thread_id).unwrap();
        assert!(!thread.ledger.allow_patient_participation);
        assert!(!thread.ledger.allow_external_organisations);
    }

    #[test]
    fn test_parse_messages_md_multiple_messages() {
        let content = r#"**Message ID:** 550e8400-e29b-41d4-a716-446655440000
**Author role:** clinician
**Timestamp:** 2026-01-22T10:30:00Z
**Author ID:** 550e8400-e29b-41d4-a716-446655440001
**Author name:** Dr. Smith

First message body

---

**Message ID:** 550e8400-e29b-41d4-a716-446655440002
**Author role:** patient
**Timestamp:** 2026-01-22T11:30:00Z
**Author ID:** 550e8400-e29b-41d4-a716-446655440003
**Author name:** Patient John

Second message body

---
"#;

        let messages = parse_messages_md(content).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].body, "First message body");
        assert_eq!(messages[1].body, "Second message body");
        assert_eq!(messages[0].author.name, "Dr. Smith");
        assert_eq!(messages[1].author.name, "Patient John");
    }

    #[test]
    fn test_parse_messages_md_with_correction() {
        let content = r#"**Message ID:** 550e8400-e29b-41d4-a716-446655440000
**Author role:** clinician
**Timestamp:** 2026-01-22T10:30:00Z
**Author ID:** 550e8400-e29b-41d4-a716-446655440001
**Author name:** Dr. Smith
**Corrects:** 550e8400-e29b-41d4-a716-446655440099

Corrected message body

---
"#;

        let messages = parse_messages_md(content).unwrap();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].corrects.is_some());
        assert_eq!(
            messages[0].corrects.unwrap().to_string(),
            "550e8400-e29b-41d4-a716-446655440099"
        );
    }

    #[test]
    fn test_message_id_generation_is_unique() {
        let id1 = generate_message_id();
        let id2 = generate_message_id();
        assert_ne!(id1, id2);
    }
}
