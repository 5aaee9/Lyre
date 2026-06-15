# Server Media Runtime Pump Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automatically drain decoded PCM from negotiated server-media sessions into `WebMediaRuntime`.

**Architecture:** Add a focused `ServerMediaRuntimePump` in `lyre-web` that owns one Tokio task per `ServerMediaSessionKey`. The task repeatedly calls the existing single-session `server_media_runtime::process_pcm_frames`, sleeps when idle or after relay errors, and exits when its `tokio_util::sync::CancellationToken` is cancelled. `AppState` starts/replaces pumps after successful server-media negotiation and stops pumps when room media is stopped or server-media sessions are closed. Stop paths cancel tokens and let tasks exit naturally; tests use a test-only await helper to prove stopped tasks finish.

**Tech Stack:** Rust, Tokio tasks, DashMap, existing `lyre-webrtc` negotiator/session DTOs, existing `WebMediaRuntime`.

---

## Reviewed Spec

Implement against:

- `docs/superpowers/specs/2026-06-15-server-media-runtime-pump-design.md`

Boundaries:

- No public REST/debug endpoints for pump state.
- No DeepFilterNet, jitter buffering, packet loss concealment, RTP/RTCP egress, browser playback, or frontend server-media mode.
- Add `tokio-util` as a workspace dependency and use `tokio_util::sync::CancellationToken` for graceful task cancellation.
- Every changed Rust file must remain under 400 lines.

## File Structure

- Create `crates/lyre-web/src/server_media_runtime_pump.rs`: pump struct, task lifecycle, room/key cancellation, and small unit tests.
- Create `crates/lyre-web/src/server_media_runtime_pump_tests.rs`: AppState lifecycle and real WebRTC pump integration tests.
- Modify `crates/lyre-web/src/lib.rs`: declare new module and test module.
- Modify `crates/lyre-web/src/api.rs`: add pump field to `AppState` and construct it.
- Modify `crates/lyre-web/src/api_server_media_state.rs`: start/stop pumps from AppState server-media lifecycle methods and expose test-only pump count.
- Modify `crates/lyre-web/src/api_server_media_tests.rs`: assert no public pump route exists and existing route tests account for pump count where useful.
- Modify `crates/lyre-web/src/server_media_runtime_tests.rs`: keep existing manual-drain tests deterministic after `AppState::answer_server_media_offer` starts pumps automatically.
- Modify `crates/lyre-webrtc/src/test_support.rs`: split the existing valid-Opus test helper into connect and send phases while preserving the current convenience method.
- Modify `Cargo.toml` and `crates/lyre-web/Cargo.toml`: add `tokio-util` for `CancellationToken`.
- Modify `MEMORY.md` and `docs/roadmap.md` only after independent implementation review approves.

## Task 1: Pump Lifecycle Type

**Files:**
- Create: `crates/lyre-web/src/server_media_runtime_pump.rs`
- Modify: `crates/lyre-web/src/lib.rs`

- [ ] **Step 1: Write pump lifecycle tests first**

Create `crates/lyre-web/src/server_media_runtime_pump.rs` with the skeleton below. Include tests at the bottom.

