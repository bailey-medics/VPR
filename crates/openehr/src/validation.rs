//! Input validation utilities.
//!
//! This module contains functions for validating user inputs to ensure they meet
//! safety and correctness requirements before being used in operations.

use crate::{OpenEhrError, OpenEhrResult};

/// Validates that a namespace string is safe for embedding in a URI.
///
/// The namespace is embedded into an external-reference URI: `ehr://{namespace}/mpi`.
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
/// Returns a `OpenEhrError::InvalidInput` if the namespace is invalid.
pub fn validate_namespace_uri_safe(namespace: &str) -> OpenEhrResult<()> {
    const MAX_NAMESPACE_LEN: usize = 253;

    if namespace.trim().is_empty() {
        return Err(OpenEhrError::InvalidInput(
            "namespace cannot be empty".into(),
        ));
    }

    if namespace.len() > MAX_NAMESPACE_LEN {
        return Err(OpenEhrError::InvalidInput(format!(
            "namespace exceeds maximum length of {} characters",
            MAX_NAMESPACE_LEN
        )));
    }

    if !namespace.is_ascii() {
        return Err(OpenEhrError::InvalidInput(
            "namespace must contain only ASCII characters".into(),
        ));
    }

    let ok = namespace
        .bytes()
        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'.' | b'-' | b'_'));

    if !ok {
        return Err(OpenEhrError::InvalidInput(
            "namespace contains invalid characters (only alphanumeric, '.', '-', '_' allowed)"
                .into(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_namespace_uri_safe_accepts_valid_namespace() {
        assert!(validate_namespace_uri_safe("valid.namespace").is_ok());
        assert!(validate_namespace_uri_safe("valid_namespace-123").is_ok());
        assert!(validate_namespace_uri_safe("a").is_ok());
    }

    #[test]
    fn test_validate_namespace_uri_safe_rejects_empty_namespace() {
        let err = validate_namespace_uri_safe("").expect_err("should reject empty");
        assert!(matches!(err, OpenEhrError::InvalidInput(msg) if msg.contains("cannot be empty")));
    }

    #[test]
    fn test_validate_namespace_uri_safe_rejects_whitespace_only() {
        let err = validate_namespace_uri_safe("   ").expect_err("should reject whitespace");
        assert!(matches!(err, OpenEhrError::InvalidInput(msg) if msg.contains("cannot be empty")));
    }

    #[test]
    fn test_validate_namespace_uri_safe_rejects_too_long_namespace() {
        let long_namespace = "a".repeat(254);
        let err = validate_namespace_uri_safe(&long_namespace).expect_err("should reject too long");
        assert!(
            matches!(err, OpenEhrError::InvalidInput(msg) if msg.contains("exceeds maximum length"))
        );
    }

    #[test]
    fn test_validate_namespace_uri_safe_rejects_non_ascii() {
        let err =
            validate_namespace_uri_safe("namespace_with_ü").expect_err("should reject non-ASCII");
        assert!(
            matches!(err, OpenEhrError::InvalidInput(msg) if msg.contains("must contain only ASCII"))
        );
    }

    #[test]
    fn test_validate_namespace_uri_safe_rejects_invalid_characters() {
        let err =
            validate_namespace_uri_safe("bad/namespace").expect_err("should reject invalid chars");
        assert!(
            matches!(err, OpenEhrError::InvalidInput(msg) if msg.contains("invalid characters"))
        );

        let err = validate_namespace_uri_safe("bad@namespace").expect_err("should reject @");
        assert!(
            matches!(err, OpenEhrError::InvalidInput(msg) if msg.contains("invalid characters"))
        );

        let err = validate_namespace_uri_safe("bad namespace").expect_err("should reject space");
        assert!(
            matches!(err, OpenEhrError::InvalidInput(msg) if msg.contains("invalid characters"))
        );
    }
}
