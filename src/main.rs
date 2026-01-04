//! Combined gRPC + REST server binary (`vpr-run`).
//!
//! ## Purpose
//! Starts both the gRPC and REST API servers concurrently.
//!
//! ## Intended use
//! This is the primary runtime entry point for VPR. It performs basic startup validation (for
//! example, ensuring the patient data directory and EHR template exist) and then serves both APIs.

use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
};
use std::net::SocketAddr;
use tonic::transport::Server;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use api_grpc::{VprService, auth_interceptor};
use api_shared::HealthService;
use api_shared::pb;
use api_shared::pb::vpr_server::VprServer;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use vpr_core::{
    Author, AuthorRegistration, CoreConfig,
    clinical::ClinicalService,
    config::{
        resolve_ehr_template_dir, rm_system_version_from_env_value,
        validate_ehr_template_dir_safe_to_copy,
    },
    demographics::DemographicsService,
};

type HealthRes = pb::HealthRes;
type ListPatientsRes = pb::ListPatientsRes;
type CreatePatientRes = pb::CreatePatientRes;
type CreatePatientReq = pb::CreatePatientReq;
type Patient = pb::Patient;

/// Application state shared across REST API handlers
///
/// Contains the services needed by the REST API endpoints.
/// Currently holds a PatientService instance for data operations.
#[derive(Clone)]
struct AppState {
    clinical_service: Arc<ClinicalService>,
    demographics_service: Arc<DemographicsService>,
}

#[derive(OpenApi)]
#[openapi(
    paths(health, list_patients, create_patient),
    components(schemas(
        HealthRes,
        ListPatientsRes,
        CreatePatientRes,
        CreatePatientReq,
        Patient
    ))
)]
struct ApiDoc;

/// Main entry point for the VPR application
///
/// Starts both gRPC and REST servers concurrently:
/// - gRPC server on port 50051 (configurable via VPR_ADDR)
/// - REST server on port 3000 (configurable via VPR_REST_ADDR)
///
/// The gRPC server requires authentication via x-api-key header.
/// The REST server provides open access to patient operations.
///
/// # Environment Variables
/// - `VPR_ADDR`: gRPC server address (default: "0.0.0.0:50051")
/// - `VPR_REST_ADDR`: REST server address (default: "0.0.0.0:3000")
/// - `PATIENT_DATA_DIR`: Directory for patient data storage (default: "/patient_data")
/// - `API_KEY`: API key for gRPC authentication
///
/// # Returns
/// * `Ok(())` - If servers start and run successfully
///
/// # Errors
/// Returns an error if the servers cannot be configured, bound, or started.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // Resolve core configuration once at startup.
    let patient_data_dir = std::env::var("PATIENT_DATA_DIR")
        .unwrap_or_else(|_| vpr_core::DEFAULT_PATIENT_DATA_DIR.into());
    let patient_data_path = Path::new(&patient_data_dir);
    if !patient_data_path.exists() {
        eprintln!(
            "Error: Patient data directory does not exist: {}",
            patient_data_path.display()
        );
        std::process::exit(1);
    }

    // Test write access by attempting to create a temp file.
    let test_file = patient_data_path.join(".vpr_test_write");
    match std::fs::write(&test_file, b"test") {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
        }
        Err(e) => {
            eprintln!(
                "Error: Patient data directory is not writable: {} ({})",
                patient_data_path.display(),
                e
            );
            std::process::exit(1);
        }
    }

    let template_override = std::env::var("VPR_EHR_TEMPLATE_DIR")
        .ok()
        .map(PathBuf::from);
    let ehr_template_dir = resolve_ehr_template_dir(template_override).unwrap_or_else(|_| {
        eprintln!("Error: Unable to resolve EHR template directory");
        std::process::exit(1);
    });
    if let Err(e) = validate_ehr_template_dir_safe_to_copy(&ehr_template_dir) {
        eprintln!(
            "Error: EHR template directory is not safe to copy: {} ({})",
            ehr_template_dir.display(),
            e
        );
        std::process::exit(1);
    }

    let rm_system_version = rm_system_version_from_env_value(
        std::env::var("RM_SYSTEM_VERSION").ok(),
    )
    .unwrap_or_else(|e| {
        eprintln!("Error: Invalid RM_SYSTEM_VERSION ({})", e);
        std::process::exit(1);
    });
    let vpr_namespace = std::env::var("VPR_NAMESPACE").unwrap_or_else(|_| "vpr.dev.1".into());

    let cfg = Arc::new(
        CoreConfig::new(
            patient_data_path.to_path_buf(),
            ehr_template_dir,
            rm_system_version,
            vpr_namespace,
        )
        .unwrap_or_else(|e| {
            eprintln!("Error: Invalid core configuration ({})", e);
            std::process::exit(1);
        }),
    );

    // Ensure clinical subdirectory exists
    let clinical_dir = cfg.patient_data_dir().join("clinical");
    if let Err(e) = std::fs::create_dir_all(&clinical_dir) {
        eprintln!(
            "Error: Failed to create clinical directory: {} ({})",
            clinical_dir.display(),
            e
        );
        std::process::exit(1);
    }

    // Ensure demographics subdirectory exists
    let demographics_dir = cfg.patient_data_dir().join("demographics");
    if let Err(e) = std::fs::create_dir_all(&demographics_dir) {
        eprintln!(
            "Error: Failed to create demographics directory: {} ({})",
            demographics_dir.display(),
            e
        );
        std::process::exit(1);
    }

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env().add_directive("vpr=info".parse()?))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let grpc_addr: SocketAddr = std::env::var("VPR_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50051".into())
        .parse()?;
    let rest_addr = std::env::var("VPR_REST_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".into());

    tracing::info!("++ Starting VPR gRPC on {}", grpc_addr);
    tracing::info!("++ Starting VPR REST on {}", rest_addr);

    // Start REST server
    let rest_state = AppState {
        clinical_service: Arc::new(ClinicalService::new(cfg.clone())),
        demographics_service: Arc::new(DemographicsService::new(cfg.clone())),
    };

    let rest_app = Router::new()
        .route("/health", get(health))
        .route("/patients", get(list_patients))
        .route("/patients", post(create_patient))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(CorsLayer::permissive())
        .with_state(rest_state);

    let rest_server = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(&rest_addr).await.unwrap();
        axum::serve(listener, rest_app).await.unwrap();
    });

    // Start gRPC server
    let grpc_server = Server::builder()
        .add_service(VprServer::with_interceptor(
            VprService::new(cfg),
            auth_interceptor,
        ))
        .serve(grpc_addr);

    // Run both
    let (rest_result, grpc_result) = tokio::join!(rest_server, grpc_server);
    rest_result.map_err(anyhow::Error::from)?;
    grpc_result.map_err(anyhow::Error::from)?;

    Ok(())
}

