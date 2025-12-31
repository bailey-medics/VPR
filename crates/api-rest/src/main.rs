//! Standalone REST API server binary.
//!
//! ## Purpose
//! Runs the REST API server on its own.
//!
//! ## Intended use
//! This binary is useful for development and debugging when you only want the REST server (with
//! OpenAPI/Swagger UI). The workspace's main `vpr-run` binary runs both gRPC and REST concurrently.

use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use api_shared::pb;
use vpr_core::{
    clinical::ClinicalService, demographics::DemographicsService, Author, AuthorRegistration,
};

/// Application state for the REST API server
///
/// Contains shared state that needs to be accessible to all request handlers,
/// including the PatientService instance for data operations.
#[derive(Clone)]
struct AppState {
    demographics_service: Arc<DemographicsService>,
}

#[derive(OpenApi)]
#[openapi(
    paths(health, list_patients, create_patient),
    components(schemas(
        pb::HealthRes,
        pb::ListPatientsRes,
        pb::CreatePatientRes,
        pb::CreatePatientReq
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
/// Returns an error if the server cannot be configured, bound, or started.
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

    let state = AppState {
        demographics_service: Arc::new(DemographicsService),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/patients", get(list_patients))
        .route("/patients", post(create_patient))
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
/// Returns `500 Internal Server Error` if patient listing fails.
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
/// Returns `500 Internal Server Error` if initialisation fails.
#[axum::debug_handler]
async fn create_patient(
    State(_state): State<AppState>,
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
    let clinical_service = ClinicalService;
    match clinical_service.initialise(author, req.care_location) {
        Ok(uuid) => {
            let resp = pb::CreatePatientRes {
                filename: "".to_string(),
                patient: Some(pb::Patient {
                    id: uuid,
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
