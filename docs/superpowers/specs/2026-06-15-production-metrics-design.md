# Production Metrics Design

## Scope

Add a lightweight production observability endpoint to the Rust API server. This increment exposes a Prometheus-compatible text endpoint at `GET /metrics` with aggregate process-local counters and gauges for room presence and server-media runtime state.

This increment does not add a metrics collector dependency, request latency histograms, frontend metrics, authentication for metrics, distributed metrics aggregation, or external observability deployment config.

## Goals

- Operators can scrape a stable `/metrics` endpoint from the `lyre-api` image.
- Metrics are aggregate only and never expose room IDs, user IDs, access tokens, nicknames, SDP, ICE candidates, RTP payloads, or persistence file paths.
- The endpoint reports current in-memory state plus monotonic join/leave/persistence-failure counters for the running process.
- Metrics generation is read-only and must not create rooms or mutate media relay state.

## Metrics

The endpoint returns `text/plain; version=0.0.4` Prometheus exposition text. It must include `# HELP` and `# TYPE` lines for each metric.

Gauges:

- `lyre_rooms_total`: number of rooms currently known by the room registry.
- `lyre_users_total`: number of users currently joined across all known rooms.
- `lyre_media_relays_active`: number of active media relay rooms.
- `lyre_media_relay_participants_total`: number of participants currently registered in active media relays.
- `lyre_server_media_sessions_active`: number of active server-media sessions.
- `lyre_server_media_runtime_pumps_active`: number of active server-media runtime pump tasks.
- `lyre_processed_audio_egress_pumps_active`: number of active processed-audio WebRTC egress pump tasks.

Counters:

- `lyre_room_joins_total`: number of successful room joins since process start.
- `lyre_room_leaves_total`: number of successful room leave mutations since process start.
- `lyre_room_state_persistence_failures_total`: number of failed room state persistence writes since process start.

## State Model

Add a small metrics state object owned by `lyre-web::AppState`. It stores only `AtomicU64` counters. Current gauges should be computed from existing registries when `/metrics` is requested.

`join_room_persisted` increments `lyre_room_joins_total` only after the join has succeeded and any configured persistence write has succeeded. If persistence write fails and the in-memory room registry is rolled back, the join counter must not increment and the persistence failure counter must increment.

`leave_room_persisted` increments `lyre_room_leaves_total` only after an authorized leave actually removes an existing user and any configured persistence write has succeeded. Unauthorized leaves, no-op leaves, and failed-persistence leaves must not increment the leave counter. If persistence write fails and the rollback path runs after a real removal, the leave counter must not increment and the persistence failure counter must increment.

In-memory mode with no persistence increments join and leave counters after the in-memory mutation returns.

## Registry Snapshot Requirements

Add read-only aggregate snapshot methods where needed instead of making metrics inspect private map fields directly:

- `RoomRegistry` should expose a snapshot with room and user counts without creating missing rooms.
- `MediaRelayRegistry` should expose active room and active participant counts without creating missing rooms.

Server-media session and pump counts can reuse existing internal counts or expose non-test `AppState` wrappers if needed.

## File Layout

Keep metrics code out of already-large API modules:

- Add metrics rendering, snapshot composition, and the Axum handler in a focused `lyre-web` module such as `crates/lyre-web/src/metrics.rs`.
- Add metrics route wiring to `crates/lyre-web/src/api.rs` with only the minimal route and module call needed.
- Add focused tests in a separate `crates/lyre-web/src/metrics_tests.rs` module.
- Add only small read-only aggregate types/methods to `lyre-core` registries. If a touched file is already over the repository's 400 LOC split threshold, do not append broad metrics rendering or test code to it.

## API Behavior

- `GET /metrics` returns `200 OK`.
- The response body is deterministic enough for tests: every metric appears once and values are integer samples.
- The route is unauthenticated in this increment. Deployments that need restricted metrics should protect it at the reverse proxy or ingress layer.
- Metrics output must not include labels in this increment.

## Tests

Add Rust tests for:

- `/metrics` exists, returns Prometheus text content type, and includes expected metric names.
- Joining and leaving users changes room/user gauges and increments the join/leave counters.
- Failed persistence writes increment `lyre_room_state_persistence_failures_total` without incrementing the corresponding join/leave counter.
- Metrics generation is non-mutating: create an empty `AppState`, call `GET /metrics`, then verify the new read-only `RoomRegistry` aggregate snapshot still reports zero rooms. Do not use `RoomRegistry::snapshot()` or any public room route for this assertion because those paths create room entries.

Run:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --manifest-path "Cargo.toml" --workspace`

## Documentation

Update:

- `MEMORY.md` with the design decision that metrics are aggregate, process-local, and privacy-preserving.
- `docs/roadmap.md` to move production observability/metrics from Next to Completed and keep richer observability work as future follow-up only if needed.
