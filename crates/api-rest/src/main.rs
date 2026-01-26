//! Standalone REST API server binary.
//!
//! ## Purpose
//! Runs the REST API server on its own.
//!
//! ## Intended use
//! This binary is useful for development and debugging when you only want the REST server (with
//! OpenAPI/Swagger UI). The workspace's main `vpr-run` binary runs both gRPC and REST concurrently.

use axum::{
    extract::{Path as AxumPath, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, put},
    Router,
};
use chrono::NaiveDate;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use api_shared::pb;
use std::path::Path;
use vpr_core::{
    config::rm_system_version_from_env_value,
    repositories::clinical::ClinicalService,
    repositories::coordination::CoordinationService,
    repositories::demographics::{DemographicsService, Uninitialised as DemographicsUninitialised},
    Author, AuthorRegistration, CoreConfig, EmailAddress, NonEmptyText, PatientService,
    ShardableUuid,
};

/// Application state for the REST API server
///
/// Contains shared state that needs to be accessible to all request handlers,
/// including the PatientService instance for data operations.
#[derive(Clone)]
struct AppState {
    cfg: Arc<CoreConfig>,
    demographics_service: Arc<DemographicsService<DemographicsUninitialised>>,
}

#[derive(OpenApi)]
#[openapi(
    paths(
        health,
        list_patients,
        create_patient,
        initialise_full_record,
        initialise_demographics,
        update_demographics,
        initialise_clinical,
        link_to_demographics,
        new_letter,
        new_letter_complete,
        read_letter,
        initialise_coordination,
    ),
    components(schemas(
        pb::HealthRes,
        pb::ListPatientsRes,
        pb::CreatePatientRes,
        pb::CreatePatientReq,
        pb::InitialiseFullRecordReq,
        pb::InitialiseFullRecordRes,
        pb::InitialiseDemographicsReq,
        pb::InitialiseDemographicsRes,
        pb::UpdateDemographicsReq,
        pb::UpdateDemographicsRes,
        pb::InitialiseClinicalReq,
        pb::InitialiseClinicalRes,
        pb::LinkToDemographicsReq,
        pb::LinkToDemographicsRes,
        pb::NewLetterReq,
        pb::NewLetterRes,
        pb::NewLetterCompleteReq,
        pb::NewLetterCompleteRes,
        pb::ReadLetterReq,
        pb::ReadLetterRes,
        pb::InitialiseCoordinationReq,
        pb::InitialiseCoordinationRes,
    ))
)]
struct ApiDoc;

