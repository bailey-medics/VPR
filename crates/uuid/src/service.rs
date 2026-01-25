//! Internal implementation of UUID services.
//!
//! This module contains the implementation details for UUID and timestamp-based
//! unique identifiers used throughout the VPR system.

use crate::{UuidError, UuidResult};
use chrono::{DateTime, Duration, Utc};
use std::path::{Path, PathBuf};
use std::{fmt, str::FromStr};

/// Re-exported for convenience.
pub use ::uuid::Uuid;

/// A validated SHA-256 hash in canonical form (64 lowercase hex characters).
///
/// This wrapper type guarantees that once constructed, the contained hash string
/// is a valid SHA-256 hash in canonical format. It provides type safety for hash
/// operations and ensures consistent representation across the system.
///
/// # Canonical SHA-256 form
/// - Length: 64 characters
/// - Characters: `0-9` and `a-f` only (lowercase hexadecimal)
/// - Example: `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`
///
/// # When to use this type
/// Use this wrapper whenever you are:
/// - Accepting a hash string from external sources (file metadata, API requests, etc.)
/// - Validating file integrity using content hashes
/// - Storing or transmitting hash values that must be validated
///
/// Once you have a `Sha256Hash`, you can safely assume the hash is valid and in canonical form.
///
/// # Construction
/// - [`Sha256Hash::parse`] validates an externally supplied hash string.
/// - [`Sha256Hash::from_bytes`] converts a 32-byte array directly to a hash.
///
/// # Errors
/// [`Sha256Hash::parse`] returns [`UuidError::InvalidInput`] if the input is not a valid
/// 64-character lowercase hex string.
///
/// # Display format
/// When displayed or converted to string, `Sha256Hash` always produces the canonical
/// 64-character lowercase hex format.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Sha256Hash(String);

impl Sha256Hash {
    /// Validates and parses a hash string that must be in canonical SHA-256 form.
    ///
    /// This does **not** normalise other representations (e.g., uppercase).
    /// The input must be exactly 64 lowercase hexadecimal characters.
    ///
    /// # Arguments
    ///
    /// * `input` - Hash string to validate. Must be exactly 64 lowercase hex characters.
    ///
    /// # Returns
    ///
    /// Returns a validated [`Sha256Hash`] on success.
    ///
    /// # Errors
    ///
    /// Returns [`UuidError::InvalidInput`] if `input` is not in canonical form.
    ///
    /// # Examples
    ///
    /// ```
    /// # use vpr_uuid::Sha256Hash;
    /// let hash = Sha256Hash::parse("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    /// assert!(hash.is_ok());
    ///
    /// let invalid = Sha256Hash::parse("not-a-hash");
    /// assert!(invalid.is_err());
    /// ```
    pub fn parse(input: &str) -> UuidResult<Self> {
        if Self::is_canonical(input) {
            return Ok(Self(input.to_string()));
        }
        Err(UuidError::InvalidInput(format!(
            "SHA-256 hash must be 64 lowercase hex characters, got: '{}'",
            input
        )))
    }

    /// Creates a `Sha256Hash` from a 32-byte array.
    ///
    /// This constructor converts a raw SHA-256 hash (32 bytes) into its hexadecimal
    /// string representation. Use this when you have computed a hash using a library
    /// like `sha2` or similar.
    ///
    /// # Arguments
    ///
    /// * `bytes` - A 32-byte array representing the SHA-256 hash.
    ///
    /// # Returns
    ///
    /// Returns a `Sha256Hash` with the hexadecimal representation of the bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// # use vpr_uuid::Sha256Hash;
    /// let bytes = [0u8; 32]; // All zeros
    /// let hash = Sha256Hash::from_bytes(&bytes);
    /// assert_eq!(hash.as_str(), "0000000000000000000000000000000000000000000000000000000000000000");
    /// ```
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        let hex_string = bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();
        Self(hex_string)
    }

    /// Returns the hash as a string slice.
    ///
    /// # Returns
    ///
    /// Returns a reference to the canonical hash string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns true if `input` is in canonical SHA-256 form.
    ///
    /// This is a purely syntactic check that validates:
    /// - Exactly 64 characters long
    /// - Contains only lowercase hex characters (`0-9` and `a-f`)
    ///
    /// This method is fast and can be used for pre-validation before calling [`Sha256Hash::parse`].
    ///
    /// # Arguments
    ///
    /// * `input` - Candidate hash string to validate.
    ///
    /// # Returns
    ///
    /// Returns `true` if `input` is canonical, otherwise `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use vpr_uuid::Sha256Hash;
    /// assert!(Sha256Hash::is_canonical("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"));
    /// assert!(!Sha256Hash::is_canonical("E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"));
    /// assert!(!Sha256Hash::is_canonical("not-a-hash"));
    /// assert!(!Sha256Hash::is_canonical("abc123"));
    /// ```
    pub fn is_canonical(input: &str) -> bool {
        input.len() == 64
            && input
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    }
}

impl fmt::Display for Sha256Hash {
    /// Formats the hash in canonical form (64 lowercase hex characters).
    ///
    /// This ensures consistent string representation across the application.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Sha256Hash {
    type Err = UuidError;

