# Opus RTP to Media Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decode valid incoming server-media Opus RTP payloads to PCM and feed them into the existing server media runtime.

**Architecture:** Keep Opus codec and RTP details inside `crates/lyre-webrtc`. Add Lyre-owned PCM/decode-failure DTOs plus a small decoder wrapper, wire the WebRTC receive task to record decoded PCM frames, expose one-shot drain methods through the peer handle and negotiator, and add a small `lyre-web` helper module that converts drained PCM DTOs into `lyre_core::AudioFrame` values for `WebMediaRuntime`.

**Tech Stack:** Rust, `opus-rs 0.1.22`, existing `webrtc 0.20.0-alpha.1`, existing Axum app state/runtime tests, `cargo nextest`.

---

## Reviewed Spec

Implement against:

- `docs/superpowers/specs/2026-06-15-opus-rtp-to-media-runtime-design.md`

Important boundaries:

- `opus-rs` is used only from `crates/lyre-webrtc`.
- Direct `opus_rs::`, `webrtc::`, and `rtc::` usage must not appear outside `crates/lyre-webrtc`.
- No public REST endpoint exposes raw RTP, decoded PCM, or decode failures.
- Update `MEMORY.md` and `docs/roadmap.md` only after independent implementation review returns `VERDICT: APPROVE`.

## File Structure

- Modify root `Cargo.toml`: add workspace dependency `opus-rs = "0.1.22"`.
- Modify `crates/lyre-webrtc/Cargo.toml`: add `opus-rs.workspace = true`.
- Create `crates/lyre-webrtc/src/opus_decode.rs`: public `ServerMediaOpusDecoder`, `ServerMediaDecodeError`, constants, and decoder unit tests.
- Create `crates/lyre-webrtc/src/test_support.rs`: feature-gated server-media integration helper used by `lyre-web` tests.
- Modify `crates/lyre-webrtc/src/media_ingress.rs`: add `ServerMediaPcmFrame`, `ServerMediaDecodeFailure`, recorder methods for PCM and decode failures, and drain tests.
- Modify `crates/lyre-webrtc/src/stack.rs`: add decoder ownership to the audio receive task, record PCM frames or decode failures, and expose drain methods on `WebRtcPeerConnectionHandle`.
- Create `crates/lyre-webrtc/src/stack_media_tests.rs`: move the real RTP ingress test and helpers that would push `stack_tests.rs` over 400 LOC, then extend it with decoded PCM assertions.
- Modify `crates/lyre-webrtc/src/lib.rs`: export new DTOs/errors and declare the new test module.
- Modify `crates/lyre-webrtc/src/negotiation.rs`: add `drain_pcm_frames` and `drain_decode_failures`.
- Modify `crates/lyre-webrtc/src/negotiation_tests.rs`: add missing/closed session drain coverage.
- Create `crates/lyre-web/src/server_media_runtime.rs`: helper for draining server media PCM frames and processing them into `WebMediaRuntime`.
- Create `crates/lyre-web/src/server_media_runtime_tests.rs`: focused AppState/helper tests, keeping `api.rs` under 400 LOC.
- Modify `crates/lyre-web/Cargo.toml`: enable the `lyre-webrtc/test-support` feature for integration tests.
- Modify `crates/lyre-web/src/api.rs`: import the helper module and add thin AppState passthrough methods.
- Modify `crates/lyre-web/src/lib.rs`: declare the new module and test module.

## Task 1: Opus Decode DTOs and Decoder Wrapper

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/lyre-webrtc/Cargo.toml`
- Create: `crates/lyre-webrtc/src/opus_decode.rs`
- Modify: `crates/lyre-webrtc/src/lib.rs`

- [x] **Step 1: Add the dependency**

Add this to the root `[workspace.dependencies]` section:

```toml
opus-rs = "0.1.22"
```

Add this to `crates/lyre-webrtc/Cargo.toml`:

```toml
opus-rs.workspace = true
```

- [x] **Step 2: Write failing decoder tests**

Create `crates/lyre-webrtc/src/opus_decode.rs` with the tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ServerMediaRtpPacket;
    use opus_rs::{Application, OpusEncoder};

    fn valid_packet() -> ServerMediaRtpPacket {
        let mut encoder = OpusEncoder::new(
            SERVER_MEDIA_OPUS_SAMPLE_RATE_HZ as i32,
            SERVER_MEDIA_OPUS_CHANNELS as usize,
            Application::Voip,
        )
        .unwrap();
        let samples = (0..SERVER_MEDIA_OPUS_FRAME_SIZE)
            .map(|index| ((index as f32) / 24.0).sin() * 0.1)
            .collect::<Vec<_>>();
        let mut payload = vec![0_u8; 512];
        let payload_len = encoder
            .encode(&samples, SERVER_MEDIA_OPUS_FRAME_SIZE, &mut payload)
            .unwrap();
        payload.truncate(payload_len);

        ServerMediaRtpPacket {
            track_id: "audio-main".to_owned(),
            sequence_number: 42,
            timestamp: 9_600,
            marker: true,
            payload_type: 111,
            payload,
        }
    }

    #[test]
    fn decoder_decodes_valid_opus_payload_to_pcm_frame() {
        let mut decoder = ServerMediaOpusDecoder::new().unwrap();

        let frame = decoder.decode_packet(&valid_packet()).unwrap();

        assert_eq!(frame.track_id, "audio-main");
        assert_eq!(frame.sequence_number, 42);
        assert_eq!(frame.rtp_timestamp, 9_600);
        assert_eq!(frame.sample_rate_hz, 48_000);
        assert_eq!(frame.channels, 1);
        assert_eq!(frame.samples.len(), SERVER_MEDIA_OPUS_FRAME_SIZE);
        assert!(frame.samples.iter().any(|sample| sample.abs() > 0.0));
    }

    #[test]
    fn decoder_rejects_empty_payload_with_decoder_message() {
        let mut decoder = ServerMediaOpusDecoder::new().unwrap();
        let mut packet = valid_packet();
        packet.payload.clear();

        assert_eq!(
            decoder.decode_packet(&packet),
            Err(ServerMediaDecodeError::Decode {
                message: "Input packet empty".to_owned(),
            })
        );
    }
}
```