/// Main entry point for the VPR REST API server
///
/// Starts the REST API server on the configured address (default: 0.0.0.0:3000).
/// Provides HTTP endpoints for patient operations with OpenAPI/Swagger documentation.
///
/// # Environment Variables
/// - `VPR_REST_ADDR`: Server address (default: "0.0.0.0:3000")
///
/// # Returns
/// * `Ok(())` - If server starts and runs successfully
///
/// # Errors
/// Returns an error if:
/// - the logging/tracing configuration cannot be initialised,
/// - the server address cannot be bound, or
/// - the HTTP server fails while running.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("api_rest=info".parse()?),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let addr = std::env::var("VPR_REST_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".into());

    tracing::info!("-- Starting VPR REST API on {}", addr);

    let patient_data_dir = std::env::var("PATIENT_DATA_DIR")
        .unwrap_or_else(|_| vpr_core::DEFAULT_PATIENT_DATA_DIR.into());
    let patient_data_path = Path::new(&patient_data_dir);
    if !patient_data_path.exists() {
        anyhow::bail!(
            "Patient data directory does not exist: {}",
            patient_data_path.display()
        );
    }

    let rm_system_version = rm_system_version_from_env_value(
        std::env::var("RM_SYSTEM_VERSION")
            .ok()
            .and_then(|s| vpr_core::NonEmptyText::new(s).ok()),
    )?;
    let vpr_namespace = std::env::var("VPR_NAMESPACE")
        .ok()
        .and_then(|s| vpr_core::NonEmptyText::new(s).ok())
        .unwrap_or_else(|| vpr_core::NonEmptyText::new("vpr.dev.1").unwrap());

    let cfg = Arc::new(CoreConfig::new(
        patient_data_path.to_path_buf(),
        rm_system_version,
        vpr_namespace,
    )?);

    let state = AppState {
        cfg: cfg.clone(),
        demographics_service: Arc::new(DemographicsService::new(cfg)),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/patients", get(list_patients))
        .route("/patients", post(create_patient))
        .route("/patients/full", post(initialise_full_record))
        .route("/demographics", post(initialise_demographics))
        .route("/demographics/:id", put(update_demographics))
        .route("/clinical", post(initialise_clinical))
        .route("/clinical/:id/link", post(link_to_demographics))
        .route("/clinical/:id/letters", post(new_letter))
        .route("/clinical/:id/letters/complete", post(new_letter_complete))
        .route("/clinical/:id/letters/:letter_id", get(read_letter))
        .route("/coordination", post(initialise_coordination))
        .merge(
            SwaggerUi::new("/swagger-ui/{_:.*}").url("/api-docs/openapi.json", ApiDoc::openapi()),
        )
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Health check response", body = pb::HealthRes)
    )
)]
/// Health check endpoint for the REST API
///
/// Returns the current health status of the VPR REST API service.
/// This endpoint is used for monitoring and load balancer health checks.
///
/// # Returns
/// * `Json<pb::HealthRes>` - Health status response containing service status
#[axum::debug_handler]
async fn health(State(_state): State<AppState>) -> Json<pb::HealthRes> {
    Json(pb::HealthRes {
        ok: true,
        message: "VPR REST API is alive".into(),
    })
}

#[utoipa::path(
    get,
    path = "/patients",
    responses(
        (status = 200, description = "List of patients", body = pb::ListPatientsRes),
        (status = 500, description = "Internal server error")
    )
)]
/// List all patients in the system
///
/// Retrieves a list of all patients by calling the underlying patient service.
/// This provides a REST interface to the patient listing functionality.
///
/// # Returns
/// * `Ok(Json<pb::ListPatientsRes>)` - List of patients with their IDs and names
/// * `Err((StatusCode, &str))` - Internal server error if listing fails
///
/// # Errors
/// Returns `500 Internal Server Error` if:
/// - patient listing fails.
#[axum::debug_handler]
async fn list_patients(
    State(state): State<AppState>,
) -> Result<Json<pb::ListPatientsRes>, (StatusCode, &'static str)> {
    let patients = state.demographics_service.list_patients();
    Ok(Json(pb::ListPatientsRes { patients }))
}