    /// Parses a string into a `Sha256Hash`, requiring canonical form.
    ///
    /// This is equivalent to calling [`Sha256Hash::parse`].
    ///
    /// # Errors
    ///
    /// Returns [`UuidError::InvalidInput`] if the string is not in canonical SHA-256 form.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Sha256Hash::parse(s)
    }
}

impl AsRef<str> for Sha256Hash {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Sha256Hash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Sha256Hash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Sha256Hash::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// VPR's canonical UUID representation (32 lowercase hex characters, no hyphens).
///
/// This wrapper type guarantees that once constructed, the contained UUID is in VPR's
/// canonical format. It provides type safety for UUID operations and ensures consistent
/// path derivation across the system.
///
/// # Why no hyphens?
///
/// The hyphen-free format is specifically chosen to support the **sharding scheme** for
/// patient directories. The [`sharded_dir`](ShardableUuid::sharded_dir) method uses simple
/// string slicing to extract shard prefixes:
///
/// ```text
/// 550e8400e29b41d4a716446655440000
/// ^^^^
/// ||└─ shard level 2: chars[2..4] = "0e"
/// └──── shard level 1: chars[0..2] = "55"
/// ```
///
/// This creates paths like `/patient_data/clinical/55/0e/550e8400e29b41d4a716446655440000`.
///
/// If hyphens were present, we'd need to strip them before sharding or handle them in the
/// slicing logic, adding unnecessary complexity. The hyphen-free format gives us a clean,
/// predictable character stream that's trivial to slice and use directly as filesystem paths.
///
/// **Note**: Other identifiers that don't use sharding (such as letter IDs) may use
/// RFC 4122 format with hyphens for better readability.
///
/// # When to use this type
/// Use this wrapper whenever you are:
/// - Accepting a UUID string from *outside* the core (CLI input, API request, etc), or
/// - Deriving a sharded storage path for a patient.
/// - Generating new patient identifiers.
///
/// Once you have a `ShardableUuid`, you can safely assume the internal UUID is valid
/// and in canonical form.
///
/// # Construction
/// - [`ShardableUuid::new`] generates a new canonical UUID (for new patient records).
/// - [`ShardableUuid::parse`] validates an externally supplied identifier.
///
/// # Errors
/// [`ShardableUuid::parse`] returns [`UuidError::InvalidInput`] if the input is not already
/// canonical.
///
/// # Display format
/// When displayed or converted to string, `ShardableUuid` always produces the canonical
/// 32-character lowercase hex format without hyphens.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ShardableUuid(Uuid);

impl Default for ShardableUuid {
    fn default() -> Self {
        Self::new()
    }
}

impl ShardableUuid {
    /// Generates a new UUID in VPR's canonical form.
    ///
    /// This is suitable for allocating a fresh identifier during patient creation.
    /// The generated UUID is cryptographically secure and follows RFC 4122 version 4.
    ///
    /// # Returns
    ///
    /// Returns a newly generated canonical UUID wrapped in `ShardableUuid`.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Validates and parses a UUID string that must already be in VPR's canonical form.
    ///
    /// This does **not** normalise other common UUID forms (for example, hyphenated or uppercase).
    /// Callers must provide the canonical representation. This strict validation ensures
    /// consistency and prevents issues with different UUID representations.
    ///
    /// # Arguments
    ///
    /// * `input` - UUID string to validate and wrap. Must be exactly 32 lowercase hex characters.
    ///
    /// # Returns
    ///
    /// Returns a validated [`ShardableUuid`] on success.
    ///
    /// # Errors
    ///
    /// Returns [`UuidError::InvalidInput`] if `input` is not in canonical form.
    pub fn parse(input: &str) -> UuidResult<Self> {
        if Self::is_canonical(input) {
            // SAFETY: is_canonical guarantees valid hex, so parse_str will succeed
            let uuid = Uuid::parse_str(input).expect("is_canonical guarantees valid UUID");
            return Ok(Self(uuid));
        }
        Err(UuidError::InvalidInput(format!(
            "UUID must be 32 lowercase hex characters without hyphens, got: '{}'",
            input
        )))
    }

    /// Creates a `ShardableUuid` from an existing `uuid::Uuid`.
    ///
    /// This constructor wraps a `Uuid` that is already known to be valid,
    /// avoiding the overhead of string parsing and validation. Use this when
    /// converting internal `Uuid` values that are already validated.
    ///
    /// # Arguments
    ///
    /// * `uuid` - A valid UUID to wrap.
    ///
    /// # Returns
    ///
    /// Returns a `ShardableUuid` wrapping the provided UUID.
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Returns the UUID as a `uuid::Uuid`.
    ///
    /// This method provides access to the underlying `uuid::Uuid` for operations
    /// that require the standard UUID library interface.
    ///
    /// # Returns
    ///
    /// Returns a copy of the inner UUID.
    ///
    /// # Note
    ///
    /// The returned UUID is guaranteed to be valid since `ShardableUuid` only
    /// contains validated UUIDs.
    pub fn uuid(&self) -> Uuid {
        self.0
    }

    /// Returns true if `input` is in VPR's canonical UUID form.
    ///
    /// This is a purely syntactic check that validates:
    /// - Exactly 32 bytes long
    /// - Contains only lowercase hex characters (`0-9` and `a-f`)
    ///
    /// This method is fast and can be used for pre-validation before calling [`ShardableUuid::parse`].
    ///
    /// # Arguments
    ///
    /// * `input` - Candidate UUID string to validate.
    ///
    /// # Returns
    ///
    /// Returns `true` if `input` is canonical, otherwise `false`.
    pub fn is_canonical(input: &str) -> bool {
        input.len() == 32
            && input
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    }

