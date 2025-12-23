use std::net::SocketAddr;
use tonic::transport::Server;
use tonic::{Request, Status};
use tonic_reflection::server::Builder;
use tower::ServiceBuilder;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use api_grpc::{pb::vpr_server::VprServer, VprService};
use api_shared::{auth, FILE_DESCRIPTOR_SET};

fn auth_interceptor(req: Request<()>) -> Result<Request<()>, Status> {
    let api_key = req
        .metadata()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Status::unauthenticated("Missing x-api-key header"))?;

    auth::validate_api_key(api_key)?;

    Ok(req)
}

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