- [x] **Step 3: Run the focused test and verify it fails**

Run:

```bash
cargo test -p lyre-webrtc opus_decode::
```

Expected: FAIL because the module/types are not implemented and not declared.

- [x] **Step 4: Implement the decoder wrapper**

Replace the top of `crates/lyre-webrtc/src/opus_decode.rs` with this production code above the tests:

```rust
use crate::ServerMediaRtpPacket;
use opus_rs::OpusDecoder;
use thiserror::Error;

pub const SERVER_MEDIA_OPUS_SAMPLE_RATE_HZ: u32 = 48_000;
pub const SERVER_MEDIA_OPUS_CHANNELS: u16 = 1;
pub const SERVER_MEDIA_OPUS_FRAME_SIZE: usize = 960;

#[derive(Debug, Clone, PartialEq)]
pub struct ServerMediaPcmFrame {
    pub track_id: String,
    pub sequence_number: u16,
    pub rtp_timestamp: u32,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerMediaDecodeFailure {
    pub track_id: String,
    pub sequence_number: u16,
    pub rtp_timestamp: u32,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ServerMediaDecodeError {
    #[error("failed to configure server media Opus decoder: {message}")]
    InvalidDecoderConfig { message: String },
    #[error("failed to decode server media Opus packet: {message}")]
    Decode { message: String },
}

pub struct ServerMediaOpusDecoder {
    decoder: OpusDecoder,
}

impl ServerMediaOpusDecoder {
    pub fn new() -> Result<Self, ServerMediaDecodeError> {
        let decoder = OpusDecoder::new(
            SERVER_MEDIA_OPUS_SAMPLE_RATE_HZ as i32,
            SERVER_MEDIA_OPUS_CHANNELS as usize,
        )
        .map_err(|source| ServerMediaDecodeError::InvalidDecoderConfig {
            message: source.to_owned(),
        })?;
        Ok(Self { decoder })
    }

    pub fn decode_packet(
        &mut self,
        packet: &ServerMediaRtpPacket,
    ) -> Result<ServerMediaPcmFrame, ServerMediaDecodeError> {
        let mut samples =
            vec![0.0_f32; SERVER_MEDIA_OPUS_FRAME_SIZE * SERVER_MEDIA_OPUS_CHANNELS as usize];
        self.decoder
            .decode(&packet.payload, SERVER_MEDIA_OPUS_FRAME_SIZE, &mut samples)
            .map_err(|source| ServerMediaDecodeError::Decode {
                message: source.to_owned(),
            })?;
        Ok(ServerMediaPcmFrame {
            track_id: packet.track_id.clone(),
            sequence_number: packet.sequence_number,
            rtp_timestamp: packet.timestamp,
            sample_rate_hz: SERVER_MEDIA_OPUS_SAMPLE_RATE_HZ,
            channels: SERVER_MEDIA_OPUS_CHANNELS,
            samples,
        })
    }
}
```

Update `crates/lyre-webrtc/src/lib.rs`:

```rust
pub mod media_ingress;
pub mod negotiation;
pub mod opus_decode;
pub mod session;
pub mod stack;

#[cfg(test)]
mod negotiation_tests;
#[cfg(test)]
mod stack_media_tests;
#[cfg(test)]
mod stack_tests;

pub use media_ingress::{ServerMediaRemoteTrack, ServerMediaRtpPacket, ServerMediaTrackKind};
pub use opus_decode::{
    ServerMediaDecodeError, ServerMediaDecodeFailure, ServerMediaOpusDecoder,
    ServerMediaPcmFrame, SERVER_MEDIA_OPUS_CHANNELS, SERVER_MEDIA_OPUS_FRAME_SIZE,
    SERVER_MEDIA_OPUS_SAMPLE_RATE_HZ,
};
```

Keep the existing `negotiation`, `session`, and `stack` exports after the new `opus_decode` export.

- [x] **Step 5: Run the focused decoder tests**

Run:

```bash
cargo test -p lyre-webrtc opus_decode::
```

Expected: PASS.

## Task 2: Media Ingress PCM and Decode Failure Drains

**Files:**
- Modify: `crates/lyre-webrtc/src/media_ingress.rs`

- [x] **Step 1: Extend the existing recorder test**

Update the `recorder_returns_remote_track_and_rtp_snapshots` test in `crates/lyre-webrtc/src/media_ingress.rs` to also record and drain PCM and decode failure snapshots:

```rust
        recorder.record_pcm_frame(ServerMediaPcmFrame {
            track_id: "audio-1".to_owned(),
            sequence_number: 7,
            rtp_timestamp: 48_000,
            sample_rate_hz: 48_000,
            channels: 1,
            samples: vec![0.1, -0.1],
        });
        recorder.record_decode_failure(ServerMediaDecodeFailure {
            track_id: "audio-1".to_owned(),
            sequence_number: 8,
            rtp_timestamp: 48_960,
            error: "Input packet empty".to_owned(),
        });
```

Then add assertions:

