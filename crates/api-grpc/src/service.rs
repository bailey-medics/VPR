//! gRPC service implementation and authentication wiring.
//!
//! ## Purpose
//! This module provides the gRPC implementation of the VPR API, including:
//! - An `x-api-key` authentication interceptor.
//! - The `VprService` implementation for protobuf-generated `Vpr` trait methods.
//!
//! ## Intended use
//! API-level concerns (authentication, request/response mapping) live here.
//! Data operations are delegated to services in `vpr-core`.

// Re-export the proto module from the shared `api-shared` crate so callers can continue to
// reference `api::service::pb`.
pub use api_shared::pb;

use api_shared::{auth, HealthService};
use std::sync::Arc;
use tonic::{Request, Response, Status};
use vpr_core::{
    repositories::clinical::ClinicalService, repositories::demographics::DemographicsService,
    Author, AuthorRegistration, CoreConfig,
};

/// Authentication interceptor for gRPC requests.
///
/// This interceptor checks for the presence of an `x-api-key` header in incoming
/// gRPC requests and validates it against the expected API key from environment
/// variables. Requests without a valid API key are rejected with an
/// UNAUTHENTICATED status.
///
/// # Arguments
/// * `req` - The incoming gRPC request
///
/// # Returns
/// * `Ok(Request<()>)` - The request with authentication validated
/// * `Err(Status)` - UNAUTHENTICATED status if API key is missing or invalid
///
/// # Errors
/// Returns `UNAUTHENTICATED` if:
/// - the `x-api-key` header is missing, or
/// - the provided API key does not match `API_KEY`.
#[allow(clippy::result_large_err)]
pub fn auth_interceptor(req: Request<()>) -> Result<Request<()>, Status> {
    let api_key = req
        .metadata()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;

    auth::validate_api_key(api_key)?;
    Ok(req)
}

// Use the shared api-shared crate for generated protobuf types.
use api_shared::pb::{vpr_server::Vpr, CreatePatientReq, CreatePatientRes, HealthRes};

/// gRPC service implementation for VPR patient operations.
///
/// This service implements the Vpr gRPC trait and provides authenticated access
/// to patient data operations. It uses the PatientService from the core crate
/// for actual data operations while handling gRPC protocol concerns and
/// authentication.
#[derive(Clone)]
pub struct VprService {
    cfg: Arc<CoreConfig>,
    demographics_service: DemographicsService,
}

impl VprService {
    pub fn new(cfg: Arc<CoreConfig>) -> Self {
        Self {
            demographics_service: DemographicsService::new(cfg.clone()),
            cfg,
        }
    }
}

#[tonic::async_trait]
impl Vpr for VprService {
    /// Health check endpoint for gRPC service
    ///
    /// Returns the current health status of the VPR service.
    /// This endpoint does not require authentication.
    ///
    /// # Arguments
    /// * `_req` - Empty health check request (unused)
    ///
    /// # Returns
    /// * `Ok(Response<HealthRes>)` - Health status response
    /// * `Err(Status)` - Should not occur for health checks
    async fn health(&self, _req: Request<()>) -> Result<Response<HealthRes>, Status> {
        let health_res = HealthService::check_health();
        Ok(Response::new(health_res))
    }

    /// Creates a new patient record via gRPC
    ///
    /// This endpoint requires authentication via the `x-api-key` header.
    /// It validates the API key, then delegates to the PatientService to
    /// create and store the patient record.
    ///
    /// # Arguments
    /// * `req` - CreatePatientReq containing first_name, last_name, author_name, and author_email
    ///
    /// # Required Headers
    /// * `x-api-key` - Valid API key for authentication
    ///
    /// # Returns
    /// * `Ok(Response<CreatePatientRes>)` - Patient creation result with ID and metadata
    /// * `Err(Status)` - UNAUTHENTICATED if API key invalid, INTERNAL_ERROR for other failures
    async fn create_patient(
        &self,
        req: Request<CreatePatientReq>,
    ) -> Result<Response<CreatePatientRes>, Status> {
        // Check API key
        let api_key = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;
        auth::validate_api_key(api_key)?;

        let req = req.into_inner();

        let registrations: Vec<AuthorRegistration> = req
            .author_registrations
            .into_iter()
            .map(|r| AuthorRegistration {
                authority: r.authority,
                number: r.number,
            })
            .collect();

        let author = Author {
            name: req.author_name,
            role: req.author_role,
            email: req.author_email,
            registrations,
            signature: if req.author_signature.is_empty() {
                None
            } else {
                Some(req.author_signature)
            },
            certificate: None,
        };
        let clinical_service = ClinicalService::new(self.cfg.clone(), None);
        match clinical_service.initialise(author, req.care_location) {
            Ok(uuid) => {
                let resp = pb::CreatePatientRes {
                    filename: "".to_string(), // No filename for initialise
                    patient: Some(pb::Patient {
                        id: uuid.simple().to_string(),
                        first_name: "".to_string(),
                        last_name: "".to_string(),
                        created_at: "".to_string(), // Could set to now, but empty for now
                        national_id: "".to_string(),
                    }),
                };
                Ok(Response::new(resp))
            }
            Err(e) => Err(Status::internal(format!(
                "Failed to initialise clinical: {}",
                e
            ))),
        }
    }

    /// Lists all patient records via gRPC
    ///
    /// This endpoint requires authentication via the `x-api-key` header.
    /// It retrieves all patient records from the file system and returns them.
    ///
    /// # Arguments
    /// * `req` - Empty list request
    ///
    /// # Returns
    /// * `Ok(Response<ListPatientsRes>)` - List of all patient records
    /// * `Err(Status)` - UNAUTHENTICATED if API key invalid
    async fn list_patients(
        &self,
        req: Request<()>,
    ) -> Result<Response<pb::ListPatientsRes>, Status> {
        // Check API key
        let api_key = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;
        auth::validate_api_key(api_key)?;

        let patients = self.demographics_service.list_patients();
        Ok(Response::new(pb::ListPatientsRes { patients }))
    }
}
