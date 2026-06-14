# TURN REST Credentials Design

## Goal

Add short-lived TURN REST credentials to Lyre's existing ICE server configuration path so browsers can receive scoped TURN credentials without exposing long-lived relay secrets.

## Context

Lyre already exposes static ICE server configuration through:

- CLI `--ice-server`
- environment `LYRE_ICE_SERVERS`
- REST `GET /api/webrtc/ice-servers`
- frontend `getIceServers()`

Static TURN usernames/passwords are browser-visible and should not be privileged long-lived secrets. The standard TURN REST credential pattern uses:

- `username = <unix_expiry>:<identity>`
- `credential = base64(HMAC-SHA1(username, shared_secret))`

The TURN server must be configured with the same shared secret for these credentials to authenticate. This increment only generates credentials for configured TURN URLs. It does not embed or run a TURN server and does not add `turn-rs`.

## Scope

In scope:

- Add a core `TurnRestCredentialsConfig` model:
  - `secret: String`
  - `ttl_seconds: u64`
  - `identity: String`
- Add deterministic credential generation:
  - input: shared secret, identity, ttl, current unix timestamp
  - output username and credential
- Add dynamic credential application to TURN ICE servers returned by `/api/webrtc/ice-servers`.
- Add CLI/env config:
  - `--turn-rest-secret`, env `LYRE_TURN_REST_SECRET`
  - `--turn-rest-ttl-seconds`, env `LYRE_TURN_REST_TTL_SECONDS`, default `3600`
  - `--turn-rest-identity`, env `LYRE_TURN_REST_IDENTITY`, default `lyre`
- Preserve static ICE behavior when no TURN REST secret is configured.
- Add tests for deterministic credential generation, non-TURN servers remaining unchanged, endpoint output, and CLI/env parsing.
- Update README, MEMORY, roadmap, and AGENTS.md. This increment adds third-party Rust dependencies, not a new workspace crate. AGENTS.md must update the Key Dependencies/project conventions area to document `hmac`, `sha1`, and `base64` for TURN REST credential generation; do not add a new workspace crate entry.
- Add focused Rust dependencies `hmac`, `sha1`, and `base64` to implement the standard algorithm. These are directly required by this feature and should be added to workspace dependencies plus `lyre-core`.

Out of scope:

- Embedding or running `turn-rs`.
- Validating credentials against a live TURN server.
- Per-user authenticated TURN identities.
- Dynamic TURN URL discovery.
- Server-side media processing or noise cancellation.

## Behavior

When no `turn_rest_credentials` config exists, `GET /api/webrtc/ice-servers` returns configured servers exactly as today.

When TURN REST credentials are configured:

- Any ICE server with at least one URL beginning with `turn:` or `turns:` receives a generated `username` and `credential`.
- STUN-only servers remain unchanged.
- Existing static TURN `username`/`credential` values are replaced in the API response.
- The generated username is `<expiry_unix_seconds>:<identity>`.
- The generated credential is standard base64 with padding of HMAC-SHA1(username, shared_secret).
- The expiry is computed from the request time plus `ttl_seconds`.

Deterministic test vector:

- shared secret: `turn-secret`
- identity: `lyre`
- current unix timestamp: `1700000000`
- ttl seconds: `3600`
- expected username: `1700003600:lyre`
- expected credential: `kPvQ2eDShdPecE5A3hgn5A03mIc=`

## API Contract

`GET /api/webrtc/ice-servers` remains the browser-facing route. No new route is needed.

Example response with generated credentials:

```json
[
  {
    "urls": ["turn:turn.example:3478"],
    "username": "1781455200:lyre",
    "credential": "base64-hmac-sha1-value"
  }
]
```

## Security Notes

- `LYRE_TURN_REST_SECRET` must never be returned to clients.
- The generated credential is intentionally browser-visible and short-lived.
- The server should reject blank secrets when TURN REST credential generation is configured.
- The default TTL is one hour.
- `hmac`, `sha1`, and `base64` are acceptable new dependencies for this feature because they directly implement the TURN REST shared-secret credential algorithm.

## WebRPC and Frontend Contract

`proto/lyre.ridl` and `frontend/src/lib/lyre.gen.ts` do not need changes in this increment because the browser-facing ICE response schema remains `IceServerConfig { urls, username?, credential? }`.

The existing frontend `getIceServers()` helper does not need behavior changes; it will receive generated credentials through the same REST route and response shape.

## Testing

- Unit test deterministic HMAC generation with fixed timestamp.
- Unit test that STUN-only servers are not modified.
- Unit test that TURN/TURNS servers get generated credentials.
- CLI tests cover flags, env, default TTL, and blank secret rejection.
- API route test verifies `/api/webrtc/ice-servers` returns generated credentials without exposing the shared secret.
- Existing frontend ICE tests keep passing because the response shape is unchanged.

## Acceptance Criteria

- Operators can configure short-lived TURN credential generation without changing frontend code.
- Static ICE behavior is unchanged by default.
- Generated credentials apply only to TURN/TURNS ICE servers.
- Documentation explains that this is compatible with TURN REST shared-secret servers, including a future `turn-rs` setup if it supports the same shared-secret credential mode.
