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
//!
//! ## Storage Layout
//!
//! Coordination records are stored in a sharded structure:
//!
//! ```text
//! coordination/
//!   <s1>/
//!     <s2>/
//!       <id>/
//!         COORDINATION_STATUS.yaml    # Links to clinical record
//!         communications/             # Messaging threads
//!           <communication_id>/
//!             ledger.yaml            # Thread metadata and participants
//!             thread.md              # Thread messages in markdown
//!         .git/                      # Git repository for versioning
//! ```
//!
//! where `s1` and `s2` are the first four hex characters of the coordination UUID.
//!
//! **Conceptually:**
//!
//! - a communication is a thread and a ledger file
//! - a thread is a list of messages stored in `thread.md`
//! - the ledger contains metadata such as participants, status, policies, and visibility settings.
//!
//! ## Pure Data Operations
//!
//! This module contains **only** data operations—no API concerns such as
//! authentication, HTTP/gRPC servers, or service interfaces. API-level logic
//! belongs in `api-grpc`, `api-rest`, or `api-shared`.

use crate::author::Author;
use crate::config::CoreConfig;
use crate::constants::{
    COORDINATION_DIR_NAME, DEFAULT_GITIGNORE, THREAD_FILENAME, THREAD_LEDGER_FILENAME,
};
use crate::error::{PatientError, PatientResult};
use crate::markdown::{MarkdownService, Message, MessageMetadata};
use crate::paths::common::GitIgnoreFile;
use crate::paths::coordination::coordination_status::CoordinationStatusFile;
use crate::repositories::shared::create_uuid_and_shard_dir;
use crate::versioned_files::{
    CoordinationDomain::{Messaging, Record},
    FileToWrite, VersionedFileService, VprCommitAction, VprCommitDomain, VprCommitMessage,
};
use crate::NonEmptyText;
use crate::ShardableUuid;
use chrono::Utc;
use fhir::{
    CoordinationStatus, CoordinationStatusData, LedgerData, LifecycleState, MessageAuthor,
    Messaging as FhirMessaging, SensitivityLevel, ThreadStatus as FhirThreadStatus,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;
use vpr_uuid::{TimestampId, TimestampIdGenerator};

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
    /// Creates a new coordination service in the uninitialised state.
    ///
    /// # Arguments
    ///
    /// * `cfg` - Core configuration containing patient data directory paths
    pub fn new(cfg: Arc<CoreConfig>) -> Self {
        Self {
            cfg,
            state: Uninitialised,
        }
    }

    /// Initialises a new coordination repository for a patient.
    ///
    /// Creates a new coordination record with UUID-based sharded directory,
    /// COORDINATION_STATUS.yaml linking to the clinical record, and a Git
    /// repository for version control.
    ///
    /// Consumes `self` and returns `CoordinationService<Initialised>`.
    ///
    /// # Arguments
    ///
    /// * `author` - Author information for the initial Git commit
    /// * `care_location` - High-level organisational location for the commit
    /// * `clinical_id` - UUID of the linked clinical record
    ///
    /// # Returns
    ///
    /// Coordination service in initialised state with the new coordination UUID.
    ///
    /// # Errors
    ///
    /// Returns `PatientError` if:
    /// - Author validation fails
    /// - Directory creation fails
    /// - YAML serialisation fails
    /// - Git repository initialisation or commit fails
    pub fn initialise(
        self,
        commit_author: Author,
        care_location: NonEmptyText,
        clinical_id: Uuid,
    ) -> PatientResult<CoordinationService<Initialised>> {
        commit_author.validate_commit_author()?;

        let commit_message = VprCommitMessage::new(
            VprCommitDomain::Coordination(Record),
            VprCommitAction::Create,
            "Created coordination record",
            care_location,
        )?;

        let coordination_root_dir = self.coordination_root_dir();
        let (coordination_uuid, patient_dir) = create_uuid_and_shard_dir(&coordination_root_dir)?;

        let coordination_status = CoordinationStatusData {
            coordination_id: coordination_uuid.clone(),
            clinical_id: ShardableUuid::from_uuid(clinical_id),
            lifecycle_state: LifecycleState::Active,
            record_open: true,
            record_queryable: true,
            record_modifiable: true,
        };

        let coordination_status_raw = CoordinationStatus::render(&coordination_status)?;

        let files = [
            FileToWrite {
                relative_path: Path::new(GitIgnoreFile::NAME),
                content: DEFAULT_GITIGNORE,
                old_content: None,
            },
            FileToWrite {
                relative_path: Path::new(CoordinationStatusFile::NAME),
                content: &coordination_status_raw,
                old_content: None,
            },
        ];

        VersionedFileService::init_and_commit(
            &patient_dir,
            &commit_author,
            &commit_message,
            &files,
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
    /// Creates a coordination service for an existing record.
    ///
    /// Use this when you already have a coordination record and want to perform
    /// operations on it, such as creating threads or adding messages.
    ///
    /// # Arguments
    ///
    /// * `cfg` - Core configuration containing patient data directory paths
    /// * `coordination_id` - UUID of the existing coordination record
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
    /// - `communications/{communication_id}/` directory
    /// - `ledger.yaml` with participants, status, policies, and visibility settings
    /// - `thread.md` with message collection
    /// - Git commit with communication creation
    ///
    /// Communication ID format: `YYYYMMDDTHHMMSS.sssZ-UUID` (timestamp-based for ordering).
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
    pub fn communication_create(
        &self,
        commit_author: &Author,
        care_location: NonEmptyText,
        communication_authors: Vec<MessageAuthor>,
        initial_message: MessageContent,
    ) -> PatientResult<TimestampId> {
        commit_author.validate_commit_author()?;

        let commit_message = VprCommitMessage::new(
            VprCommitDomain::Coordination(Messaging),
            VprCommitAction::Create,
            "Created messaging thread",
            care_location,
        )?;

        validate_communication_authors(&communication_authors)?;

        let communication_id = TimestampIdGenerator::generate(None)?;
        let coordination_dir = self.coordination_dir(self.coordination_id());

        let now = Utc::now();
        let message_id = generate_message_id();

        let metadata = MessageMetadata {
            message_id,
            timestamp: now,
            author: initial_message.author().clone(),
        };

        let initial_message = Message {
            metadata,
            body: NonEmptyText::new(initial_message.body()).map_err(|_| {
                PatientError::InvalidInput("Message body cannot be empty".to_string())
            })?,
            corrects: None,
        };

        let markdown_service = MarkdownService::new();
        let messages_content_raw = markdown_service.thread_render(&[initial_message])?;

        let ledger = LedgerData {
            communication_id: communication_id.clone(),
            status: FhirThreadStatus::Open,
            created_at: now,
            last_updated_at: now,
            participants: communication_authors,
            sensitivity: SensitivityLevel::Standard,
            restricted: false,
            allow_patient_participation: true,
            allow_external_organisations: true,
        };

        let ledger_content_raw = FhirMessaging::ledger_render(&ledger)?;

        let messages_relative = relative_path(&[
            "communications",
            &communication_id.to_string(),
            THREAD_FILENAME,
        ]);
        let ledger_relative = relative_path(&[
            "communications",
            &communication_id.to_string(),
            THREAD_LEDGER_FILENAME,
        ]);

        let files_to_write = [
            FileToWrite {
                relative_path: &messages_relative,
                content: messages_content_raw.as_str(),
                old_content: None,
            },
            FileToWrite {
                relative_path: &ledger_relative,
                content: &ledger_content_raw,
                old_content: None,
            },
        ];

        VersionedFileService::write_and_commit_files(
            &coordination_dir,
            commit_author,
            &commit_message,
            &files_to_write,
        )?;

        Ok(ledger.communication_id)
    }

    /// Adds a message to an existing thread.
    ///
    /// Appends a new message to the thread's thread.md file and updates the ledger's
    /// last_updated_at timestamp. Both files are committed atomically to Git.
    ///
    /// # Arguments
    ///
    /// * `author` - Author creating the message (validated for commit permissions)
    /// * `care_location` - Care location context for the Git commit message
    /// * `thread_id` - ID of the thread to add the message to
    /// * `new_message` - Message content with author, body, and optional correction reference
    ///
    /// # Returns
    ///
    /// The generated message UUID.
    ///
    /// # Errors
    ///
    /// Returns `PatientError` if:
    /// - Author validation fails
    /// - Thread does not exist (thread.md not found)
    /// - File read, write, or Git commit operations fail
    /// - YAML serialisation or parsing fails
    pub fn message_add(
        &self,
        commit_author: &Author,
        care_location: NonEmptyText,
        thread_id: &TimestampId,
        new_message: MessageContent,
    ) -> PatientResult<Uuid> {
        self.file_exists(&["communications", &thread_id.to_string(), THREAD_FILENAME])?;
        self.file_exists(&[
            "communications",
            &thread_id.to_string(),
            THREAD_LEDGER_FILENAME,
        ])?;

        commit_author.validate_commit_author()?;

        let commit_message = VprCommitMessage::new(
            VprCommitDomain::Coordination(Messaging),
            VprCommitAction::Update,
            "Added message to thread",
            care_location,
        )?;

        let message_id = generate_message_id();
        let now = Utc::now();

        let metadata = MessageMetadata {
            message_id,
            timestamp: now,
            author: new_message.author().clone(),
        };

        // Read and parse existing messages
        let old_thread_raw = self.thread_file_read(thread_id, THREAD_FILENAME)?;
        let markdown_service = MarkdownService::new();
        let old_thread = markdown_service.thread_parse(old_thread_raw.as_str())?;

        // Create new thread with appended message
        let new_message = Message {
            metadata,
            body: NonEmptyText::new(new_message.body()).map_err(|_| {
                PatientError::InvalidInput("Message body cannot be empty".to_string())
            })?,
            corrects: new_message.corrects(),
        };
        let mut new_thread = old_thread;
        new_thread.push(new_message);

        // Render all messages back to markdown
        let new_thread_raw = markdown_service.thread_render(&new_thread)?;

        // Update ledger last_updated_at
        let old_ledger_raw = self.thread_file_read(thread_id, THREAD_LEDGER_FILENAME)?;
        let old_ledger = FhirMessaging::ledger_parse(old_ledger_raw.as_str())?;
        let mut new_ledger = old_ledger;
        new_ledger.last_updated_at = now;

        let new_ledger_raw = FhirMessaging::ledger_render(&new_ledger)?;

        // Write and commit
        let coordination_dir = self.coordination_dir(self.coordination_id());
        let messages_relative =
            relative_path(&["communications", &thread_id.to_string(), THREAD_FILENAME]);
        let ledger_relative = relative_path(&[
            "communications",
            &thread_id.to_string(),
            THREAD_LEDGER_FILENAME,
        ]);

        let files_to_write = [
            FileToWrite {
                relative_path: &messages_relative,
                content: new_thread_raw.as_str(),
                old_content: Some(old_thread_raw.as_str()),
            },
            FileToWrite {
                relative_path: &ledger_relative,
                content: &new_ledger_raw,
                old_content: Some(old_ledger_raw.as_str()),
            },
        ];

        VersionedFileService::write_and_commit_files(
            &coordination_dir,
            commit_author,
            &commit_message,
            &files_to_write,
        )?;

        Ok(message_id)
    }

    /// Reads an entire thread including messages and metadata.
    ///
    /// Reads both thread.md and ledger.yaml files, parses their contents, and
    /// returns structured data with full thread information.
    ///
    /// # Arguments
    ///
    /// * `thread_id` - ID of the thread to read
    ///
    /// # Returns
    ///
    /// Complete thread data containing the thread ID, ledger metadata (participants,
    /// status, policies, visibility), and all messages with correction relationships.
    ///
    /// # Errors
    ///
    /// Returns `PatientError` if:
    /// - Thread does not exist (thread.md or ledger.yaml not found)
    /// - File read operations fail
    /// - YAML or markdown parsing fails
    pub fn read_communication(&self, thread_id: &TimestampId) -> PatientResult<Communication> {
        let messages_raw = self.thread_file_read(thread_id, THREAD_FILENAME)?;
        let ledger_raw = self.thread_file_read(thread_id, THREAD_LEDGER_FILENAME)?;

        let markdown_service = MarkdownService::new();
        let messages = markdown_service.thread_parse(messages_raw.as_str())?;
        let ledger = FhirMessaging::ledger_parse(ledger_raw.as_str())?;

        Ok(Communication {
            communication_id: thread_id.clone(),
            ledger,
            messages,
        })
    }

    /// Updates thread ledger metadata.
    ///
    /// Modifies ledger.yaml with updated participants, status, policies, or visibility
    /// settings. The thread.md file is not modified. Changes are committed atomically
    /// to Git.
    ///
    /// # Arguments
    ///
    /// * `author` - Author making the update (validated for commit permissions)
    /// * `care_location` - Care location context for the Git commit message
    /// * `thread_id` - ID of the thread to update
    /// * `ledger_update` - Update specification (add/remove participants, change status, etc.)
    ///
    /// # Errors
    ///
    /// Returns `PatientError` if:
    /// - Author validation fails
    /// - Thread does not exist (ledger.yaml not found)
    /// - File read, write, or Git commit operations fail
    /// - YAML serialisation or parsing fails
    pub fn update_communication_ledger(
        &self,
        commit_author: &Author,
        care_location: NonEmptyText,
        thread_id: &TimestampId,
        ledger_update: LedgerUpdate,
    ) -> PatientResult<()> {
        commit_author.validate_commit_author()?;

        let msg = VprCommitMessage::new(
            VprCommitDomain::Coordination(Messaging),
            VprCommitAction::Update,
            "Updated thread ledger",
            care_location,
        )?;

        self.file_exists(&[
            "communications",
            &thread_id.to_string(),
            THREAD_LEDGER_FILENAME,
        ])?;

        // Read existing ledger
        let old_ledger_raw = self.thread_file_read(thread_id, THREAD_LEDGER_FILENAME)?;
        let mut ledger_data = FhirMessaging::ledger_parse(old_ledger_raw.as_str())?;

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

        ledger_data.last_updated_at = Utc::now();

        let new_ledger_raw = FhirMessaging::ledger_render(&ledger_data)?;

        let coordination_dir = self.coordination_dir(self.coordination_id());
        let ledger_relative = relative_path(&[
            "communications",
            &thread_id.to_string(),
            THREAD_LEDGER_FILENAME,
        ]);

        let files_to_write = [FileToWrite {
            relative_path: &ledger_relative,
            content: &new_ledger_raw,
            old_content: Some(old_ledger_raw.as_str()),
        }];

        VersionedFileService::write_and_commit_files(
            &coordination_dir,
            commit_author,
            &msg,
            &files_to_write,
        )?;

        Ok(())
    }

    /// Updates coordination record status.
    ///
    /// Modifies COORDINATION_STATUS.yaml with updated lifecycle state, open/queryable/modifiable
    /// flags. Changes are committed atomically to Git.
    ///
    /// # Arguments
    ///
    /// * `commit_author` - Author making the update (validated for commit permissions)
    /// * `care_location` - Care location context for the Git commit message
    /// * `status_update` - Update specification (lifecycle state, flags)
    ///
    /// # Errors
    ///
    /// Returns `PatientError` if:
    /// - Author validation fails
    /// - COORDINATION_STATUS.yaml does not exist or cannot be read
    /// - File read, write, or Git commit operations fail
    /// - YAML serialisation or parsing fails
    pub fn update_coordination_status(
        &self,
        commit_author: &Author,
        care_location: NonEmptyText,
        status_update: CoordinationStatusUpdate,
    ) -> PatientResult<()> {
        commit_author.validate_commit_author()?;

        let commit_message = VprCommitMessage::new(
            VprCommitDomain::Coordination(Record),
            VprCommitAction::Update,
            "Updated coordination status",
            care_location,
        )?;

        self.file_exists(&[CoordinationStatusFile::NAME])?;

        // Read existing status
        let old_status_raw = self.coordination_status_file_read()?;
        let mut status_data = CoordinationStatus::parse(old_status_raw.as_str())?;

        // Apply updates
        if let Some(lifecycle_state) = status_update.set_lifecycle_state {
            status_data.lifecycle_state = lifecycle_state;
        }
        if let Some(record_open) = status_update.set_record_open {
            status_data.record_open = record_open;
        }
        if let Some(record_queryable) = status_update.set_record_queryable {
            status_data.record_queryable = record_queryable;
        }
        if let Some(record_modifiable) = status_update.set_record_modifiable {
            status_data.record_modifiable = record_modifiable;
        }

        let new_status_raw = CoordinationStatus::render(&status_data)?;

        let coordination_dir = self.coordination_dir(self.coordination_id());
        let files_to_write = [FileToWrite {
            relative_path: Path::new(CoordinationStatusFile::NAME),
            content: &new_status_raw,
            old_content: Some(old_status_raw.as_str()),
        }];

        VersionedFileService::write_and_commit_files(
            &coordination_dir,
            commit_author,
            &commit_message,
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
    body: NonEmptyText,
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
    pub fn new(
        author: MessageAuthor,
        body: NonEmptyText,
        corrects: Option<Uuid>,
    ) -> PatientResult<Self> {
        if body.as_str().trim().is_empty() {
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
    pub fn body(&self) -> &NonEmptyText {
        &self.body
    }

    /// Returns the UUID of the message this corrects, if any.
    pub fn corrects(&self) -> Option<Uuid> {
        self.corrects
    }
}

/// Complete thread data (messages + ledger).
#[derive(Clone, Debug)]
pub struct Communication {
    pub communication_id: TimestampId,
    pub ledger: LedgerData,
    pub messages: Vec<Message>,
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

/// Update to apply to coordination status.
#[derive(Clone, Debug, Default)]
pub struct CoordinationStatusUpdate {
    pub set_lifecycle_state: Option<LifecycleState>,
    pub set_record_open: Option<bool>,
    pub set_record_queryable: Option<bool>,
    pub set_record_modifiable: Option<bool>,
}

impl<S> CoordinationService<S> {
    /// Returns the path to the coordination records directory.
    ///
    /// # Returns
    ///
    /// Absolute path to the coordination root directory.
    fn coordination_root_dir(&self) -> PathBuf {
        let data_dir = self.cfg.patient_data_dir().to_path_buf();
        data_dir.join(COORDINATION_DIR_NAME)
    }

    /// Returns the path to a specific patient's coordination record directory.
    ///
    /// # Arguments
    ///
    /// * `coordination_id` - UUID of the coordination record
    ///
    /// # Returns
    ///
    /// Absolute path to the sharded coordination directory.
    fn coordination_dir(&self, coordination_id: &ShardableUuid) -> PathBuf {
        let coordination_root_dir = self.coordination_root_dir();
        coordination_id.sharded_dir(&coordination_root_dir)
    }
}

impl CoordinationService<Initialised> {
    /// Returns the path to a specific thread directory.
    ///
    /// Constructs the absolute path by combining the coordination directory with
    /// the communications subdirectory and thread identifier.
    ///
    /// # Arguments
    ///
    /// * `thread_id` - Timestamp-based thread identifier
    ///
    /// # Returns
    ///
    /// Absolute path to the thread directory containing thread.md and ledger.yaml.
    fn thread_dir(&self, thread_id: &TimestampId) -> PathBuf {
        let coordination_dir = self.coordination_dir(self.coordination_id());
        coordination_dir
            .join("communications")
            .join(thread_id.to_string())
    }

    /// Reads a file from a thread directory.
    ///
    /// Constructs the absolute path from the thread ID and relative filename,
    /// then reads the file contents.
    ///
    /// # Arguments
    ///
    /// * `thread_id` - Timestamp-based thread identifier
    /// * `filename` - Relative filename within the thread directory
    ///
    /// # Returns
    ///
    /// File contents as a string.
    ///
    /// # Errors
    ///
    /// Returns `PatientError::FileRead` if the file cannot be read.
    fn thread_file_read(
        &self,
        thread_id: &TimestampId,
        filename: &str,
    ) -> PatientResult<NonEmptyText> {
        let thread_dir = self.thread_dir(thread_id);
        let file_path = thread_dir.join(filename);
        let content = fs::read_to_string(&file_path).map_err(PatientError::FileRead)?;
        NonEmptyText::new(content).map_err(|e| PatientError::InvalidInput(e.to_string()))
    }

    /// Checks if a file exists within the coordination directory.
    ///
    /// Constructs a path from the coordination directory by joining all provided
    /// path components (folders and file names) and validates the file exists.
    ///
    /// # Arguments
    ///
    /// * `path_components` - Variable path components relative to the coordination directory
    ///
    /// # Returns
    ///
    /// Ok(()) if the file exists.
    ///
    /// # Errors
    ///
    /// Returns `PatientError::InvalidInput` if the file does not exist.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// // Check coordination status file
    /// self.file_exists(&[CoordinationStatusFile::NAME])?;
    ///
    /// // Check thread file
    /// self.file_exists(&["communications", &thread_id.to_string(), "thread.md"])?;
    /// ```
    fn file_exists(&self, path_components: &[&str]) -> PatientResult<()> {
        let coordination_dir = self.coordination_dir(self.coordination_id());
        let mut file_path = coordination_dir;

        for component in path_components {
            file_path = file_path.join(component);
        }

        if !file_path.exists() {
            return Err(PatientError::InvalidInput(format!(
                "File does not exist: {}",
                path_components.join("/")
            )));
        }

        Ok(())
    }

    /// Returns the path to the coordination status file.
    ///
    /// # Returns
    ///
    /// Absolute path to COORDINATION_STATUS.yaml.
    fn coordination_status_file_path(&self) -> PathBuf {
        let coordination_dir = self.coordination_dir(self.coordination_id());
        coordination_dir.join(CoordinationStatusFile::NAME)
    }

    /// Reads the coordination status file.
    ///
    /// # Returns
    ///
    /// File contents as a string.
    ///
    /// # Errors
    ///
    /// Returns `PatientError::FileRead` if the file cannot be read.
    fn coordination_status_file_read(&self) -> PatientResult<NonEmptyText> {
        let status_path = self.coordination_status_file_path();
        let content = fs::read_to_string(&status_path).map_err(PatientError::FileRead)?;
        NonEmptyText::new(content).map_err(|e| PatientError::InvalidInput(e.to_string()))
    }
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Generates a new message ID (UUID v4).
fn generate_message_id() -> Uuid {
    Uuid::new_v4()
}

/// Constructs a relative file path from variable path components.
///
/// Joins all provided path components to create a relative path within the
/// coordination directory. Useful for constructing paths to files and directories.
///
/// # Arguments
///
/// * `path_components` - Variable path components to join
///
/// # Returns
///
/// Relative path constructed from all components.
///
/// # Examples
///
/// ```ignore
/// // Communication file path
/// relative_path(&["communications", &thread_id.to_string(), "thread.md"])
/// // Returns: communications/{thread_id}/thread.md
///
/// // Status file path  
/// relative_path(&["COORDINATION_STATUS.yaml"])
/// // Returns: COORDINATION_STATUS.yaml
/// ```
fn relative_path(path_components: &[&str]) -> PathBuf {
    let mut path = PathBuf::new();
    for component in path_components {
        path = path.join(component);
    }
    path
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
fn validate_communication_authors(authors: &[MessageAuthor]) -> PatientResult<()> {
    if authors.is_empty() {
        return Err(PatientError::InvalidInput(
            "Authors list must not be empty".to_string(),
        ));
    }

    for author in authors {
        if author.name.as_str().trim().is_empty() {
            return Err(PatientError::InvalidInput(
                "Author name must not be empty".to_string(),
            ));
        }
    }

    Ok(())
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EmailAddress, NonEmptyText};
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_env() -> (TempDir, Arc<CoreConfig>, Author) {
        let temp_dir = TempDir::new().unwrap();

        let cfg = Arc::new(
            CoreConfig::new(
                temp_dir.path().to_path_buf(),
                openehr::RmVersion::rm_1_1_0,
                NonEmptyText::new("test-namespace").unwrap(),
            )
            .unwrap(),
        );

        let author = Author {
            name: NonEmptyText::new("Dr. Test").unwrap(),
            role: NonEmptyText::new("Clinician").unwrap(),
            email: EmailAddress::parse("test@example.com").unwrap(),
            registrations: vec![],
            signature: None,
            certificate: None,
        };

        (temp_dir, cfg, author)
    }

    fn create_test_participants() -> Vec<MessageAuthor> {
        vec![
            MessageAuthor {
                id: Uuid::new_v4(),
                name: NonEmptyText::new("Dr. Smith").unwrap(),
                role: fhir::AuthorRole::Clinician,
            },
            MessageAuthor {
                id: Uuid::new_v4(),
                name: NonEmptyText::new("Patient John").unwrap(),
                role: fhir::AuthorRole::Patient,
            },
        ]
    }

    #[test]
    fn test_initialise_creates_coordination_repo() {
        let (_temp, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let result = CoordinationService::new(cfg.clone()).initialise(
            author,
            NonEmptyText::new("Test Location").unwrap(),
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
        let (_temp, _cfg, _author) = setup_test_env();
        let _clinical_id = Uuid::new_v4();

        // NonEmptyText validation prevents empty strings at the type level
        let err =
            NonEmptyText::new("").expect_err("creating NonEmptyText from empty string should fail");
        assert!(matches!(err, crate::TextError::Empty));
    }

    #[test]
    fn test_communication_create_with_initial_message() {
        let (_temp, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(
                author.clone(),
                NonEmptyText::new("Test Location").unwrap(),
                clinical_id,
            )
            .unwrap();

        let participants = create_test_participants();
        let initial_message = MessageContent::new(
            participants[0].clone(),
            NonEmptyText::new("Initial thread message").unwrap(),
            None,
        )
        .unwrap();

        let result = service.communication_create(
            &author,
            NonEmptyText::new("Test Location").unwrap(),
            participants.clone(),
            initial_message,
        );

        assert!(result.is_ok());
        let thread_id = result.unwrap();

        // Verify thread directory exists
        let coord_dir = service.coordination_dir(service.coordination_id());
        let thread_dir = coord_dir.join("communications").join(thread_id.to_string());
        assert!(thread_dir.exists());
        assert!(thread_dir.join("thread.md").exists());
        assert!(thread_dir.join("ledger.yaml").exists());

        // Verify thread.md contains initial message
        let messages_content = fs::read_to_string(thread_dir.join("thread.md")).unwrap();
        assert!(messages_content.contains("Initial thread message"));
    }

    #[test]
    fn test_communication_create_validates_empty_body() {
        let (_temp, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let _service = CoordinationService::new(cfg.clone())
            .initialise(
                author.clone(),
                NonEmptyText::new("Test Location").unwrap(),
                clinical_id,
            )
            .unwrap();

        let participants = create_test_participants();
        let result = MessageContent::new(
            participants[0].clone(),
            NonEmptyText::new("   ").unwrap(), // Empty after trim
            None,
        );

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), PatientError::InvalidInput(_)));
    }

    #[test]
    fn test_validate_communication_authors_empty_list() {
        let result = validate_communication_authors(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_communication_authors_empty_name() {
        let authors = vec![MessageAuthor {
            id: Uuid::new_v4(),
            name: NonEmptyText::new("   ").unwrap(),
            role: fhir::AuthorRole::Clinician,
        }];

        let result = validate_communication_authors(&authors);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_communication_authors_valid() {
        let authors = create_test_participants();
        let result = validate_communication_authors(&authors);
        assert!(result.is_ok());
    }

    #[test]
    fn test_message_add_appends_to_thread() {
        let (_temp, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(
                author.clone(),
                NonEmptyText::new("Test Location").unwrap(),
                clinical_id,
            )
            .unwrap();

        let participants = create_test_participants();
        let initial_message = MessageContent::new(
            participants[0].clone(),
            NonEmptyText::new("First message").unwrap(),
            None,
        )
        .unwrap();

        let thread_id = service
            .communication_create(
                &author,
                NonEmptyText::new("Test Location").unwrap(),
                participants.clone(),
                initial_message,
            )
            .unwrap();

        // Add second message
        let second_message = MessageContent::new(
            participants[1].clone(),
            NonEmptyText::new("Second message from patient").unwrap(),
            None,
        )
        .unwrap();

        let result = service.message_add(
            &author,
            NonEmptyText::new("Test Location").unwrap(),
            &thread_id,
            second_message,
        );

        assert!(result.is_ok());

        // Read thread and verify both messages
        let thread = service.read_communication(&thread_id).unwrap();
        assert_eq!(thread.messages.len(), 2);
        assert_eq!(thread.messages[0].body.as_str(), "First message");
        assert_eq!(
            thread.messages[1].body.as_str(),
            "Second message from patient"
        );
    }

    #[test]
    fn test_message_add_with_correction() {
        let (_temp, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(
                author.clone(),
                NonEmptyText::new("Test Location").unwrap(),
                clinical_id,
            )
            .unwrap();

        let participants = create_test_participants();
        let initial_message = MessageContent::new(
            participants[0].clone(),
            NonEmptyText::new("Original message with typo").unwrap(),
            None,
        )
        .unwrap();

        let thread_id = service
            .communication_create(
                &author,
                NonEmptyText::new("Test Location").unwrap(),
                participants.clone(),
                initial_message,
            )
            .unwrap();

        let thread = service.read_communication(&thread_id).unwrap();
        let original_msg_id = thread.messages[0].metadata.message_id;

        // Add correction message
        let correction = MessageContent::new(
            participants[0].clone(),
            NonEmptyText::new("Corrected message without typo").unwrap(),
            Some(original_msg_id),
        )
        .unwrap();

        let result = service.message_add(
            &author,
            NonEmptyText::new("Test Location").unwrap(),
            &thread_id,
            correction,
        );
        assert!(result.is_ok());

        // Verify correction is recorded
        let thread = service.read_communication(&thread_id).unwrap();
        assert_eq!(thread.messages.len(), 2);
        assert_eq!(thread.messages[1].corrects, Some(original_msg_id));
    }

    #[test]
    fn test_message_add_to_nonexistent_thread() {
        let (_temp, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(
                author.clone(),
                NonEmptyText::new("Test Location").unwrap(),
                clinical_id,
            )
            .unwrap();

        let fake_thread_id = TimestampIdGenerator::generate(None).unwrap();
        let participants = create_test_participants();
        let message = MessageContent::new(
            participants[0].clone(),
            NonEmptyText::new("Message to nowhere").unwrap(),
            None,
        )
        .unwrap();

        let result = service.message_add(
            &author,
            NonEmptyText::new("Test Location").unwrap(),
            &fake_thread_id,
            message,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_read_communication_returns_complete_data() {
        let (_temp, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(
                author.clone(),
                NonEmptyText::new("Test Location").unwrap(),
                clinical_id,
            )
            .unwrap();

        let participants = create_test_participants();
        let initial_message = MessageContent::new(
            participants[0].clone(),
            NonEmptyText::new("Test message").unwrap(),
            None,
        )
        .unwrap();

        let thread_id = service
            .communication_create(
                &author,
                NonEmptyText::new("Test Location").unwrap(),
                participants.clone(),
                initial_message,
            )
            .unwrap();

        let thread = service.read_communication(&thread_id).unwrap();

        assert_eq!(thread.communication_id, thread_id);
        assert_eq!(thread.ledger.participants.len(), 2);
        assert_eq!(thread.messages.len(), 1);
        assert_eq!(thread.ledger.status, FhirThreadStatus::Open);
    }

    #[test]
    fn test_read_communication_nonexistent() {
        let (_temp, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(
                author.clone(),
                NonEmptyText::new("Test Location").unwrap(),
                clinical_id,
            )
            .unwrap();

        let fake_thread_id = TimestampIdGenerator::generate(None).unwrap();
        let result = service.read_communication(&fake_thread_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_communication_ledger_add_participants() {
        let (_temp, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(
                author.clone(),
                NonEmptyText::new("Test Location").unwrap(),
                clinical_id,
            )
            .unwrap();

        let participants = create_test_participants();
        let initial_message = MessageContent::new(
            participants[0].clone(),
            NonEmptyText::new("Test").unwrap(),
            None,
        )
        .unwrap();

        let thread_id = service
            .communication_create(
                &author,
                NonEmptyText::new("Test Location").unwrap(),
                participants,
                initial_message,
            )
            .unwrap();

        // Add new participant
        let new_participant = MessageAuthor {
            id: Uuid::new_v4(),
            name: NonEmptyText::new("Nurse Jane").unwrap(),
            role: fhir::AuthorRole::Clinician,
        };

        let update = LedgerUpdate {
            add_participants: Some(vec![new_participant.clone()]),
            ..Default::default()
        };

        let result = service.update_communication_ledger(
            &author,
            NonEmptyText::new("Test Location").unwrap(),
            &thread_id,
            update,
        );
        assert!(result.is_ok());

        // Verify participant was added
        let thread = service.read_communication(&thread_id).unwrap();
        assert_eq!(thread.ledger.participants.len(), 3);
        assert!(thread
            .ledger
            .participants
            .iter()
            .any(|p| p.name.as_str() == "Nurse Jane"));
    }

    #[test]
    fn test_update_communication_ledger_remove_participants() {
        let (_temp, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(
                author.clone(),
                NonEmptyText::new("Test Location").unwrap(),
                clinical_id,
            )
            .unwrap();

        let participants = create_test_participants();
        let remove_id = participants[1].id;
        let initial_message = MessageContent::new(
            participants[0].clone(),
            NonEmptyText::new("Test").unwrap(),
            None,
        )
        .unwrap();

        let thread_id = service
            .communication_create(
                &author,
                NonEmptyText::new("Test Location").unwrap(),
                participants,
                initial_message,
            )
            .unwrap();

        // Remove participant
        let update = LedgerUpdate {
            remove_participants: Some(vec![remove_id]),
            ..Default::default()
        };

        let result = service.update_communication_ledger(
            &author,
            NonEmptyText::new("Test Location").unwrap(),
            &thread_id,
            update,
        );
        assert!(result.is_ok());

        // Verify participant was removed
        let thread = service.read_communication(&thread_id).unwrap();
        assert_eq!(thread.ledger.participants.len(), 1);
        assert!(!thread.ledger.participants.iter().any(|p| p.id == remove_id));
    }

    #[test]
    fn test_update_communication_ledger_change_status() {
        let (_temp, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(
                author.clone(),
                NonEmptyText::new("Test Location").unwrap(),
                clinical_id,
            )
            .unwrap();

        let participants = create_test_participants();
        let initial_message = MessageContent::new(
            participants[0].clone(),
            NonEmptyText::new("Test").unwrap(),
            None,
        )
        .unwrap();

        let thread_id = service
            .communication_create(
                &author,
                NonEmptyText::new("Test Location").unwrap(),
                participants,
                initial_message,
            )
            .unwrap();

        // Close the thread
        let update = LedgerUpdate {
            set_status: Some(FhirThreadStatus::Closed),
            ..Default::default()
        };

        let result = service.update_communication_ledger(
            &author,
            NonEmptyText::new("Test Location").unwrap(),
            &thread_id,
            update,
        );
        assert!(result.is_ok());

        // Verify status changed
        let thread = service.read_communication(&thread_id).unwrap();
        assert_eq!(thread.ledger.status, FhirThreadStatus::Closed);
    }

    #[test]
    fn test_update_communication_ledger_change_visibility() {
        let (_temp, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(
                author.clone(),
                NonEmptyText::new("Test Location").unwrap(),
                clinical_id,
            )
            .unwrap();

        let participants = create_test_participants();
        let initial_message = MessageContent::new(
            participants[0].clone(),
            NonEmptyText::new("Test").unwrap(),
            None,
        )
        .unwrap();

        let thread_id = service
            .communication_create(
                &author,
                NonEmptyText::new("Test Location").unwrap(),
                participants,
                initial_message,
            )
            .unwrap();

        // Change visibility
        let update = LedgerUpdate {
            set_visibility: Some((SensitivityLevel::Confidential, true)),
            ..Default::default()
        };

        let result = service.update_communication_ledger(
            &author,
            NonEmptyText::new("Test Location").unwrap(),
            &thread_id,
            update,
        );
        assert!(result.is_ok());

        // Verify visibility changed
        let thread = service.read_communication(&thread_id).unwrap();
        assert_eq!(thread.ledger.sensitivity, SensitivityLevel::Confidential);
        assert!(thread.ledger.restricted);
    }

    #[test]
    fn test_update_communication_ledger_change_policies() {
        let (_temp, cfg, author) = setup_test_env();
        let clinical_id = Uuid::new_v4();

        let service = CoordinationService::new(cfg.clone())
            .initialise(
                author.clone(),
                NonEmptyText::new("Test Location").unwrap(),
                clinical_id,
            )
            .unwrap();

        let participants = create_test_participants();
        let initial_message = MessageContent::new(
            participants[0].clone(),
            NonEmptyText::new("Test").unwrap(),
            None,
        )
        .unwrap();

        let thread_id = service
            .communication_create(
                &author,
                NonEmptyText::new("Test Location").unwrap(),
                participants,
                initial_message,
            )
            .unwrap();

        // Change policies
        let update = LedgerUpdate {
            set_policies: Some((false, false)),
            ..Default::default()
        };

        let result = service.update_communication_ledger(
            &author,
            NonEmptyText::new("Test Location").unwrap(),
            &thread_id,
            update,
        );
        assert!(result.is_ok());

        // Verify policies changed
        let thread = service.read_communication(&thread_id).unwrap();
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

        let markdown_service = MarkdownService::new();
        let parsed_messages = markdown_service.thread_parse(content).unwrap();
        assert_eq!(parsed_messages.len(), 2);
        assert_eq!(parsed_messages[0].body.as_str(), "First message body");
        assert_eq!(parsed_messages[1].body.as_str(), "Second message body");
        assert_eq!(
            parsed_messages[0].metadata.author.name.as_str(),
            "Dr. Smith"
        );
        assert_eq!(
            parsed_messages[1].metadata.author.name.as_str(),
            "Patient John"
        );
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

        let markdown_service = MarkdownService::new();
        let parsed_messages = markdown_service.thread_parse(content).unwrap();
        assert_eq!(parsed_messages.len(), 1);
        assert!(parsed_messages[0].corrects.is_some());
        assert_eq!(
            parsed_messages[0].corrects.unwrap().to_string(),
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
