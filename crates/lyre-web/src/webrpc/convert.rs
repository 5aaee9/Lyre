use super::dto::*;

impl From<lyre_core::NoiseProvider> for NoiseProvider {
    fn from(provider: lyre_core::NoiseProvider) -> Self {
        match provider {
            lyre_core::NoiseProvider::Off => Self::OFF,
            lyre_core::NoiseProvider::Rnnoise => Self::RNNOISE,
            lyre_core::NoiseProvider::Deepfilternet => Self::DEEPFILTERNET,
            lyre_core::NoiseProvider::Dpdfnet => Self::DPDFNET,
        }
    }
}

impl From<NoiseProvider> for lyre_core::NoiseProvider {
    fn from(provider: NoiseProvider) -> Self {
        match provider {
            NoiseProvider::OFF => Self::Off,
            NoiseProvider::RNNOISE => Self::Rnnoise,
            NoiseProvider::DEEPFILTERNET => Self::Deepfilternet,
            NoiseProvider::DPDFNET => Self::Dpdfnet,
        }
    }
}

impl From<lyre_core::NoiseCancellationConfig> for NoiseCancellationConfig {
    fn from(config: lyre_core::NoiseCancellationConfig) -> Self {
        Self {
            provider: config.provider.into(),
            intensity: config.intensity,
            voice_activity_threshold: config.voice_activity_threshold,
            dpdfnet: DpdfNetConfig {
                model: config.dpdfnet.model,
            },
        }
    }
}

impl From<NoiseCancellationConfig> for lyre_core::NoiseCancellationConfig {
    fn from(config: NoiseCancellationConfig) -> Self {
        Self {
            provider: config.provider.into(),
            intensity: config.intensity,
            voice_activity_threshold: config.voice_activity_threshold,
            dpdfnet: lyre_core::DpdfNetConfig {
                model: config.dpdfnet.model,
            },
        }
    }
}

impl From<lyre_core::IceServerConfig> for IceServerConfig {
    fn from(config: lyre_core::IceServerConfig) -> Self {
        Self {
            urls: config.urls,
            username: config.username,
            credential: config.credential,
        }
    }
}

impl From<lyre_core::MediaTopologyMode> for MediaTopologyMode {
    fn from(mode: lyre_core::MediaTopologyMode) -> Self {
        match mode {
            lyre_core::MediaTopologyMode::MediaRelay => Self::MEDIA_RELAY,
        }
    }
}

impl From<lyre_core::MediaTopology> for MediaTopology {
    fn from(topology: lyre_core::MediaTopology) -> Self {
        Self {
            mode: topology.mode.into(),
            turn_relay_supported: topology.turn_relay_supported,
            server_side_audio_processing: topology.server_side_audio_processing,
            server_side_noise_cancelling: topology.server_side_noise_cancelling,
            server_noise_cancelling_requires: topology.server_noise_cancelling_requires.into(),
        }
    }
}

impl From<lyre_core::MediaRelayStatus> for MediaRelayStatus {
    fn from(status: lyre_core::MediaRelayStatus) -> Self {
        match status {
            lyre_core::MediaRelayStatus::Inactive => Self::INACTIVE,
            lyre_core::MediaRelayStatus::Active => Self::ACTIVE,
        }
    }
}

impl From<lyre_core::MediaRelayMode> for MediaRelayMode {
    fn from(mode: lyre_core::MediaRelayMode) -> Self {
        match mode {
            lyre_core::MediaRelayMode::MediaRelay => Self::MEDIA_RELAY,
        }
    }
}

impl From<lyre_core::MediaTrackKind> for MediaTrackKind {
    fn from(kind: lyre_core::MediaTrackKind) -> Self {
        match kind {
            lyre_core::MediaTrackKind::Audio => Self::AUDIO,
            lyre_core::MediaTrackKind::Video => Self::VIDEO,
        }
    }
}

impl From<MediaTrackKind> for lyre_core::MediaTrackKind {
    fn from(kind: MediaTrackKind) -> Self {
        match kind {
            MediaTrackKind::AUDIO => Self::Audio,
            MediaTrackKind::VIDEO => Self::Video,
        }
    }
}

impl From<lyre_core::MediaRelayTrack> for MediaRelayTrack {
    fn from(track: lyre_core::MediaRelayTrack) -> Self {
        Self {
            track_id: track.track_id,
            kind: track.kind.into(),
        }
    }
}

