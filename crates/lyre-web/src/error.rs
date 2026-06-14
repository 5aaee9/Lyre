use axum::{http::StatusCode, response::IntoResponse, Json};
use lyre_core::RoomIdError;
use serde::Serialize;

#[derive(Debug)]
pub enum ApiError {
    BadRoomId(RoomIdError),
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

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, error) = match self {
            Self::BadRoomId(error) => (StatusCode::BAD_REQUEST, error.to_string()),
        };
        (status, Json(ErrorBody { error })).into_response()
    }
}
