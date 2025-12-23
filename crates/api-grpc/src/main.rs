use std::net::SocketAddr;
use tonic::transport::Server;
use tonic::{Request, Status};
use tonic_reflection::server::Builder;
use tower::ServiceBuilder;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use api_grpc::{pb::vpr_server::VprServer, VprService};
use api_shared::{auth, FILE_DESCRIPTOR_SET};

/// Authentication interceptor for gRPC requests
///
/// Validates the x-api-key header in incoming gRPC requests.
/// The API key is compared against the configured API_KEY environment variable.
///
/// # Parameters
/// * `req` - The incoming gRPC request
///
/// # Returns
/// * `Ok(Request<()>)` - Request with authentication validated
/// * `Err(Status)` - Authentication failed (missing or invalid API key)
fn auth_interceptor(req: Request<()>) -> Result<Request<()>, Status> {
    let api_key = req
        .metadata()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;

    auth::validate_api_key(api_key)?;

    Ok(req)
}

/// Main entry point for the VPR gRPC server
///
/// Starts the gRPC server on the configured address (default: 0.0.0.0:50051).
/// Includes authentication interceptor and optional gRPC reflection for debugging.
///
/// # Environment Variables
/// - `VPR_ADDR`: Server address (default: "0.0.0.0:50051")
/// - `VPR_ENABLE_REFLECTION`: Enable gRPC reflection (default: "false")
/// - `API_KEY`: API key for authentication
///
/// # Returns
/// * `Ok(())` - If server starts and runs successfully
/// * `Err(anyhow::Error)` - If server startup or runtime fails
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env().add_directive("vpr=info".parse()?))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let addr: SocketAddr = std::env::var("VPR_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50051".into())
        .parse()?;

    tracing::info!("-- Starting VPR gRPC on {}", addr);

    let svc = VprService;
    let layer = ServiceBuilder::new().layer_fn(auth_interceptor);
    let mut server_builder = Server::builder()
        .layer(layer)
        .add_service(VprServer::new(svc));

    if std::env::var("VPR_ENABLE_REFLECTION").unwrap_or_else(|_| "false".to_string()) == "true" {
        let reflection_service = Builder::configure()
            .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
            .build_v1()
            .unwrap();
        server_builder = server_builder.add_service(reflection_service);
        tracing::info!("gRPC server reflection enabled");
    } else {
        tracing::info!("gRPC server reflection disabled");
    }

    server_builder.serve(addr).await?;

    Ok(())
}
