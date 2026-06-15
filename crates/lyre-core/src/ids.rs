use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
use thiserror::Error;
use ulid::Ulid;

pub const DEFAULT_ROOM_ID: &str = "DEFAULT";

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RoomIdError {
    #[error("room id must not be blank")]
    Blank,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct RoomId(String);

impl RoomId {
    pub fn parse_boundary(input: impl AsRef<str>) -> Result<Self, RoomIdError> {
        let trimmed = input.as_ref().trim();
        if trimmed.is_empty() {
            return Err(RoomIdError::Blank);
        }
        Ok(Self(trimmed.to_owned()))
    }

    pub fn default_room() -> Self {
        Self(DEFAULT_ROOM_ID.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RoomId {
    fn default() -> Self {
        Self::default_room()
    }
}

impl fmt::Display for RoomId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for RoomId {
    type Err = RoomIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse_boundary(value)
    }
}

impl<'de> Deserialize<'de> for RoomId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse_boundary(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(String);

impl UserId {
    pub fn new() -> Self {
        Self(format!("user_{}", Ulid::new()))
    }

    pub fn from_external(input: impl Into<String>) -> Self {
        Self(input.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for UserId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_room_id() {
        let room_id = RoomId::parse_boundary(" DEFAULT ").unwrap();
        assert_eq!(room_id.as_str(), DEFAULT_ROOM_ID);
    }

    #[test]
    fn rejects_blank_room_id() {
        assert_eq!(RoomId::parse_boundary("  "), Err(RoomIdError::Blank));
    }
}
