use dashmap::DashMap;
use lyre_core::{RoomId, UserId};

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ServerMediaSessionKey {
    pub room_id: RoomId,
    pub user_id: UserId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerMediaSessionConfig {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub audio_track_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerMediaSessionState {
    New,
    Negotiating,
    Connected,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerMediaSessionStatus {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub audio_track_id: String,
    pub state: ServerMediaSessionState,
}

#[derive(Debug, Clone)]
struct ServerMediaSession {
    audio_track_id: String,
    state: ServerMediaSessionState,
}

#[derive(Debug, Default)]
pub struct ServerMediaSessionRegistry {
    sessions: DashMap<ServerMediaSessionKey, ServerMediaSession>,
}

impl ServerMediaSessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&self, config: ServerMediaSessionConfig) -> ServerMediaSessionStatus {
        let key = ServerMediaSessionKey {
            room_id: config.room_id,
            user_id: config.user_id,
        };
        let session = ServerMediaSession {
            audio_track_id: config.audio_track_id,
            state: ServerMediaSessionState::New,
        };
        self.sessions.insert(key.clone(), session.clone());
        status_from_session(&key, &session)
    }

    pub fn sessions(&self) -> Vec<ServerMediaSessionStatus> {
        sorted_statuses(self.sessions.iter().map(|entry| {
            let key = entry.key().clone();
            let session = entry.value().clone();
            status_from_session(&key, &session)
        }))
    }

    pub fn active_sessions(&self) -> Vec<ServerMediaSessionStatus> {
        sorted_statuses(
            self.sessions
                .iter()
                .filter(|entry| entry.value().state != ServerMediaSessionState::Closed)
                .map(|entry| {
                    let key = entry.key().clone();
                    let session = entry.value().clone();
                    status_from_session(&key, &session)
                }),
        )
    }

    pub fn close(&self, key: &ServerMediaSessionKey) -> Option<ServerMediaSessionStatus> {
        let mut session = self.sessions.get_mut(key)?;
        session.state = ServerMediaSessionState::Closed;
        Some(status_from_session(key, &session))
    }

    pub fn close_room(&self, room_id: &RoomId) -> Vec<ServerMediaSessionStatus> {
        let statuses = self.sessions.iter_mut().filter_map(|mut entry| {
            if &entry.key().room_id != room_id {
                return None;
            }
            entry.value_mut().state = ServerMediaSessionState::Closed;
            Some(status_from_session(entry.key(), entry.value()))
        });
        sorted_statuses(statuses)
    }
}

fn status_from_session(
    key: &ServerMediaSessionKey,
    session: &ServerMediaSession,
) -> ServerMediaSessionStatus {
    ServerMediaSessionStatus {
        room_id: key.room_id.clone(),
        user_id: key.user_id.clone(),
        audio_track_id: session.audio_track_id.clone(),
        state: session.state,
    }
}

fn sorted_statuses(
    statuses: impl IntoIterator<Item = ServerMediaSessionStatus>,
) -> Vec<ServerMediaSessionStatus> {
    let mut statuses = statuses.into_iter().collect::<Vec<_>>();
    statuses.sort_by(|left, right| {
        left.room_id
            .cmp(&right.room_id)
            .then_with(|| left.user_id.cmp(&right.user_id))
    });
    statuses
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(room: &str, user: &str, track: &str) -> ServerMediaSessionConfig {
        ServerMediaSessionConfig {
            room_id: RoomId::parse_boundary(room).unwrap(),
            user_id: UserId::from_external(user),
            audio_track_id: track.to_owned(),
        }
    }

    #[test]
    fn start_replaces_existing_session_and_resets_state() {
        let registry = ServerMediaSessionRegistry::new();
        let first = registry.start(config("DEFAULT", "user_01", "audio-main"));
        assert_eq!(first.state, ServerMediaSessionState::New);
        registry.close(&ServerMediaSessionKey {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external("user_01"),
        });

        let replaced = registry.start(config("DEFAULT", "user_01", "audio-retry"));

        assert_eq!(replaced.audio_track_id, "audio-retry");
        assert_eq!(replaced.state, ServerMediaSessionState::New);
        assert_eq!(registry.sessions().len(), 1);
    }

    #[test]
    fn sessions_are_sorted_by_room_and_user() {
        let registry = ServerMediaSessionRegistry::new();
        registry.start(config("ROOM_B", "user_b", "audio-main"));
        registry.start(config("ROOM_A", "user_c", "audio-main"));
        registry.start(config("ROOM_A", "user_a", "audio-main"));

        let keys = registry
            .sessions()
            .into_iter()
            .map(|status| format!("{}:{}", status.room_id, status.user_id))
            .collect::<Vec<_>>();

        assert_eq!(
            keys,
            vec!["ROOM_A:user_a", "ROOM_A:user_c", "ROOM_B:user_b"]
        );
    }

    #[test]
    fn close_keeps_closed_session_in_all_sessions_only() {
        let registry = ServerMediaSessionRegistry::new();
        let key = ServerMediaSessionKey {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external("user_01"),
        };
        registry.start(config("DEFAULT", "user_01", "audio-main"));

        let closed = registry.close(&key).unwrap();

        assert_eq!(closed.state, ServerMediaSessionState::Closed);
        assert_eq!(
            registry.sessions()[0].state,
            ServerMediaSessionState::Closed
        );
        assert!(registry.active_sessions().is_empty());
    }

    #[test]
    fn close_missing_session_returns_none() {
        let registry = ServerMediaSessionRegistry::new();

        assert_eq!(
            registry.close(&ServerMediaSessionKey {
                room_id: RoomId::default_room(),
                user_id: UserId::from_external("missing"),
            }),
            None
        );
    }

    #[test]
    fn close_room_closes_only_matching_room() {
        let registry = ServerMediaSessionRegistry::new();
        registry.start(config("DEFAULT", "user_01", "audio-main"));
        registry.start(config("OTHER", "user_02", "audio-main"));

        let closed = registry.close_room(&RoomId::default_room());

        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].state, ServerMediaSessionState::Closed);
        assert_eq!(registry.active_sessions()[0].room_id.as_str(), "OTHER");
    }
}