    /// Returns `parent_dir/<s1>/<s2>/<uuid>/` where `s1`/`s2` are derived from this UUID.
    ///
    /// This implements VPR's sharding scheme:
    /// - `s1` is the first two hex characters of the UUID
    /// - `s2` is the next two hex characters
    /// - The full UUID forms the leaf directory
    ///
    /// This sharding prevents filesystem performance issues with large numbers of patient
    /// directories in a single location.
    ///
    /// # Arguments
    ///
    /// * `parent_dir` - Base directory under which to shard the UUID.
    ///
    /// # Returns
    ///
    /// Returns the fully qualified sharded directory path for this UUID.
    pub fn sharded_dir(&self, parent_dir: &Path) -> PathBuf {
        let canonical = self.0.simple().to_string();
        let s1 = &canonical[0..2];
        let s2 = &canonical[2..4];
        parent_dir.join(s1).join(s2).join(&canonical)
    }
}

impl fmt::Display for ShardableUuid {
    /// Formats the UUID in canonical form (32 lowercase hex characters, no hyphens).
    ///
    /// This ensures consistent string representation across the application.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Display in canonical (simple) form
        write!(f, "{}", self.0.simple())
    }
}

impl FromStr for ShardableUuid {
    type Err = UuidError;

    /// Parses a string into a `ShardableUuid`, requiring canonical form.
    ///
    /// This is equivalent to calling [`ShardableUuid::parse`].
    ///
    /// # Errors
    ///
    /// Returns [`UuidError::InvalidInput`] if the string is not in canonical UUID form.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ShardableUuid::parse(s)
    }
}

/// A time-prefixed timestamp identifier for ordering events and records.
///
/// `TimestampId` combines a UTC timestamp with a UUID to create a globally unique,
/// time-ordered identifier. This is the primary mechanism for ordering patient records
/// in the VPR system.
///
/// # Format
///
/// `YYYYMMDDTHHMMSS.mmmZ-<uuid>`
///
/// Where:
/// - `YYYYMMDD` - Date (year, month, day)
/// - `T` - ISO 8601 separator
/// - `HHMMSS` - Time (hours, minutes, seconds in 24-hour format)
/// - `.mmm` - Milliseconds (3 digits)
/// - `Z` - UTC timezone indicator
/// - `-` - Separator between timestamp and UUID
/// - `<uuid>` - Standard hyphenated UUID (8-4-4-4-12 format)
///
/// # Example
///
/// ```text
/// 20260113T143522.045Z-550e8400-e29b-41d4-a716-446655440000
/// └─────────┬────────┘ └──────────────┬───────────────────┘
///       timestamp                    UUID
/// ```
///
/// # Design Properties
///
/// This is a **value object** with the following guarantees:
///
/// - **Immutable**: Once created, the timestamp and UUID cannot be changed
/// - **Comparable**: Implements `PartialEq` and `Eq` for equality checks
/// - **Hashable**: Can be used as a key in hash maps and sets
/// - **Parseable**: Can be constructed from a string representation
/// - **Clock-free**: Does not access the system clock; use [`TimestampIdGenerator`] for generation
/// - **Thread-safe**: Can be safely shared across threads (`Send` + `Sync`)
///
/// # Value Object Pattern
///
/// `TimestampId` follows the value object pattern from Domain-Driven Design:
///
/// - It represents a conceptual whole (a point in time with unique identity)
/// - It has no identity separate from its attributes
/// - It is compared by value, not reference
/// - It is immutable after construction
///
/// # Usage
///
/// - Use [`TimestampId::new`] when you already have a timestamp and UUID
/// - Use [`FromStr`] or [`TimestampId::from_str`] to parse a string representation
/// - Use [`TimestampIdGenerator::generate`] to create new IDs with monotonicity guarantees
///
/// # Monotonicity
///
/// When using [`TimestampIdGenerator`], timestamps are guaranteed to be strictly increasing
/// even if the system clock hasn't advanced or moves backward. This is essential for:
///
/// - Maintaining correct event ordering in distributed systems
/// - Ensuring audit log integrity
/// - Preventing timestamp collisions in high-frequency scenarios
///
/// # Thread Safety
///
/// `TimestampId` itself is thread-safe, but monotonicity guarantees require external
/// synchronization (e.g., per-patient locks) when generating new IDs.
///
/// # Display and Parsing
///
/// The string representation uses hyphens in the UUID part for better readability.
/// Format: `20260113T143522.045Z-550e8400-e29b-41d4-a716-446655440000`
///
/// Parsing accepts the same format and validates both timestamp and UUID components.
///
/// # See Also
///
/// - [`TimestampIdGenerator`] - For generating new IDs with monotonicity
/// - [`Uuid`] - For the UUID component
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TimestampId {
    /// The UTC timestamp component.
    ///
    /// This represents the logical time when the event or record occurred.
    /// Note: This may not match the system clock time if monotonicity adjustments were applied.
    timestamp: DateTime<Utc>,

    /// The unique identifier component.
    ///
    /// This provides global uniqueness even when multiple events have the same timestamp.
    uuid: Uuid,
}