```rust
use crate::{media_runtime::WebMediaRuntime, server_media_runtime};
use dashmap::DashMap;
use lyre_core::RoomId;
use lyre_webrtc::{ServerMediaNegotiator, ServerMediaSessionKey};
use std::{sync::Arc, time::Duration};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub const SERVER_MEDIA_RUNTIME_PUMP_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug)]
pub struct ServerMediaRuntimePump {
    tasks: DashMap<ServerMediaSessionKey, ServerMediaRuntimePumpTask>,
    runtime: Arc<WebMediaRuntime>,
    negotiator: Arc<ServerMediaNegotiator>,
}

#[derive(Debug)]
struct ServerMediaRuntimePumpTask {
    token: CancellationToken,
    handle: JoinHandle<()>,
}

impl ServerMediaRuntimePump {
    pub fn new(
        runtime: Arc<WebMediaRuntime>,
        negotiator: Arc<ServerMediaNegotiator>,
    ) -> Self {
        Self {
            tasks: DashMap::new(),
            runtime,
            negotiator,
        }
    }

    pub fn start(&self, key: ServerMediaSessionKey) {
        let _ = key;
        unimplemented!("pump start is implemented after the failing lifecycle tests");
    }

    pub fn stop(&self, key: &ServerMediaSessionKey) {
        let _ = key;
        unimplemented!("pump stop is implemented after the failing lifecycle tests");
    }

    pub fn stop_room(&self, room_id: &RoomId) {
        let _ = room_id;
        unimplemented!("pump room stop is implemented after the failing lifecycle tests");
    }

    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    #[cfg(test)]
    pub async fn stop_and_wait_for_test(&self, key: &ServerMediaSessionKey) {
        let Some((_, task)) = self.tasks.remove(key) else {
            return;
        };
        task.token.cancel();
        task.handle.await.unwrap();
    }

    #[cfg(test)]
    pub async fn stop_room_and_wait_for_test(&self, room_id: &RoomId) {
        let keys = self
            .tasks
            .iter()
            .filter(|entry| &entry.key().room_id == room_id)
            .map(|entry| entry.key().clone())
            .collect::<Vec<_>>();
        for key in keys {
            self.stop_and_wait_for_test(&key).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyre_core::{MediaRelayRegistry, UserId};
    use lyre_webrtc::{ServerMediaSessionRegistry, WebRtcStack};

    fn pump() -> ServerMediaRuntimePump {
        let relays = Arc::new(MediaRelayRegistry::new());
        let runtime = Arc::new(WebMediaRuntime::new(Arc::clone(&relays)));
        let sessions = Arc::new(ServerMediaSessionRegistry::new());
        let negotiator = Arc::new(ServerMediaNegotiator::new(WebRtcStack::new(), sessions));
        ServerMediaRuntimePump::new(runtime, negotiator)
    }

    fn key(user: &str) -> ServerMediaSessionKey {
        ServerMediaSessionKey {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external(user),
        }
    }

    #[tokio::test]
    async fn start_replaces_existing_task_for_key() {
        let pump = pump();
        let key = key("user_01");

        pump.start(key.clone());
        assert_eq!(pump.task_count(), 1);
        pump.start(key.clone());
        assert_eq!(pump.task_count(), 1);

        pump.stop_and_wait_for_test(&key).await;
        assert_eq!(pump.task_count(), 0);
    }

    #[tokio::test]
    async fn stop_room_removes_only_matching_room_tasks() {
        let pump = pump();
        let default_key = key("user_01");
        let other_key = ServerMediaSessionKey {
            room_id: RoomId::parse_boundary("OTHER").unwrap(),
            user_id: UserId::from_external("user_02"),
        };

        pump.start(default_key.clone());
        pump.start(other_key.clone());
        pump.stop_room(&RoomId::default_room());

        assert_eq!(pump.task_count(), 1);
        pump.stop_and_wait_for_test(&other_key).await;
        assert_eq!(pump.task_count(), 0);
    }

    #[tokio::test]
    async fn stop_waits_for_cancelled_task_to_exit_for_tests() {
        let pump = pump();
        let key = key("user_01");

        pump.start(key.clone());
        pump.stop_and_wait_for_test(&key).await;

        assert_eq!(pump.task_count(), 0);
    }

    #[tokio::test]
    async fn stop_room_waits_for_cancelled_tasks_to_exit_for_tests() {
        let pump = pump();

        pump.start(key("user_01"));
        pump.start(key("user_02"));
        pump.stop_room_and_wait_for_test(&RoomId::default_room()).await;

        assert_eq!(pump.task_count(), 0);
    }
}
```

In `crates/lyre-web/src/lib.rs`, add:

```rust
pub mod server_media_runtime_pump;

#[cfg(test)]
mod server_media_runtime_pump_tests;
```

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p lyre-web server_media_runtime_pump::tests::
```

Expected: FAIL because `start`, `stop`, and `stop_room` are not implemented.

- [ ] **Step 3: Implement pump task lifecycle**

Replace the `start`, `stop`, and `stop_room` methods with:

```rust
    pub fn start(&self, key: ServerMediaSessionKey) {
        self.stop(&key);
        let runtime = Arc::clone(&self.runtime);
        let negotiator = Arc::clone(&self.negotiator);
        let token = CancellationToken::new();
        let task_token = token.clone();
        let task_key = key.clone();
        let handle = tokio::spawn(async move {
            loop {
                if task_token.is_cancelled() {
                    break;
                }
                if let Err(error) =
                    server_media_runtime::process_pcm_frames(&runtime, &negotiator, &task_key)
                {
                    tracing::warn!(
                        error = format_args!("{error:#}"),
                        room_id = %task_key.room_id,
                        user_id = %task_key.user_id,
                        "server media runtime pump failed to process decoded PCM batch"
                    );
                }
                tokio::select! {
                    () = task_token.cancelled() => break,
                    () = tokio::time::sleep(SERVER_MEDIA_RUNTIME_PUMP_INTERVAL) => {}
                }
            }
        });
        self.tasks
            .insert(key, ServerMediaRuntimePumpTask { token, handle });
    }

    pub fn stop(&self, key: &ServerMediaSessionKey) {
        if let Some((_, task)) = self.tasks.remove(key) {
            task.token.cancel();
            tokio::spawn(async move {
                if let Err(error) = task.handle.await {
                    tracing::debug!(
                        error = format_args!("{error:#}"),
                        "server media runtime pump task ended with join error"
                    );
                }
            });
        }
    }

    pub fn stop_room(&self, room_id: &RoomId) {
        let keys = self
            .tasks
            .iter()
            .filter(|entry| &entry.key().room_id == room_id)
            .map(|entry| entry.key().clone())
            .collect::<Vec<_>>();
        for key in keys {
            self.stop(&key);
        }
    }
```

Also update dependencies:

```toml
# Cargo.toml [workspace.dependencies]
tokio-util = "0.7"

# crates/lyre-web/Cargo.toml [dependencies]
tokio-util.workspace = true
```

- [ ] **Step 4: Run pump lifecycle tests**

Run:

```bash
cargo test -p lyre-web server_media_runtime_pump::tests::
wc -l crates/lyre-web/src/server_media_runtime_pump.rs
```

Expected: PASS and file is under 400 LOC.

## Task 2: AppState Pump Integration

**Files:**
- Modify: `crates/lyre-web/src/api.rs`
- Modify: `crates/lyre-web/src/api_server_media_state.rs`
- Create/Modify: `crates/lyre-web/src/server_media_runtime_pump_tests.rs`

- [ ] **Step 1: Add failing AppState lifecycle tests**

Create `crates/lyre-web/src/server_media_runtime_pump_tests.rs`:

```rust
use crate::api::AppState;
use lyre_core::{RoomId, UserId};
use lyre_webrtc::{ServerMediaOffer, ServerMediaSessionKey, WebRtcStack};

async fn offer_sdp() -> String {
    let offerer = WebRtcStack::new().create_peer_connection().await.unwrap();
    offerer.create_local_offer_for_test().await.unwrap()
}

fn key() -> ServerMediaSessionKey {
    ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
    }
}

