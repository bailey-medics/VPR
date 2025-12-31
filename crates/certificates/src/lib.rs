//! X.509 certificate generation utilities.
//!
//! ## Purpose
//! Provides helper functionality to generate self-signed X.509 certificates.
//!
//! ## Intended use
//! Certificates are used for user authentication and commit signing within the wider VPR system.

use rcgen::{
    CertificateParams, DistinguishedName, DnType, Ia5String, IsCa, KeyPair, KeyUsagePurpose,
    SanType, SerialNumber,
};
use thiserror::Error;

/// Errors that can occur during certificate creation.
#[derive(Error, Debug)]
pub enum CertificateError {
    #[error("Invalid certificate input: {0}")]
    InvalidInput(String),
    #[error("Failed to generate certificate: {0}")]
    GenerationError(String),
}

/// A struct representing a digital certificate for professional registration.
/// This generates X.509 certificates compliant with the specified requirements.
pub struct Certificate;

impl Certificate {
    /// Creates a new X.509 certificate with the specified parameters.
    ///
    /// # Arguments
    ///
    /// * `name` - The full name of the person (used as Common Name in Subject).
    /// * `registration_authority` - The registration authority (e.g., "GMC", "NMC").
    /// * `registration_number` - The professional registration number.
    ///
    /// # Returns
    ///
    /// A tuple of (X.509 certificate PEM, private key PEM).
    ///
    /// # Errors
    ///
    /// Returns `CertificateError::GenerationError` if certificate generation fails.
    pub fn create(
        name: &str,
        registration_authority: &str,
        registration_number: &str,
    ) -> Result<(String, String), CertificateError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(CertificateError::InvalidInput(
                "name must not be empty".to_string(),
            ));
        }
        if name.contains(['\n', '\r']) {
            return Err(CertificateError::InvalidInput(
                "name must not contain newlines".to_string(),
            ));
        }

        let registration_authority = registration_authority.trim();
        if registration_authority.is_empty() {
            return Err(CertificateError::InvalidInput(
                "registration_authority must not be empty".to_string(),
            ));
        }
        if registration_authority.contains(['\n', '\r']) {
            return Err(CertificateError::InvalidInput(
                "registration_authority must not contain newlines".to_string(),
            ));
        }

        let registration_number = registration_number.trim();
        if registration_number.is_empty() {
            return Err(CertificateError::InvalidInput(
                "registration_number must not be empty".to_string(),
            ));
        }
        if registration_number.contains(['\n', '\r']) {
            return Err(CertificateError::InvalidInput(
                "registration_number must not contain newlines".to_string(),
            ));
        }

        let mut params = CertificateParams::default();

        // Subject DN fields are designed to be human-readable in tools like:
        // `openssl x509 -noout -subject`
        let mut subject = DistinguishedName::new();
        subject.push(DnType::CommonName, name);
        subject.push(DnType::OrganizationName, registration_authority);
        // X.520 serialNumber (OID 2.5.4.5) – commonly displayed by OpenSSL as `serialNumber=...`.
        subject.push(DnType::CustomDnType(vec![2, 5, 4, 5]), registration_number);
        params.distinguished_name = subject;

        // Set issuer (assuming self-signed for simplicity, but in practice this would be a CA)
        params.is_ca = IsCa::NoCa;

        // Add registration number as a URI in subjectAltName
        let uri = format!("vpr://{}/{}", registration_authority, registration_number);
        let uri = Ia5String::try_from(uri).map_err(|e| {
            CertificateError::InvalidInput(format!("invalid registration URI: {e}"))
        })?;
        params.subject_alt_names.push(SanType::URI(uri));

        // Set key usage for signing
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::ContentCommitment,
        ];

        // Set validity period (1 year from now)
        let now = time::OffsetDateTime::now_utc();
        params.not_before = now;
        params.not_after = now + time::Duration::days(365);

        // Generate serial number
        params.serial_number = Some(SerialNumber::from(vec![0, 1, 2, 3, 4, 5, 6, 7]));

        // Generate key pair
        let key_pair =
            KeyPair::generate().map_err(|e| CertificateError::GenerationError(e.to_string()))?;

        // Generate the certificate
        let cert = params
            .self_signed(&key_pair)
            .map_err(|e| CertificateError::GenerationError(e.to_string()))?;

        Ok((cert.pem(), key_pair.serialize_pem()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject_and_san_uris_from_pem(cert_pem: &str) -> (String, Vec<String>) {
        let (_, pem) = x509_parser::pem::parse_x509_pem(cert_pem.as_bytes()).unwrap();
        let (_, cert) = x509_parser::parse_x509_certificate(pem.contents.as_slice()).unwrap();

        let subject = cert.subject().to_string();
        let mut uris = Vec::new();

        for ext in cert.extensions() {
            if let x509_parser::extensions::ParsedExtension::SubjectAlternativeName(san) =
                ext.parsed_extension()
            {
                for name in san.general_names.iter() {
                    if let x509_parser::extensions::GeneralName::URI(uri) = name {
                        uris.push((*uri).to_string());
                    }
                }
            }
        }

        (subject, uris)
    }

    #[test]
    fn test_create_certificate() {
        let name = "John Doe";
        let authority = "GMC";
        let number = "123456";

        let (cert_pem, key_pem) = Certificate::create(name, authority, number).unwrap();

        // Basic checks
        assert!(cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(cert_pem.contains("END CERTIFICATE"));
        assert!(key_pem.contains("BEGIN PRIVATE KEY"));
        assert!(key_pem.contains("END PRIVATE KEY"));
        // Note: The actual content is DER encoded, so we can't check plain text

        let (subject, uris) = subject_and_san_uris_from_pem(&cert_pem);
        assert!(subject.contains("CN=John Doe"));
        assert!(subject.contains("O=GMC"));
        assert!(subject.contains("serialNumber=123456") || subject.contains("2.5.4.5=123456"));
        assert!(uris.iter().any(|u| u == "vpr://GMC/123456"));
    }

    #[test]
    fn test_certificate_creation_does_not_fail() {
        let name = "Jane Smith";
        let authority = "NMC";
        let number = "789012";

        let result = Certificate::create(name, authority, number);
        assert!(result.is_ok());
        let (cert, key) = result.unwrap();
        assert!(!cert.is_empty());
        assert!(!key.is_empty());
    }
}