impl TimestampId {
    /// Constructs a `TimestampId` from explicit components.
    ///
    /// This is a low-level constructor that assumes ordering and monotonicity have already
    /// been handled by the caller. For generating new IDs, prefer using
    /// [`TimestampIdGenerator::generate`] which provides monotonicity guarantees.
    ///
    /// # Arguments
    ///
    /// * `timestamp` - The UTC timestamp for this identifier
    /// * `uuid` - The unique identifier component
    ///
    /// # Returns
    ///
    /// A new `TimestampId` with the specified components.
    pub fn new(timestamp: DateTime<Utc>, uuid: Uuid) -> Self {
        Self { timestamp, uuid }
    }

    /// Returns the timestamp component.
    ///
    /// This is the logical time associated with this identifier. In cases where
    /// monotonicity adjustments were applied, this may be slightly ahead of the
    /// actual system clock time.
    ///
    /// # Returns
    ///
    /// The UTC timestamp as a [`DateTime<Utc>`].
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    /// Returns a reference to the UUID component.
    ///
    /// The UUID provides global uniqueness even when multiple events share the same timestamp.
    ///
    /// # Returns
    ///
    /// A reference to the [`Uuid`] component.
    pub fn uuid(&self) -> &Uuid {
        &self.uuid
    }
}

impl FromStr for TimestampId {
    type Err = UuidError;

    /// Parses a `TimestampId` from its string representation.
    ///
    /// The string must be in the format: `YYYYMMDDTHHMMSS.mmmZ-<uuid>`
    ///
    /// # Format Requirements
    ///
    /// - Timestamp part must end with 'Z' (UTC timezone indicator)
    /// - Must contain a hyphen separator between timestamp and UUID
    /// - Timestamp must be valid (no invalid dates like Feb 30)
    /// - UUID must be in standard hyphenated format (8-4-4-4-12)
    ///
    /// # Arguments
    ///
    /// * `s` - String slice to parse
    ///
    /// # Returns
    ///
    /// * `Ok(TimestampId)` - Successfully parsed identifier
    /// * `Err(UuidError::InvalidInput)` - If the format is invalid
    ///
    /// # Errors
    ///
    /// Returns [`UuidError::InvalidInput`] if:
    /// - The string doesn't contain a hyphen separator
    /// - The timestamp doesn't end with 'Z'
    /// - The timestamp format is invalid (e.g., "20260199" for invalid day)
    /// - The UUID is malformed
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Split at first hyphen to separate timestamp from UUID
        let (timestamp_str, uuid_str) = s.split_once('-').ok_or_else(|| {
            UuidError::InvalidInput(format!("Invalid timestamp ID format: '{}'", s))
        })?;

        // Validate that timestamp ends with 'Z' (UTC indicator)
        if !timestamp_str.ends_with('Z') {
            return Err(UuidError::InvalidInput(format!(
                "Timestamp must end with 'Z': '{}'",
                timestamp_str
            )));
        }

        // Remove the 'Z' suffix before parsing
        let timestamp_str_no_z = &timestamp_str[..timestamp_str.len() - 1];

        // Parse the timestamp using chrono's format string
        let timestamp_naive =
            chrono::NaiveDateTime::parse_from_str(timestamp_str_no_z, "%Y%m%dT%H%M%S%.3f")
                .map_err(|e| {
                    UuidError::InvalidInput(format!(
                        "Invalid timestamp format '{}': {}",
                        timestamp_str, e
                    ))
                })?;

        // Convert to UTC DateTime
        let timestamp = DateTime::<Utc>::from_naive_utc_and_offset(timestamp_naive, Utc);

        // Parse the UUID component
        let parsed_uuid = Uuid::parse_str(uuid_str)
            .map_err(|e| UuidError::InvalidInput(format!("Invalid UUID '{}': {}", uuid_str, e)))?;

        Ok(Self {
            timestamp,
            uuid: parsed_uuid,
        })
    }
}

impl fmt::Display for TimestampId {
    /// Formats the `TimestampId` as a string.
    ///
    /// The format is: `YYYYMMDDTHHMMSS.mmmZ-<uuid>`
    ///
    /// Where the UUID is rendered in standard hyphenated format (8-4-4-4-12)
    /// for better readability compared to VPR's canonical unhyphenated UUID format.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}-{}",
            self.timestamp.format("%Y%m%dT%H%M%S%.3fZ"),
            self.uuid.hyphenated()
        )
    }
}

