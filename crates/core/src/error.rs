#[allow(clippy::single_component_path_imports)]
use serde_yaml;

#[derive(Debug, thiserror::Error)]
pub enum PatientError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("failed to create storage directory: {0}")]
    StorageDirCreation(std::io::Error),
    #[error("failed to create patient directory: {0}")]
    PatientDirCreation(std::io::Error),
    #[error(
        "initialise failed and cleanup also failed (path: {path}): init={init_error}; cleanup={cleanup_error}",
        path = path.display()
    )]
    CleanupAfterInitialiseFailed {
        path: std::path::PathBuf,
        #[source]
        init_error: Box<PatientError>,
        cleanup_error: std::io::Error,
    },
    #[error("failed to write patient file: {0}")]
    FileWrite(std::io::Error),
    #[error("failed to read patient file: {0}")]
    FileRead(std::io::Error),
    #[error("failed to serialize patient: {0}")]
    Serialization(serde_json::Error),
    #[error("failed to deserialize patient: {0}")]
    Deserialization(serde_json::Error),
    #[error("failed to serialize YAML: {0}")]
    YamlSerialization(serde_yaml::Error),
    #[error("failed to deserialize YAML: {0}")]
    YamlDeserialization(serde_yaml::Error),

    #[error("openEHR error: {0}")]
    Openehr(#[from] openehr::OpenEhrError),
    #[error("failed to initialise git repository: {0}")]
    GitInit(git2::Error),
    #[error("failed to access git index: {0}")]
    GitIndex(git2::Error),
    #[error("failed to add file to git index: {0}")]
    GitAdd(git2::Error),
    #[error("failed to write git tree: {0}")]
    GitWriteTree(git2::Error),
    #[error("failed to find git tree: {0}")]
    GitFindTree(git2::Error),
    #[error("failed to create git signature: {0}")]
    GitSignature(git2::Error),
    #[error("failed to create initial git commit: {0}")]
    GitCommit(git2::Error),
    #[error("failed to parse PEM: {0}")]
    PemParse(::pem::PemError),
    #[error("failed to parse ECDSA private key: {0}")]
    EcdsaPrivateKeyParse(Box<dyn std::error::Error + Send + Sync>),
    #[error("failed to parse ECDSA public key/certificate: {0}")]
    EcdsaPublicKeyParse(Box<dyn std::error::Error + Send + Sync>),
    #[error("author certificate public key does not match signing key")]
    AuthorCertificatePublicKeyMismatch,
    #[error("invalid embedded commit signature payload")]
    InvalidCommitSignaturePayload,
    #[error("failed to sign: {0}")]
    EcdsaSign(Box<dyn std::error::Error + Send + Sync>),
    #[error("failed to create commit buffer: {0}")]
    GitCommitBuffer(git2::Error),
    #[error("failed to create signed commit: {0}")]
    GitCommitSigned(git2::Error),
    #[error("failed to convert commit buffer to string: {0}")]
    CommitBufferToString(std::string::FromUtf8Error),
    #[error("failed to open git repository: {0}")]
    GitOpen(git2::Error),
    #[error("failed to create/update git reference: {0}")]
    GitReference(git2::Error),
    #[error("failed to get git head: {0}")]
    GitHead(git2::Error),
    #[error("failed to set git head: {0}")]
    GitSetHead(git2::Error),
    #[error("failed to peel git commit: {0}")]
    GitPeel(git2::Error),
    #[error("invalid timestamp")]
    InvalidTimestamp,

    #[error("missing Author-Name")]
    MissingAuthorName,
    #[error("missing Author-Role")]
    MissingAuthorRole,
    #[error("invalid Author-Registration")]
    InvalidAuthorRegistration,
    #[error("author trailer keys are reserved")]
    ReservedAuthorTrailerKey,

    #[error("invalid Care-Location")]
    InvalidCareLocation,
    #[error("missing Care-Location")]
    MissingCareLocation,
    #[error("Care-Location trailer key is reserved")]
    ReservedCareLocationTrailerKey,
}

pub type PatientResult<T> = std::result::Result<T, PatientError>;
