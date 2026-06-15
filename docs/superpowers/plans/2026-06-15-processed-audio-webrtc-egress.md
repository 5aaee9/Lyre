# Processed Audio WebRTC Egress Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Encode processed server audio to Opus RTP and write it to negotiated server-media WebRTC peers through an internal room egress pump.

**Architecture:** Extend `lyre-webrtc` with a local server-to-client Opus track and per-peer Opus/RTP egress state, then add a `lyre-web` room pump that subscribes to processed audio frames, uses existing fanout rules, and sends frames to recipient server-media sessions. Keep the path internal and test-visible only; do not add public RTP/debug endpoints or frontend server-media mode.

**Reviewed Spec:** `docs/superpowers/specs/2026-06-15-processed-audio-webrtc-egress-design.md`

---

## File Structure

- Modify `crates/lyre-webrtc/src/stack.rs`: add local Opus track, egress DTO/error types, per-peer encoder/RTP state, and test-only sent packet snapshots.
- Modify `crates/lyre-webrtc/src/negotiation.rs`: expose `send_processed_audio_frame` through `ServerMediaNegotiator`.
- Modify `crates/lyre-webrtc/src/lib.rs`: export new egress DTO/error types.
- Modify `crates/lyre-webrtc/src/stack_media_tests.rs`: add direct peer egress tests.
- Modify `crates/lyre-webrtc/src/negotiation_tests.rs`: add negotiator egress tests.
- Create `crates/lyre-web/src/processed_audio_webrtc_egress_pump.rs`: room pump lifecycle and send loop.
- Create `crates/lyre-web/src/processed_audio_webrtc_egress_pump_tests.rs`: AppState/pump lifecycle and real fanout integration tests.
- Modify `crates/lyre-web/src/lib.rs`: declare new module/tests.
- Modify `crates/lyre-web/src/api.rs`: add pump field, construct it, and make `start_media_relay` an AppState method that starts the pump.
- Modify `crates/lyre-web/src/api_server_media_state.rs`: stop egress pump on room server-media close.
- Modify existing media relay route/tests to use `AppState::start_media_relay`.
- Modify `MEMORY.md` and `docs/roadmap.md` only after independent implementation review approves.
- Create a local commit after final verification. The leader may push afterward only when operating under the thread's standing goal/workflow authority; implementation workers must not push.

Keep all changed Rust files under 400 LOC. If `stack.rs` would exceed 400 LOC, split egress code into a new `crates/lyre-webrtc/src/egress.rs` and export from `lib.rs`.

## Task 1: WebRTC Egress DTOs and Encoder

**Files:**
- Modify or create: `crates/lyre-webrtc/src/egress.rs`
- Modify: `crates/lyre-webrtc/src/lib.rs`

- [ ] **Step 1: Add failing unit tests for egress frame validation and Opus packetization**

Create a focused egress module if needed:

```rust
pub const SERVER_MEDIA_EGRESS_PAYLOAD_TYPE: u8 = 111;
pub const SERVER_MEDIA_EGRESS_SAMPLE_RATE_HZ: u32 = 48_000;
pub const SERVER_MEDIA_EGRESS_CHANNELS: u16 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct ServerMediaProcessedAudioFrame {
    pub sequence: u64,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerMediaEgressRtpPacket {
    pub sequence_number: u16,
    pub timestamp: u32,
    pub payload_type: u8,
    pub payload: Vec<u8>,
}
```

Add an internal `ServerMediaOpusEgressEncoder` that validates 48 kHz mono frames and encodes 960-sample chunks to Opus payloads. Tests should cover valid 960-sample frame, invalid sample rate, invalid channels, empty samples, and non-960 multiple sample count.

- [ ] **Step 2: Implement encoder and errors**