#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Health check response", body = HealthRes)
    )
)]
/// Health check endpoint for the REST API
///
/// Returns the current health status of the VPR service.
/// This endpoint is used for monitoring and load balancer health checks.
///
/// # Returns
/// * `Json<HealthRes>` - Health status response containing service status
async fn health(State(_state): State<AppState>) -> Json<HealthRes> {
    Json(HealthService::check_health())
}

#[utoipa::path(
    get,
    path = "/patients",
    responses(
        (status = 200, description = "List of patients", body = ListPatientsRes),
        (status = 500, description = "Internal server error")
    )
)]
/// List all patients in the system
///
/// Retrieves a list of all patients stored in the patient data directory.
/// Patients are stored in a sharded directory structure for efficient access.
///
/// # Returns
/// * `Ok(Json<ListPatientsRes>)` - List of patients with their IDs and names
/// * `Err((StatusCode, &str))` - Internal server error if listing fails
async fn list_patients(
    State(state): State<AppState>,
) -> Result<Json<ListPatientsRes>, (StatusCode, &'static str)> {
    let patients = state.demographics_service.list_patients();
    Ok(Json(ListPatientsRes { patients }))
}

#[utoipa::path(
    post,
    path = "/patients",
    request_body = CreatePatientReq,
    responses(
        (status = 201, description = "Patient created", body = CreatePatientRes),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    )
)]
/// Create a new patient record
///
/// Creates a new clinical record and returns the generated identifier.
///
/// Note: this endpoint currently initialises a clinical record only; it does not yet populate
/// patient demographics.
///
/// # Arguments
/// * `req` - Request body containing author information used for the initial Git commit
///
/// # Returns
/// * `Ok(Json<CreatePatientRes>)` - Initialised clinical with generated UUID
/// * `Err((StatusCode, &str))` - Internal server error
///
/// # Errors
/// Returns `500 Internal Server Error` if initialisation fails.
async fn create_patient(
    State(state): State<AppState>,
    Json(req): Json<CreatePatientReq>,
) -> Result<Json<CreatePatientRes>, (StatusCode, &'static str)> {
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
    match state.clinical_service.initialise(author, req.care_location) {
        Ok(uuid) => {
            let resp = CreatePatientRes {
                filename: "".to_string(),
                patient: Some(Patient {
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
