use std::env;

/// Validates the provided API key against the expected API key from environment.
///
/// Returns `Ok(())` if the key is valid, or an error if invalid or missing.
#[allow(clippy::result_large_err)]
pub fn validate_api_key(provided_key: &str) -> Result<(), tonic::Status> {
    let expected_key = env::var("API_KEY")
        .map_err(|_| tonic::Status::internal("API_KEY not set in environment"))?;

    if provided_key == expected_key {
        Ok(())
    } else {
        Err(tonic::Status::unauthenticated("Invalid API key"))
    }
}