```rust
        assert_eq!(
            recorder.drain_pcm_frames(),
            vec![ServerMediaPcmFrame {
                track_id: "audio-1".to_owned(),
                sequence_number: 7,
                rtp_timestamp: 48_000,
                sample_rate_hz: 48_000,
                channels: 1,
                samples: vec![0.1, -0.1],
            }]
        );
        assert!(recorder.drain_pcm_frames().is_empty());
        assert_eq!(
            recorder.drain_decode_failures(),
            vec![ServerMediaDecodeFailure {
                track_id: "audio-1".to_owned(),
                sequence_number: 8,
                rtp_timestamp: 48_960,
                error: "Input packet empty".to_owned(),
            }]
        );
        assert!(recorder.drain_decode_failures().is_empty());
```

Add imports at the top of the test module:

```rust
use crate::{ServerMediaDecodeFailure, ServerMediaPcmFrame};
```

- [x] **Step 2: Run the recorder test and verify it fails**

Run:

```bash
cargo test -p lyre-webrtc media_ingress::tests::recorder_returns_remote_track_and_rtp_snapshots
```

Expected: FAIL because recorder PCM/failure methods do not exist.

- [x] **Step 3: Implement recorder storage and drains**

Update `crates/lyre-webrtc/src/media_ingress.rs` imports:

```rust
use crate::{ServerMediaDecodeFailure, ServerMediaPcmFrame};
use std::sync::{Arc, Mutex};
```

Add fields to `MediaIngressState`:

```rust
    pcm_frames: Vec<ServerMediaPcmFrame>,
    decode_failures: Vec<ServerMediaDecodeFailure>,
```

Add methods to `impl MediaIngressRecorder`:

```rust
    pub(crate) fn record_pcm_frame(&self, frame: ServerMediaPcmFrame) {
        self.inner
            .lock()
            .expect("media ingress recorder lock must not be poisoned")
            .pcm_frames
            .push(frame);
    }

    pub(crate) fn record_decode_failure(&self, failure: ServerMediaDecodeFailure) {
        self.inner
            .lock()
            .expect("media ingress recorder lock must not be poisoned")
            .decode_failures
            .push(failure);
    }

    pub(crate) fn drain_pcm_frames(&self) -> Vec<ServerMediaPcmFrame> {
        std::mem::take(
            &mut self
                .inner
                .lock()
                .expect("media ingress recorder lock must not be poisoned")
                .pcm_frames,
        )
    }

    pub(crate) fn drain_decode_failures(&self) -> Vec<ServerMediaDecodeFailure> {
        std::mem::take(
            &mut self
                .inner
                .lock()
                .expect("media ingress recorder lock must not be poisoned")
                .decode_failures,
        )
    }
```

- [x] **Step 4: Run the recorder test**

Run:

```bash
cargo test -p lyre-webrtc media_ingress::tests::recorder_returns_remote_track_and_rtp_snapshots
```

Expected: PASS.

## Task 3: WebRTC Receive Task Decodes Opus RTP

**Files:**
- Modify: `crates/lyre-webrtc/src/stack.rs`
- Modify: `crates/lyre-webrtc/src/stack_tests.rs`
- Create: `crates/lyre-webrtc/src/stack_media_tests.rs`
- Modify: `crates/lyre-webrtc/src/lib.rs`

- [x] **Step 1: Move media-heavy stack tests to a new module**

Move these helpers and tests from `crates/lyre-webrtc/src/stack_tests.rs` into `crates/lyre-webrtc/src/stack_media_tests.rs`:

- `TestPeerConnectionHandler`
- `opus_offerer`
- `local_description_sdp_after_gathering`
- `wait_for_connected`
- `wait_for_test_candidates`
- `wait_for_local_candidates`
- `to_server_candidate`
- `to_webrtc_candidate`
- `opus_offerer_helper_creates_media_offer`
- `answer_remote_offer_records_audio_track_and_rtp_packet`
- `test_rtp_packet`

Keep the simpler offer/ICE tests in `stack_tests.rs`.

Ensure `crates/lyre-webrtc/src/lib.rs` contains:

```rust
#[cfg(test)]
mod stack_media_tests;
#[cfg(test)]
mod stack_tests;
```

- [x] **Step 2: Run both stack test modules**

Run:

```bash
cargo test -p lyre-webrtc stack_tests::
cargo test -p lyre-webrtc stack_media_tests::
wc -l crates/lyre-webrtc/src/stack_tests.rs crates/lyre-webrtc/src/stack_media_tests.rs
```

Expected: tests PASS and both files are below 400 LOC.

- [x] **Step 3: Make the real RTP test expect decoded PCM**

In `crates/lyre-webrtc/src/stack_media_tests.rs`, change the local offerer to send a valid Opus payload rather than arbitrary bytes. Add:

```rust
use crate::SERVER_MEDIA_OPUS_FRAME_SIZE;
use opus_rs::{Application, OpusEncoder};
```

Add helper:

```rust
fn encoded_opus_payload() -> Vec<u8> {
    let mut encoder = OpusEncoder::new(48_000, 1, Application::Voip).unwrap();
    let samples = (0..SERVER_MEDIA_OPUS_FRAME_SIZE)
        .map(|index| ((index as f32) / 24.0).sin() * 0.1)
        .collect::<Vec<_>>();
    let mut payload = vec![0_u8; 512];
    let payload_len = encoder
        .encode(&samples, SERVER_MEDIA_OPUS_FRAME_SIZE, &mut payload)
        .unwrap();
    payload.truncate(payload_len);
    payload
}
```

Update `answer_remote_offer_records_audio_track_and_rtp_packet` so it stores the payload, writes that payload, still checks raw RTP payload equality, and additionally waits for:

