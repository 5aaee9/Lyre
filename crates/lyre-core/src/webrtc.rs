use base64::{engine::general_purpose::STANDARD, Engine as _};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use thiserror::Error;

type HmacSha1 = Hmac<Sha1>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IceServerConfig {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaTopologyMode {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRestCredentialsConfig {
    pub secret: String,
    pub ttl_seconds: u64,
    pub identity: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRestCredentials {
    pub username: String,
    pub credential: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TurnRestCredentialsError {
    #[error("TURN REST shared secret must not be blank")]
    BlankSecret,
    #[error("TURN REST identity must not be blank")]
    BlankIdentity,
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
        mode: MediaTopologyMode::MediaRelay,
        turn_relay_supported: true,
        server_side_audio_processing: true,
        server_side_noise_cancelling: true,
        server_noise_cancelling_requires: MediaTopologyMode::MediaRelay,
    }
}

pub fn generate_turn_rest_credentials(
    config: &TurnRestCredentialsConfig,
    now_unix_seconds: u64,
) -> Result<TurnRestCredentials, TurnRestCredentialsError> {
    if config.secret.trim().is_empty() {
        return Err(TurnRestCredentialsError::BlankSecret);
    }
    if config.identity.trim().is_empty() {
        return Err(TurnRestCredentialsError::BlankIdentity);
    }
    let username = format!(
        "{}:{}",
        now_unix_seconds.saturating_add(config.ttl_seconds),
        config.identity.trim()
    );
    let mut mac = HmacSha1::new_from_slice(config.secret.as_bytes())
        .expect("HMAC-SHA1 accepts keys of any length");
    mac.update(username.as_bytes());
    let credential = STANDARD.encode(mac.finalize().into_bytes());
    Ok(TurnRestCredentials {
        username,
        credential,
    })
}

pub fn ice_servers_with_turn_rest_credentials(
    servers: &[IceServerConfig],
    config: Option<&TurnRestCredentialsConfig>,
    now_unix_seconds: u64,
) -> Result<Vec<IceServerConfig>, TurnRestCredentialsError> {
    let Some(config) = config else {
        return Ok(servers.to_vec());
    };
    let credentials = generate_turn_rest_credentials(config, now_unix_seconds)?;
    Ok(servers
        .iter()
        .map(|server| {
            if is_turn_server(server) {
                IceServerConfig {
                    urls: server.urls.clone(),
                    username: Some(credentials.username.clone()),
                    credential: Some(credentials.credential.clone()),
                }
            } else {
                server.clone()
            }
        })
        .collect())
}

fn is_turn_server(server: &IceServerConfig) -> bool {
    server.urls.iter().any(|url| {
        let lower = url.to_ascii_lowercase();
        lower.starts_with("turn:") || lower.starts_with("turns:")
    })
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
    fn current_topology_uses_server_media_relay() {
        let topology = current_media_topology();

        assert_eq!(topology.mode, MediaTopologyMode::MediaRelay);
        assert!(topology.turn_relay_supported);
        assert!(topology.server_side_audio_processing);
        assert!(topology.server_side_noise_cancelling);
        assert_eq!(
            topology.server_noise_cancelling_requires,
            MediaTopologyMode::MediaRelay
        );
    }

    #[test]
    fn media_topology_serializes_contract_fields() {
        let json = serde_json::to_value(current_media_topology()).unwrap();

        assert_eq!(json["mode"], "media_relay");
        assert_eq!(json["turn_relay_supported"], true);
        assert_eq!(json["server_side_audio_processing"], true);
        assert_eq!(json["server_side_noise_cancelling"], true);
        assert_eq!(json["server_noise_cancelling_requires"], "media_relay");
    }

    #[test]
    fn turn_rest_credentials_match_standard_test_vector() {
        let config = TurnRestCredentialsConfig {
            secret: "turn-secret".to_owned(),
            ttl_seconds: 3600,
            identity: "lyre".to_owned(),
        };

        let credentials = generate_turn_rest_credentials(&config, 1_700_000_000).unwrap();

        assert_eq!(credentials.username, "1700003600:lyre");
        assert_eq!(credentials.credential, "kPvQ2eDShdPecE5A3hgn5A03mIc=");
    }

    #[test]
    fn turn_rest_credentials_reject_blank_secret_or_identity() {
        let mut config = TurnRestCredentialsConfig {
            secret: " ".to_owned(),
            ttl_seconds: 3600,
            identity: "lyre".to_owned(),
        };
        assert_eq!(
            generate_turn_rest_credentials(&config, 1),
            Err(TurnRestCredentialsError::BlankSecret)
        );
        config.secret = "secret".to_owned();
        config.identity = " ".to_owned();
        assert_eq!(
            generate_turn_rest_credentials(&config, 1),
            Err(TurnRestCredentialsError::BlankIdentity)
        );
    }

    #[test]
    fn turn_rest_credentials_apply_only_to_turn_servers() {
        let servers = vec![
            IceServerConfig {
                urls: vec!["stun:stun.example:3478".to_owned()],
                username: None,
                credential: None,
            },
            IceServerConfig {
                urls: vec!["turn:turn.example:3478".to_owned()],
                username: Some("static-user".to_owned()),
                credential: Some("static-pass".to_owned()),
            },
            IceServerConfig {
                urls: vec!["turns:turn.example:5349".to_owned()],
                username: None,
                credential: None,
            },
        ];
        let config = TurnRestCredentialsConfig {
            secret: "turn-secret".to_owned(),
            ttl_seconds: 3600,
            identity: "lyre".to_owned(),
        };

        let rewritten =
            ice_servers_with_turn_rest_credentials(&servers, Some(&config), 1_700_000_000).unwrap();

        assert_eq!(rewritten[0], servers[0]);
        assert_eq!(rewritten[1].username.as_deref(), Some("1700003600:lyre"));
        assert_eq!(
            rewritten[1].credential.as_deref(),
            Some("kPvQ2eDShdPecE5A3hgn5A03mIc=")
        );
        assert_eq!(rewritten[2].username.as_deref(), Some("1700003600:lyre"));
    }
}