impl From<lyre_core::MediaRelayParticipant> for MediaRelayParticipant {
    fn from(participant: lyre_core::MediaRelayParticipant) -> Self {
        Self {
            user_id: participant.user_id.as_str().to_owned(),
            tracks: participant.tracks.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<lyre_core::MediaRelayRoomStatus> for MediaRelayRoomStatus {
    fn from(status: lyre_core::MediaRelayRoomStatus) -> Self {
        Self {
            room_id: status.room_id.as_str().to_owned(),
            status: status.status.into(),
            mode: status.mode.into(),
            server_side_audio_processing: status.server_side_audio_processing,
            server_side_noise_cancelling: status.server_side_noise_cancelling,
            noise: status.noise.into(),
            participants: status.participants.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<lyre_core::UserProfile> for UserProfile {
    fn from(user: lyre_core::UserProfile) -> Self {
        Self {
            id: user.id.as_str().to_owned(),
            nickname: user.nickname,
            joined_at: user.joined_at,
            noise: user.noise.into(),
        }
    }
}

impl From<lyre_core::RoomSnapshot> for RoomSnapshot {
    fn from(snapshot: lyre_core::RoomSnapshot) -> Self {
        Self {
            room_id: snapshot.room_id.as_str().to_owned(),
            users: snapshot.users.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<lyre_webrtc::ServerMediaSessionState> for ServerMediaSessionState {
    fn from(state: lyre_webrtc::ServerMediaSessionState) -> Self {
        match state {
            lyre_webrtc::ServerMediaSessionState::New => Self::NEW,
            lyre_webrtc::ServerMediaSessionState::Negotiating => Self::NEGOTIATING,
            lyre_webrtc::ServerMediaSessionState::Connected => Self::CONNECTED,
            lyre_webrtc::ServerMediaSessionState::Closed => Self::CLOSED,
        }
    }
}

impl From<lyre_webrtc::ServerMediaAnswer> for ServerMediaAnswer {
    fn from(answer: lyre_webrtc::ServerMediaAnswer) -> Self {
        Self {
            room_id: answer.room_id.as_str().to_owned(),
            user_id: answer.user_id.as_str().to_owned(),
            audio_track_id: answer.audio_track_id,
            sdp: answer.sdp,
            state: answer.state.into(),
        }
    }
}

impl From<lyre_webrtc::ServerMediaSessionStatus> for ServerMediaSessionStatus {
    fn from(status: lyre_webrtc::ServerMediaSessionStatus) -> Self {
        Self {
            room_id: status.room_id.as_str().to_owned(),
            user_id: status.user_id.as_str().to_owned(),
            audio_track_id: status.audio_track_id,
            state: status.state.into(),
        }
    }
}

impl From<lyre_webrtc::ServerMediaIceCandidate> for ServerMediaIceCandidate {
    fn from(candidate: lyre_webrtc::ServerMediaIceCandidate) -> Self {
        Self {
            room_id: candidate.room_id.as_str().to_owned(),
            user_id: candidate.user_id.as_str().to_owned(),
            candidate: candidate.candidate,
            sdp_mid: candidate.sdp_mid,
            sdp_mline_index: candidate.sdp_mline_index,
            username_fragment: candidate.username_fragment,
        }
    }
}

impl From<crate::api_server_media_state::CloseServerMediaSessionResponse>
    for ClosedServerMediaSession
{
    fn from(response: crate::api_server_media_state::CloseServerMediaSessionResponse) -> Self {
        Self {
            media_relay: response.media_relay.into(),
            session: response.session.map(Into::into),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_config_serializes_with_webrpc_enum_and_camel_case_threshold() {
        let json = serde_json::to_value(NoiseCancellationConfig::from(
            lyre_core::NoiseCancellationConfig {
                provider: lyre_core::NoiseProvider::Rnnoise,
                intensity: 0.8,
                voice_activity_threshold: 0.2,
                ..lyre_core::NoiseCancellationConfig::default()
            },
        ))
        .unwrap();

        assert_eq!(json["provider"], "RNNOISE");
        assert!(
            (json["voiceActivityThreshold"].as_f64().unwrap() - 0.2).abs() < 0.000_001,
            "{json}"
        );
    }

    #[test]
    fn room_snapshot_serializes_room_id_and_joined_at() {
        let joined = lyre_core::RoomRegistry::new()
            .join(lyre_core::RoomId::default_room(), Default::default());

        let json = serde_json::to_value(RoomSnapshot::from(joined.room)).unwrap();

        assert_eq!(json["roomID"], "DEFAULT");
        assert!(json["users"][0]["joinedAt"].is_string());
        assert_eq!(json["users"][0]["noise"]["provider"], "OFF");
    }
}
