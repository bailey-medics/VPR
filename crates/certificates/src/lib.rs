use rcgen::{
    CertificateParams, DistinguishedName, DnType, Ia5String, IsCa, KeyPair, KeyUsagePurpose,
    SanType, SerialNumber,
};
use thiserror::Error;

/// Errors that can occur during certificate creation.
#[derive(Error, Debug)]
pub enum CertificateError {
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
        let mut params = CertificateParams::default();

        // Set subject with only Common Name
        let mut subject = DistinguishedName::new();
        subject.push(DnType::CommonName, name);
        params.distinguished_name = subject;

        // Set issuer (assuming self-signed for simplicity, but in practice this would be a CA)
        params.is_ca = IsCa::NoCa;

        // Add registration number as a URI in subjectAltName
        let uri = format!("vpr://{}/{}", registration_authority, registration_number);
        params
            .subject_alt_names
            .push(SanType::URI(Ia5String::try_from(uri).unwrap()));

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