```rust
let frames = server.drain_pcm_frames();
if frames.iter().any(|frame| {
    frame.track_id == "lyre-test"
        && frame.sequence_number == 42
        && frame.rtp_timestamp == 1234
        && frame.sample_rate_hz == 48_000
        && frame.channels == 1
        && frame.samples.len() == SERVER_MEDIA_OPUS_FRAME_SIZE
}) {
    assert!(server.drain_pcm_frames().is_empty());
    return;
}
```

Keep imports minimal; remove unused imports before clippy.

Add a second focused media test for malformed Opus RTP:

```rust
#[tokio::test]
async fn answer_remote_offer_records_decode_failure_for_malformed_audio_rtp() {
    use std::time::Duration;

    let server = WebRtcStack::new().create_peer_connection().await.unwrap();
    let (offerer, track, offerer_candidates, mut gather_complete_rx, mut connected_rx) =
        opus_offerer().await;

    let offer = offerer.create_offer(None).await.unwrap();
    offerer.set_local_description(offer).await.unwrap();
    let offer_sdp = local_description_sdp_after_gathering(&offerer, &mut gather_complete_rx).await;

    let answer_sdp = server.answer_remote_offer(offer_sdp).await.unwrap();
    let answer = webrtc::peer_connection::RTCSessionDescription::answer(answer_sdp).unwrap();
    offerer.set_remote_description(answer).await.unwrap();

    for candidate in wait_for_test_candidates(&offerer_candidates).await {
        server
            .add_remote_ice_candidate(to_server_candidate(candidate))
            .await
            .unwrap();
    }
    for candidate in wait_for_local_candidates(&server).await {
        if candidate.candidate.is_empty() {
            continue;
        }
        offerer
            .add_ice_candidate(to_webrtc_candidate(candidate))
            .await
            .unwrap();
    }

    assert!(wait_for_connected(&mut connected_rx).await);

    for _ in 0..100 {
        let _ = track.write_rtp(test_rtp_packet(Vec::new())).await;
        let failures = server.drain_decode_failures();
        if failures.iter().any(|failure| {
            failure.track_id == "lyre-test"
                && failure.sequence_number == 42
                && failure.rtp_timestamp == 1234
                && failure.error == "Input packet empty"
        }) {
            assert!(server.drain_pcm_frames().is_empty());
            assert!(server.drain_decode_failures().is_empty());
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("server did not record the malformed Opus RTP decode failure");
}
```

- [x] **Step 4: Run the media stack test and verify it fails**

Run:

```bash
cargo test -p lyre-webrtc stack_media_tests::answer_remote_offer_records_audio_track_and_rtp_packet
```

Expected: FAIL because `drain_pcm_frames` is not exposed and the receive task does not decode packets.

- [x] **Step 5: Wire decoder into the receive task**

Update `crates/lyre-webrtc/src/stack.rs` imports:

```rust
use crate::{
    media_ingress::MediaIngressRecorder, ServerMediaDecodeError, ServerMediaDecodeFailure,
    ServerMediaOpusDecoder, ServerMediaPcmFrame, ServerMediaRemoteTrack, ServerMediaRtpPacket,
    ServerMediaTrackKind,
};
use tracing::warn;
```

In the audio `tokio::spawn` block, create a decoder before the loop:

```rust
let mut decoder = match ServerMediaOpusDecoder::new() {
    Ok(decoder) => decoder,
    Err(error) => {
        warn!(error = %error, "failed to initialize server media Opus decoder");
        return;
    }
};
```

Inside `TrackRemoteEvent::OnRtpPacket`, first build a `ServerMediaRtpPacket`, record it, then decode it:

```rust
let packet = ServerMediaRtpPacket {
    track_id: track_id.clone(),
    sequence_number: packet.header.sequence_number,
    timestamp: packet.header.timestamp,
    marker: packet.header.marker,
    payload_type: packet.header.payload_type,
    payload: packet.payload.to_vec(),
};
media_ingress.record_rtp_packet(packet.clone());
match decoder.decode_packet(&packet) {
    Ok(frame) => media_ingress.record_pcm_frame(frame),
    Err(error) => {
        let message = match &error {
            ServerMediaDecodeError::InvalidDecoderConfig { message }
            | ServerMediaDecodeError::Decode { message } => message.clone(),
        };
        warn!(error = %error, "failed to decode server media Opus RTP packet");
        media_ingress.record_decode_failure(ServerMediaDecodeFailure {
            track_id: packet.track_id,
            sequence_number: packet.sequence_number,
            rtp_timestamp: packet.timestamp,
            error: message,
        });
    }
}
```

Add handle methods:

```rust
    pub fn drain_pcm_frames(&self) -> Vec<ServerMediaPcmFrame> {
        self.media_ingress.drain_pcm_frames()
    }

    pub fn drain_decode_failures(&self) -> Vec<ServerMediaDecodeFailure> {
        self.media_ingress.drain_decode_failures()
    }
```

- [x] **Step 6: Run focused stack tests and LOC check**

Run:

```bash
cargo test -p lyre-webrtc stack_tests::
cargo test -p lyre-webrtc stack_media_tests::
wc -l crates/lyre-webrtc/src/stack.rs crates/lyre-webrtc/src/stack_tests.rs crates/lyre-webrtc/src/stack_media_tests.rs
```

Expected: tests PASS and all listed files stay below 400 LOC.

## Task 4: Negotiator Drain Methods

**Files:**
- Modify: `crates/lyre-webrtc/src/negotiation.rs`
- Modify: `crates/lyre-webrtc/src/negotiation_tests.rs`

- [x] **Step 1: Add failing missing/closed session tests**

In `crates/lyre-webrtc/src/negotiation_tests.rs`, extend `server_media_snapshot_queries_return_empty_for_missing_session`:

```rust
    assert!(negotiator.drain_pcm_frames(&missing_key).is_empty());
    assert!(negotiator.drain_decode_failures(&missing_key).is_empty());
```