Add:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ServerMediaEgressError {
    #[error("server media egress requires 48 kHz audio, got {sample_rate_hz} Hz")]
    InvalidSampleRate { sample_rate_hz: u32 },
    #[error("server media egress requires mono audio, got {channels} channels")]
    InvalidChannels { channels: u16 },
    #[error("server media egress requires non-empty 20 ms Opus frame chunks")]
    InvalidFrameSize { samples: usize },
    #[error("failed to initialize server media Opus egress encoder")]
    EncoderInit { #[source] source: opus_rs::OpusError },
    #[error("failed to encode server media Opus egress frame")]
    Encode { #[source] source: opus_rs::OpusError },
    #[error("failed to write server media egress RTP packet")]
    WriteRtp { #[source] source: Box<dyn std::error::Error + Send + Sync> },
    #[error("server media egress peer is missing for room `{room_id}` user `{user_id}`")]
    PeerMissing { room_id: lyre_core::RoomId, user_id: lyre_core::UserId },
}
```

Keep lower-level sources in error variants. Export public DTO/error types from `lib.rs`.

- [ ] **Step 3: Run tests and LOC**

Run:

```bash
cargo test -p lyre-webrtc egress
wc -l crates/lyre-webrtc/src/stack.rs crates/lyre-webrtc/src/egress.rs crates/lyre-webrtc/src/lib.rs
```

Expected: PASS and all listed Rust files under 400 LOC.

## Task 2: Local WebRTC Audio Track Sending

**Files:**
- Modify: `crates/lyre-webrtc/src/stack.rs`
- Modify: `crates/lyre-webrtc/src/stack_media_tests.rs`

- [ ] **Step 1: Add failing direct peer egress tests**

Add tests that:

- create a server peer and client offer with audio support,
- verify the answer SDP advertises a server-sendable audio m-line,
- call `WebRtcPeerConnectionHandle::send_processed_audio_frame`,
- assert a test-only `sent_egress_rtp_packets_for_test()` snapshot includes payload type 111, monotonic sequence/timestamp, and non-empty payload,
- assert invalid frame shape returns `ServerMediaEgressError`.

Use the existing `opus_offerer` test helper patterns. It is acceptable for this increment to verify server-side sent RTP snapshots instead of browser playback.

- [ ] **Step 2: Implement local track and send method**

In `WebRtcStack::create_peer_connection`:

- create an `Arc<TrackLocalStaticRTP>` Opus audio track,
- add it to the peer connection with `add_track`,
- do not add an extra independent recvonly audio transceiver after adding the local audio track,
- rely on the browser offer's audio m-line plus the added local audio track to negotiate a single intended sendrecv audio path,
- preserve incoming Opus RTP decode through the existing `on_track` handler,
- add a regression assertion that the answer SDP exposes one audio media section and can receive browser Opus while writing server egress RTP,
- store the local track plus egress state in `WebRtcPeerConnectionHandle`.

Add:

```rust
pub async fn send_processed_audio_frame(
    &self,
    frame: ServerMediaProcessedAudioFrame,
) -> Result<usize, ServerMediaEgressError>;

#[cfg(test)]
pub fn sent_egress_rtp_packets_for_test(&self) -> Vec<ServerMediaEgressRtpPacket>;
```

The send method should encode chunks, construct `rtc::rtp::Packet`s with payload type 111, write each to the local track, record test snapshots after successful write, and return the sent packet count.

- [ ] **Step 3: Run direct peer egress tests**

Run:

```bash
cargo test -p lyre-webrtc stack_media_tests::processed_audio_frame_writes_server_egress_rtp
cargo test -p lyre-webrtc stack_media_tests::processed_audio_egress_rejects_invalid_pcm_shape
wc -l crates/lyre-webrtc/src/stack.rs crates/lyre-webrtc/src/egress.rs crates/lyre-webrtc/src/stack_media_tests.rs
```

Expected: PASS and files under 400 LOC.

## Task 3: Negotiator Egress API

**Files:**
- Modify: `crates/lyre-webrtc/src/negotiation.rs`
- Modify: `crates/lyre-webrtc/src/negotiation_tests.rs`

- [ ] **Step 1: Add failing negotiator tests**

Add tests for:

- successful `send_processed_audio_frame` routes to an existing key and returns packet count,
- missing key returns `ServerMediaEgressError::PeerMissing` with room/user context,
- closing a session removes egress access.

- [ ] **Step 2: Implement negotiator send method**

Add:

```rust
pub async fn send_processed_audio_frame(
    &self,
    key: &ServerMediaSessionKey,
    frame: ServerMediaProcessedAudioFrame,
) -> Result<usize, ServerMediaEgressError>
```

Look up the peer handle, clone it, return `PeerMissing` if absent, and delegate to `WebRtcPeerConnectionHandle`.

- [ ] **Step 3: Run negotiator egress tests**

Run:

```bash
cargo test -p lyre-webrtc negotiation_tests::send_processed_audio_frame_routes_to_existing_peer
cargo test -p lyre-webrtc negotiation_tests::send_processed_audio_frame_missing_peer_returns_context
```

Expected: PASS.

## Task 4: Web Room Egress Pump

**Files:**
- Create: `crates/lyre-web/src/processed_audio_webrtc_egress_pump.rs`
- Modify: `crates/lyre-web/src/lib.rs`

- [ ] **Step 1: Add pump lifecycle tests first**

Create a `ProcessedAudioWebRtcEgressPump` with:

- `DashMap<RoomId, ProcessedAudioWebRtcEgressPumpTask>`,
- `Arc<WebMediaRuntime>`,
- `Arc<ProcessedAudioEgressFanout>`,
- `Arc<ServerMediaNegotiator>`,
- `CancellationToken` + `JoinHandle`,
- `start(room_id)`, `stop(room_id)`, `task_count()`,
- test-only `stop_and_wait_for_test`.

Tests should verify start/replace, stop removes one room, stop waits for task exit.

- [ ] **Step 2: Implement pump send loop**

Task body:

- subscribe to processed frames for the room,
- `tokio::select!` on cancellation or broadcast receive,
- for each processed frame, call fanout,
- for each egress recipient, build `ServerMediaSessionKey` and `ServerMediaProcessedAudioFrame`,
- call negotiator `send_processed_audio_frame`,
- log fanout/send errors with full context and continue.

Handle `broadcast::error::RecvError::Lagged` by logging and continuing. Handle `Closed` by exiting the task. The task map is still cleared by explicit stop paths; the task body must not spin after the sender is gone.

- [ ] **Step 3: Run pump lifecycle tests**

Run:

```bash
cargo test -p lyre-web processed_audio_webrtc_egress_pump::tests::
wc -l crates/lyre-web/src/processed_audio_webrtc_egress_pump.rs
```

Expected: PASS and file under 400 LOC.

## Task 5: AppState Integration

**Files:**
- Modify: `crates/lyre-web/src/api.rs`
- Modify: `crates/lyre-web/src/api_server_media_state.rs`
- Create: `crates/lyre-web/src/processed_audio_webrtc_egress_pump_tests.rs`
- Modify: existing route tests as needed

- [ ] **Step 1: Add failing AppState lifecycle tests**

Tests should assert:

- `AppState::start_media_relay` starts/replaces the room egress pump,
- route `/media-relay/start` uses that AppState method,
- `stop_media_relay` stops the room egress pump before clearing processed frames,
- `close_server_media_sessions_for_room` stops the room egress pump,
- test-only `processed_audio_webrtc_egress_pump_count()`.

- [ ] **Step 2: Implement AppState field and methods**

Add pump construction in `AppState::new`. Add:

```rust
pub fn start_media_relay(
    &self,
    room_id: RoomId,
    request: lyre_core::StartMediaRelayRequest,
) -> lyre_core::MediaRelayRoomStatus
```

This should call `media_relays.start`, then start/replace the egress pump for the room.

Update route handler `start_media_relay` to call `state.start_media_relay(...)`.

Update `stop_media_relay` ordering:

1. stop egress pump,
2. stop relay,
3. clear processed frames,
4. close server-media sessions.

Update `close_server_media_sessions_for_room` to stop the egress pump as well as the runtime pump before closing peer handles.

- [ ] **Step 3: Run AppState lifecycle tests**

Run:

```bash
cargo test -p lyre-web processed_audio_webrtc_egress_pump_tests::app_state_start_and_stop_manage_egress_pump
cargo test -p lyre-web api_media_tests::media_relay_start_registers_track_and_stop_clears_state
```

Expected: PASS.

## Task 6: Real Fanout Integration and Route Guards

**Files:**
- Modify: `crates/lyre-web/src/processed_audio_webrtc_egress_pump_tests.rs`
- Modify: `crates/lyre-web/src/api_server_media_tests.rs`

- [ ] **Step 1: Add real two-peer fanout test**

Build an integration test with:

- active relay in `DEFAULT`,
- source track registered for user A and recipient audio track registered for user B,
- negotiated server-media peer for user B,
- process a valid `AudioFrame` or real server-media decoded frame from user A,
- wait until `state.server_media_peer_connection_for_test(user B).sent_egress_rtp_packets_for_test()` has at least one packet.

Assert user A does not receive its own frame if user A also has a negotiated peer.

- [ ] **Step 2: Add no public egress routes tests**

Add negative tests for:

- `/api/rooms/DEFAULT/server-media/egress`
- `/api/rooms/DEFAULT/server-media/egress-packets`
- `/api/rooms/DEFAULT/server-media/encode-failures`

Expected: all return 404.

- [ ] **Step 3: Run integration and route tests**

Run:

```bash
cargo test -p lyre-web processed_audio_webrtc_egress_pump_tests::processed_audio_frame_is_sent_to_recipient_server_media_peer
cargo test -p lyre-web api_server_media_tests::server_media_egress_routes_do_not_exist
```

Expected: PASS.

## Task 7: Verification and Implementation Review Gate

- [ ] **Step 1: Run Rust checks**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

- [ ] **Step 2: Run frontend checks**

```bash
cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build
```

- [ ] **Step 3: Run static checks**

```bash
git diff --check
wc -l crates/lyre-webrtc/src/*.rs crates/lyre-web/src/*.rs
```

- [ ] **Step 4: Dispatch independent implementation review**

Include reviewed spec, reviewed plan, full diff including new files, verification output, and SDD implementation verdict format. Do not update docs before implementation review returns `VERDICT: APPROVE`.

## Task 8: Post-Review Documentation, Final Verification, and Commit

- [ ] **Step 1: Update `MEMORY.md`**

Record that processed server audio now has an internal WebRTC egress path and that frontend server-media mode/browser playback, jitter buffering, PLC, and DeepFilterNet remain future work.

- [ ] **Step 2: Update `docs/roadmap.md`**

Move internal processed WebRTC egress from Next to Completed if present. Keep frontend server-media playback, jitter/PLC, DeepFilterNet, auth, persistence, observability, and WebRPC Rust runtime as Next items.

- [ ] **Step 3: Run final verification**

Repeat Task 7 checks.

- [ ] **Step 4: Commit locally**

Commit only implementation and final documentation files. Do not include SDD spec/plan artifacts unless the repository policy changes. Do not push from an implementation subtask. The main workflow leader can push after the local commit if the active thread workflow still requires it.

Use Lore protocol:

```bash
git commit -m "Send processed server audio over WebRTC egress" -m "Constraint: Server-side noise-cancelled audio must leave the runtime through an internal WebRTC media path before frontend playback can switch to server media.
Rejected: Browser playback UI in this increment | The backend egress path needs isolated verification before frontend mode switching.
Confidence: medium
Scope-risk: broad
Directive: Keep jitter buffering, packet loss concealment, and DeepFilterNet separate from this minimal egress path.
Tested: cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path Cargo.toml --workspace; frontend generate/test/typecheck/lint/build; git diff --check; LOC check
Not-tested: Browser playback of egress audio; jitter buffering; packet loss concealment; DeepFilterNet processing"
```
