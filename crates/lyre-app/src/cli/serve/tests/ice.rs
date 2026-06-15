use super::{default_serve_args, EnvVarGuard, ENV_LOCK};
use crate::cli::serve::IceServerConfigError;
use lyre_core::{default_ice_servers, IceServerConfig};

#[test]
fn ice_servers_default_when_unconfigured() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _ice_servers = EnvVarGuard::remove("LYRE_ICE_SERVERS");
    let args = default_serve_args();

    assert_eq!(args.effective_ice_servers().unwrap(), default_ice_servers());
}

#[test]
fn embedded_turn_auto_generates_ice_server_when_unconfigured() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _ice_servers = EnvVarGuard::remove("LYRE_ICE_SERVERS");
    let mut args = default_serve_args();
    args.embedded_turn = true;

    assert_eq!(
        args.effective_ice_servers().unwrap(),
        vec![IceServerConfig {
            urls: vec!["turn:127.0.0.1:3478".to_owned()],
            username: None,
            credential: None,
        }]
    );
}

#[test]
fn parses_cli_ice_servers_with_credentials() {
    let mut args = default_serve_args();
    args.ice_servers = vec![
        "stun:a.example:3478,stun:b.example:3478".to_owned(),
        "turn:turn.example:3478|user|pass".to_owned(),
    ];

    let servers = args.effective_ice_servers().unwrap();

    assert_eq!(
        servers[0].urls,
        ["stun:a.example:3478", "stun:b.example:3478"]
    );
    assert_eq!(servers[1].username.as_deref(), Some("user"));
    assert_eq!(servers[1].credential.as_deref(), Some("pass"));
}

#[test]
fn env_ice_servers_are_semicolon_separated() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _ice_servers = EnvVarGuard::set(
        "LYRE_ICE_SERVERS",
        "stun:a.example:3478;turn:turn.example:3478||pass",
    );
    let args = default_serve_args();

    let servers = args.effective_ice_servers().unwrap();

    assert_eq!(servers.len(), 2);
    assert_eq!(servers[1].username.as_deref(), Some(""));
    assert_eq!(servers[1].credential.as_deref(), Some("pass"));
}

#[test]
fn cli_ice_servers_take_precedence_over_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _ice_servers = EnvVarGuard::set("LYRE_ICE_SERVERS", "stun:env.example:3478");
    let mut args = default_serve_args();
    args.ice_servers = vec!["stun:cli.example:3478".to_owned()];

    let servers = args.effective_ice_servers().unwrap();

    assert_eq!(servers[0].urls, ["stun:cli.example:3478"]);
}

#[test]
fn explicit_cli_ice_servers_take_precedence_over_embedded_turn() {
    let mut args = default_serve_args();
    args.embedded_turn = true;
    args.ice_servers = vec!["stun:cli.example:3478".to_owned()];

    let servers = args.effective_ice_servers().unwrap();

    assert_eq!(servers[0].urls, ["stun:cli.example:3478"]);
}

#[test]
fn env_ice_servers_take_precedence_over_embedded_turn() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _ice_servers = EnvVarGuard::set("LYRE_ICE_SERVERS", "stun:env.example:3478");
    let mut args = default_serve_args();
    args.embedded_turn = true;

    let servers = args.effective_ice_servers().unwrap();

    assert_eq!(servers[0].urls, ["stun:env.example:3478"]);
}

#[test]
fn rejects_embedded_turn_hostname_external_for_ice_config() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _ice_servers = EnvVarGuard::remove("LYRE_ICE_SERVERS");
    let mut args = default_serve_args();
    args.embedded_turn = true;
    args.embedded_turn_external = "turn.example.com:3478".to_owned();

    assert_eq!(
        args.effective_ice_servers(),
        Err(IceServerConfigError::InvalidEmbeddedTurnExternal {
            value: "turn.example.com:3478".to_owned()
        })
    );
}