Extend `close_and_close_room_remove_stored_handles` after each close:

```rust
    assert!(negotiator.drain_pcm_frames(&key).is_empty());
    assert!(negotiator.drain_decode_failures(&key).is_empty());
```

- [x] **Step 2: Run negotiator tests and verify they fail**

Run:

```bash
cargo test -p lyre-webrtc negotiation_tests::
```

Expected: FAIL because drain methods do not exist.

- [x] **Step 3: Implement negotiator methods**

Update imports in `crates/lyre-webrtc/src/negotiation.rs`:

```rust
use crate::{
    ServerMediaDecodeFailure, ServerMediaIceCandidateInit, ServerMediaPcmFrame,
    ServerMediaRemoteTrack, ServerMediaRtpPacket, ServerMediaSessionConfig,
    ServerMediaSessionKey, ServerMediaSessionRegistry, ServerMediaSessionState,
    WebRtcPeerConnectionHandle, WebRtcStack, WebRtcStackError,
};
```

Add methods:

```rust
    pub fn drain_pcm_frames(&self, key: &ServerMediaSessionKey) -> Vec<ServerMediaPcmFrame> {
        self.peer_connections
            .get(key)
            .map(|peer_connection| peer_connection.drain_pcm_frames())
            .unwrap_or_default()
    }

    pub fn drain_decode_failures(
        &self,
        key: &ServerMediaSessionKey,
    ) -> Vec<ServerMediaDecodeFailure> {
        self.peer_connections
            .get(key)
            .map(|peer_connection| peer_connection.drain_decode_failures())
            .unwrap_or_default()
    }

```

- [x] **Step 4: Run negotiator tests and LOC check**

Run:

```bash
cargo test -p lyre-webrtc negotiation_tests::
wc -l crates/lyre-webrtc/src/negotiation.rs crates/lyre-webrtc/src/negotiation_tests.rs
```

Expected: tests PASS and both files stay below 400 LOC.

## Task 5: Web AppState PCM Runtime Ingest

**Files:**
- Create: `crates/lyre-web/src/server_media_runtime.rs`
- Create: `crates/lyre-web/src/server_media_runtime_tests.rs`
- Modify: `crates/lyre-web/src/api.rs`
- Modify: `crates/lyre-web/src/lib.rs`
- Modify: `crates/lyre-web/src/api_server_media_tests.rs`

- [x] **Step 1: Add focused failing tests**

Create `crates/lyre-web/src/server_media_runtime_tests.rs`:

```rust
use crate::{api::AppState, server_media_runtime};
use lyre_core::{
    MediaRelayError, MediaTrackKind, NoiseProvider, RegisterMediaTrackRequest, RoomId,
    StartMediaRelayRequest, UserId,
};
use lyre_webrtc::{ServerMediaPcmFrame, ServerMediaSessionKey};

fn key() -> ServerMediaSessionKey {
    ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
    }
}

fn pcm_frame(track_id: impl Into<String>, sequence_number: u16) -> ServerMediaPcmFrame {
    ServerMediaPcmFrame {
        track_id: track_id.into(),
        sequence_number,
        rtp_timestamp: 48_000,
        sample_rate_hz: 48_000,
        channels: 1,
        samples: vec![0.25; 960],
    }
}

fn start_relay_with_audio_track(state: &AppState) {
    let key = key();
    state
        .media_relays
        .start(key.room_id.clone(), StartMediaRelayRequest::default());
    state
        .media_relays
        .register_track(
            key.room_id,
            RegisterMediaTrackRequest {
                user_id: key.user_id,
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
}

#[test]
fn app_state_processes_server_media_pcm_frames_into_runtime() {
    let state = AppState::default();
    let key = key();
    start_relay_with_audio_track(&state);

    assert_eq!(
        state.process_server_media_pcm_frame(&key, pcm_frame("audio-main", 7)),
        Ok(())
    );

    let frames = state.processed_media_frames(&key.room_id);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].room_id, key.room_id);
    assert_eq!(frames[0].user_id, key.user_id);
    assert_eq!(frames[0].track_id, "audio-main");
    assert_eq!(frames[0].sequence, 7);
    assert_eq!(frames[0].sample_rate_hz, 48_000);
    assert_eq!(frames[0].channels, 1);
    assert_eq!(frames[0].samples.len(), 960);
    assert_eq!(frames[0].noise.provider, NoiseProvider::Off);
}

#[test]
fn app_state_process_server_media_pcm_frame_returns_relay_error() {
    let state = AppState::default();
    let key = key();

    assert_eq!(
        state.process_server_media_pcm_frame(&key, pcm_frame("audio-main", 7)),
        Err(MediaRelayError::Inactive {
            room_id: key.room_id,
        })
    );
}

#[test]
fn server_media_runtime_batch_stops_on_first_error_without_processing_later_frames() {
    let state = AppState::default();
    let key = key();

    assert_eq!(
        server_media_runtime::process_pcm_frame_batch(
            &state.media_runtime,
            &key,
            vec![pcm_frame("missing-track", 7), pcm_frame("audio-main", 8)],
        ),
        Err(MediaRelayError::Inactive {
            room_id: key.room_id.clone(),
        })
    );

    start_relay_with_audio_track(&state);

    assert_eq!(
        server_media_runtime::process_pcm_frame_batch(
            &state.media_runtime,
            &key,
            vec![pcm_frame("missing-track", 9), pcm_frame("audio-main", 10)],
        ),
        Err(MediaRelayError::TrackNotFound {
            room_id: key.room_id.clone(),
            user_id: key.user_id.clone(),
            track_id: "missing-track".to_owned(),
        })
    );
    assert!(state.processed_media_frames(&key.room_id).is_empty());
}
```

