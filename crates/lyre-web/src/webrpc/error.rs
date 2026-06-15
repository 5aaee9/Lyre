use crate::error::ApiError;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use lyre_webrtc::{ServerMediaNegotiationError, WebRtcStackError};
use serde::Serialize;
use std::error::Error;

#[derive(Debug, Serialize)]
struct WebrpcErrorBody {
    name: &'static str,
    code: i32,
    message: String,
    status: u16,
}

#[derive(Debug)]
pub(crate) struct WebrpcError {
    status: StatusCode,
    message: String,
}

impl WebrpcError {
    pub(crate) fn bad_request() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: "bad request".to_owned(),
        }
    }

    pub(crate) fn from_api(error: ApiError) -> Self {
        match error {
            ApiError::BadRoomId(error) => Self {
                status: StatusCode::BAD_REQUEST,
                message: error.to_string(),
            },
            ApiError::MediaRelay(error) => Self {
                status: StatusCode::CONFLICT,
                message: error.to_string(),
            },
            ApiError::Persistence(error) => {
                tracing::error!(error = %format!("{error:#}"), "room state persistence failed");
                Self {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    message: "room state persistence failed".to_owned(),
                }
            }
            ApiError::ServerMediaNegotiation(error) => {
                tracing::error!(
                    error = %error_chain(&error),
                    "server media negotiation failed"
                );
                Self {
                    status: server_media_status(&error),
                    message: "server media negotiation failed".to_owned(),
                }
            }
            ApiError::TurnRestCredentials(error) => Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                message: error.to_string(),
            },
            ApiError::Unauthorized => Self {
                status: StatusCode::UNAUTHORIZED,
                message: "room access token is invalid".to_owned(),
            },
        }
    }
}

impl IntoResponse for WebrpcError {
    fn into_response(self) -> Response {
        let body = WebrpcErrorBody {
            name: "WebrpcEndpoint",
            code: 0,
            message: self.message,
            status: self.status.as_u16(),
        };
        (self.status, Json(body)).into_response()
    }
}

impl From<ApiError> for WebrpcError {
    fn from(error: ApiError) -> Self {
        Self::from_api(error)
    }
}

impl From<lyre_core::RoomIdError> for WebrpcError {
    fn from(error: lyre_core::RoomIdError) -> Self {
        Self::from_api(ApiError::from(error))
    }
}

impl From<lyre_core::MediaRelayError> for WebrpcError {
    fn from(error: lyre_core::MediaRelayError) -> Self {
        Self::from_api(ApiError::from(error))
    }
}

impl From<lyre_core::TurnRestCredentialsError> for WebrpcError {
    fn from(error: lyre_core::TurnRestCredentialsError) -> Self {
        Self::from_api(ApiError::from(error))
    }
}

impl From<ServerMediaNegotiationError> for WebrpcError {
    fn from(error: ServerMediaNegotiationError) -> Self {
        Self::from_api(ApiError::from(error))
    }
}

fn server_media_status(error: &ServerMediaNegotiationError) -> StatusCode {
    match error {
        ServerMediaNegotiationError::WebRtc {
            source: WebRtcStackError::CreateAnswer { .. },
        }
        | ServerMediaNegotiationError::WebRtc {
            source: WebRtcStackError::AddIceCandidate { .. },
        } => StatusCode::BAD_REQUEST,
        ServerMediaNegotiationError::WebRtc { .. }
        | ServerMediaNegotiationError::SessionMissing => StatusCode::INTERNAL_SERVER_ERROR,
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
