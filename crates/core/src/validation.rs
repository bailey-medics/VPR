//! Input validation utilities.
//!
//! This module contains functions for validating user inputs to ensure they meet
//! safety and correctness requirements before being used in operations.

use crate::{PatientError, PatientResult};

/// Validates that a namespace string is safe for embedding in a URI.
///
/// The namespace is embedded into an external-reference URI: `vpr://{namespace}/mpi`.
/// This function applies defensive guardrails to prevent injection or malformed URIs:
/// - Rejects empty or whitespace-only strings
/// - Bounds the length to avoid pathological inputs
/// - Restricts characters to a conservative ASCII set suitable for a URI authority
///
/// # Arguments
///
/// * `namespace` - The namespace string to validate.
///
/// # Errors
///
/// Returns a `PatientError::InvalidInput` if the namespace is invalid.
pub fn validate_namespace_safe_for_uri(namespace: &str) -> PatientResult<()> {
    const MAX_NAMESPACE_LEN: usize = 253;

    if namespace.trim().is_empty() {
        return Err(PatientError::InvalidInput(
            "namespace cannot be empty".into(),
        ));
    }

    if namespace.len() > MAX_NAMESPACE_LEN {
        return Err(PatientError::InvalidInput(format!(
            "namespace exceeds maximum length of {} characters",
            MAX_NAMESPACE_LEN
        )));
    }

    if !namespace.is_ascii() {
        return Err(PatientError::InvalidInput(
            "namespace must contain only ASCII characters".into(),
        ));
    }

    let ok = namespace
        .bytes()
        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'.' | b'-' | b'_'));

    if !ok {
        return Err(PatientError::InvalidInput(
            "namespace contains invalid characters (only alphanumeric, '.', '-', '_' allowed)"
                .into(),
        ));
    }

    Ok(())
}