In `crates/lyre-web/src/api_server_media_tests.rs`, add to `app_state_server_media_snapshots_are_internal_and_empty_for_missing_session`:

```rust
    assert!(state.drain_server_media_pcm_frames(&key).is_empty());
    assert!(state.drain_server_media_decode_failures(&key).is_empty());
    assert_eq!(state.process_server_media_pcm_frames(&key), Ok(0));
```

Add a focused async integration test in `crates/lyre-web/src/api_server_media_tests.rs` that uses the real server-media drain path. Reuse the `stack_media_tests.rs` offerer pattern locally in this test file if needed, but keep direct `webrtc::`/`rtc::` usage out of `lyre-web`; instead, add a small public test helper in `lyre-webrtc` that still returns only Lyre-owned values or sends through its own internal peer connection.

Create `crates/lyre-webrtc/src/test_support.rs` gated by a `test-support` feature and enable that feature for `lyre-web` tests:

```toml
[features]
test-support = []
```

In `crates/lyre-web/Cargo.toml`, add a dev-dependency entry with the extra test feature while keeping the normal dependency unchanged:

```toml
[dev-dependencies]
lyre-webrtc = { path = "../lyre-webrtc", features = ["test-support"] }
```

The test-support API must be available only with the `test-support` feature:

```rust
#[cfg(feature = "test-support")]
pub mod test_support;
```

The helper should own all direct `webrtc`/`rtc`/`opus_rs` usage and expose a Lyre-owned async function:

```rust
pub async fn send_valid_opus_packet_to_server(
    server: &crate::WebRtcPeerConnectionHandle,
) {
    // Builds a local offerer, negotiates ICE with `server`, sends one valid
    // Opus RTP packet, and waits until a raw RTP snapshot proves delivery.
    // It must not call `server.drain_pcm_frames()`.
}
```

Use that helper in `api_server_media_tests.rs`:

```rust
#[tokio::test]
async fn process_server_media_pcm_frames_discards_real_drained_batch_on_error() {
    let state = AppState::default();
    let key = ServerMediaSessionKey {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
    };

    negotiate_server_media(&state).await;
    let server = state.server_media_peer_connection_for_test(&key).unwrap();
    lyre_webrtc::test_support::send_valid_opus_packet_to_server(&server).await;

    for _ in 0..100 {
        match state.process_server_media_pcm_frames(&key) {
            Err(error) => {
                assert_eq!(
                    error,
                    lyre_core::MediaRelayError::Inactive {
                        room_id: key.room_id.clone(),
                    }
                );
                assert_eq!(state.process_server_media_pcm_frames(&key), Ok(0));
                return;
            }
            Ok(0) => tokio::time::sleep(std::time::Duration::from_millis(20)).await,
            Ok(count) => panic!("processed {count} frames without an active relay"),
        }
    }

    panic!("decoded server media PCM frame was not drained by AppState");
}
```

Add this AppState test helper under `#[cfg(test)]`:

```rust
pub fn server_media_peer_connection_for_test(
    &self,
    key: &ServerMediaSessionKey,
) -> Option<WebRtcPeerConnectionHandle> {
    self.server_media_negotiator.peer_connection_for_test(key)
}
```

Add this negotiator test helper under `#[cfg(any(test, feature = "test-support"))]`:

```rust
pub fn peer_connection_for_test(
    &self,
    key: &ServerMediaSessionKey,
) -> Option<WebRtcPeerConnectionHandle> {
    self.peer_connections.get(key).map(|entry| entry.value().clone())
}
```

This test-support path must not add a public raw RTP/PCM HTTP endpoint and must keep direct media dependencies inside `lyre-webrtc`.

Add another route absence assertion:

```rust
#[tokio::test]
async fn server_media_pcm_frames_route_does_not_exist() {
    let app = router(AppState::default());

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/rooms/DEFAULT/server-media/pcm-frames?user_id=user_01")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
```

- [x] **Step 2: Run focused web tests and verify they fail**

Run:

```bash
cargo test -p lyre-web server_media_runtime_tests::
cargo test -p lyre-web api_server_media_tests::app_state_server_media_snapshots_are_internal_and_empty_for_missing_session
```

Expected: FAIL because module/methods are not implemented.

- [x] **Step 3: Implement the server media runtime helper**

Create `crates/lyre-web/src/server_media_runtime.rs`:

```rust
use lyre_core::{AudioFrame, MediaRelayError};
use lyre_webrtc::{
    ServerMediaDecodeFailure, ServerMediaNegotiator, ServerMediaPcmFrame, ServerMediaSessionKey,
};

use crate::media_runtime::WebMediaRuntime;

pub fn drain_pcm_frames(
    negotiator: &ServerMediaNegotiator,
    key: &ServerMediaSessionKey,
) -> Vec<ServerMediaPcmFrame> {
    negotiator.drain_pcm_frames(key)
}

pub fn drain_decode_failures(
    negotiator: &ServerMediaNegotiator,
    key: &ServerMediaSessionKey,
) -> Vec<ServerMediaDecodeFailure> {
    negotiator.drain_decode_failures(key)
}

pub fn process_pcm_frame(
    runtime: &WebMediaRuntime,
    key: &ServerMediaSessionKey,
    frame: ServerMediaPcmFrame,
) -> Result<(), MediaRelayError> {
    runtime.process_frame(AudioFrame {
        room_id: key.room_id.clone(),
        user_id: key.user_id.clone(),
        track_id: frame.track_id,
        sample_rate_hz: frame.sample_rate_hz,
        channels: frame.channels,
        sequence: u64::from(frame.sequence_number),
        samples: frame.samples,
    })
}

pub fn process_pcm_frame_batch(
    runtime: &WebMediaRuntime,
    key: &ServerMediaSessionKey,
    frames: Vec<ServerMediaPcmFrame>,
) -> Result<usize, MediaRelayError> {
    let mut processed = 0;
    for frame in frames {
        process_pcm_frame(runtime, key, frame)?;
        processed += 1;
    }
    Ok(processed)
}

pub fn process_pcm_frames(
    runtime: &WebMediaRuntime,
    negotiator: &ServerMediaNegotiator,
    key: &ServerMediaSessionKey,
) -> Result<usize, MediaRelayError> {
    process_pcm_frame_batch(runtime, key, negotiator.drain_pcm_frames(key))
}
```

