# Room State Persistence Design

## Scope

This increment adds optional file-backed persistence for the anonymous room registry. It persists the data required for existing anonymous browser sessions to survive an API process restart: rooms, users, user noise settings, joined timestamps, and room access tokens.

Persistence is disabled by default. Operators enable it with a single JSON state file path.

## Goals

- Preserve joined anonymous users and their access tokens across API restarts when persistence is configured.
- Let a browser tab that still has `sessionStorage["lyre.roomSession"]` reconnect to the same room after the backend restarts.
- Keep public room snapshots and protected route authorization behavior unchanged.
- Keep the persistence implementation local and dependency-light; do not introduce a database in this increment.

## Non-Goals

- No accounts, passwords, roles, room ownership, refresh tokens, or user identity beyond existing anonymous sessions.
- No cross-process concurrent writers to the same state file.
- No automatic stale user expiration or online/offline presence semantics.
- No persistence for WebSocket peer handles, WebRTC sessions, relay pumps, processed audio buffers, TURN state, or media runtime state.
- No frontend UI changes.

## Configuration

`lyre serve` gains:

- `--state-file <path>`
- `LYRE_STATE_FILE=<path>`

When unset, Lyre keeps the current in-memory-only behavior. When set, server startup loads the JSON file if it exists. A missing file starts from an empty registry only when the configured parent directory exists or the path has no explicit parent directory. A missing parent directory fails startup with context instead of silently accepting a misconfigured state path.

Blank paths are rejected during CLI config resolution. Invalid JSON or invalid persisted room ids fail startup with the lower-level parse/source error preserved in the error chain. Persisted user ids and access tokens are accepted as opaque external strings because the current `UserId` and `RoomAccessToken` boundary types do not have additional parse-time validity rules.

## Persisted Data Model

The persisted file is a JSON document owned by Lyre:

```json
{
  "rooms": [
    {
      "room_id": "DEFAULT",
      "users": [
        {
          "profile": {
            "id": "user_...",
            "nickname": "Ada",
            "joined_at": "2026-06-15T00:00:00Z",
            "noise": {
              "provider": "rnnoise",
              "intensity": 0.8,
              "voice_activity_threshold": 0.35
            }
          },
          "access_token": "opaque-token"
        }
      ]
    }
  ]
}
```

Access tokens are intentionally present in the state file because they are the bearer secret needed to restore anonymous sessions. They remain server-private: they are not added to `UserProfile`, `RoomSnapshot`, signalling payloads, public API responses other than successful join, or logs.

The schema does not need a public WebRPC change because the file format is not an API contract.

## Core Registry Boundary

`lyre-core` owns serialization of the room registry state but not filesystem I/O. Add Lyre-owned DTOs for persisted rooms and users, plus these registry operations:

- construct a registry from persisted state;
- export the current registry to persisted state;
- preserve access-token validation behavior after restore;
- preserve deterministic room snapshot sorting after restore.

The core loader validates state at the same trust boundary as other external inputs:

- room ids are parsed through `RoomId::parse_boundary`;
- user ids and access tokens are treated as external strings;
- duplicate user ids within a room resolve to the last entry in file order, matching current `DashMap::insert` replacement behavior.
- replacing the full registry from a persisted snapshot is supported so the web layer can roll back a failed persisted mutation.

## Web Persistence Boundary

`lyre-web` owns file I/O through a small persistence wrapper attached to `AppState`. It loads configured state before constructing the router and persists room registry mutations:

- `POST /api/rooms/{room_id}/join`;
- protected `POST /api/rooms/{room_id}/leave`.

When persistence is enabled, join and leave mutations are serialized with an `AppState`-owned async mutex. Each persisted mutation follows this order:

1. export the current registry snapshot as rollback state;
2. apply the join or leave mutation in memory;
3. write the new persisted snapshot;
4. on write success, emit presence events and return the normal response;
5. on write failure, restore the rollback snapshot and return `500`.

This keeps failed joins from returning newly generated tokens and keeps failed leaves from resurrecting departed tokens after restart. Runtime persistence failures log the full lower-level error chain server-side and return a generic `500` JSON error body. The response must not include access tokens, state-file paths, or internal error details except for the normal access token in a successful join response.

The persisted file is written atomically using a collision-safe sibling temporary path unique to the current process and write attempt, followed by rename. Parent directories are not auto-created. A missing parent directory during startup load remains a deployment/configuration error surfaced with context through the server startup error chain; a missing parent encountered during a request follows the generic runtime failure response described above. Cross-process concurrent writers to the same state file remain unsupported.

## Testing

Core tests:

- exporting an empty registry produces no rooms;
- joined users export with access tokens;
- restoring persisted state makes snapshots contain users but not access tokens;
- restored access tokens validate for the restored room/user and reject wrong users;
- duplicate persisted users in one room use the last entry.

Web/API tests:

- a router created from a state file sees persisted users in `GET /api/rooms/{room_id}`;
- a restored token authorizes `POST /leave`;
- successful join writes a JSON file containing the joined user and token;
- successful leave rewrites the file without the departed user/token;
- failed persisted join rolls back the in-memory user and returns a server error without an access token;
- failed persisted leave rolls back the in-memory user/token and returns a server error;
- runtime persistence error responses do not include state-file paths or lower-level filesystem details;
- malformed state file fails server state construction with parse context.

CLI tests:

- `--state-file` and `LYRE_STATE_FILE` resolve into `ServeConfig`;
- blank state file paths are rejected;
- explicit CLI path takes precedence over env.

## Documentation

Update `MEMORY.md` with the decision to use optional JSON file persistence for anonymous sessions and to keep media/WebRTC runtime state in memory.

Update `docs/roadmap.md` to move persistent room/user/session state from Next to Completed and keep production-grade database/session management out of scope for this milestone.
