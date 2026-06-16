pub mod api;
pub mod api_server_media;
pub mod api_server_media_state;
pub mod app_state;
pub mod error;
pub mod media_egress;
pub mod media_runtime;
pub mod metrics;
pub mod processed_audio_webrtc_egress_pump;
pub mod raw_opus_webrtc_egress_pump;
pub mod server;
mod server_media_ice_diagnostics;
pub mod server_media_runtime;
pub mod server_media_runtime_pump;
pub mod signalling;
pub mod state_persistence;
pub mod webrpc;

#[cfg(test)]
mod api_media_broadcast_tests;
#[cfg(test)]
mod api_media_tests;
#[cfg(test)]
mod api_server_media_close_tests;
#[cfg(test)]
mod api_server_media_tests;
#[cfg(test)]
mod api_tests;
#[cfg(test)]
mod api_webrtc_session_tests;
#[cfg(test)]
mod media_egress_tests;
#[cfg(test)]
mod metrics_tests;
#[cfg(test)]
mod processed_audio_webrtc_egress_pump_tests;
#[cfg(test)]
mod server_media_runtime_pump_tests;
#[cfg(test)]
mod server_media_runtime_tests;
#[cfg(test)]
mod signalling_tests;
#[cfg(test)]
mod state_persistence_tests;
#[cfg(test)]
mod webrpc_tests;
#[cfg(test)]
mod websocket_disconnect_tests;
#[cfg(test)]
mod websocket_server_media_candidates_tests;

pub use api::{router, router_with_cors};
pub use app_state::AppState;
pub use lyre_webrtc::ServerMediaPortRange;
pub use server::{serve, ServeConfig};