Update `crates/lyre-web/src/lib.rs`:

```rust
pub mod server_media_runtime;

#[cfg(test)]
mod server_media_runtime_tests;
```

Keep existing module declarations.

- [x] **Step 4: Add AppState passthrough methods**

Update imports in `crates/lyre-web/src/api.rs`:

```rust
use crate::{
    error::ApiError,
    media_egress::{ProcessedAudioEgressFanout, ProcessedAudioEgressFrame},
    media_runtime::WebMediaRuntime,
    server_media_runtime,
    signalling::{route_signal_message, PeerHub, SignalMessage, SignalPayload},
};
```

Update `lyre_webrtc` import list to include:

```rust
ServerMediaDecodeFailure, ServerMediaPcmFrame,
```

Add AppState methods near the existing server media snapshot methods:

```rust
    pub fn drain_server_media_pcm_frames(
        &self,
        key: &ServerMediaSessionKey,
    ) -> Vec<ServerMediaPcmFrame> {
        server_media_runtime::drain_pcm_frames(&self.server_media_negotiator, key)
    }

    pub fn drain_server_media_decode_failures(
        &self,
        key: &ServerMediaSessionKey,
    ) -> Vec<ServerMediaDecodeFailure> {
        server_media_runtime::drain_decode_failures(&self.server_media_negotiator, key)
    }

    pub fn process_server_media_pcm_frame(
        &self,
        key: &ServerMediaSessionKey,
        frame: ServerMediaPcmFrame,
    ) -> Result<(), MediaRelayError> {
        server_media_runtime::process_pcm_frame(&self.media_runtime, key, frame)
    }

    pub fn process_server_media_pcm_frames(
        &self,
        key: &ServerMediaSessionKey,
    ) -> Result<usize, MediaRelayError> {
        server_media_runtime::process_pcm_frames(
            &self.media_runtime,
            &self.server_media_negotiator,
            key,
        )
    }
```

- [x] **Step 5: Run focused web tests and LOC check**

Run:

```bash
cargo test -p lyre-web server_media_runtime_tests::
cargo test -p lyre-web api_server_media_tests::
wc -l crates/lyre-web/src/api.rs crates/lyre-web/src/server_media_runtime.rs crates/lyre-web/src/server_media_runtime_tests.rs crates/lyre-web/src/api_server_media_tests.rs
```

Expected: tests PASS and all listed files stay below 400 LOC.

## Task 6: Verification and Implementation Review Gate

**Files:**
- No docs changes in this task.
- No product code changes unless verification exposes a defect in this increment.

- [x] **Step 1: Run Rust formatting and linting**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: PASS.

- [x] **Step 2: Run full Rust tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: PASS.

- [x] **Step 3: Run frontend contract and build checks**

Run:

```bash
cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build
```

Expected: PASS.

- [x] **Step 4: Run static boundary checks**

Run:

```bash
git diff --check
rg -n '(^|[^[:alnum:]_])webrtc::[[:alpha:]_]' crates --glob '!crates/lyre-webrtc/**'; test $? -eq 1
rg -n '(^|[^[:alnum:]_])rtc::[[:alpha:]_]' crates --glob '!crates/lyre-webrtc/**'; test $? -eq 1
rg -n '(^|[^[:alnum:]_])opus_rs::[[:alpha:]_]' crates --glob '!crates/lyre-webrtc/**'; test $? -eq 1
cargo rustdoc -p lyre-webrtc --lib -- -D warnings
wc -l crates/lyre-webrtc/src/stack.rs crates/lyre-webrtc/src/stack_tests.rs crates/lyre-webrtc/src/stack_media_tests.rs crates/lyre-webrtc/src/media_ingress.rs crates/lyre-webrtc/src/opus_decode.rs crates/lyre-webrtc/src/negotiation.rs crates/lyre-web/src/api.rs crates/lyre-web/src/server_media_runtime.rs crates/lyre-web/src/server_media_runtime_tests.rs crates/lyre-web/src/api_server_media_tests.rs
```

Expected:

- `git diff --check` exits 0.
- The `rg` leak checks find no direct `webrtc::`, `rtc::`, or `opus_rs::` usage outside `crates/lyre-webrtc`.
- `cargo rustdoc -p lyre-webrtc --lib -- -D warnings` passes.
- Every listed Rust file remains below 400 lines.

- [x] **Step 5: Dispatch independent implementation review**

Before updating `MEMORY.md` or `docs/roadmap.md`, dispatch an independent implementation reviewer with:

- the reviewed spec path,
- this reviewed plan path,
- the full diff,
- verification output from Steps 1-4,
- the required SDD implementation verdict format.

Expected: reviewer returns `VERDICT: APPROVE`. If it returns `REVISE`, fix the gaps, rerun relevant verification, and repeat this review gate.

## Task 7: Post-Review Documentation, Final Verification, Commit, and Push

**Files:**
- Modify: `MEMORY.md`
- Modify: `docs/roadmap.md`
- Modify only docs after Task 6 receives implementation `VERDICT: APPROVE`.

