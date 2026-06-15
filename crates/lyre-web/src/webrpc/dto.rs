#![allow(non_camel_case_types, clippy::upper_case_acronyms)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct GetRoomRequest {
    #[serde(rename = "roomID")]
    pub room_id: String,
}

#[derive(Debug, Serialize)]
pub struct GetRoomResponse {
    pub room: RoomSnapshot,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinRoomRequest {
    #[serde(rename = "roomID")]
    pub room_id: String,
    pub nickname: Option<String>,
    pub noise: Option<NoiseCancellationConfig>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinRoomResponse {
    pub user: UserProfile,
    pub room: RoomSnapshot,
    pub access_token: String,
}

#[derive(Debug, Deserialize)]
pub struct LeaveRoomRequest {
    #[serde(rename = "roomID")]
    pub room_id: String,
    #[serde(rename = "userID")]
    pub user_id: String,
}

#[derive(Debug, Serialize)]
pub struct LeaveRoomResponse {
    pub room: RoomSnapshot,
}

#[derive(Debug, Serialize)]
pub struct GetNoiseProvidersResponse {
    pub providers: Vec<NoiseCancellationConfig>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetIceServersResponse {
    pub ice_servers: Vec<IceServerConfig>,
}

#[derive(Debug, Serialize)]
pub struct GetMediaTopologyResponse {
    pub topology: MediaTopology,
}

#[derive(Debug, Deserialize)]
pub struct GetMediaRelayRequest {
    #[serde(rename = "roomID")]
    pub room_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMediaRelayResponse {
    pub media_relay: MediaRelayRoomStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartMediaRelayRequest {
    #[serde(rename = "roomID")]
    pub room_id: String,
    pub noise: Option<NoiseCancellationConfig>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartMediaRelayResponse {
    pub media_relay: MediaRelayRoomStatus,
}

#[derive(Debug, Deserialize)]
pub struct StopMediaRelayRequest {
    #[serde(rename = "roomID")]
    pub room_id: String,
    #[serde(rename = "userID")]
    pub user_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StopMediaRelayResponse {
    pub media_relay: MediaRelayRoomStatus,
}

#[derive(Debug, Deserialize)]
pub struct RegisterMediaTrackRequest {
    #[serde(rename = "roomID")]
    pub room_id: String,
    #[serde(rename = "userID")]
    pub user_id: String,
    #[serde(rename = "trackID")]
    pub track_id: String,
    pub kind: MediaTrackKind,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterMediaTrackResponse {
    pub media_relay: MediaRelayRoomStatus,
}

#[derive(Debug, Deserialize)]
pub struct AnswerServerMediaOfferRequest {
    #[serde(rename = "roomID")]
    pub room_id: String,
    #[serde(rename = "userID")]
    pub user_id: String,
    #[serde(rename = "audioTrackID")]
    pub audio_track_id: String,
    pub sdp: String,
}

#[derive(Debug, Serialize)]
pub struct AnswerServerMediaOfferResponse {
    pub answer: ServerMediaAnswer,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddServerMediaIceCandidateRequest {
    #[serde(rename = "roomID")]
    pub room_id: String,
    #[serde(rename = "userID")]
    pub user_id: String,
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u16>,
    pub username_fragment: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AddServerMediaIceCandidateResponse {
    pub accepted: ServerMediaIceCandidate,
}

#[derive(Debug, Deserialize)]
pub struct GetServerMediaIceCandidatesRequest {
    #[serde(rename = "roomID")]
    pub room_id: String,
    #[serde(rename = "userID")]
    pub user_id: String,
}

#[derive(Debug, Serialize)]
pub struct GetServerMediaIceCandidatesResponse {
    pub candidates: Vec<ServerMediaIceCandidate>,
}

#[derive(Debug, Deserialize)]
pub struct CloseServerMediaSessionRequest {
    #[serde(rename = "roomID")]
    pub room_id: String,
    #[serde(rename = "userID")]
    pub user_id: String,
}

#[derive(Debug, Serialize)]
pub struct CloseServerMediaSessionResponse {
    pub closed: ClosedServerMediaSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoiseProvider {
    OFF,
    RNNOISE,
    DEEPFILTERNET,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaTopologyMode {
    P2P_MESH,
    MEDIA_RELAY,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaRelayStatus {
    INACTIVE,
    ACTIVE,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaRelayMode {
    P2P_MESH,
    MEDIA_RELAY,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaTrackKind {
    AUDIO,
    VIDEO,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerMediaSessionState {
    NEW,
    NEGOTIATING,
    CONNECTED,
    CLOSED,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoiseCancellationConfig {
    pub provider: NoiseProvider,
    pub intensity: f32,
    pub voice_activity_threshold: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct IceServerConfig {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaTopology {
    pub mode: MediaTopologyMode,
    pub turn_relay_supported: bool,
    pub server_side_audio_processing: bool,
    pub server_side_noise_cancelling: bool,
    pub server_noise_cancelling_requires: MediaTopologyMode,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaRelayTrack {
    #[serde(rename = "trackID")]
    pub track_id: String,
    pub kind: MediaTrackKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaRelayParticipant {
    #[serde(rename = "userID")]
    pub user_id: String,
    pub tracks: Vec<MediaRelayTrack>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRelayRoomStatus {
    #[serde(rename = "roomID")]
    pub room_id: String,
    pub status: MediaRelayStatus,
    pub mode: MediaRelayMode,
    pub server_side_audio_processing: bool,
    pub server_side_noise_cancelling: bool,
    pub noise: NoiseCancellationConfig,
    pub participants: Vec<MediaRelayParticipant>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerMediaAnswer {
    #[serde(rename = "roomID")]
    pub room_id: String,
    #[serde(rename = "userID")]
    pub user_id: String,
    #[serde(rename = "audioTrackID")]
    pub audio_track_id: String,
    pub sdp: String,
    pub state: ServerMediaSessionState,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerMediaSessionStatus {
    #[serde(rename = "roomID")]
    pub room_id: String,
    #[serde(rename = "userID")]
    pub user_id: String,
    #[serde(rename = "audioTrackID")]
    pub audio_track_id: String,
    pub state: ServerMediaSessionState,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerMediaIceCandidate {
    #[serde(rename = "roomID")]
    pub room_id: String,
    #[serde(rename = "userID")]
    pub user_id: String,
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u16>,
    pub username_fragment: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosedServerMediaSession {
    pub media_relay: MediaRelayRoomStatus,
    pub session: Option<ServerMediaSessionStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserProfile {
    pub id: String,
    pub nickname: String,
    #[serde(rename = "joinedAt")]
    pub joined_at: chrono::DateTime<chrono::Utc>,
    pub noise: NoiseCancellationConfig,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoomSnapshot {
    #[serde(rename = "roomID")]
    pub room_id: String,
    pub users: Vec<UserProfile>,
}
