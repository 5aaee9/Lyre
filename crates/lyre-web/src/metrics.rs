use crate::api::AppState;
use axum::{
    extract::State,
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct MetricsState {
    joins: AtomicU64,
    leaves: AtomicU64,
    persistence_failures: AtomicU64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricsSnapshot {
    pub rooms: usize,
    pub users: usize,
    pub active_media_relays: usize,
    pub media_relay_participants: usize,
    pub active_server_media_sessions: usize,
    pub server_media_runtime_pumps: usize,
    pub processed_audio_egress_pumps: usize,
    pub joins: u64,
    pub leaves: u64,
    pub persistence_failures: u64,
}

impl MetricsState {
    pub fn record_join(&self) {
        self.joins.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_leave(&self) {
        self.leaves.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_persistence_failure(&self) {
        self.persistence_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn counters(&self) -> (u64, u64, u64) {
        (
            self.joins.load(Ordering::Relaxed),
            self.leaves.load(Ordering::Relaxed),
            self.persistence_failures.load(Ordering::Relaxed),
        )
    }
}

pub fn snapshot(state: &AppState) -> MetricsSnapshot {
    let rooms = state.registry.aggregate();
    let media_relays = state.media_relays.aggregate();
    let (joins, leaves, persistence_failures) = state.metrics.counters();
    MetricsSnapshot {
        rooms: rooms.rooms,
        users: rooms.users,
        active_media_relays: media_relays.active_rooms,
        media_relay_participants: media_relays.participants,
        active_server_media_sessions: state.active_server_media_sessions().len(),
        server_media_runtime_pumps: state.server_media_runtime_pump_count(),
        processed_audio_egress_pumps: state.processed_audio_webrtc_egress_pump_count(),
        joins,
        leaves,
        persistence_failures,
    }
}

pub async fn metrics(State(state): State<AppState>) -> Response {
    let body = render_metrics(snapshot(&state));
    let mut response = body.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; version=0.0.4"),
    );
    response
}

pub fn render_metrics(snapshot: MetricsSnapshot) -> String {
    let mut output = String::new();
    write_metric(
        &mut output,
        "lyre_rooms_total",
        "gauge",
        "Known rooms.",
        snapshot.rooms,
    );
    write_metric(
        &mut output,
        "lyre_users_total",
        "gauge",
        "Joined users.",
        snapshot.users,
    );
    write_metric(
        &mut output,
        "lyre_media_relays_active",
        "gauge",
        "Active media relay rooms.",
        snapshot.active_media_relays,
    );
    write_metric(
        &mut output,
        "lyre_media_relay_participants_total",
        "gauge",
        "Participants in active media relays.",
        snapshot.media_relay_participants,
    );
    write_metric(
        &mut output,
        "lyre_server_media_sessions_active",
        "gauge",
        "Active server media sessions.",
        snapshot.active_server_media_sessions,
    );
    write_metric(
        &mut output,
        "lyre_server_media_runtime_pumps_active",
        "gauge",
        "Active server media runtime pump tasks.",
        snapshot.server_media_runtime_pumps,
    );
    write_metric(
        &mut output,
        "lyre_processed_audio_egress_pumps_active",
        "gauge",
        "Active processed audio WebRTC egress pump tasks.",
        snapshot.processed_audio_egress_pumps,
    );
    write_metric(
        &mut output,
        "lyre_room_joins_total",
        "counter",
        "Successful room joins since process start.",
        snapshot.joins,
    );
    write_metric(
        &mut output,
        "lyre_room_leaves_total",
        "counter",
        "Successful room leaves since process start.",
        snapshot.leaves,
    );
    write_metric(
        &mut output,
        "lyre_room_state_persistence_failures_total",
        "counter",
        "Failed room state persistence writes since process start.",
        snapshot.persistence_failures,
    );
    output
}

fn write_metric(
    output: &mut String,
    name: &str,
    kind: &str,
    help: &str,
    value: impl std::fmt::Display,
) {
    output.push_str("# HELP ");
    output.push_str(name);
    output.push(' ');
    output.push_str(help);
    output.push('\n');
    output.push_str("# TYPE ");
    output.push_str(name);
    output.push(' ');
    output.push_str(kind);
    output.push('\n');
    output.push_str(name);
    output.push(' ');
    output.push_str(&value.to_string());
    output.push('\n');
}