- [x] **Step 1: Update `MEMORY.md` after implementation approval**

Add a concise entry:

```markdown
## 2026-06-15 Opus RTP to Media Runtime

- Added a pure-Rust Opus decode bridge in `lyre-webrtc` using `opus-rs`.
- Valid incoming server-media Opus RTP packets now produce Lyre-owned 48 kHz mono PCM frame DTOs that `lyre-web` can drain into the existing `WebMediaRuntime`.
- Decode failures preserve the original decoder error message in internal snapshots; no public raw RTP, PCM, or decode-failure endpoint was added.
- Packet loss concealment, jitter buffering, processed RTP/RTCP egress, browser playback, and DeepFilterNet remain future work.
```

- [x] **Step 2: Update `docs/roadmap.md` after implementation approval**

Move “Decode incoming Opus RTP into PCM frames and feed the existing server media runtime” into the completed section. Keep next TODO focused on wiring real RNNoise/DeepFilterNet behavior to decoded WebRTC tracks, then processed RTP/RTCP egress and browser playback.

- [x] **Step 3: Run final docs-inclusive verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build
git diff --check
rg -n '(^|[^[:alnum:]_])webrtc::[[:alpha:]_]' crates --glob '!crates/lyre-webrtc/**'; test $? -eq 1
rg -n '(^|[^[:alnum:]_])rtc::[[:alpha:]_]' crates --glob '!crates/lyre-webrtc/**'; test $? -eq 1
rg -n '(^|[^[:alnum:]_])opus_rs::[[:alpha:]_]' crates --glob '!crates/lyre-webrtc/**'; test $? -eq 1
cargo rustdoc -p lyre-webrtc --lib -- -D warnings
wc -l crates/lyre-webrtc/src/stack.rs crates/lyre-webrtc/src/stack_tests.rs crates/lyre-webrtc/src/stack_media_tests.rs crates/lyre-webrtc/src/media_ingress.rs crates/lyre-webrtc/src/opus_decode.rs crates/lyre-webrtc/src/negotiation.rs crates/lyre-web/src/api.rs crates/lyre-web/src/server_media_runtime.rs crates/lyre-web/src/server_media_runtime_tests.rs crates/lyre-web/src/api_server_media_tests.rs
```

Expected: all commands PASS or produce the expected empty grep result.

- [x] **Step 4: Review final diff**

Run:

```bash
git status --short
git diff --stat
git diff -- Cargo.toml Cargo.lock crates/lyre-webrtc/Cargo.toml crates/lyre-webrtc/src/opus_decode.rs crates/lyre-webrtc/src/test_support.rs crates/lyre-webrtc/src/media_ingress.rs crates/lyre-webrtc/src/stack.rs crates/lyre-webrtc/src/stack_tests.rs crates/lyre-webrtc/src/stack_media_tests.rs crates/lyre-webrtc/src/lib.rs crates/lyre-webrtc/src/negotiation.rs crates/lyre-webrtc/src/negotiation_tests.rs crates/lyre-web/Cargo.toml crates/lyre-web/src/api.rs crates/lyre-web/src/lib.rs crates/lyre-web/src/server_media_runtime.rs crates/lyre-web/src/server_media_runtime_tests.rs crates/lyre-web/src/api_server_media_tests.rs MEMORY.md docs/roadmap.md docs/superpowers/specs/2026-06-15-opus-rtp-to-media-runtime-design.md docs/superpowers/plans/2026-06-15-opus-rtp-to-media-runtime.md
```

Expected: only intended files changed.

- [ ] **Step 5: Commit and push**

Commit with Lore protocol:

```bash
git add Cargo.toml Cargo.lock crates/lyre-webrtc/Cargo.toml crates/lyre-webrtc/src/opus_decode.rs crates/lyre-webrtc/src/test_support.rs crates/lyre-webrtc/src/media_ingress.rs crates/lyre-webrtc/src/stack.rs crates/lyre-webrtc/src/stack_tests.rs crates/lyre-webrtc/src/stack_media_tests.rs crates/lyre-webrtc/src/lib.rs crates/lyre-webrtc/src/negotiation.rs crates/lyre-webrtc/src/negotiation_tests.rs crates/lyre-web/Cargo.toml crates/lyre-web/src/api.rs crates/lyre-web/src/lib.rs crates/lyre-web/src/server_media_runtime.rs crates/lyre-web/src/server_media_runtime_tests.rs crates/lyre-web/src/api_server_media_tests.rs MEMORY.md docs/roadmap.md docs/superpowers/specs/2026-06-15-opus-rtp-to-media-runtime-design.md docs/superpowers/plans/2026-06-15-opus-rtp-to-media-runtime.md
git commit -m "Decode server Opus RTP into media runtime" -m "Constraint: Server-side noise cancellation needs decoded PCM from real WebRTC tracks before processed audio can be forwarded.
Rejected: Exposing decoded PCM over REST | It would create a debugging API instead of product behavior.
Confidence: medium
Scope-risk: moderate
Directive: Keep Opus codec and concrete WebRTC/RTP types isolated inside crates/lyre-webrtc; do not claim server playback until processed RTP/RTCP egress exists.
Tested: cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path Cargo.toml --workspace; frontend generate/test/typecheck/lint/build; git diff --check; webrtc/rtc/opus_rs dependency leak checks; cargo rustdoc -p lyre-webrtc --lib -- -D warnings; LOC check
Not-tested: Browser end-to-end server media playback; jitter buffering; packet loss concealment; DeepFilterNet processing"
git push
```

Expected: commit and push succeed. If push is blocked by credentials, network, branch policy, or remote rejection, report the local commit SHA and exact push error.