#[utoipa::path(
    post,
    path = "/patients",
    request_body = pb::CreatePatientReq,
    responses(
        (status = 201, description = "Patient created", body = pb::CreatePatientRes),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    )
)]
/// Create a new patient record
///
/// Creates a new clinical record by calling the underlying clinical service.
///
/// Note: this endpoint currently initialises a clinical record and returns the generated
/// identifier; it does not yet populate patient demographics.
///
/// # Arguments
/// * `req` - Request body containing author information used for the initial Git commit
///
/// # Returns
/// * `Ok(Json<pb::CreatePatientRes>)` - Initialised clinical with generated UUID
/// * `Err((StatusCode, &str))` - Internal server error if initialisation fails
///
/// # Errors
/// Returns `500 Internal Server Error` if:
/// - clinical initialisation fails.
#[axum::debug_handler]
async fn create_patient(
    State(state): State<AppState>,
    Json(req): Json<pb::CreatePatientReq>,
) -> Result<Json<pb::CreatePatientRes>, (StatusCode, &'static str)> {
    let registrations: Vec<AuthorRegistration> = req
        .author_registrations
        .into_iter()
        .map(|r| AuthorRegistration::new(r.authority, r.number).expect("valid registration"))
        .collect();

    let name = NonEmptyText::new(&req.author_name)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid author name"))?;
    let role = NonEmptyText::new(&req.author_role)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid author role"))?;
    let email = EmailAddress::parse(&req.author_email)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid author email"))?;

    let author = Author {
        name,
        role,
        email,
        registrations,
        signature: if req.author_signature.is_empty() {
            None
        } else {
            Some(req.author_signature.into_bytes())
        },
        certificate: None,
    };
    let care_location = NonEmptyText::new(&req.care_location)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid care_location"))?;

    let clinical_service = ClinicalService::new(state.cfg.clone());
    match clinical_service.initialise(author, care_location) {
        Ok(service) => {
            let resp = pb::CreatePatientRes {
                filename: "".to_string(),
                patient: Some(pb::Patient {
                    id: service.clinical_id().simple().to_string(),
                    first_name: "".to_string(),
                    last_name: "".to_string(),
                    created_at: "".to_string(),
                    national_id: "".to_string(),
                }),
            };
            Ok(Json(resp))
        }
        Err(e) => {
            tracing::error!("Initialise clinical error: {:?}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))
        }
    }
}
#[utoipa::path(
    post,
    path = "/patients/full",
    request_body = pb::InitialiseFullRecordReq,
    responses(
        (status = 201, description = "Full patient record created", body = pb::InitialiseFullRecordRes),
        (status = 500, description = "Internal server error")
    )
)]
#[axum::debug_handler]
async fn initialise_full_record(
    State(state): State<AppState>,
    Json(req): Json<pb::InitialiseFullRecordReq>,
) -> Result<Json<pb::InitialiseFullRecordRes>, (StatusCode, &'static str)> {
    let author = build_author(
        req.author_name,
        req.author_email,
        req.author_role,
        req.author_registrations,
        req.author_signature,
    )?;
    let care_location = NonEmptyText::new(&req.care_location)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid care_location"))?;

    let patient_service = PatientService::new(state.cfg.clone());

    let given_names: Vec<NonEmptyText> = req
        .given_names
        .into_iter()
        .map(|name| {
            NonEmptyText::new(name).map_err(|_| (StatusCode::BAD_REQUEST, "Invalid given name"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let last_name = NonEmptyText::new(req.last_name)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid last name"))?;

    let birth_date = NaiveDate::parse_from_str(&req.birth_date, "%Y-%m-%d")
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid birth date"))?;

    let namespace = if req.namespace.is_empty() {
        None
    } else {
        Some(
            NonEmptyText::new(req.namespace)
                .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid namespace"))?,
        )
    };

    match patient_service.initialise_full_record(
        author,
        care_location,
        given_names,
        last_name,
        birth_date,
        namespace,
    ) {
        Ok(record) => Ok(Json(pb::InitialiseFullRecordRes {
            demographics_uuid: record.demographics_uuid.to_string(),
            clinical_uuid: record.clinical_uuid.to_string(),
            coordination_uuid: record.coordination_uuid.to_string(),
        })),
        Err(e) => {
            tracing::error!("Initialise full record error: {:?}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))
        }
    }
}

#[utoipa::path(
    post,
    path = "/demographics",
    request_body = pb::InitialiseDemographicsReq,
    responses(
        (status = 201, description = "Demographics created", body = pb::InitialiseDemographicsRes),
        (status = 500, description = "Internal server error")
    )
)]
#[axum::debug_handler]
async fn initialise_demographics(
    State(state): State<AppState>,
    Json(req): Json<pb::InitialiseDemographicsReq>,
) -> Result<Json<pb::InitialiseDemographicsRes>, (StatusCode, &'static str)> {
    let author = build_author(
        req.author_name,
        req.author_email,
        req.author_role,
        req.author_registrations,
        req.author_signature,
    )?;
    let care_location = NonEmptyText::new(&req.care_location)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid care_location"))?;

    let demographics_service = DemographicsService::new(state.cfg.clone());
    match demographics_service.initialise(author, care_location) {
        Ok(service) => Ok(Json(pb::InitialiseDemographicsRes {
            demographics_uuid: service.demographics_id().to_string(),
        })),
        Err(e) => {
            tracing::error!("Initialise demographics error: {:?}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))
        }
    }
}

#[utoipa::path(
    put,
    path = "/demographics/{id}",
    request_body = pb::UpdateDemographicsReq,
    responses(
        (status = 200, description = "Demographics updated", body = pb::UpdateDemographicsRes),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    )
)]
#[axum::debug_handler]
async fn update_demographics(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(mut req): Json<pb::UpdateDemographicsReq>,
) -> Result<Json<pb::UpdateDemographicsRes>, (StatusCode, &'static str)> {
    req.demographics_uuid = id;

    let demographics_service =
        match DemographicsService::with_id(state.cfg.clone(), &req.demographics_uuid) {
            Ok(svc) => svc,
            Err(e) => {
                tracing::error!("Invalid demographics UUID: {:?}", e);
                return Err((StatusCode::BAD_REQUEST, "Invalid demographics UUID"));
            }
        };

    let given_names: Vec<NonEmptyText> = req
        .given_names
        .into_iter()
        .map(|name| {
            NonEmptyText::new(name).map_err(|_| (StatusCode::BAD_REQUEST, "Invalid given name"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let last_name = NonEmptyText::new(req.last_name)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid last name"))?;

    let birth_date = NaiveDate::parse_from_str(&req.birth_date, "%Y-%m-%d")
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid birth date"))?;

    let birth_date_str = birth_date.format("%Y-%m-%d").to_string();

    match demographics_service.update(given_names, last_name.as_str(), &birth_date_str) {
        Ok(()) => Ok(Json(pb::UpdateDemographicsRes { success: true })),
        Err(e) => {
            tracing::error!("Update demographics error: {:?}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))
        }
    }
}

#[utoipa::path(
    post,
    path = "/clinical",
    request_body = pb::InitialiseClinicalReq,
    responses(
        (status = 201, description = "Clinical created", body = pb::InitialiseClinicalRes),
        (status = 500, description = "Internal server error")
    )
)]
#[axum::debug_handler]
async fn initialise_clinical(
    State(state): State<AppState>,
    Json(req): Json<pb::InitialiseClinicalReq>,
) -> Result<Json<pb::InitialiseClinicalRes>, (StatusCode, &'static str)> {
    let author = build_author(
        req.author_name,
        req.author_email,
        req.author_role,
        req.author_registrations,
        req.author_signature,
    )?;
    let care_location = NonEmptyText::new(&req.care_location)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid care_location"))?;

    let clinical_service = ClinicalService::new(state.cfg.clone());
    match clinical_service.initialise(author, care_location) {
        Ok(service) => Ok(Json(pb::InitialiseClinicalRes {
            clinical_uuid: service.clinical_id().simple().to_string(),
        })),
        Err(e) => {
            tracing::error!("Initialise clinical error: {:?}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))
        }
    }
}

#[utoipa::path(
    post,
    path = "/clinical/{id}/link",
    request_body = pb::LinkToDemographicsReq,
    responses(
        (status = 200, description = "Linked to demographics", body = pb::LinkToDemographicsRes),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    )
)]
#[axum::debug_handler]
async fn link_to_demographics(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(mut req): Json<pb::LinkToDemographicsReq>,
) -> Result<Json<pb::LinkToDemographicsRes>, (StatusCode, &'static str)> {
    req.clinical_uuid = id;

    let author = build_author(
        req.author_name,
        req.author_email,
        req.author_role,
        req.author_registrations,
        req.author_signature,
    )?;

    let clinical_uuid = match ShardableUuid::parse(&req.clinical_uuid) {
        Ok(uuid) => uuid.uuid(),
        Err(e) => {
            tracing::error!("Invalid clinical UUID: {:?}", e);
            return Err((StatusCode::BAD_REQUEST, "Invalid clinical UUID"));
        }
    };
    let care_location = NonEmptyText::new(&req.care_location)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid care_location"))?;

    let clinical_service = ClinicalService::with_id(state.cfg.clone(), clinical_uuid);

    let namespace = if req.namespace.is_empty() {
        None
    } else {
        Some(
            NonEmptyText::new(req.namespace)
                .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid namespace"))?,
        )
    };

    match clinical_service.link_to_demographics(
        &author,
        care_location,
        &req.demographics_uuid,
        namespace,
    ) {
        Ok(()) => Ok(Json(pb::LinkToDemographicsRes { success: true })),
        Err(e) => {
            tracing::error!("Link to demographics error: {:?}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))
        }
    }
}

#[utoipa::path(
    post,
    path = "/clinical/{id}/letters",
    request_body = pb::NewLetterReq,
    responses(
        (status = 201, description = "Letter created", body = pb::NewLetterRes),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    )
)]
#[axum::debug_handler]
async fn new_letter(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(mut req): Json<pb::NewLetterReq>,
) -> Result<Json<pb::NewLetterRes>, (StatusCode, &'static str)> {
    req.clinical_uuid = id;

    let author = build_author(
        req.author_name,
        req.author_email,
        req.author_role,
        req.author_registrations,
        req.author_signature,
    )?;

    let clinical_uuid = match ShardableUuid::parse(&req.clinical_uuid) {
        Ok(uuid) => uuid.uuid(),
        Err(e) => {
            tracing::error!("Invalid clinical UUID: {:?}", e);
            return Err((StatusCode::BAD_REQUEST, "Invalid clinical UUID"));
        }
    };
    let care_location = NonEmptyText::new(&req.care_location)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid care_location"))?;

    let clinical_service = ClinicalService::with_id(state.cfg.clone(), clinical_uuid);

    let content =
        NonEmptyText::new(req.content).map_err(|_| (StatusCode::BAD_REQUEST, "Invalid content"))?;

    match clinical_service.new_letter(&author, care_location, content, None) {
        Ok(timestamp_id) => Ok(Json(pb::NewLetterRes {
            timestamp_id: timestamp_id.to_string(),
        })),
        Err(e) => {
            tracing::error!("New letter error: {:?}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))
        }
    }
}

#[utoipa::path(
    post,
    path = "/clinical/{id}/letters/complete",
    request_body = pb::NewLetterCompleteReq,
    responses(
        (status = 201, description = "Complete letter created", body = pb::NewLetterCompleteRes),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    )
)]
#[axum::debug_handler]
async fn new_letter_complete(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(mut req): Json<pb::NewLetterCompleteReq>,
) -> Result<Json<pb::NewLetterCompleteRes>, (StatusCode, &'static str)> {
    req.clinical_uuid = id;

    let author = build_author(
        req.author_name,
        req.author_email,
        req.author_role,
        req.author_registrations,
        req.author_signature,
    )?;

    let clinical_uuid = match ShardableUuid::parse(&req.clinical_uuid) {
        Ok(uuid) => uuid.uuid(),
        Err(e) => {
            tracing::error!("Invalid clinical UUID: {:?}", e);
            return Err((StatusCode::BAD_REQUEST, "Invalid clinical UUID"));
        }
    };

    // Write attachment files to temporary directory
    let temp_dir = std::env::temp_dir().join(format!("vpr_attachments_{}", uuid::Uuid::new_v4()));
    if let Err(e) = std::fs::create_dir_all(&temp_dir) {
        tracing::error!("Failed to create temp dir: {:?}", e);
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error"));
    }

    let mut attachment_paths = Vec::new();
    for (i, (content, name)) in req
        .attachment_files
        .iter()
        .zip(&req.attachment_names)
        .enumerate()
    {
        let file_path = temp_dir.join(format!("{}_{}", i, name));
        if let Err(e) = std::fs::write(&file_path, content) {
            tracing::error!("Failed to write attachment: {:?}", e);
            let _ = std::fs::remove_dir_all(&temp_dir);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error"));
        }
        attachment_paths.push(file_path);
    }
    let care_location = NonEmptyText::new(&req.care_location)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid care_location"))?;

    let content =
        NonEmptyText::new(req.content).map_err(|_| (StatusCode::BAD_REQUEST, "Invalid content"))?;

    let clinical_service = ClinicalService::with_id(state.cfg.clone(), clinical_uuid);
    let result = clinical_service.create_letter(
        &author,
        care_location,
        Some(content),
        &attachment_paths,
        None,
    );

    // Clean up temp files
    let _ = std::fs::remove_dir_all(&temp_dir);

    match result {
        Ok(timestamp_id) => Ok(Json(pb::NewLetterCompleteRes {
            timestamp_id: timestamp_id.to_string(),
        })),
        Err(e) => {
            tracing::error!("New complete letter error: {:?}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))
        }
    }
}

#[utoipa::path(
    get,
    path = "/clinical/{id}/letters/{letter_id}",
    responses(
        (status = 200, description = "Letter retrieved", body = pb::ReadLetterRes),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    )
)]
#[axum::debug_handler]
async fn read_letter(
    State(state): State<AppState>,
    AxumPath((clinical_uuid, letter_id)): AxumPath<(String, String)>,
) -> Result<Json<pb::ReadLetterRes>, (StatusCode, &'static str)> {
    let clinical_uuid_parsed = match ShardableUuid::parse(&clinical_uuid) {
        Ok(uuid) => uuid.uuid(),
        Err(e) => {
            tracing::error!("Invalid clinical UUID: {:?}", e);
            return Err((StatusCode::BAD_REQUEST, "Invalid clinical UUID"));
        }
    };

    let clinical_service = ClinicalService::with_id(state.cfg.clone(), clinical_uuid_parsed);
    match clinical_service.read_letter(&letter_id) {
        Ok(result) => Ok(Json(pb::ReadLetterRes {
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
        Err(e) => {
            tracing::error!("Read letter error: {:?}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))
        }
    }
}

#[utoipa::path(
    post,
    path = "/coordination",
    request_body = pb::InitialiseCoordinationReq,
    responses(
        (status = 201, description = "Coordination created", body = pb::InitialiseCoordinationRes),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    )
)]
#[axum::debug_handler]
async fn initialise_coordination(
    State(state): State<AppState>,
    Json(req): Json<pb::InitialiseCoordinationReq>,
) -> Result<Json<pb::InitialiseCoordinationRes>, (StatusCode, &'static str)> {
    let author = build_author(
        req.author_name,
        req.author_email,
        req.author_role,
        req.author_registrations,
        req.author_signature,
    )?;

    let clinical_uuid = match uuid::Uuid::parse_str(&req.clinical_uuid) {
        Ok(uuid) => uuid,
        Err(e) => {
            tracing::error!("Invalid clinical UUID: {:?}", e);
            return Err((StatusCode::BAD_REQUEST, "Invalid clinical UUID"));
        }
    };

    let coordination_service = CoordinationService::new(state.cfg.clone());

    let care_location = NonEmptyText::new(req.care_location)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid care_location"))?;

    match coordination_service.initialise(author, care_location, clinical_uuid) {
        Ok(service) => Ok(Json(pb::InitialiseCoordinationRes {
            coordination_uuid: service.coordination_id().to_string(),
        })),
        Err(e) => {
            tracing::error!("Initialise coordination error: {:?}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))
        }
    }
}

// Helper function
fn build_author(
    name: String,
    email: String,
    role: String,
    registrations: Vec<pb::AuthorRegistration>,
    signature: String,
) -> Result<Author, (StatusCode, &'static str)> {
    let name =
        NonEmptyText::new(&name).map_err(|_| (StatusCode::BAD_REQUEST, "Invalid author name"))?;
    let role =
        NonEmptyText::new(&role).map_err(|_| (StatusCode::BAD_REQUEST, "Invalid author role"))?;
    let email = EmailAddress::parse(&email)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid author email"))?;

    Ok(Author {
        name,
        email,
        role,
        registrations: registrations
            .into_iter()
            .map(|r| AuthorRegistration::new(r.authority, r.number).expect("valid registration"))
            .collect(),
        signature: if signature.is_empty() {
            None
        } else {
            Some(signature.into_bytes())
        },
        certificate: None,
    })
}
