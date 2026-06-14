# Embedded TURN Service Design

## Goal

Add an optional embedded TURN relay service to Lyre so local deployments can run the Rust API and a TURN/STUN relay from the same `lyre serve` process when explicitly enabled.

## Context

Lyre already exposes ICE server configuration and can generate TURN REST shared-secret credentials. The user asked whether `turn-rs` can be used and whether that prevents server-side noise cancellation. The current architecture must keep these concerns separate:

- TURN relay improves NAT traversal by relaying encrypted WebRTC packets.
- TURN relay does not terminate DTLS-SRTP media and cannot run RNNoise or DeepFilterNet.
- Server-side noise cancellation still requires a future media relay/SFU-like pipeline that terminates WebRTC media, decodes audio, processes PCM, re-encodes, and broadcasts.

The crate named `turn-rs` on crates.io is GPL-2.0-or-later and is a lower-level TURN session library. The same upstream repository publishes `turn-server` 4.1.2 under MIT with a usable `turn_server::start_server(config)` entry point and config support for `auth.static-auth-secret`. This increment uses `turn-server`, documents it as the service crate from the `turn-rs` project, and keeps GPL `turn-rs` out of Lyre's dependency graph.

## Scope

Implement a minimal, opt-in embedded TURN service:

- Add a new local crate `crates/lyre-turn` as the adapter boundary around `turn-server`.
- Add workspace dependency `turn-server = { version = "4.1.2", default-features = false, features = ["udp"] }`.
- Add `--embedded-turn`, `--embedded-turn-listen`, `--embedded-turn-external`, `--embedded-turn-realm`, and `--embedded-turn-port-range` to `lyre serve`.
- Add env equivalents:
  - `LYRE_EMBEDDED_TURN`
  - `LYRE_EMBEDDED_TURN_LISTEN`
  - `LYRE_EMBEDDED_TURN_EXTERNAL`
  - `LYRE_EMBEDDED_TURN_REALM`
  - `LYRE_EMBEDDED_TURN_PORT_RANGE`
- Require `--turn-rest-secret` / `LYRE_TURN_REST_SECRET` when embedded TURN is enabled.
- Start the embedded TURN server alongside Axum when enabled.
- Add a `turn:<external-host>:<external-port>` ICE server automatically when embedded TURN is enabled and no explicit `--ice-server` / `LYRE_ICE_SERVERS` is configured.
- Preserve explicit ICE server configuration exactly. If operators provide `--ice-server` or `LYRE_ICE_SERVERS`, Lyre does not auto-inject the embedded TURN URL.
- Keep `/api/webrtc/topology` as P2P mesh with TURN relay support and no server-side noise cancellation.
- Update README, MEMORY, roadmap, and AGENTS.md.

## Non-Goals

- TCP/TLS TURN listeners.
- Exposing the `turn-server` gRPC API or Prometheus endpoint.
- Verifying TURN allocation with a real browser or external network.
- Per-user TURN authentication.
- Enforcing TURN REST username expiry inside `turn-server`.
- Media relay, SFU, MCU, WebRTC termination, RNNoise, or DeepFilterNet processing.
- Adding the GPL `turn-rs` crate.

## Runtime Behavior

Embedded TURN is disabled by default.

When enabled:

1. CLI parsing builds an `EmbeddedTurnConfig` with:
   - `listen`: UDP socket address, default `0.0.0.0:3478`
   - `external`: browser-visible UDP socket address, default `127.0.0.1:3478`
   - `realm`: default `lyre.local`
   - `port_range`: default `49152..65535`
   - `static_auth_secret`: copied from the already validated TURN REST shared secret
2. `lyre_web::serve` starts the TURN server and Axum API concurrently.
3. If either server exits with an error, `serve` aborts the other task and returns the full error chain.
4. If no explicit ICE servers are configured, `effective_ice_servers()` returns the embedded TURN URL instead of the default public STUN server.
5. `/api/webrtc/ice-servers` applies the existing TURN REST credential generator to the embedded TURN URL before returning it to browsers.

`--embedded-turn-external` is intentionally independent from `listen`. It must be an IP socket address (`<ip>:<port>`), not a hostname, because `turn-server::config::Interface::Udp.external` is a `SocketAddr`. The default is local-only and advertises `turn:127.0.0.1:3478` so Lyre never returns `turn:0.0.0.0:3478`. Operators deploying behind NAT or on a public host must set `--embedded-turn-external <public-ip>:3478`. Hostname-based ICE advertisement is future work and is not part of this increment.

`--embedded-turn-port-range` and `LYRE_EMBEDDED_TURN_PORT_RANGE` use inclusive Rust range syntax: `<start>..<end>`, where both values must parse as `u16`, `start >= 49152`, `end <= 65535`, and `start <= end`. Valid example: `49152..65535`. Invalid examples that must be rejected: `49152-65535`, `49152..`, `49151..65535`, `60000..59999`, and `49152..70000`.

## Security Notes

`turn-server` supports `auth.static-auth-secret`, which matches Lyre's generated HMAC-SHA1 TURN REST credentials. Its upstream implementation computes the expected password from the username and shared secret, but it does not validate the timestamp embedded in `username`; upstream source comments state that the external web service guarantees that security. Lyre must document this limitation and keep TTL short. This increment does not claim hard expiry enforcement at the TURN relay layer.

## API and Frontend Impact

No WebRPC RIDL or frontend schema changes are needed. Browsers keep fetching `/api/webrtc/ice-servers` and receive normal ICE server objects.

## Tests

Rust tests must cover:

- Embedded TURN defaults.
- Embedded TURN rejects missing TURN REST secret.
- Embedded TURN parses listen/external/realm/port range CLI args.
- Embedded TURN parses env equivalents.
- Hostnames in `--embedded-turn-external` are rejected by socket-address parsing.
- Invalid port ranges are rejected.
- Auto ICE server generation returns `turn:<external>` when embedded TURN is enabled and no explicit ICE servers exist.
- Explicit CLI/env ICE servers still take precedence.
- `lyre-turn` converts Lyre config to `turn_server::config::Config` with one UDP interface and `auth.static_auth_secret`.
- Server orchestration helper aborts the sibling task and returns the original error context when either the API or TURN runtime exits with an error. This can be covered with test futures rather than binding real sockets.

Full verification after implementation:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`
- Frontend tests/build/typecheck/lint are not behaviorally affected, but run the standard frontend verification if any frontend or WebRPC files change. This increment should not change WebRPC generation.

## Acceptance Criteria

- Embedded TURN remains opt-in and cannot start without a TURN REST shared secret.
- Lyre can advertise its embedded TURN URL through the existing ICE server endpoint without frontend changes, and the default advertised URL is `turn:127.0.0.1:3478` rather than a wildcard address.
- `--embedded-turn-external` accepts only IP socket addresses, matching the `turn-server` configuration type.
- The `turn-server` dependency is isolated behind `crates/lyre-turn`.
- If the API or TURN runtime fails, the sibling task is aborted and the returned error preserves lower-level context.
- Documentation clearly says embedded TURN does not provide server-side noise cancellation.
- Documentation records that `turn-server` does not enforce REST credential timestamp expiry itself.
- Roadmap moves embedded TURN service evaluation to Completed and keeps media relay/server-side noise cancellation as Next.
