use crate::pb::HealthRes;

/// Simple health service that can be used by both gRPC and REST APIs
///
/// This service provides a standardised way to check the health status of the VPR system.
/// It can be used both as a static utility and as an instantiated service.
#[derive(Clone)]
pub struct HealthService;

impl HealthService {
    /// Creates a new instance of HealthService.
    ///
    /// # Returns
    /// A new `HealthService` instance.
    pub fn new() -> Self {
        Self
    }

    /// Static method to check health without creating an instance
    ///
    /// This is the preferred method for health checks as it doesn't require
    /// instantiating the service.
    ///
    /// # Returns
    /// A `HealthRes` indicating the service is healthy.
    pub fn check_health() -> HealthRes {
        HealthRes {
            ok: true,
            message: "VPR is alive".into(),
        }
    }

    /// Instance method for compatibility
    ///
    /// This method is provided for backward compatibility but delegates
    /// to the static `check_health()` method.
    ///
    /// # Returns
    /// A `HealthRes` indicating the service is healthy.
    pub fn check_health_instance(&self) -> HealthRes {
        Self::check_health()
    }
}

impl Default for HealthService {
    fn default() -> Self {
        Self::new()
    }
}
