use std::net::SocketAddr;
use tonic::transport::Server;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use api::{VprService, pb::vpr_server::VprServer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env().add_directive("vpr=info".parse()?))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let addr: SocketAddr = std::env::var("VPR_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:50051".into())
        .parse()?;

    tracing::info!("++ Starting VPR gRPC on {}", addr);

    let svc = VprService;
    Server::builder()
        .add_service(VprServer::new(svc))
        .serve(addr)
        .await?;

    Ok(())
}
