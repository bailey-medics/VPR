use crate::pb::HealthRes;

/// Simple health service that can be used by both gRPC and REST APIs
#[derive(Clone)]
pub struct HealthService;

impl HealthService {
    pub fn new() -> Self {
        Self
    }

    /// Static method to check health without creating an instance
    pub fn check_health() -> HealthRes {
        HealthRes {
            ok: true,
            message: "VPR is alive".into(),
        }
    }

    /// Instance method for compatibility
    pub fn check_health_instance(&self) -> HealthRes {
        Self::check_health()
    }
}

impl Default for HealthService {
    fn default() -> Self {
        Self::new()
    }
}
