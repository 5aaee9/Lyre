use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IceServerConfig {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

pub fn default_ice_servers() -> Vec<IceServerConfig> {
    vec![IceServerConfig {
        urls: vec!["stun:stun.l.google.com:19302".to_owned()],
        username: None,
        credential: None,
    }]
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
}
