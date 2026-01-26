//! Author-related types and functions.
//!
//! This module contains types and utilities for handling author information,
//! signatures, and commit validation in the VPR system.

use crate::error::{PatientError, PatientResult};
use crate::{EmailAddress, NonEmptyText};
use base64::{engine::general_purpose, Engine as _};
use serde::Deserialize;
use x509_parser::prelude::*;

/// Represents an author of a commit or record operation.
#[derive(Clone, Debug)]
pub struct Author {
    /// The full name of the author.
    pub name: NonEmptyText,

    /// The professional role of the author (e.g., "Clinician", "Nurse").
    pub role: NonEmptyText,

    /// The email address of the author.
    pub email: EmailAddress,

    /// Professional registrations for the author (e.g., GMC number, NMC PIN).
    pub registrations: Vec<AuthorRegistration>,

    /// Optional digital signature for the commit.
    pub signature: Option<Vec<u8>>,

    /// Optional X.509 certificate for the author.
    pub certificate: Option<Vec<u8>>,
}

/// Material embedded in the Git commit object to enable offline verification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmbeddedCommitSignature {
    /// Raw 64-byte ECDSA P-256 signature (`r || s`).
    pub signature: Vec<u8>,
    /// SEC1-encoded public key bytes.
    pub public_key: Vec<u8>,
    /// Optional X.509 certificate bytes (PEM or DER).
    pub certificate: Option<Vec<u8>>,
}

#[derive(Deserialize)]
struct VprCommitSignaturePayloadV1 {
    signature: String,
    public_key: String,
    #[serde(default)]
    certificate: Option<String>,
}

fn extract_cert_public_key_sec1(cert_bytes: &[u8]) -> PatientResult<Vec<u8>> {
    let cert_der: Vec<u8> = if cert_bytes
        .windows("-----BEGIN CERTIFICATE-----".len())
        .any(|w| w == b"-----BEGIN CERTIFICATE-----")
    {
        let (_, pem) = x509_parser::pem::parse_x509_pem(cert_bytes)
            .map_err(|e| PatientError::EcdsaPublicKeyParse(Box::new(e)))?;
        pem.contents.to_vec()
    } else {
        cert_bytes.to_vec()
    };

    let (_, cert) = X509Certificate::from_der(cert_der.as_slice())
        .map_err(|e| PatientError::EcdsaPublicKeyParse(Box::new(e)))?;
    let spk = cert.public_key();
    Ok(spk.subject_public_key.data.to_vec())
}

/// Extract the embedded signature material from a commit.
///
/// VPR stores a base64-encoded JSON container in the commit's `gpgsig` header that includes:
/// - `signature` (base64 raw 64-byte `r||s`)
/// - `public_key` (base64 SEC1 public key bytes)
/// - optional `certificate` (base64 of PEM or DER bytes)
///
/// If a certificate is present, this validates that it corresponds to the embedded public key.
pub fn extract_embedded_commit_signature(
    commit: &git2::Commit<'_>,
) -> PatientResult<EmbeddedCommitSignature> {
    let sig_field = commit
        .header_field_bytes("gpgsig")
        .map_err(|_| PatientError::InvalidCommitSignaturePayload)?;
    if sig_field.is_empty() {
        return Err(PatientError::InvalidCommitSignaturePayload);
    }

    let sig_field_str = std::str::from_utf8(sig_field.as_ref())
        .map_err(|_| PatientError::InvalidCommitSignaturePayload)?;
    let sig_b64: String = sig_field_str.lines().map(|l| l.trim()).collect();

    let payload_bytes = general_purpose::STANDARD
        .decode(sig_b64)
        .map_err(|_| PatientError::InvalidCommitSignaturePayload)?;
    let payload: VprCommitSignaturePayloadV1 = serde_json::from_slice(&payload_bytes)
        .map_err(|_| PatientError::InvalidCommitSignaturePayload)?;

    let signature = general_purpose::STANDARD
        .decode(payload.signature)
        .map_err(|_| PatientError::InvalidCommitSignaturePayload)?;
    let public_key = general_purpose::STANDARD
        .decode(payload.public_key)
        .map_err(|_| PatientError::InvalidCommitSignaturePayload)?;
    let certificate = match payload.certificate {
        Some(cert_b64) => Some(
            general_purpose::STANDARD
                .decode(cert_b64)
                .map_err(|_| PatientError::InvalidCommitSignaturePayload)?,
        ),
        None => None,
    };

    if let Some(cert_bytes) = certificate.as_deref() {
        let cert_public_key = extract_cert_public_key_sec1(cert_bytes)?;
        if cert_public_key != public_key {
            return Err(PatientError::AuthorCertificatePublicKeyMismatch);
        }
    }

    Ok(EmbeddedCommitSignature {
        signature,
        public_key,
        certificate,
    })
}

/// A declared professional registration for an author.
///
/// This is rendered in commit trailers as:
///
/// `Author-Registration: <authority> <number>`
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct AuthorRegistration {
    pub authority: NonEmptyText,
    pub number: NonEmptyText,
}

impl AuthorRegistration {
    pub fn new(authority: impl Into<String>, number: impl Into<String>) -> PatientResult<Self> {
        let authority_str = authority.into().trim().to_string();
        let number_str = number.into().trim().to_string();

        if authority_str.is_empty()
            || number_str.is_empty()
            || authority_str.contains(['\n', '\r'])
            || number_str.contains(['\n', '\r'])
            || authority_str.chars().any(char::is_whitespace)
            || number_str.chars().any(char::is_whitespace)
        {
            return Err(PatientError::InvalidAuthorRegistration);
        }

        let authority = NonEmptyText::new(authority_str)
            .map_err(|_| PatientError::InvalidAuthorRegistration)?;
        let number =
            NonEmptyText::new(number_str).map_err(|_| PatientError::InvalidAuthorRegistration)?;

        Ok(Self { authority, number })
    }
}

impl Author {
    /// Validate that this author contains the mandatory commit author metadata.
    ///
    /// This validation is intended to run before commit creation/signing.
    pub fn validate_commit_author(&self) -> PatientResult<()> {
        // Role is guaranteed non-empty by NonEmptyText type
        // Authority and number are guaranteed non-empty by NonEmptyText type

        for reg in &self.registrations {
            AuthorRegistration::new(reg.authority.as_str(), reg.number.as_str())?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod author_tests {
    use super::*;

    fn base_author() -> Author {
        Author {
            name: NonEmptyText::new("Test Author").unwrap(),
            role: NonEmptyText::new("Clinician").unwrap(),
            email: EmailAddress::parse("test@example.com").unwrap(),
            registrations: vec![],
            signature: None,
            certificate: None,
        }
    }

    #[test]
    fn validate_commit_author_rejects_invalid_registration() {
        let _author = base_author();
        // Try to create registration with invalid authority (contains space)
        let err =
            AuthorRegistration::new("G MC", "12345").expect_err("expected validation failure");
        assert!(matches!(err, PatientError::InvalidAuthorRegistration));
    }

    #[test]
    fn validate_commit_author_accepts_valid_author() {
        let mut author = base_author();
        author.registrations =
            vec![AuthorRegistration::new("GMC", "12345").expect("valid registration")];

        author
            .validate_commit_author()
            .expect("expected validation to succeed");
    }
}
