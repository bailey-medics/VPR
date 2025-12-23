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

use api_proto::pb;
use api_proto::pb::vpr_server::Vpr;
use core::VprService;

#[derive(Clone)]
struct AppState {
    service: Arc<VprService>,
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
        service: Arc::new(VprService),
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
#[axum::debug_handler]
async fn list_patients(
    State(state): State<AppState>,
) -> Result<Json<pb::ListPatientsRes>, (StatusCode, &'static str)> {
    let service = &state.service;
    match service.list_patients(tonic::Request::new(())).await {
        Ok(resp) => Ok(Json(resp.into_inner())),
        Err(_) => Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error")),
    }
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
#[axum::debug_handler]
async fn create_patient(
    State(state): State<AppState>,
    Json(req): Json<pb::CreatePatientReq>,
) -> Result<Json<pb::CreatePatientRes>, (StatusCode, &'static str)> {
    let service = &state.service;
    match service.create_patient(tonic::Request::new(req)).await {
        Ok(resp) => Ok(Json(resp.into_inner())),
        Err(e) => {
            tracing::error!("Create patient error: {:?}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, "Internal error"))
        }
    }
}
