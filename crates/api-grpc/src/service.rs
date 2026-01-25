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
use fhir::{
    coordination_status::LifecycleState, messaging::SensitivityLevel,
    messaging::ThreadStatus as FhirThreadStatus, AuthorRole, MessageAuthor as FhirMessageAuthor,
};
use std::sync::Arc;
use tonic::{Request, Response, Status};
use vpr_core::{
    repositories::clinical::ClinicalService,
    repositories::coordination::{
        CoordinationService, CoordinationStatusUpdate, LedgerUpdate, MessageContent,
    },
    repositories::demographics::{DemographicsService, Uninitialised as DemographicsUninitialised},
    types::NonEmptyText,
    Author, AuthorRegistration, CoreConfig, PatientService, ShardableUuid, TimestampId,
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
    demographics_service: DemographicsService<DemographicsUninitialised>,
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
        let care_location = NonEmptyText::new(&req.care_location)
            .map_err(|e| Status::invalid_argument(format!("Invalid care_location: {}", e)))?;
        let clinical_service = ClinicalService::new(self.cfg.clone());
        match clinical_service.initialise(author, care_location) {
            Ok(service) => {
                let resp = pb::CreatePatientRes {
                    filename: "".to_string(), // No filename for initialise
                    patient: Some(pb::Patient {
                        id: service.clinical_id().simple().to_string(),
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

    async fn initialise_full_record(
        &self,
        req: Request<pb::InitialiseFullRecordReq>,
    ) -> Result<Response<pb::InitialiseFullRecordRes>, Status> {
        let api_key = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;
        auth::validate_api_key(api_key)?;

        let req = req.into_inner();
        let author = build_author(
            req.author_name,
            req.author_email,
            req.author_role,
            req.author_registrations,
            req.author_signature,
        );

        let care_location = NonEmptyText::new(&req.care_location)
            .map_err(|e| Status::invalid_argument(format!("Invalid care_location: {}", e)))?;

        let patient_service = PatientService::new(self.cfg.clone());
        match patient_service.initialise_full_record(
            author,
            care_location,
            req.given_names,
            req.last_name,
            req.birth_date,
            if req.namespace.is_empty() {
                None
            } else {
                Some(req.namespace)
            },
        ) {
            Ok(record) => Ok(Response::new(pb::InitialiseFullRecordRes {
                demographics_uuid: record.demographics_uuid,
                clinical_uuid: record.clinical_uuid,
                coordination_uuid: record.coordination_uuid,
            })),
            Err(e) => Err(Status::internal(format!(
                "Failed to initialise full record: {}",
                e
            ))),
        }
    }

    async fn initialise_demographics(
        &self,
        req: Request<pb::InitialiseDemographicsReq>,
    ) -> Result<Response<pb::InitialiseDemographicsRes>, Status> {
        let api_key = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;
        auth::validate_api_key(api_key)?;

        let req = req.into_inner();
        let author = build_author(
            req.author_name,
            req.author_email,
            req.author_role,
            req.author_registrations,
            req.author_signature,
        );

        let care_location = NonEmptyText::new(&req.care_location)
            .map_err(|e| Status::invalid_argument(format!("Invalid care_location: {}", e)))?;

        let demographics_service = DemographicsService::new(self.cfg.clone());
        match demographics_service.initialise(author, care_location) {
            Ok(service) => Ok(Response::new(pb::InitialiseDemographicsRes {
                demographics_uuid: service.demographics_id().to_string(),
            })),
            Err(e) => Err(Status::internal(format!(
                "Failed to initialise demographics: {}",
                e
            ))),
        }
    }

    async fn update_demographics(
        &self,
        req: Request<pb::UpdateDemographicsReq>,
    ) -> Result<Response<pb::UpdateDemographicsRes>, Status> {
        let api_key = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;
        auth::validate_api_key(api_key)?;

        let req = req.into_inner();
        let demographics_service =
            DemographicsService::with_id(self.cfg.clone(), &req.demographics_uuid).map_err(
                |e| Status::invalid_argument(format!("Invalid demographics UUID: {}", e)),
            )?;

        match demographics_service.update(req.given_names, &req.last_name, &req.birth_date) {
            Ok(()) => Ok(Response::new(pb::UpdateDemographicsRes { success: true })),
            Err(e) => Err(Status::internal(format!(
                "Failed to update demographics: {}",
                e
            ))),
        }
    }

    async fn initialise_clinical(
        &self,
        req: Request<pb::InitialiseClinicalReq>,
    ) -> Result<Response<pb::InitialiseClinicalRes>, Status> {
        let api_key = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;
        auth::validate_api_key(api_key)?;

        let req = req.into_inner();
        let author = build_author(
            req.author_name,
            req.author_email,
            req.author_role,
            req.author_registrations,
            req.author_signature,
        );

        let care_location = NonEmptyText::new(&req.care_location)
            .map_err(|e| Status::invalid_argument(format!("Invalid care_location: {}", e)))?;

        let clinical_service = ClinicalService::new(self.cfg.clone());
        match clinical_service.initialise(author, care_location) {
            Ok(service) => Ok(Response::new(pb::InitialiseClinicalRes {
                clinical_uuid: service.clinical_id().simple().to_string(),
            })),
            Err(e) => Err(Status::internal(format!(
                "Failed to initialise clinical: {}",
                e
            ))),
        }
    }

    async fn link_to_demographics(
        &self,
        req: Request<pb::LinkToDemographicsReq>,
    ) -> Result<Response<pb::LinkToDemographicsRes>, Status> {
        let api_key = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;
        auth::validate_api_key(api_key)?;

        let req = req.into_inner();
        let author = build_author(
            req.author_name,
            req.author_email,
            req.author_role,
            req.author_registrations,
            req.author_signature,
        );

        let clinical_uuid = ShardableUuid::parse(&req.clinical_uuid)
            .map_err(|e| Status::invalid_argument(format!("Invalid clinical UUID: {}", e)))?
            .uuid();

        let care_location = NonEmptyText::new(&req.care_location)
            .map_err(|e| Status::invalid_argument(format!("Invalid care_location: {}", e)))?;

        let clinical_service = ClinicalService::with_id(self.cfg.clone(), clinical_uuid);
        match clinical_service.link_to_demographics(
            &author,
            care_location,
            &req.demographics_uuid,
            if req.namespace.is_empty() {
                None
            } else {
                Some(req.namespace)
            },
        ) {
            Ok(()) => Ok(Response::new(pb::LinkToDemographicsRes { success: true })),
            Err(e) => Err(Status::internal(format!(
                "Failed to link to demographics: {}",
                e
            ))),
        }
    }

    async fn new_letter(
        &self,
        req: Request<pb::NewLetterReq>,
    ) -> Result<Response<pb::NewLetterRes>, Status> {
        let api_key = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;
        auth::validate_api_key(api_key)?;

        let req = req.into_inner();
        let author = build_author(
            req.author_name,
            req.author_email,
            req.author_role,
            req.author_registrations,
            req.author_signature,
        );

        let clinical_uuid = ShardableUuid::parse(&req.clinical_uuid)
            .map_err(|e| Status::invalid_argument(format!("Invalid clinical UUID: {}", e)))?
            .uuid();

        let care_location = NonEmptyText::new(&req.care_location)
            .map_err(|e| Status::invalid_argument(format!("Invalid care_location: {}", e)))?;

        let clinical_service = ClinicalService::with_id(self.cfg.clone(), clinical_uuid);
        match clinical_service.new_letter(&author, care_location, req.content, None) {
            Ok(timestamp_id) => Ok(Response::new(pb::NewLetterRes { timestamp_id })),
            Err(e) => Err(Status::internal(format!("Failed to create letter: {}", e))),
        }
    }

    async fn read_letter(
        &self,
        req: Request<pb::ReadLetterReq>,
    ) -> Result<Response<pb::ReadLetterRes>, Status> {
        let api_key = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;
        auth::validate_api_key(api_key)?;

        let req = req.into_inner();
        let clinical_uuid = ShardableUuid::parse(&req.clinical_uuid)
            .map_err(|e| Status::invalid_argument(format!("Invalid clinical UUID: {}", e)))?
            .uuid();

        let clinical_service = ClinicalService::with_id(self.cfg.clone(), clinical_uuid);
        match clinical_service.read_letter(&req.letter_timestamp_id) {
            Ok(result) => Ok(Response::new(pb::ReadLetterRes {
                body_content: result.body_content.to_string(),
                rm_version: format!("{:?}", result.letter_data.rm_version),
                composer_name: result.letter_data.composer_name,
                composer_role: result.letter_data.composer_role,
                start_time: result.letter_data.start_time.to_rfc3339(),
                clinical_lists: result
                    .letter_data
                    .clinical_lists
                    .into_iter()
                    .map(|list| pb::ClinicalList {
                        name: list.name,
                        kind: list.kind,
                    })
                    .collect(),
            })),
            Err(e) => Err(Status::internal(format!("Failed to read letter: {}", e))),
        }
    }

    async fn new_letter_with_attachments(
        &self,
        req: Request<pb::NewLetterWithAttachmentsReq>,
    ) -> Result<Response<pb::NewLetterWithAttachmentsRes>, Status> {
        let api_key = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;
        auth::validate_api_key(api_key)?;

        let req = req.into_inner();
        let author = build_author(
            req.author_name,
            req.author_email,
            req.author_role,
            req.author_registrations,
            req.author_signature,
        );

        let clinical_uuid = ShardableUuid::parse(&req.clinical_uuid)
            .map_err(|e| Status::invalid_argument(format!("Invalid clinical UUID: {}", e)))?
            .uuid();

        // Write attachment files to temporary directory
        let temp_dir =
            std::env::temp_dir().join(format!("vpr_attachments_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| Status::internal(format!("Failed to create temp dir: {}", e)))?;

        let mut attachment_paths = Vec::new();
        for (i, (content, name)) in req
            .attachment_files
            .iter()
            .zip(&req.attachment_names)
            .enumerate()
        {
            let file_path = temp_dir.join(format!("{}_{}", i, name));
            std::fs::write(&file_path, content)
                .map_err(|e| Status::internal(format!("Failed to write attachment: {}", e)))?;
            attachment_paths.push(file_path);
        }

        let care_location = NonEmptyText::new(&req.care_location)
            .map_err(|e| Status::invalid_argument(format!("Invalid care_location: {}", e)))?;

        let clinical_service = ClinicalService::with_id(self.cfg.clone(), clinical_uuid);
        let result = clinical_service.new_letter_with_attachments(
            &author,
            care_location,
            &attachment_paths,
            None,
        );

        // Clean up temp files
        let _ = std::fs::remove_dir_all(&temp_dir);

        match result {
            Ok(timestamp_id) => Ok(Response::new(pb::NewLetterWithAttachmentsRes {
                timestamp_id,
            })),
            Err(e) => Err(Status::internal(format!(
                "Failed to create letter with attachments: {}",
                e
            ))),
        }
    }

    async fn new_letter_complete(
        &self,
        req: Request<pb::NewLetterCompleteReq>,
    ) -> Result<Response<pb::NewLetterCompleteRes>, Status> {
        let api_key = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;
        auth::validate_api_key(api_key)?;

        let req = req.into_inner();
        let author = build_author(
            req.author_name,
            req.author_email,
            req.author_role,
            req.author_registrations,
            req.author_signature,
        );

        let clinical_uuid = ShardableUuid::parse(&req.clinical_uuid)
            .map_err(|e| Status::invalid_argument(format!("Invalid clinical UUID: {}", e)))?
            .uuid();

        // Write attachment files to temporary directory
        let temp_dir =
            std::env::temp_dir().join(format!("vpr_attachments_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir)
            .map_err(|e| Status::internal(format!("Failed to create temp dir: {}", e)))?;

        let mut attachment_paths = Vec::new();
        for (i, (content, name)) in req
            .attachment_files
            .iter()
            .zip(&req.attachment_names)
            .enumerate()
        {
            let file_path = temp_dir.join(format!("{}_{}", i, name));
            std::fs::write(&file_path, content)
                .map_err(|e| Status::internal(format!("Failed to write attachment: {}", e)))?;
            attachment_paths.push(file_path);
        }

        let care_location = NonEmptyText::new(&req.care_location)
            .map_err(|e| Status::invalid_argument(format!("Invalid care_location: {}", e)))?;

        let clinical_service = ClinicalService::with_id(self.cfg.clone(), clinical_uuid);
        let result = clinical_service.create_letter(
            &author,
            care_location,
            Some(req.content),
            &attachment_paths,
            None,
        );

        // Clean up temp files
        let _ = std::fs::remove_dir_all(&temp_dir);

        match result {
            Ok(timestamp_id) => Ok(Response::new(pb::NewLetterCompleteRes { timestamp_id })),
            Err(e) => Err(Status::internal(format!(
                "Failed to create complete letter: {}",
                e
            ))),
        }
    }

    async fn get_letter_attachments(
        &self,
        req: Request<pb::GetLetterAttachmentsReq>,
    ) -> Result<Response<pb::GetLetterAttachmentsRes>, Status> {
        let api_key = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;
        auth::validate_api_key(api_key)?;

        let req = req.into_inner();
        let clinical_uuid = ShardableUuid::parse(&req.clinical_uuid)
            .map_err(|e| Status::invalid_argument(format!("Invalid clinical UUID: {}", e)))?
            .uuid();

        let clinical_service = ClinicalService::with_id(self.cfg.clone(), clinical_uuid);
        match clinical_service.get_letter_attachments(&req.letter_timestamp_id) {
            Ok(attachments) => Ok(Response::new(pb::GetLetterAttachmentsRes {
                attachments: attachments
                    .into_iter()
                    .map(|att| pb::LetterAttachment {
                        metadata: Some(pb::AttachmentMetadata {
                            filename: att.metadata.metadata_filename.to_string(),
                            original_filename: att.metadata.original_filename.to_string(),
                            hash: att.metadata.hash.to_string(),
                            file_storage_path: att.metadata.file_storage_path.to_string(),
                            size_bytes: att.metadata.size_bytes as i64,
                            media_type: att
                                .metadata
                                .media_type
                                .map(|mt| mt.to_string())
                                .unwrap_or_default(),
                        }),
                        content: att.content,
                    })
                    .collect(),
            })),
            Err(e) => Err(Status::internal(format!(
                "Failed to get letter attachments: {}",
                e
            ))),
        }
    }

    async fn initialise_coordination(
        &self,
        req: Request<pb::InitialiseCoordinationReq>,
    ) -> Result<Response<pb::InitialiseCoordinationRes>, Status> {
        let api_key = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;
        auth::validate_api_key(api_key)?;

        let req = req.into_inner();
        let author = build_author(
            req.author_name,
            req.author_email,
            req.author_role,
            req.author_registrations,
            req.author_signature,
        );

        let clinical_uuid = uuid::Uuid::parse_str(&req.clinical_uuid)
            .map_err(|e| Status::invalid_argument(format!("Invalid clinical UUID: {}", e)))?;

        let coordination_service = CoordinationService::new(self.cfg.clone());
        match coordination_service.initialise(author, req.care_location, clinical_uuid) {
            Ok(service) => Ok(Response::new(pb::InitialiseCoordinationRes {
                coordination_uuid: service.coordination_id().to_string(),
            })),
            Err(e) => Err(Status::internal(format!(
                "Failed to initialise coordination: {}",
                e
            ))),
        }
    }

    async fn create_thread(
        &self,
        req: Request<pb::CreateThreadReq>,
    ) -> Result<Response<pb::CreateThreadRes>, Status> {
        let api_key = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;
        auth::validate_api_key(api_key)?;

        let req = req.into_inner();
        let author = build_author(
            req.author_name,
            req.author_email,
            req.author_role,
            req.author_registrations,
            req.author_signature,
        );

        let coordination_uuid = ShardableUuid::parse(&req.coordination_uuid)
            .map_err(|e| Status::invalid_argument(format!("Invalid coordination UUID: {}", e)))?
            .uuid();

        let participants: Vec<FhirMessageAuthor> = req
            .participants
            .into_iter()
            .map(|p| {
                Ok(FhirMessageAuthor {
                    id: uuid::Uuid::parse_str(&p.id).map_err(|e| {
                        Status::invalid_argument(format!("Invalid participant UUID: {}", e))
                    })?,
                    name: p.name,
                    role: parse_author_role(&p.role)?,
                })
            })
            .collect::<Result<Vec<_>, Status>>()?;

        let initial_message_author = req
            .initial_message_author
            .ok_or_else(|| Status::invalid_argument("Missing initial message author"))?;

        let message = MessageContent::new(
            FhirMessageAuthor {
                id: uuid::Uuid::parse_str(&initial_message_author.id)
                    .map_err(|e| Status::invalid_argument(format!("Invalid author UUID: {}", e)))?,
                name: initial_message_author.name,
                role: parse_author_role(&initial_message_author.role)?,
            },
            req.initial_message_body,
            None,
        )
        .map_err(|e| Status::invalid_argument(format!("Invalid message: {}", e)))?;

        let coordination_service =
            CoordinationService::with_id(self.cfg.clone(), coordination_uuid);
        match coordination_service.communication_create(
            &author,
            req.care_location,
            participants,
            message,
        ) {
            Ok(thread_id) => Ok(Response::new(pb::CreateThreadRes {
                thread_id: thread_id.to_string(),
            })),
            Err(e) => Err(Status::internal(format!("Failed to create thread: {}", e))),
        }
    }

    async fn add_message(
        &self,
        req: Request<pb::AddMessageReq>,
    ) -> Result<Response<pb::AddMessageRes>, Status> {
        let api_key = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;
        auth::validate_api_key(api_key)?;

        let req = req.into_inner();
        let author = build_author(
            req.author_name,
            req.author_email,
            req.author_role,
            req.author_registrations,
            req.author_signature,
        );

        let coordination_uuid = ShardableUuid::parse(&req.coordination_uuid)
            .map_err(|e| Status::invalid_argument(format!("Invalid coordination UUID: {}", e)))?
            .uuid();

        let thread_id: TimestampId = req
            .thread_id
            .parse()
            .map_err(|e| Status::invalid_argument(format!("Invalid thread ID: {}", e)))?;

        let message_author = req
            .message_author
            .ok_or_else(|| Status::invalid_argument("Missing message author"))?;

        let corrects =
            if req.corrects.is_empty() {
                None
            } else {
                Some(uuid::Uuid::parse_str(&req.corrects).map_err(|e| {
                    Status::invalid_argument(format!("Invalid corrects UUID: {}", e))
                })?)
            };

        let message = MessageContent::new(
            FhirMessageAuthor {
                id: uuid::Uuid::parse_str(&message_author.id)
                    .map_err(|e| Status::invalid_argument(format!("Invalid author UUID: {}", e)))?,
                name: message_author.name,
                role: parse_author_role(&message_author.role)?,
            },
            req.message_body,
            corrects,
        )
        .map_err(|e| Status::invalid_argument(format!("Invalid message: {}", e)))?;

        let coordination_service =
            CoordinationService::with_id(self.cfg.clone(), coordination_uuid);
        match coordination_service.message_add(&author, req.care_location, &thread_id, message) {
            Ok(message_id) => Ok(Response::new(pb::AddMessageRes {
                message_id: message_id.to_string(),
            })),
            Err(e) => Err(Status::internal(format!("Failed to add message: {}", e))),
        }
    }

    async fn read_communication(
        &self,
        req: Request<pb::ReadCommunicationReq>,
    ) -> Result<Response<pb::ReadCommunicationRes>, Status> {
        let api_key = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;
        auth::validate_api_key(api_key)?;

        let req = req.into_inner();
        let coordination_uuid = ShardableUuid::parse(&req.coordination_uuid)
            .map_err(|e| Status::invalid_argument(format!("Invalid coordination UUID: {}", e)))?
            .uuid();

        let thread_id: TimestampId = req
            .thread_id
            .parse()
            .map_err(|e| Status::invalid_argument(format!("Invalid thread ID: {}", e)))?;

        let coordination_service =
            CoordinationService::with_id(self.cfg.clone(), coordination_uuid);
        match coordination_service.read_communication(&thread_id) {
            Ok(comm) => Ok(Response::new(pb::ReadCommunicationRes {
                communication_id: comm.communication_id.clone(),
                ledger: Some(pb::Ledger {
                    communication_id: comm.communication_id,
                    status: format!("{:?}", comm.ledger.status).to_lowercase(),
                    participants: comm
                        .ledger
                        .participants
                        .into_iter()
                        .map(|p| pb::LedgerParticipant {
                            id: p.id.to_string(),
                            name: p.name,
                            role: format!("{:?}", p.role).to_lowercase(),
                        })
                        .collect(),
                    sensitivity: format!("{:?}", comm.ledger.sensitivity).to_lowercase(),
                    restricted: comm.ledger.restricted,
                    allow_patient_participation: comm.ledger.allow_patient_participation,
                    allow_external_organisations: comm.ledger.allow_external_organisations,
                    created_at: comm.ledger.created_at.to_rfc3339(),
                    last_updated_at: comm.ledger.last_updated_at.to_rfc3339(),
                }),
                messages: comm
                    .messages
                    .into_iter()
                    .map(|msg| pb::Message {
                        metadata: Some(pb::MessageMetadata {
                            message_id: msg.metadata.message_id.to_string(),
                            author: Some(pb::MessageAuthor {
                                id: msg.metadata.author.id.to_string(),
                                name: msg.metadata.author.name,
                                role: format!("{:?}", msg.metadata.author.role).to_lowercase(),
                            }),
                            timestamp: msg.metadata.timestamp.to_rfc3339(),
                            corrects: msg.corrects.map(|id| id.to_string()).unwrap_or_default(),
                        }),
                        body: msg.body,
                    })
                    .collect(),
            })),
            Err(e) => Err(Status::internal(format!(
                "Failed to read communication: {}",
                e
            ))),
        }
    }

    async fn update_communication_ledger(
        &self,
        req: Request<pb::UpdateCommunicationLedgerReq>,
    ) -> Result<Response<pb::UpdateCommunicationLedgerRes>, Status> {
        let api_key = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;
        auth::validate_api_key(api_key)?;

        let req = req.into_inner();
        let author = build_author(
            req.author_name,
            req.author_email,
            req.author_role,
            req.author_registrations,
            req.author_signature,
        );

        let coordination_uuid = ShardableUuid::parse(&req.coordination_uuid)
            .map_err(|e| Status::invalid_argument(format!("Invalid coordination UUID: {}", e)))?
            .uuid();

        let thread_id: TimestampId = req
            .thread_id
            .parse()
            .map_err(|e| Status::invalid_argument(format!("Invalid thread ID: {}", e)))?;

        let add_participants = if req.add_participants.is_empty() {
            None
        } else {
            Some(
                req.add_participants
                    .into_iter()
                    .map(|p| {
                        Ok(FhirMessageAuthor {
                            id: uuid::Uuid::parse_str(&p.id).map_err(|e| {
                                Status::invalid_argument(format!("Invalid participant UUID: {}", e))
                            })?,
                            name: p.name,
                            role: parse_author_role(&p.role)?,
                        })
                    })
                    .collect::<Result<Vec<_>, Status>>()?,
            )
        };

        let remove_participants = if req.remove_participant_ids.is_empty() {
            None
        } else {
            Some(
                req.remove_participant_ids
                    .into_iter()
                    .map(|id| {
                        uuid::Uuid::parse_str(&id)
                            .map_err(|e| Status::invalid_argument(format!("Invalid UUID: {}", e)))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            )
        };

        let set_status = if req.set_status.is_empty() {
            None
        } else {
            Some(parse_thread_status(&req.set_status)?)
        };

        let set_visibility = if req.set_sensitivity.is_empty() {
            None
        } else {
            Some((
                parse_sensitivity_level(&req.set_sensitivity)?,
                req.set_restricted.unwrap_or(false),
            ))
        };

        let set_policies = match (req.set_allow_patient, req.set_allow_external) {
            (Some(ap), Some(ae)) => Some((ap, ae)),
            (Some(ap), None) => Some((ap, true)),
            (None, Some(ae)) => Some((true, ae)),
            (None, None) => None,
        };

        let ledger_update = LedgerUpdate {
            add_participants,
            remove_participants,
            set_status,
            set_visibility,
            set_policies,
        };

        let coordination_service =
            CoordinationService::with_id(self.cfg.clone(), coordination_uuid);
        match coordination_service.update_communication_ledger(
            &author,
            req.care_location,
            &thread_id,
            ledger_update,
        ) {
            Ok(()) => Ok(Response::new(pb::UpdateCommunicationLedgerRes {
                success: true,
            })),
            Err(e) => Err(Status::internal(format!(
                "Failed to update communication ledger: {}",
                e
            ))),
        }
    }

    async fn update_coordination_status(
        &self,
        req: Request<pb::UpdateCoordinationStatusReq>,
    ) -> Result<Response<pb::UpdateCoordinationStatusRes>, Status> {
        let api_key = req
            .metadata()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;
        auth::validate_api_key(api_key)?;

        let req = req.into_inner();
        let author = build_author(
            req.author_name,
            req.author_email,
            req.author_role,
            req.author_registrations,
            req.author_signature,
        );

        let coordination_uuid = ShardableUuid::parse(&req.coordination_uuid)
            .map_err(|e| Status::invalid_argument(format!("Invalid coordination UUID: {}", e)))?
            .uuid();

        let set_lifecycle_state = if req.set_lifecycle_state.is_empty() {
            None
        } else {
            Some(parse_lifecycle_state(&req.set_lifecycle_state)?)
        };

        let status_update = CoordinationStatusUpdate {
            set_lifecycle_state,
            set_record_open: req.set_record_open,
            set_record_queryable: req.set_record_queryable,
            set_record_modifiable: req.set_record_modifiable,
        };

        let coordination_service =
            CoordinationService::with_id(self.cfg.clone(), coordination_uuid);
        match coordination_service.update_coordination_status(
            &author,
            req.care_location,
            status_update,
        ) {
            Ok(()) => Ok(Response::new(pb::UpdateCoordinationStatusRes {
                success: true,
            })),
            Err(e) => Err(Status::internal(format!(
                "Failed to update coordination status: {}",
                e
            ))),
        }
    }
}

// Helper functions
fn build_author(
    name: String,
    email: String,
    role: String,
    registrations: Vec<pb::AuthorRegistration>,
    signature: String,
) -> Author {
    Author {
        name,
        email,
        role,
        registrations: registrations
            .into_iter()
            .map(|r| AuthorRegistration {
                authority: r.authority,
                number: r.number,
            })
            .collect(),
        signature: if signature.is_empty() {
            None
        } else {
            Some(signature)
        },
        certificate: None,
    }
}

#[allow(clippy::result_large_err)]
fn parse_author_role(role: &str) -> Result<AuthorRole, Status> {
    match role.to_lowercase().as_str() {
        "clinician" => Ok(AuthorRole::Clinician),
        "careadministrator" | "care_administrator" | "care-administrator" => {
            Ok(AuthorRole::CareAdministrator)
        }
        "patient" => Ok(AuthorRole::Patient),
        "patientassociate" | "patient_associate" | "patient-associate" => {
            Ok(AuthorRole::PatientAssociate)
        }
        "system" => Ok(AuthorRole::System),
        _ => Err(Status::invalid_argument(format!("Invalid role: {}", role))),
    }
}

#[allow(clippy::result_large_err)]
fn parse_thread_status(status: &str) -> Result<FhirThreadStatus, Status> {
    match status.to_lowercase().as_str() {
        "open" => Ok(FhirThreadStatus::Open),
        "closed" => Ok(FhirThreadStatus::Closed),
        "archived" => Ok(FhirThreadStatus::Archived),
        _ => Err(Status::invalid_argument(format!(
            "Invalid status: {}",
            status
        ))),
    }
}

#[allow(clippy::result_large_err)]
fn parse_sensitivity_level(sensitivity: &str) -> Result<SensitivityLevel, Status> {
    match sensitivity.to_lowercase().as_str() {
        "standard" => Ok(SensitivityLevel::Standard),
        "confidential" => Ok(SensitivityLevel::Confidential),
        "restricted" => Ok(SensitivityLevel::Restricted),
        _ => Err(Status::invalid_argument(format!(
            "Invalid sensitivity: {}",
            sensitivity
        ))),
    }
}

#[allow(clippy::result_large_err)]
fn parse_lifecycle_state(state: &str) -> Result<LifecycleState, Status> {
    match state.to_lowercase().as_str() {
        "active" => Ok(LifecycleState::Active),
        "suspended" => Ok(LifecycleState::Suspended),
        "closed" => Ok(LifecycleState::Closed),
        _ => Err(Status::invalid_argument(format!(
            "Invalid lifecycle state: {}",
            state
        ))),
    }
}