/// Generator for creating [`TimestampId`] values with monotonicity guarantees.
///
/// `TimestampIdGenerator` is a stateless utility that encapsulates the clock access
/// and monotonicity logic required to safely generate time-ordered identifiers. It
/// ensures that generated timestamps are strictly increasing, even in scenarios where:
///
/// - The system clock hasn't advanced between calls
/// - The system clock moves backward (clock skew)
/// - Multiple IDs are generated in rapid succession
///
/// # Design Philosophy
///
/// This generator follows the principle of separating concerns:
///
/// - [`TimestampId`] is a pure value object (clock-free, immutable)
/// - [`TimestampIdGenerator`] handles the impure operations (clock access, monotonicity logic)
///
/// This separation enables:
/// - Easy testing of `TimestampId` without mocking time
/// - Clear understanding of when system clock is accessed
/// - Explicit control over monotonicity guarantees
///
/// # Monotonicity Guarantees
///
/// When a `previous` timestamp is provided, the generator ensures the new timestamp is
/// **strictly greater** than the previous one. If the current time hasn't advanced past
/// the previous timestamp, the generator adds 1 millisecond to the previous value.
///
/// This guarantees total ordering of events:
/// ```text
/// T1 < T2 < T3 < ... < Tn
/// ```
///
/// Even if generated at the same instant or during clock skew.
///
/// # Thread Safety and Synchronization
///
/// **Important**: While the generator itself is stateless and thread-safe, maintaining
/// monotonicity across multiple threads requires external synchronization. The typical
/// pattern is:
///
/// 1. Acquire a per-patient lock
/// 2. Read the most recent timestamp ID for that patient
/// 3. Generate the next ID using `generate(Some(previous))`
/// 4. Write the new ID
/// 5. Release the lock
///
/// This ensures that even with concurrent writes to different patients, each patient's
/// timeline remains strictly ordered.
///
/// # Millisecond Precision
///
/// The generator uses millisecond precision (3 decimal places). This means:
///
/// - Maximum theoretical throughput: 1,000 events per second per patient
/// - In practice, with locking overhead: ~100-500 events per second
/// - For higher throughput scenarios, consider batching or sequence numbers
///
/// # Clock Skew Handling
///
/// If the system clock moves backward, the generator detects this by comparing
/// against the previous timestamp and advances from the previous value instead
/// of using the (earlier) current time. This prevents:
///
/// - Timestamp collisions
/// - Out-of-order events
/// - Audit log inconsistencies
///
/// # Stateless Design
///
/// The generator is stateless - it doesn't store any previous timestamps internally.
/// This means:
///
/// - No memory overhead per patient
/// - No cleanup required
/// - Caller controls what "previous" means (from database, cache, etc.)
/// - Easy to use in distributed systems
///
/// # Performance Characteristics
///
/// - **Time complexity**: O(1) - constant time for all operations
/// - **Memory**: Zero-cost abstraction - compiles to a simple function call
/// - **System calls**: One call to `Utc::now()` per invocation
///
/// # When to Use
///
/// Use this generator when you need:
/// - Time-ordered event logging
/// - Audit trails with strict ordering
/// - Patient record versioning
/// - Any scenario requiring monotonically increasing timestamps
///
/// # When Not to Use
///
/// Don't use this generator for:
/// - Random identifiers (use `Uuid::new_v4()` directly)
/// - High-frequency events requiring sub-millisecond precision
/// - Scenarios where logical clocks (Lamport/Vector clocks) are more appropriate
///
/// # See Also
///
/// - [`TimestampId`] - The value object produced by this generator
/// - [`Uuid`] - For the UUID component
pub struct TimestampIdGenerator;

