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
use vpr_core::PatientService;

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
    patient_service: PatientService,
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
/// * `Err(anyhow::Error)` - If server startup or runtime fails
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

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

    let patient_service = PatientService::new();

    // Start REST server
    let rest_app = Router::new()
        .route("/health", get(health))
        .route("/patients", get(list_patients))
        .route("/patients", post(create_patient))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(CorsLayer::permissive())
        .with_state(AppState {
            patient_service: patient_service.clone(),
        });

    let rest_server = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(&rest_addr).await.unwrap();
        axum::serve(listener, rest_app).await.unwrap();
    });

    // Start gRPC server
    let grpc_server = Server::builder()
        .add_service(VprServer::with_interceptor(
            VprService::default(),
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
    let patients = state.patient_service.list_patients();
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
/// Creates a new patient with the provided first and last name.
/// The patient data is stored as JSON in a sharded directory structure
/// under the configured patient data directory.
///
/// # Parameters
/// * `req` - Patient creation request containing first_name, last_name, author_name, and author_email
///
/// # Returns
/// * `Ok(Json<CreatePatientRes>)` - Created patient with generated UUID
/// * `Err((StatusCode, &str))` - Bad request or internal server error
async fn create_patient(
    State(state): State<AppState>,
    Json(req): Json<CreatePatientReq>,
) -> Result<Json<CreatePatientRes>, (StatusCode, &'static str)> {
    match state.patient_service.create_patient(
        req.first_name,
        req.last_name,
        req.author_name,
        req.author_email,
    ) {
        Ok(resp) => Ok(Json(resp)),
        Err(e) => {
            tracing::error!("Create patient error: {:?}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))
        }
    }
}
