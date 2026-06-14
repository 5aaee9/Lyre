use crate::{api::AppState, router};
use anyhow::{Context, Result};
use lyre_core::IceServerConfig;
use std::{net::SocketAddr, str::FromStr};
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
pub struct ServeConfig {
    pub host: String,
    pub port: u16,
    pub ice_servers: Vec<IceServerConfig>,
}

impl ServeConfig {
    pub fn addr(&self) -> Result<SocketAddr> {
        SocketAddr::from_str(&format!("{}:{}", self.host, self.port))
            .with_context(|| format!("invalid bind address {}:{}", self.host, self.port))
    }
}

pub async fn serve(config: ServeConfig) -> Result<()> {
    let addr = config.addr()?;
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind Lyre API listener at {addr}"))?;
    tracing::info!(%addr, "Lyre API listening");
    axum::serve(listener, router(AppState::new(config.ice_servers)))
        .await
        .context("Lyre API server failed")
}