impl TimestampIdGenerator {
    /// Generates a new `TimestampId` with optional monotonicity relative to a previous ID.
    ///
    /// This is the primary method for creating time-ordered identifiers in the VPR system.
    /// It combines the current UTC time with a freshly generated UUID to create a globally
    /// unique, time-ordered identifier.
    ///
    /// # Monotonicity Behavior
    ///
    /// - **If `previous` is `None`**: Uses the current system time (`Utc::now()`)
    /// - **If `previous` is `Some(id)`**: Ensures the new timestamp is strictly greater
    ///   than the previous one, advancing by 1ms if necessary
    ///
    /// # Arguments
    ///
    /// * `previous` - Optional string representation of the previous `TimestampId`.
    ///   Must be in valid format if provided: `YYYYMMDDTHHMMSS.mmmZ-<uuid>`
    ///
    /// # Returns
    ///
    /// * `Ok(TimestampId)` - A new identifier with guaranteed monotonicity
    /// * `Err(UuidError)` - If the `previous` string is malformed
    ///
    /// # Errors
    ///
    /// Returns [`UuidError::InvalidInput`] if `previous` is provided but cannot be parsed.
    /// This typically indicates:
    /// - Invalid timestamp format
    /// - Malformed UUID
    /// - Missing required components
    ///
    /// # Monotonicity Algorithm
    ///
    /// ```text
    /// now = current_time()
    /// if previous exists and now <= previous.timestamp:
    ///     new_timestamp = previous.timestamp + 1ms
    /// else:
    ///     new_timestamp = now
    /// ```
    ///
    /// This ensures strictly increasing timestamps even during:
    /// - Rapid successive calls
    /// - System clock adjustments
    /// - NTP corrections
    ///
    /// # Thread Safety
    ///
    /// This method is thread-safe but does not provide atomicity guarantees across
    /// multiple calls. For maintaining monotonicity across threads, wrap calls in a
    /// per-patient mutex or other synchronization primitive.
    ///
    /// # Performance
    ///
    /// - Single system call to `Utc::now()`
    /// - Single UUID generation
    /// - Optional parsing if `previous` is provided
    /// - Total time: ~1-5 microseconds (depending on system)
    pub fn generate(previous: Option<&str>) -> UuidResult<TimestampId> {
        let previous = match previous {
            Some(s) => Some(TimestampId::from_str(s)?),
            None => None,
        };

        let now = Utc::now();

        // Ensure monotonicity: if current time hasn't advanced past the previous timestamp,
        // increment by 1ms to guarantee strict ordering even during clock skew or rapid calls.
        let timestamp = match &previous {
            Some(prev) if now <= prev.timestamp => prev.timestamp + Duration::milliseconds(1),
            _ => now,
        };

        Ok(TimestampId::new(timestamp, Uuid::new_v4()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_generates_valid_uuid() {
        let uuid_service = ShardableUuid::new();
        let canonical = uuid_service.to_string();

        // Verify the generated UUID is in canonical form
        assert_eq!(canonical.len(), 32);
        assert!(ShardableUuid::is_canonical(&canonical));
    }

    #[test]
    fn test_parse_valid_canonical_uuid() {
        let canonical = "550e8400e29b41d4a716446655440000";
        let result = ShardableUuid::parse(canonical);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), canonical);
    }

    #[test]
    fn test_parse_rejects_hyphenated_uuid() {
        let hyphenated = "550e8400-e29b-41d4-a716-446655440000";
        let result = ShardableUuid::parse(hyphenated);

        assert!(result.is_err());
        match result {
            Err(UuidError::InvalidInput(msg)) => {
                assert!(msg.contains("32 lowercase hex characters"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[test]
    fn test_parse_rejects_uppercase_uuid() {
        let uppercase = "550E8400E29B41D4A716446655440000";
        let result = ShardableUuid::parse(uppercase);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rejects_mixed_case_uuid() {
        let mixed = "550e8400E29b41d4A716446655440000";
        let result = ShardableUuid::parse(mixed);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rejects_too_short() {
        let short = "550e8400e29b41d4a71644665544000";
        let result = ShardableUuid::parse(short);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rejects_too_long() {
        let long = "550e8400e29b41d4a7164466554400000";
        let result = ShardableUuid::parse(long);

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_rejects_invalid_characters() {
        let invalid = "550e8400e29b41d4a716446655440zzz";
        let result = ShardableUuid::parse(invalid);

        assert!(result.is_err());
    }

    #[test]
    fn test_is_canonical_valid() {
        assert!(ShardableUuid::is_canonical(
            "550e8400e29b41d4a716446655440000"
        ));
        assert!(ShardableUuid::is_canonical(
            "00000000000000000000000000000000"
        ));
        assert!(ShardableUuid::is_canonical(
            "ffffffffffffffffffffffffffffffff"
        ));
    }

    #[test]
    fn test_is_canonical_invalid() {
        // Uppercase
        assert!(!ShardableUuid::is_canonical(
            "550E8400E29B41D4A716446655440000"
        ));

        // Hyphenated
        assert!(!ShardableUuid::is_canonical(
            "550e8400-e29b-41d4-a716-446655440000"
        ));

        // Too short
        assert!(!ShardableUuid::is_canonical(
            "550e8400e29b41d4a71644665544000"
        ));

        // Too long
        assert!(!ShardableUuid::is_canonical(
            "550e8400e29b41d4a7164466554400000"
        ));

        // Invalid characters
        assert!(!ShardableUuid::is_canonical(
            "550e8400e29b41d4a716446655440zzz"
        ));

        // Empty string
        assert!(!ShardableUuid::is_canonical(""));
    }

    #[test]
    fn test_sharded_dir_structure() {
        let uuid = ShardableUuid::parse("550e8400e29b41d4a716446655440000").unwrap();
        let parent = Path::new("/patient_data/clinical");
        let sharded = uuid.sharded_dir(parent);

        assert_eq!(
            sharded,
            PathBuf::from("/patient_data/clinical/55/0e/550e8400e29b41d4a716446655440000")
        );
    }

    #[test]
    fn test_sharded_dir_different_uuids() {
        let uuid1 = ShardableUuid::parse("00112233445566778899aabbccddeeff").unwrap();
        let uuid2 = ShardableUuid::parse("aabbccddeeff00112233445566778899").unwrap();

        let parent = Path::new("/data");

        let sharded1 = uuid1.sharded_dir(parent);
        let sharded2 = uuid2.sharded_dir(parent);

        assert_eq!(
            sharded1,
            PathBuf::from("/data/00/11/00112233445566778899aabbccddeeff")
        );
        assert_eq!(
            sharded2,
            PathBuf::from("/data/aa/bb/aabbccddeeff00112233445566778899")
        );
        assert_ne!(sharded1, sharded2);
    }

    #[test]
    fn test_display_format() {
        let uuid = ShardableUuid::parse("550e8400e29b41d4a716446655440000").unwrap();
        let displayed = format!("{}", uuid);

        assert_eq!(displayed, "550e8400e29b41d4a716446655440000");
        assert!(ShardableUuid::is_canonical(&displayed));
    }

    #[test]
    fn test_from_str_valid() {
        let canonical = "550e8400e29b41d4a716446655440000";
        let result: Result<ShardableUuid, _> = canonical.parse();

        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), canonical);
    }

    #[test]
    fn test_from_str_invalid() {
        let hyphenated = "550e8400-e29b-41d4-a716-446655440000";
        let result: Result<ShardableUuid, _> = hyphenated.parse();

        assert!(result.is_err());
    }

    #[test]
    fn test_uuid_method_returns_valid_uuid() {
        let uuid_service = ShardableUuid::parse("550e8400e29b41d4a716446655440000").unwrap();
        let inner_uuid = uuid_service.uuid();

        // Verify the inner UUID matches the canonical form
        assert_eq!(
            inner_uuid.simple().to_string(),
            "550e8400e29b41d4a716446655440000"
        );
    }

    #[test]
    fn test_round_trip_new_to_string_to_parse() {
        let original = ShardableUuid::new();
        let as_string = original.to_string();
        let parsed = ShardableUuid::parse(&as_string).unwrap();

        assert_eq!(original, parsed);
    }

    #[test]
    fn test_clone_and_equality() {
        let uuid1 = ShardableUuid::parse("550e8400e29b41d4a716446655440000").unwrap();
        let uuid2 = uuid1.clone();

        assert_eq!(uuid1, uuid2);
    }

    #[test]
    fn test_hash_consistency() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let uuid1 = ShardableUuid::parse("550e8400e29b41d4a716446655440000").unwrap();
        let uuid2 = ShardableUuid::parse("550e8400e29b41d4a716446655440000").unwrap();

        let mut hasher1 = DefaultHasher::new();
        let mut hasher2 = DefaultHasher::new();

        uuid1.hash(&mut hasher1);
        uuid2.hash(&mut hasher2);

        assert_eq!(hasher1.finish(), hasher2.finish());
    }

    // Sha256Hash tests
    #[test]
    fn test_sha256_parse_valid() {
        let valid = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let result = Sha256Hash::parse(valid);

        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str(), valid);
    }

    #[test]
    fn test_sha256_parse_rejects_uppercase() {
        let uppercase = "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855";
        let result = Sha256Hash::parse(uppercase);

        assert!(result.is_err());
    }

    #[test]
    fn test_sha256_parse_rejects_too_short() {
        let short = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85";
        let result = Sha256Hash::parse(short);

        assert!(result.is_err());
    }

    #[test]
    fn test_sha256_parse_rejects_too_long() {
        let long = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b8555";
        let result = Sha256Hash::parse(long);

        assert!(result.is_err());
    }

    #[test]
    fn test_sha256_parse_rejects_invalid_characters() {
        // cspell:disable-next-line
        let invalid = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852bxyz";
        let result = Sha256Hash::parse(invalid);

        assert!(result.is_err());
    }

    #[test]
    fn test_sha256_is_canonical_valid() {
        assert!(Sha256Hash::is_canonical(
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        ));
        assert!(Sha256Hash::is_canonical(
            "0000000000000000000000000000000000000000000000000000000000000000"
        ));
        assert!(Sha256Hash::is_canonical(
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
        ));
    }

    #[test]
    fn test_sha256_is_canonical_invalid() {
        // Uppercase
        assert!(!Sha256Hash::is_canonical(
            "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"
        ));

        // Too short
        assert!(!Sha256Hash::is_canonical(
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b85"
        ));

        // Too long
        assert!(!Sha256Hash::is_canonical(
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b8555"
        ));

        // Invalid characters
        // cspell:disable-next-line
        assert!(!Sha256Hash::is_canonical(
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852bxyz"
        ));

        // Random string
        assert!(!Sha256Hash::is_canonical("not-a-hash"));
    }

    #[test]
    fn test_sha256_from_bytes() {
        let bytes = [0u8; 32];
        let hash = Sha256Hash::from_bytes(&bytes);
        assert_eq!(
            hash.as_str(),
            "0000000000000000000000000000000000000000000000000000000000000000"
        );
    }

    #[test]
    fn test_sha256_display() {
        let hash =
            Sha256Hash::parse("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
                .unwrap();
        assert_eq!(
            hash.to_string(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sha256_from_str() {
        let hash_str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let result: Result<Sha256Hash, _> = hash_str.parse();

        assert!(result.is_ok());
        assert_eq!(result.unwrap().as_str(), hash_str);
    }

    #[test]
    fn test_sha256_clone_and_equality() {
        let hash1 =
            Sha256Hash::parse("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
                .unwrap();
        let hash2 = hash1.clone();

        assert_eq!(hash1, hash2);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_sha256_serde_roundtrip() {
        let hash =
            Sha256Hash::parse("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
                .unwrap();

        let json = serde_json::to_string(&hash).unwrap();
        assert_eq!(
            json,
            "\"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\""
        );

        let deserialized: Sha256Hash = serde_json::from_str(&json).unwrap();
        assert_eq!(hash, deserialized);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_sha256_serde_rejects_invalid() {
        let invalid_json = "\"not-a-valid-hash\"";
        let result: Result<Sha256Hash, _> = serde_json::from_str(invalid_json);

        assert!(result.is_err());
    }

    #[test]
    fn test_debug_format() {
        let uuid = ShardableUuid::parse("550e8400e29b41d4a716446655440000").unwrap();
        let debug = format!("{:?}", uuid);

        // Debug format should contain the UUID value
        assert!(debug.contains("550e8400"));
    }

    // TimestampId tests

    #[test]
    fn test_timestamp_id_generate_new() {
        let uid = TimestampIdGenerator::generate(None).unwrap();

        // Should have a valid UUID component
        let uuid_str = uid.uuid().to_string();
        assert_eq!(uuid_str.len(), 36); // Hyphenated format: 8-4-4-4-12
                                        // Verify it's a valid UUID by parsing it
        assert!(Uuid::parse_str(&uuid_str).is_ok());
    }

    #[test]
    fn test_timestamp_id_generate_monotonic() {
        let uid1 = TimestampIdGenerator::generate(None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let uid1_str = uid1.to_string();
        let uid2 = TimestampIdGenerator::generate(Some(&uid1_str)).unwrap();

        // Second timestamp should be strictly greater
        assert!(uid2.timestamp() > uid1.timestamp());
    }

    #[test]
    fn test_timestamp_id_generate_monotonic_same_instant() {
        let uid1 = TimestampIdGenerator::generate(None).unwrap();
        // Don't sleep - force the monotonic increment logic
        let uid1_str = uid1.to_string();
        let uid2 = TimestampIdGenerator::generate(Some(&uid1_str)).unwrap();

        // Even with no elapsed time, second should be strictly later
        assert!(uid2.timestamp() > uid1.timestamp());
    }

    #[test]
    fn test_timestamp_id_display_format() {
        let uid = TimestampIdGenerator::generate(None).unwrap();
        let displayed = uid.to_string();

        // Should contain hyphens (timestamp-uuid separator plus UUID hyphens)
        assert!(displayed.contains('-'));

        // Should end with 'Z' in the timestamp portion
        assert!(displayed.starts_with("20"));

        // Should be parseable and have correct structure
        assert!(displayed.contains('Z'));
        let z_pos = displayed.find('Z').unwrap();
        let uuid_part = &displayed[z_pos + 2..]; // Skip 'Z-'

        // UUID part should be hyphenated (8-4-4-4-12 format)
        assert_eq!(uuid_part.len(), 36); // Standard UUID with hyphens
    }

    #[test]
    fn test_timestamp_id_parse_valid() {
        let valid = "20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000";
        let result = TimestampId::from_str(valid);

        assert!(result.is_ok());
        let uid = result.unwrap();
        assert_eq!(
            uid.uuid().hyphenated().to_string(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn test_timestamp_id_parse_missing_hyphen() {
        let invalid = "20260111T143522.045Z550e8400-e29b-41d4-a716-446655440000";
        let result = TimestampId::from_str(invalid);

        assert!(result.is_err());
        match result {
            Err(UuidError::InvalidInput(msg)) => {
                assert!(msg.contains("must end with 'Z'"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[test]
    fn test_timestamp_id_parse_missing_z_suffix() {
        let invalid = "20260111T143522.045-550e8400-e29b-41d4-a716-446655440000";
        let result = TimestampId::from_str(invalid);

        assert!(result.is_err());
        match result {
            Err(UuidError::InvalidInput(msg)) => {
                assert!(msg.contains("must end with 'Z'"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[test]
    fn test_timestamp_id_parse_invalid_timestamp() {
        let invalid = "20260199T143522.045Z-550e8400-e29b-41d4-a716-446655440000";
        let result = TimestampId::from_str(invalid);

        assert!(result.is_err());
        match result {
            Err(UuidError::InvalidInput(msg)) => {
                assert!(msg.contains("Invalid timestamp format"));
            }
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[test]
    fn test_timestamp_id_parse_invalid_uuid() {
        let invalid = "20260111T143522.045Z-not-a-valid-uuid";
        let result = TimestampId::from_str(invalid);

        assert!(result.is_err());
    }

    #[test]
    fn test_timestamp_id_round_trip() {
        // Use a specific timestamp that has no sub-millisecond precision
        // to ensure clean round-trip through the %.3f format
        let original_str = "20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000";
        let original = TimestampId::from_str(original_str).unwrap();
        let as_string = original.to_string();
        let parsed = TimestampId::from_str(&as_string).unwrap();

        assert_eq!(as_string, original_str);
        assert_eq!(original, parsed);
        assert_eq!(original.timestamp(), parsed.timestamp());
        assert_eq!(original.uuid(), parsed.uuid());
    }

    #[test]
    fn test_timestamp_id_generate_with_previous() {
        let prev = "20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000";
        let result = TimestampIdGenerator::generate(Some(prev));

        assert!(result.is_ok());
        let new_uid = result.unwrap();
        let prev_uid = TimestampId::from_str(prev).unwrap();

        assert!(new_uid.timestamp() > prev_uid.timestamp());
    }

    #[test]
    fn test_timestamp_id_generate_without_previous() {
        let result = TimestampIdGenerator::generate(None);

        assert!(result.is_ok());
        let uid = result.unwrap();
        // Verify it's a valid hyphenated UUID
        assert!(Uuid::parse_str(&uid.uuid().to_string()).is_ok());
    }

    #[test]
    fn test_timestamp_id_generate_invalid() {
        let invalid = "not-a-valid-timestamp-uid";
        let result = TimestampIdGenerator::generate(Some(invalid));

        assert!(result.is_err());
    }

    #[test]
    fn test_timestamp_id_equality() {
        let uid1 =
            TimestampId::from_str("20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000")
                .unwrap();
        let uid2 =
            TimestampId::from_str("20260111T143522.045Z-550e8400-e29b-41d4-a716-446655440000")
                .unwrap();

        assert_eq!(uid1, uid2);
    }

    #[test]
    fn test_timestamp_id_clone() {
        let uid1 = TimestampIdGenerator::generate(None).unwrap();
        let uid2 = uid1.clone();

        assert_eq!(uid1, uid2);
    }
}
