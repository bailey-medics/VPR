// Re-export the proto module from the shared `api-shared` crate so callers
// can continue to reference `api::service::pb`.
pub use api_shared::pb;

use api_shared::auth;
use api_shared::HealthService;
use tonic::{Request, Response, Status};
use vpr_core::PatientService;

/// Authentication interceptor for gRPC requests
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

#[derive(Default, Clone)]
pub struct VprService {
    patient_service: PatientService,
}

#[tonic::async_trait]
impl Vpr for VprService {
    async fn health(&self, _req: Request<()>) -> Result<Response<HealthRes>, Status> {
        let health_res = HealthService::check_health();
        Ok(Response::new(health_res))
    }

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
        match self
            .patient_service
            .create_patient(req.first_name, req.last_name)
        {
            Ok(resp) => Ok(Response::new(resp)),
            Err(e) => Err(Status::internal(format!("Failed to create patient: {}", e))),
        }
    }

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

        let patients = self.patient_service.list_patients();
        Ok(Response::new(pb::ListPatientsRes { patients }))
    }
}
