use std::error::Error;

use axum::{http::StatusCode, response::IntoResponse, Json};
use lyre_core::{MediaRelayError, RoomIdError, TurnRestCredentialsError};
use lyre_webrtc::ServerMediaNegotiationError;
use serde::Serialize;

#[derive(Debug)]
pub enum ApiError {
    BadRoomId(RoomIdError),
    MediaRelay(MediaRelayError),
    ServerMediaNegotiation(ServerMediaNegotiationError),
    TurnRestCredentials(TurnRestCredentialsError),
    Unauthorized,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

impl From<RoomIdError> for ApiError {
    fn from(error: RoomIdError) -> Self {
        Self::BadRoomId(error)
    }
}

impl From<MediaRelayError> for ApiError {
    fn from(error: MediaRelayError) -> Self {
        Self::MediaRelay(error)
    }
}

impl From<TurnRestCredentialsError> for ApiError {
    fn from(error: TurnRestCredentialsError) -> Self {
        Self::TurnRestCredentials(error)
    }
}

impl From<ServerMediaNegotiationError> for ApiError {
    fn from(error: ServerMediaNegotiationError) -> Self {
        Self::ServerMediaNegotiation(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, error) = match self {
            Self::BadRoomId(error) => (StatusCode::BAD_REQUEST, error.to_string()),
            Self::MediaRelay(error) => (StatusCode::CONFLICT, error.to_string()),
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "room access token is invalid".to_owned(),
            ),
            Self::ServerMediaNegotiation(error) => {
                let status = match &error {
                    ServerMediaNegotiationError::WebRtc {
                        source: lyre_webrtc::WebRtcStackError::CreateAnswer { .. },
                    }
                    | ServerMediaNegotiationError::WebRtc {
                        source: lyre_webrtc::WebRtcStackError::AddIceCandidate { .. },
                    } => StatusCode::BAD_REQUEST,
                    ServerMediaNegotiationError::WebRtc { .. }
                    | ServerMediaNegotiationError::SessionMissing => {
                        StatusCode::INTERNAL_SERVER_ERROR
                    }
                };
                (status, error_chain(&error))
            }
            Self::TurnRestCredentials(error) => {
                (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
            }
        };
        (status, Json(ErrorBody { error })).into_response()
    }
}

fn error_chain(error: &dyn Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(error) = source {
        message.push_str(": ");
        message.push_str(&error.to_string());
        source = error.source();
    }
    message
}
