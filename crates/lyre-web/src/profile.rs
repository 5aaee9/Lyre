use axum::{
    extract::Query,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use pprof::protos::Message;
use serde::Deserialize;
use std::{env, time::Duration};

const DEFAULT_SECONDS: u64 = 30;
const MAX_SECONDS: u64 = 300;
const SAMPLE_FREQUENCY: i32 = 100;

#[derive(Debug, Deserialize)]
struct ProfileQuery {
    seconds: Option<u64>,
}

pub fn enabled_from_env() -> bool {
    env::var("LYRE_ENABLE_PROF")
        .ok()
        .map(|value| {
            matches!(
                value.as_str(),
                "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
            )
        })
        .unwrap_or(false)
}

pub fn router() -> Router<crate::AppState> {
    Router::new().route("/debug/pprof/profile", get(cpu_profile))
}

async fn cpu_profile(Query(query): Query<ProfileQuery>) -> Result<Response, ProfileError> {
    let seconds = query.seconds.unwrap_or(DEFAULT_SECONDS).min(MAX_SECONDS);
    let profile = tokio::task::spawn_blocking(move || collect_cpu_profile(seconds))
        .await
        .map_err(ProfileError::Join)??;
    Ok((
        [(header::CONTENT_TYPE, "application/octet-stream")],
        profile,
    )
        .into_response())
}

fn collect_cpu_profile(seconds: u64) -> Result<Vec<u8>, ProfileError> {
    let guard = pprof::ProfilerGuard::new(SAMPLE_FREQUENCY).map_err(ProfileError::Pprof)?;
    std::thread::sleep(Duration::from_secs(seconds));
    let profile = guard
        .report()
        .build()
        .and_then(|report| report.pprof())
        .map_err(ProfileError::Pprof)?;
    let mut body = Vec::new();
    profile
        .encode(&mut body)
        .map_err(|error| ProfileError::Encode(error.to_string()))?;
    Ok(body)
}

#[derive(Debug)]
enum ProfileError {
    Encode(String),
    Join(tokio::task::JoinError),
    Pprof(pprof::Error),
}

impl IntoResponse for ProfileError {
    fn into_response(self) -> Response {
        let detail = match &self {
            Self::Encode(error) => format!("failed to encode CPU profile: {error}"),
            Self::Join(error) => format!("CPU profile task failed: {error}"),
            Self::Pprof(error) => format!("failed to collect CPU profile: {error}"),
        };
        tracing::error!(error = %detail, "CPU profile request failed");
        (StatusCode::INTERNAL_SERVER_ERROR, detail).into_response()
    }
}
