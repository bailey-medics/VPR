use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
};
use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Server;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use api_grpc::{
    VprService,
    pb::vpr_server::{Vpr, VprServer},
};

type HealthRes = api_grpc::pb::HealthRes;
type ListPatientsRes = api_grpc::pb::ListPatientsRes;
type CreatePatientRes = api_grpc::pb::CreatePatientRes;
type CreatePatientReq = api_grpc::pb::CreatePatientReq;

#[derive(Clone)]
struct AppState {
    service: Arc<VprService>,
}

#[derive(OpenApi)]
#[openapi(
    paths(health, list_patients, create_patient),
    components(schemas(HealthRes, ListPatientsRes, CreatePatientRes, CreatePatientReq))
)]
struct ApiDoc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    let svc = Arc::new(VprService);

    // Start REST server
    let rest_svc = Arc::clone(&svc);
    let rest_app = Router::new()
        .route("/health", get(health))
        .route("/patients", get(list_patients))
        .route("/patients", post(create_patient))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(CorsLayer::permissive())
        .with_state(AppState { service: rest_svc });

    let rest_server = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(&rest_addr).await.unwrap();
        axum::serve(listener, rest_app).await.unwrap();
    });

    // Start gRPC server
    let grpc_server = Server::builder()
        .add_service(VprServer::new((*svc).clone()))
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
async fn health(State(_state): State<AppState>) -> Json<HealthRes> {
    Json(api_grpc::pb::HealthRes {
        ok: true,
        message: "VPR is alive".into(),
    })
}

#[utoipa::path(
    get,
    path = "/patients",
    responses(
        (status = 200, description = "List of patients", body = ListPatientsRes),
        (status = 500, description = "Internal server error")
    )
)]
async fn list_patients(
    State(state): State<AppState>,
) -> Result<Json<ListPatientsRes>, (StatusCode, &'static str)> {
    let service = &state.service;
    match service.list_patients(tonic::Request::new(())).await {
        Ok(resp) => Ok(Json(resp.into_inner())),
        Err(_) => Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error")),
    }
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
async fn create_patient(
    State(state): State<AppState>,
    Json(req): Json<CreatePatientReq>,
) -> Result<Json<CreatePatientRes>, (StatusCode, &'static str)> {
    let service = &state.service;
    match service.create_patient(tonic::Request::new(req)).await {
        Ok(resp) => Ok(Json(resp.into_inner())),
        Err(e) => {
            tracing::error!("Create patient error: {:?}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))
        }
    }
}