async fn answer_offer(state: &AppState, track_id: &str) {
    let key = key();
    state
        .answer_server_media_offer(ServerMediaOffer {
            room_id: key.room_id,
            user_id: key.user_id,
            audio_track_id: track_id.to_owned(),
            sdp: offer_sdp().await,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn successful_offer_starts_and_replaces_runtime_pump() {
    let state = AppState::default();

    answer_offer(&state, "audio-main").await;
    assert_eq!(state.server_media_runtime_pump_count(), 1);
    answer_offer(&state, "audio-retry").await;
    assert_eq!(state.server_media_runtime_pump_count(), 1);
}

#[tokio::test]
async fn failed_offer_does_not_start_runtime_pump() {
    let state = AppState::default();
    let key = key();

    let result = state
        .answer_server_media_offer(ServerMediaOffer {
            room_id: key.room_id,
            user_id: key.user_id,
            audio_track_id: "audio-main".to_owned(),
            sdp: "not sdp".to_owned(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(state.server_media_runtime_pump_count(), 0);
}

#[tokio::test]
async fn room_close_and_media_relay_stop_cancel_runtime_pumps() {
    let state = AppState::default();
    let key = key();

    answer_offer(&state, "audio-main").await;
    assert_eq!(state.server_media_runtime_pump_count(), 1);
    state.close_server_media_sessions_for_room(&RoomId::default_room());
    assert_eq!(state.server_media_runtime_pump_count(), 0);

    answer_offer(&state, "audio-main").await;
    assert_eq!(state.server_media_runtime_pump_count(), 1);
    state.stop_media_relay(
        key.room_id,
        lyre_core::StopMediaRelayRequest {
            user_id: key.user_id,
        },
    );
    assert_eq!(state.server_media_runtime_pump_count(), 0);
}
```

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
cargo test -p lyre-web server_media_runtime_pump_tests::successful_offer_starts_and_replaces_runtime_pump
```

Expected: FAIL because `AppState` has no pump field/count integration yet.

- [ ] **Step 3: Add pump field to AppState**

In `crates/lyre-web/src/api.rs`, import:

```rust
server_media_runtime_pump::ServerMediaRuntimePump,
```

Add to `AppState`:

```rust
pub server_media_runtime_pump: Arc<ServerMediaRuntimePump>,
```

In `AppState::new`, after constructing `server_media_negotiator`, construct:

```rust
let media_runtime = Arc::new(WebMediaRuntime::new(Arc::clone(&media_relays)));
let server_media_runtime_pump = Arc::new(ServerMediaRuntimePump::new(
    Arc::clone(&media_runtime),
    Arc::clone(&server_media_negotiator),
));
```

Then store `media_runtime` and `server_media_runtime_pump` in `Self`.

- [ ] **Step 4: Start and stop pumps from AppState lifecycle**

In `crates/lyre-web/src/api_server_media_state.rs`, update `answer_server_media_offer`:

```rust
    pub async fn answer_server_media_offer(
        &self,
        offer: ServerMediaOffer,
    ) -> Result<ServerMediaAnswer, ServerMediaNegotiationError> {
        let answer = self.server_media_negotiator.answer_offer(offer).await?;
        self.server_media_runtime_pump
            .start(ServerMediaSessionKey {
                room_id: answer.room_id.clone(),
                user_id: answer.user_id.clone(),
            });
        Ok(answer)
    }
```

Update `close_server_media_sessions_for_room` so pump cancellation/removal happens before peer handles are removed:

```rust
self.server_media_runtime_pump.stop_room(room_id);
self.server_media_negotiator.close_room(room_id);
```

Add test-only count method:

```rust
    #[cfg(test)]
    pub fn server_media_runtime_pump_count(&self) -> usize {
        self.server_media_runtime_pump.task_count()
    }
```

- [ ] **Step 5: Run AppState lifecycle tests and LOC**

Run:

```bash
cargo test -p lyre-web server_media_runtime_pump_tests::
wc -l crates/lyre-web/src/api.rs crates/lyre-web/src/api_server_media_state.rs crates/lyre-web/src/server_media_runtime_pump.rs crates/lyre-web/src/server_media_runtime_pump_tests.rs
```

Expected: tests PASS and all files are under 400 LOC.

## Task 3: Preserve Existing Manual Runtime Tests

**Files:**
- Modify: `crates/lyre-web/src/server_media_runtime_tests.rs`

- [ ] **Step 1: Update old real WebRTC manual-drain tests**

After `AppState::answer_server_media_offer` starts a pump automatically, the existing manual-drain real WebRTC tests in `server_media_runtime_tests.rs` must no longer negotiate through `AppState::answer_server_media_offer`, because the background pump can drain decoded PCM before the test does.

Update these tests before adding the new automatic pump tests:

- `app_state_processes_real_drained_server_media_pcm_batch`
- `app_state_discards_real_drained_server_media_pcm_batch_on_error`

Keep their intent deterministic by negotiating directly through `state.server_media_negotiator.answer_offer(...)` and `state.server_media_negotiator.add_remote_ice_candidate(...)`, then continue using the existing manual `drain_server_media_pcm_frames` / `process_server_media_pcm_frames` assertions. This keeps the unit-level runtime behavior covered without auto-starting a pump.

- [ ] **Step 2: Run updated manual runtime tests**

Run:

```bash
cargo test -p lyre-web server_media_runtime_tests::app_state_processes_real_drained_server_media_pcm_batch
cargo test -p lyre-web server_media_runtime_tests::app_state_discards_real_drained_server_media_pcm_batch_on_error
wc -l crates/lyre-web/src/server_media_runtime_tests.rs
```

Expected: both tests PASS and the file remains under 400 LOC.

## Task 4: Automatic Real PCM Processing

**Files:**
- Modify: `crates/lyre-webrtc/src/test_support.rs`
- Modify: `crates/lyre-web/src/server_media_runtime_pump_tests.rs`

- [ ] **Step 1: Split test-support Opus helper into connect and send phases**

In `crates/lyre-webrtc/src/test_support.rs`, keep the existing `ServerMediaTestOffer::accept_answer_and_send_valid_opus` public method but implement it in terms of two new public test-support methods:

`test_support.rs` is currently close enough to the 400 LOC limit that the implementer must check it after edits. If it would exceed 400 LOC, split the helper into a small `test_support` module tree (for example `test_support/mod.rs` plus `test_support/opus.rs`) instead of leaving an over-limit file.

```rust
    pub async fn accept_answer(
        mut self,
        answer: &ServerMediaAnswer,
        server_candidates: Vec<ServerMediaIceCandidate>,
    ) -> ServerMediaConnectedOffer {
        let answer =
            webrtc::peer_connection::RTCSessionDescription::answer(answer.sdp.clone()).unwrap();
        self.offerer.set_remote_description(answer).await.unwrap();

        for candidate in server_candidates {
            if candidate.candidate.is_empty() {
                continue;
            }
            self.offerer
                .add_ice_candidate(to_webrtc_candidate(ServerMediaIceCandidateInit {
                    candidate: candidate.candidate,
                    sdp_mid: candidate.sdp_mid,
                    sdp_mline_index: candidate.sdp_mline_index,
                    username_fragment: candidate.username_fragment,
                }))
                .await
                .unwrap();
        }

        let _ = wait_for_connected(&mut self.connected_rx).await;
        ServerMediaConnectedOffer {
            _offerer: self.offerer,
            track: self.track,
        }
    }
```

Add:

```rust
pub struct ServerMediaConnectedOffer {
    _offerer: Arc<dyn PeerConnection>,
    track: Arc<TrackLocalStaticRTP>,
}

impl ServerMediaConnectedOffer {
    pub async fn send_valid_opus_packets(&self, count: usize) {
        let payload = encoded_opus_payload();
        for _ in 0..count {
            let _ = self.track.write_rtp(test_rtp_packet(payload.clone())).await;
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }
}
```

Then make `accept_answer_and_send_valid_opus` call `self.accept_answer(...).await.send_valid_opus_packets(100).await`.

- [ ] **Step 2: Add real pump processing test**

Append to `server_media_runtime_pump_tests.rs`:

```rust
use lyre_core::{MediaTrackKind, NoiseCancellationConfig, NoiseProvider, RegisterMediaTrackRequest, StartMediaRelayRequest};

#[tokio::test]
async fn runtime_pump_processes_real_decoded_pcm_without_manual_drain() {
    let state = AppState::default();
    let key = key();
    state.media_relays.start(
        key.room_id.clone(),
        StartMediaRelayRequest {
            noise: Some(NoiseCancellationConfig {
                provider: NoiseProvider::Rnnoise,
                intensity: 0.5,
                voice_activity_threshold: 0.35,
            }),
        },
    );
    state
        .media_relays
        .register_track(
            key.room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: key.user_id.clone(),
                track_id: "audio".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();

    let offer = lyre_webrtc::test_support::server_media_offer_with_valid_opus_sender().await;
    let answer = state
        .answer_server_media_offer(ServerMediaOffer {
            room_id: key.room_id.clone(),
            user_id: key.user_id.clone(),
            audio_track_id: "audio-main".to_owned(),
            sdp: offer.offer_sdp.clone(),
        })
        .await
        .unwrap();
    for candidate in offer.remote_candidates().await {
        state
            .add_server_media_ice_candidate(lyre_webrtc::ServerMediaIceCandidate {
                room_id: key.room_id.clone(),
                user_id: key.user_id.clone(),
                candidate: candidate.candidate,
                sdp_mid: candidate.sdp_mid,
                sdp_mline_index: candidate.sdp_mline_index,
                username_fragment: candidate.username_fragment,
            })
            .await
            .unwrap();
    }
    offer
        .accept_answer_and_send_valid_opus(&answer, state.server_media_ice_candidates(&key))
        .await;

    for _ in 0..150 {
        let frames = state.processed_media_frames(&key.room_id);
        if frames.iter().any(|frame| {
            frame.user_id == key.user_id
                && frame.track_id == "audio"
                && frame.sequence == 42
                && frame.noise.provider == NoiseProvider::Rnnoise
                && frame.samples.len() == lyre_webrtc::SERVER_MEDIA_OPUS_FRAME_SIZE
        }) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("server media runtime pump did not process decoded PCM");
}
```

- [ ] **Step 3: Add delayed relay/track registration test**

Append:

```rust
#[tokio::test]
async fn runtime_pump_processes_after_delayed_relay_and_track_registration() {
    let state = AppState::default();
    let key = key();

    let offer = lyre_webrtc::test_support::server_media_offer_with_valid_opus_sender().await;
    let answer = state
        .answer_server_media_offer(ServerMediaOffer {
            room_id: key.room_id.clone(),
            user_id: key.user_id.clone(),
            audio_track_id: "audio-main".to_owned(),
            sdp: offer.offer_sdp.clone(),
        })
        .await
        .unwrap();
    for candidate in offer.remote_candidates().await {
        state
            .add_server_media_ice_candidate(lyre_webrtc::ServerMediaIceCandidate {
                room_id: key.room_id.clone(),
                user_id: key.user_id.clone(),
                candidate: candidate.candidate,
                sdp_mid: candidate.sdp_mid,
                sdp_mline_index: candidate.sdp_mline_index,
                username_fragment: candidate.username_fragment,
            })
            .await
            .unwrap();
    }
    let connected = offer
        .accept_answer(&answer, state.server_media_ice_candidates(&key))
        .await;

    tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    assert!(state.processed_media_frames(&key.room_id).is_empty());

    state
        .media_relays
        .start(key.room_id.clone(), StartMediaRelayRequest::default());
    state
        .media_relays
        .register_track(
            key.room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: key.user_id.clone(),
                track_id: "audio".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();

    connected.send_valid_opus_packets(100).await;

    for _ in 0..150 {
        if !state.processed_media_frames(&key.room_id).is_empty() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("server media runtime pump did not process after delayed relay registration");
}
```

- [ ] **Step 4: Run real pump tests**

Run:

```bash
cargo test -p lyre-webrtc test_support
cargo test -p lyre-web server_media_runtime_pump_tests::runtime_pump_processes_real_decoded_pcm_without_manual_drain
cargo test -p lyre-web server_media_runtime_pump_tests::runtime_pump_processes_after_delayed_relay_and_track_registration
```

Expected: PASS.

## Task 5: Public Route Guard

**Files:**
- Modify: `crates/lyre-web/src/api_server_media_tests.rs`

- [ ] **Step 1: Add no public pump/debug route tests**

Append:

```rust
#[tokio::test]
async fn server_media_runtime_pump_route_does_not_exist() {
    let app = router(AppState::default());

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/rooms/DEFAULT/server-media/pump?user_id=user_01")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn server_media_decode_failures_route_does_not_exist() {
    let app = router(AppState::default());

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/rooms/DEFAULT/server-media/decode-failures?user_id=user_01")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn server_media_debug_route_does_not_exist() {
    let app = router(AppState::default());

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/rooms/DEFAULT/server-media/debug?user_id=user_01")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
```

- [ ] **Step 2: Run route tests and LOC**

Run:

```bash
cargo test -p lyre-web api_server_media_tests::server_media_runtime_pump_route_does_not_exist
cargo test -p lyre-web api_server_media_tests::server_media_decode_failures_route_does_not_exist
cargo test -p lyre-web api_server_media_tests::server_media_debug_route_does_not_exist
wc -l crates/lyre-web/src/api_server_media_tests.rs
```

Expected: PASS and file remains under 400 LOC.

## Task 6: Verification and Implementation Review Gate

**Files:**
- No docs changes in this task.
- No product code changes unless verification exposes a defect.

- [ ] **Step 1: Run Rust checks**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: PASS.

- [ ] **Step 2: Run frontend checks**

Run:

```bash
cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build
```

Expected: PASS.

- [ ] **Step 3: Run static checks**

Run:

```bash
git diff --check
wc -l crates/lyre-web/src/api.rs crates/lyre-web/src/api_server_media_state.rs crates/lyre-web/src/server_media_runtime_pump.rs crates/lyre-web/src/server_media_runtime_pump_tests.rs crates/lyre-web/src/server_media_runtime_tests.rs crates/lyre-web/src/api_server_media_tests.rs crates/lyre-webrtc/src/test_support.rs
```

Expected: `git diff --check` exits 0 and all listed Rust files are below 400 lines.

- [ ] **Step 4: Dispatch independent implementation review**

Before updating docs, dispatch an independent implementation reviewer with:

- reviewed spec path,
- this reviewed plan path,
- full diff,
- verification output from Steps 1-3,
- SDD implementation verdict format.

Expected: reviewer returns `VERDICT: APPROVE`. If it returns `REVISE`, fix gaps, rerun relevant verification, and re-review.

## Task 7: Post-Review Documentation, Final Verification, Commit, and Push

**Files:**
- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`
- Modify only docs after Task 6 receives implementation `VERDICT: APPROVE`.

- [ ] **Step 1: Update `MEMORY.md`**

Add:

```markdown
## 2026-06-15 Server Media Runtime Pump

- Added an internal `lyre-web` runtime pump that starts after successful server-media negotiation and automatically drains decoded PCM into `WebMediaRuntime`.
- Pump tasks are keyed by room/user server-media session, replaced on renegotiation, and cancelled when server-media sessions or media relays are stopped for a room.
- The pump keeps polling through inactive relay or missing track errors so relay/track registration can arrive after negotiation.
- No public pump, raw RTP, decoded PCM, or decode-failure endpoint was added; RTP/RTCP egress and browser playback remain future work.
```

- [ ] **Step 2: Update `docs/roadmap.md`**

Move automatic server-media draining/processing into Completed. Keep Next focused on DeepFilterNet, jitter/PLC, RTP/RTCP egress, browser playback, frontend server-media mode, and persistence/auth/observability.

- [ ] **Step 3: Run final verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build
git diff --check
wc -l crates/lyre-web/src/api.rs crates/lyre-web/src/api_server_media_state.rs crates/lyre-web/src/server_media_runtime_pump.rs crates/lyre-web/src/server_media_runtime_pump_tests.rs crates/lyre-web/src/server_media_runtime_tests.rs crates/lyre-web/src/api_server_media_tests.rs crates/lyre-webrtc/src/test_support.rs
```

Expected: all checks PASS and all listed Rust files stay below 400 lines.

- [ ] **Step 4: Review final diff**

Run:

```bash
git status --short
git diff --stat
git diff -- Cargo.toml Cargo.lock crates/lyre-web/Cargo.toml crates/lyre-web/src/api.rs crates/lyre-web/src/api_server_media_state.rs crates/lyre-web/src/lib.rs crates/lyre-web/src/server_media_runtime_pump.rs crates/lyre-web/src/server_media_runtime_pump_tests.rs crates/lyre-web/src/server_media_runtime_tests.rs crates/lyre-web/src/api_server_media_tests.rs crates/lyre-webrtc/src/test_support.rs MEMORY.md docs/roadmap.md
```

Expected: only intended files changed.

- [ ] **Step 5: Commit and push**

Commit with Lore protocol:

```bash
git add Cargo.toml Cargo.lock crates/lyre-web/Cargo.toml crates/lyre-web/src/api.rs crates/lyre-web/src/api_server_media_state.rs crates/lyre-web/src/lib.rs crates/lyre-web/src/server_media_runtime_pump.rs crates/lyre-web/src/server_media_runtime_pump_tests.rs crates/lyre-web/src/server_media_runtime_tests.rs crates/lyre-web/src/api_server_media_tests.rs crates/lyre-webrtc/src/test_support.rs MEMORY.md docs/roadmap.md
git commit -m "Pump server media PCM into runtime automatically" -m "Constraint: Server-media decoded PCM must enter the noise-cancelling runtime without test-only manual drain calls.
Rejected: Exposing a pump/debug endpoint | The pump is an internal lifecycle concern and not product API.
Confidence: medium
Scope-risk: moderate
Directive: Keep browser playback and processed RTP/RTCP egress separate; this only moves decoded PCM into the runtime.
Tested: cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path Cargo.toml --workspace; frontend generate/test/typecheck/lint/build; git diff --check; LOC check
Not-tested: Browser playback of server-processed audio; RTP/RTCP egress; DeepFilterNet processing"
git push
```
