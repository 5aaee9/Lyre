use super::{default_serve_args, EnvVarGuard, ENV_LOCK};
use crate::cli::serve::ServerMediaConfigError;

#[test]
fn parses_cli_server_media_public_ip() {
    let mut args = default_serve_args();
    args.server_media_public_ip = Some("203.0.113.10".to_owned());

    assert_eq!(
        args.effective_server_media_public_ip().unwrap(),
        Some("203.0.113.10".parse().unwrap())
    );
}

#[test]
fn parses_env_server_media_public_ip() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _server_media_public_ip = EnvVarGuard::set("LYRE_SERVER_MEDIA_PUBLIC_IP", "203.0.113.11");
    let args = default_serve_args();

    assert_eq!(
        args.effective_server_media_public_ip().unwrap(),
        Some("203.0.113.11".parse().unwrap())
    );
}

#[test]
fn cli_server_media_public_ip_takes_precedence_over_env() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _server_media_public_ip = EnvVarGuard::set("LYRE_SERVER_MEDIA_PUBLIC_IP", "203.0.113.11");
    let mut args = default_serve_args();
    args.server_media_public_ip = Some("203.0.113.10".to_owned());

    assert_eq!(
        args.effective_server_media_public_ip().unwrap(),
        Some("203.0.113.10".parse().unwrap())
    );
}

#[test]
fn rejects_invalid_server_media_public_ip() {
    let mut args = default_serve_args();
    args.server_media_public_ip = Some("public.example.test".to_owned());

    assert_eq!(
        args.effective_server_media_public_ip(),
        Err(ServerMediaConfigError::InvalidPublicIp {
            value: "public.example.test".to_owned()
        })
    );
}
