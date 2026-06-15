use crate::{api::AppState, router, state_persistence::RoomStatePersistence};
use anyhow::{Context, Result};
use lyre_core::{IceServerConfig, TurnRestCredentialsConfig};
use std::{net::SocketAddr, path::PathBuf, str::FromStr};
use tokio::net::TcpListener;

#[derive(Debug, Clone)]
pub struct ServeConfig {
    pub host: String,
    pub port: u16,
    pub ice_servers: Vec<IceServerConfig>,
    pub turn_rest_credentials: Option<TurnRestCredentialsConfig>,
    pub embedded_turn: Option<lyre_turn::EmbeddedTurnConfig>,
    pub state_file: Option<PathBuf>,
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
    let room_state_persistence = config.state_file.clone().map(RoomStatePersistence::new);
    let state = AppState::with_room_state_persistence(
        config.ice_servers,
        config.turn_rest_credentials,
        room_state_persistence,
    )
    .context("failed to initialize Lyre room state")?;
    let api = async move {
        axum::serve(listener, router(state))
            .await
            .context("Lyre API server failed")
    };
    let turn = config.embedded_turn.map(lyre_turn::run_embedded_turn);
    run_api_and_optional_turn(api, turn).await
}

async fn run_api_and_optional_turn<A, T>(api: A, embedded_turn: Option<T>) -> Result<()>
where
    A: std::future::Future<Output = Result<()>> + Send + 'static,
    T: std::future::Future<Output = Result<()>> + Send + 'static,
{
    match embedded_turn {
        None => api.await,
        Some(turn) => {
            let mut api_task = tokio::spawn(api);
            let mut turn_task = tokio::spawn(turn);
            tokio::select! {
                api_result = &mut api_task => {
                    turn_task.abort();
                    api_result
                        .context("Lyre API task join failed while embedded TURN was enabled")?
                        .context("Lyre API task exited while embedded TURN was enabled")
                }
                turn_result = &mut turn_task => {
                    api_task.abort();
                    turn_result
                        .context("embedded TURN task join failed")?
                        .context("embedded TURN task exited")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn api_error_is_returned_when_turn_is_enabled() {
        let err = run_api_and_optional_turn(
            async { anyhow::bail!("api boom") },
            Some(async { std::future::pending::<Result<()>>().await }),
        )
        .await
        .unwrap_err();

        assert!(format!("{err:#}").contains("api boom"));
        assert!(format!("{err:#}").contains("Lyre API task exited"));
    }

    #[tokio::test]
    async fn turn_error_is_returned_when_api_is_running() {
        let err = run_api_and_optional_turn(
            async { std::future::pending::<Result<()>>().await },
            Some(async { anyhow::bail!("turn boom") }),
        )
        .await
        .unwrap_err();

        assert!(format!("{err:#}").contains("turn boom"));
        assert!(format!("{err:#}").contains("embedded TURN task exited"));
    }
}
