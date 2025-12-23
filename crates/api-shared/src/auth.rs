use std::env;

/// Validates the provided API key against the expected API key from environment.
///
/// This function compares the provided API key with the value of the `API_KEY`
/// environment variable. It's used by both gRPC and REST authentication mechanisms.
///
/// # Arguments
/// * `provided_key` - The API key provided by the client
///
/// # Returns
/// * `Ok(())` - If the provided key matches the expected key
/// * `Err(Status)` - INTERNAL error if API_KEY env var not set, UNAUTHENTICATED if keys don't match
///
/// # Environment Variables
/// * `API_KEY` - The expected API key value (must be set)
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
