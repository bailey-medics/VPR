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
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use api_shared::pb;
use std::path::Path;
use std::path::PathBuf;
use vpr_core::{
    config::rm_system_version_from_env_value,
    repositories::clinical::ClinicalService,
    repositories::coordination::CoordinationService,
    repositories::demographics::{DemographicsService, Uninitialised as DemographicsUninitialised},
    repositories::shared::{resolve_clinical_template_dir, validate_template, TemplateDirKind},
    Author, AuthorRegistration, CoreConfig, PatientService, ShardableUuid,
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

    let template_override = std::env::var("VPR_CLINICAL_TEMPLATE_DIR")
        .ok()
        .map(PathBuf::from);
    let clinical_template_dir = resolve_clinical_template_dir(template_override)?;
    validate_template(&TemplateDirKind::Clinical, &clinical_template_dir)?;

    let rm_system_version =
        rm_system_version_from_env_value(std::env::var("RM_SYSTEM_VERSION").ok())?;
    let vpr_namespace = std::env::var("VPR_NAMESPACE").unwrap_or_else(|_| "vpr.dev.1".into());

    let cfg = Arc::new(CoreConfig::new(
        patient_data_path.to_path_buf(),
        clinical_template_dir,
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
    let clinical_service = ClinicalService::new(state.cfg.clone());
    match clinical_service.initialise(author, req.care_location) {
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
    );

    let patient_service = PatientService::new(state.cfg.clone());
    match patient_service.initialise_full_record(
        author,
        req.care_location,
        req.given_names,
        req.last_name,
        req.birth_date,
        if req.namespace.is_empty() {
            None
        } else {
            Some(req.namespace)
        },
    ) {
        Ok(record) => Ok(Json(pb::InitialiseFullRecordRes {
            demographics_uuid: record.demographics_uuid,
            clinical_uuid: record.clinical_uuid,
            coordination_uuid: record.coordination_uuid,
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
    );

    let demographics_service = DemographicsService::new(state.cfg.clone());
    match demographics_service.initialise(author, req.care_location) {
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

    match demographics_service.update(req.given_names, &req.last_name, &req.birth_date) {
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
    );

    let clinical_service = ClinicalService::new(state.cfg.clone());
    match clinical_service.initialise(author, req.care_location) {
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
    );

    let clinical_uuid = match ShardableUuid::parse(&req.clinical_uuid) {
        Ok(uuid) => uuid.uuid(),
        Err(e) => {
            tracing::error!("Invalid clinical UUID: {:?}", e);
            return Err((StatusCode::BAD_REQUEST, "Invalid clinical UUID"));
        }
    };

    let clinical_service = ClinicalService::with_id(state.cfg.clone(), clinical_uuid);
    match clinical_service.link_to_demographics(
        &author,
        req.care_location,
        &req.demographics_uuid,
        if req.namespace.is_empty() {
            None
        } else {
            Some(req.namespace)
        },
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
    );

    let clinical_uuid = match ShardableUuid::parse(&req.clinical_uuid) {
        Ok(uuid) => uuid.uuid(),
        Err(e) => {
            tracing::error!("Invalid clinical UUID: {:?}", e);
            return Err((StatusCode::BAD_REQUEST, "Invalid clinical UUID"));
        }
    };

    let clinical_service = ClinicalService::with_id(state.cfg.clone(), clinical_uuid);
    match clinical_service.new_letter(&author, req.care_location, req.content, None) {
        Ok(timestamp_id) => Ok(Json(pb::NewLetterRes { timestamp_id })),
        Err(e) => {
            tracing::error!("New letter error: {:?}", e);
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
            body_content: result.body_content,
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
    );

    let clinical_uuid = match uuid::Uuid::parse_str(&req.clinical_uuid) {
        Ok(uuid) => uuid,
        Err(e) => {
            tracing::error!("Invalid clinical UUID: {:?}", e);
            return Err((StatusCode::BAD_REQUEST, "Invalid clinical UUID"));
        }
    };

    let coordination_service = CoordinationService::new(state.cfg.clone());
    match coordination_service.initialise(author, req.care_location, clinical_uuid) {
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
