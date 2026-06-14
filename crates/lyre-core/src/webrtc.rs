use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IceServerConfig {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaTopologyMode {
    P2pMesh,
    MediaRelay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaTopology {
    pub mode: MediaTopologyMode,
    pub turn_relay_supported: bool,
    pub server_side_audio_processing: bool,
    pub server_side_noise_cancelling: bool,
    pub server_noise_cancelling_requires: MediaTopologyMode,
}

pub fn default_ice_servers() -> Vec<IceServerConfig> {
    vec![IceServerConfig {
        urls: vec!["stun:stun.l.google.com:19302".to_owned()],
        username: None,
        credential: None,
    }]
}

pub fn current_media_topology() -> MediaTopology {
    MediaTopology {
        mode: MediaTopologyMode::P2pMesh,
        turn_relay_supported: true,
        server_side_audio_processing: false,
        server_side_noise_cancelling: false,
        server_noise_cancelling_requires: MediaTopologyMode::MediaRelay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_ice_server_uses_public_stun() {
        let servers = default_ice_servers();
        assert_eq!(servers[0].urls, ["stun:stun.l.google.com:19302"]);
        assert_eq!(servers[0].username, None);
        assert_eq!(servers[0].credential, None);
    }

    #[test]
    fn ice_server_serializes_browser_field_names() {
        let server = IceServerConfig {
            urls: vec!["turn:turn.example:3478".to_owned()],
            username: Some("user".to_owned()),
            credential: Some("pass".to_owned()),
        };

        let json = serde_json::to_value(server).unwrap();

        assert_eq!(json["urls"][0], "turn:turn.example:3478");
        assert_eq!(json["username"], "user");
        assert_eq!(json["credential"], "pass");
    }

    #[test]
    fn current_topology_separates_turn_relay_from_server_processing() {
        let topology = current_media_topology();

        assert_eq!(topology.mode, MediaTopologyMode::P2pMesh);
        assert!(topology.turn_relay_supported);
        assert!(!topology.server_side_audio_processing);
        assert!(!topology.server_side_noise_cancelling);
        assert_eq!(
            topology.server_noise_cancelling_requires,
            MediaTopologyMode::MediaRelay
        );
    }

    #[test]
    fn media_topology_serializes_contract_fields() {
        let json = serde_json::to_value(current_media_topology()).unwrap();

        assert_eq!(json["mode"], "p2p_mesh");
        assert_eq!(json["turn_relay_supported"], true);
        assert_eq!(json["server_side_audio_processing"], false);
        assert_eq!(json["server_side_noise_cancelling"], false);
        assert_eq!(json["server_noise_cancelling_requires"], "media_relay");
    }
}
