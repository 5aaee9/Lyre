# Server Media PCM Packet Loss Concealment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate deterministic PCM fallback frames for server-media RTP packet loss after the jitter buffer identifies missing packets.

**Architecture:** Add a focused `ServerMediaPcmConcealer` in `lyre-webrtc` that keeps the last usable PCM frame and synthesizes a faded 960-sample mono replacement for jitter-buffer loss events. Wire the existing audio ingress helper to record synthesized PCM when possible and keep the existing decode-failure path when no usable baseline exists.

**Tech Stack:** Rust, existing `lyre-webrtc` DTOs, existing WebRTC test helpers, Tokio tests.

**Reviewed Spec:** `docs/superpowers/specs/2026-06-15-server-media-pcm-plc-design.md`

---

## File Structure

- Create `crates/lyre-webrtc/src/pcm_concealment.rs`: `ServerMediaPcmConcealer` and unit tests.
- Modify `crates/lyre-webrtc/src/lib.rs`: register the module and re-export `ServerMediaPcmConcealer` internally.
- Modify `crates/lyre-webrtc/src/stack_audio_ingress.rs`: accept a concealer state, seed it after successful decoded/synthesized PCM, and synthesize loss frames when possible.
- Modify `crates/lyre-webrtc/src/stack.rs`: instantiate one concealer per audio track task and pass it to the helper.
- Create `crates/lyre-webrtc/src/stack_pcm_plc_tests.rs`: stack integration tests for synthesized missing frames and fallback failure cases.
- Modify after implementation review approval: `MEMORY.md` and `docs/roadmap.md`.

Keep changed Rust files below 400 LOC. Do not add public REST, WebRPC, frontend, Docker, or GitHub Actions changes.

## Task 0: SDD Pre-Implementation Gate

**Files:**
- Read: `docs/superpowers/specs/2026-06-15-server-media-pcm-plc-design.md`
- Read: `docs/superpowers/plans/2026-06-15-server-media-pcm-plc.md`

- [ ] **Step 1: Confirm approved spec review exists**

Required evidence before implementation:

```text
VERDICT: APPROVE
ISSUES:
- None
REQUIRED_CHANGES:
- None
```

- [ ] **Step 2: Dispatch independent plan reviewer**

Dispatch an independent reviewer with the approved spec and this plan. Required verdict:

```text
VERDICT: APPROVE
```

Do not edit implementation files until the plan review approves.

## Task 1: Add PCM Concealer Unit

**Files:**
- Create: `crates/lyre-webrtc/src/pcm_concealment.rs`
- Modify: `crates/lyre-webrtc/src/lib.rs`

- [ ] **Step 1: Register module and internal export**

In `crates/lyre-webrtc/src/lib.rs`, add:

```rust
mod pcm_concealment;
```

and:

```rust
pub(crate) use jitter_buffer::{
    ServerMediaConcealmentRequired, ServerMediaJitterBuffer, ServerMediaJitterBufferOutput,
};
pub(crate) use pcm_concealment::ServerMediaPcmConcealer;
```

This replaces the existing narrower jitter-buffer internal export:

```rust
pub(crate) use jitter_buffer::{ServerMediaJitterBuffer, ServerMediaJitterBufferOutput};
```

- [ ] **Step 2: Add failing concealer tests**

Create `crates/lyre-webrtc/src/pcm_concealment.rs` with this test-first skeleton:

```rust
use crate::{
    ServerMediaConcealmentRequired, ServerMediaPcmFrame, SERVER_MEDIA_OPUS_CHANNELS,
    SERVER_MEDIA_OPUS_FRAME_SIZE, SERVER_MEDIA_OPUS_SAMPLE_RATE_HZ,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn event(sequence_number: u16, rtp_timestamp: u32) -> ServerMediaConcealmentRequired {
        ServerMediaConcealmentRequired {
            track_id: "audio-main".to_owned(),
            sequence_number,
            rtp_timestamp,
        }
    }

    fn frame(sequence_number: u16, rtp_timestamp: u32, samples: Vec<f32>) -> ServerMediaPcmFrame {
        ServerMediaPcmFrame {
            track_id: "audio-main".to_owned(),
            sequence_number,
            rtp_timestamp,
            sample_rate_hz: SERVER_MEDIA_OPUS_SAMPLE_RATE_HZ,
            channels: SERVER_MEDIA_OPUS_CHANNELS,
            samples,
        }
    }

    #[test]
    fn returns_none_without_prior_frame() {
        let mut concealer = ServerMediaPcmConcealer::default();

        assert_eq!(concealer.conceal(&event(8, 7_680)), None);
    }

    #[test]
    fn synthesizes_metadata_and_960_samples_from_prior_frame() {
        let mut concealer = ServerMediaPcmConcealer::default();
        let samples = (0..SERVER_MEDIA_OPUS_FRAME_SIZE)
            .map(|index| index as f32 / SERVER_MEDIA_OPUS_FRAME_SIZE as f32)
            .collect::<Vec<_>>();
        concealer.observe_decoded(frame(7, 6_720, samples.clone()));

        let concealed = concealer.conceal(&event(8, 7_680)).unwrap();

        assert_eq!(concealed.track_id, "audio-main");
        assert_eq!(concealed.sequence_number, 8);
        assert_eq!(concealed.rtp_timestamp, 7_680);
        assert_eq!(concealed.sample_rate_hz, SERVER_MEDIA_OPUS_SAMPLE_RATE_HZ);
        assert_eq!(concealed.channels, SERVER_MEDIA_OPUS_CHANNELS);
        assert_eq!(concealed.samples.len(), SERVER_MEDIA_OPUS_FRAME_SIZE);
        assert!(concealed.samples[0] > concealed.samples[SERVER_MEDIA_OPUS_FRAME_SIZE - 1]);
        assert_eq!(concealed.samples[SERVER_MEDIA_OPUS_FRAME_SIZE - 1], 0.0);
        assert!(concealed.samples.iter().all(|sample| (-1.0..=1.0).contains(sample)));
    }

    #[test]
    fn uses_synthetic_frame_as_seed_for_followup_loss() {
        let mut concealer = ServerMediaPcmConcealer::default();
        concealer.observe_decoded(frame(40, 40_000, vec![0.5; SERVER_MEDIA_OPUS_FRAME_SIZE]));

        let first = concealer.conceal(&event(41, 40_960)).unwrap();
        let second = concealer.conceal(&event(42, 41_920)).unwrap();

        assert_eq!(first.sequence_number, 41);
        assert_eq!(second.sequence_number, 42);
        assert_eq!(second.rtp_timestamp, 41_920);
        assert!(second.samples[0].abs() <= first.samples[0].abs());
    }

    #[test]
    fn repeats_short_prior_frame_to_fill_opus_frame() {
        let mut concealer = ServerMediaPcmConcealer::default();
        concealer.observe_decoded(frame(1, 960, vec![0.25, -0.25]));

        let concealed = concealer.conceal(&event(2, 1_920)).unwrap();

        assert_eq!(concealed.samples.len(), SERVER_MEDIA_OPUS_FRAME_SIZE);
        assert!(concealed.samples.iter().any(|sample| *sample > 0.0));
        assert!(concealed.samples.iter().any(|sample| *sample < 0.0));
    }

    #[test]
    fn invalid_prior_shape_returns_none_without_synthetic_state() {
        let mut concealer = ServerMediaPcmConcealer::default();
        let mut invalid = frame(1, 960, Vec::new());
        invalid.channels = 2;
        concealer.observe_decoded(invalid);

        assert_eq!(concealer.conceal(&event(2, 1_920)), None);

        concealer.observe_decoded(frame(3, 2_880, vec![0.25; SERVER_MEDIA_OPUS_FRAME_SIZE]));
        let concealed = concealer.conceal(&event(4, 3_840)).unwrap();
        assert_eq!(concealed.sequence_number, 4);
        assert_eq!(concealed.samples.len(), SERVER_MEDIA_OPUS_FRAME_SIZE);
    }

    #[test]
    fn invalid_observation_does_not_overwrite_last_valid_frame() {
        let mut concealer = ServerMediaPcmConcealer::default();
        concealer.observe_decoded(frame(1, 960, vec![0.5; SERVER_MEDIA_OPUS_FRAME_SIZE]));
        let mut invalid = frame(2, 1_920, Vec::new());
        invalid.channels = 2;
        concealer.observe_decoded(invalid);

        let concealed = concealer.conceal(&event(3, 2_880)).unwrap();

        assert_eq!(concealed.sequence_number, 3);
        assert_eq!(concealed.samples.len(), SERVER_MEDIA_OPUS_FRAME_SIZE);
        assert!(concealed.samples.iter().any(|sample| sample.abs() > 0.0));
    }
}
```

- [ ] **Step 3: Run tests and observe failure**

Run:

```bash
cargo test -p lyre-webrtc pcm_concealment
```

Expected before implementation: compile fails because `ServerMediaPcmConcealer` is not implemented.

- [ ] **Step 4: Implement concealer**

Add above the tests:

```rust
#[derive(Debug, Default)]
pub struct ServerMediaPcmConcealer {
    last_frame: Option<ServerMediaPcmFrame>,
}

impl ServerMediaPcmConcealer {
    pub fn observe_decoded(&mut self, frame: ServerMediaPcmFrame) {
        if !is_usable_seed(&frame) {
            return;
        }
        self.last_frame = Some(frame);
    }

    pub fn conceal(
        &mut self,
        event: &ServerMediaConcealmentRequired,
    ) -> Option<ServerMediaPcmFrame> {
        let previous = self.last_frame.as_ref()?;
        let samples = synthesize_samples(&previous.samples);
        let frame = ServerMediaPcmFrame {
            track_id: event.track_id.clone(),
            sequence_number: event.sequence_number,
            rtp_timestamp: event.rtp_timestamp,
            sample_rate_hz: previous.sample_rate_hz,
            channels: previous.channels,
            samples,
        };
        self.last_frame = Some(frame.clone());
        Some(frame)
    }
}

fn is_usable_seed(frame: &ServerMediaPcmFrame) -> bool {
    frame.sample_rate_hz == SERVER_MEDIA_OPUS_SAMPLE_RATE_HZ
        && frame.channels == SERVER_MEDIA_OPUS_CHANNELS
        && !frame.samples.is_empty()
}

fn synthesize_samples(previous: &[f32]) -> Vec<f32> {
    let start = previous.len().saturating_sub(SERVER_MEDIA_OPUS_FRAME_SIZE);
    let seed = previous[start..].iter().rev().copied().collect::<Vec<_>>();
    (0..SERVER_MEDIA_OPUS_FRAME_SIZE)
        .map(|index| {
            let sample = seed[index % seed.len()];
            let fade = 0.60
                * (1.0
                    - index as f32 / (SERVER_MEDIA_OPUS_FRAME_SIZE.saturating_sub(1)) as f32);
            (sample * fade).clamp(-1.0, 1.0)
        })
        .collect()
}
```

- [ ] **Step 5: Verify unit tests**

Run:

```bash
cargo test -p lyre-webrtc pcm_concealment
```

Expected: pass.

## Task 2: Wire Concealer Into Audio Ingress

**Files:**
- Modify: `crates/lyre-webrtc/src/stack_audio_ingress.rs`
- Modify: `crates/lyre-webrtc/src/stack.rs`

- [ ] **Step 1: Update helper signature**

In `stack_audio_ingress.rs`, import `ServerMediaPcmConcealer` and update `handle_audio_rtp_packet`:

```rust
pub(crate) fn handle_audio_rtp_packet(
    media_ingress: &MediaIngressRecorder,
    decoder: &mut ServerMediaOpusDecoder,
    jitter_buffer: &mut ServerMediaJitterBuffer,
    concealer: &mut ServerMediaPcmConcealer,
    packet: ServerMediaRtpPacket,
) {
    media_ingress.record_rtp_packet(packet.clone());

    for output in jitter_buffer.push(packet) {
        match output {
            ServerMediaJitterBufferOutput::Packet(packet) => {
                decode_packet(media_ingress, decoder, concealer, packet);
            }
            ServerMediaJitterBufferOutput::ConcealmentRequired(event) => {
                record_concealment(media_ingress, concealer, event);
            }
        }
    }
}
```

- [ ] **Step 2: Update decode success path to seed concealer**

Replace `decode_packet` with:

```rust
fn decode_packet(
    media_ingress: &MediaIngressRecorder,
    decoder: &mut ServerMediaOpusDecoder,
    concealer: &mut ServerMediaPcmConcealer,
    packet: ServerMediaRtpPacket,
) {
    match decoder.decode_packet(&packet) {
        Ok(frame) => {
            concealer.observe_decoded(frame.clone());
            media_ingress.record_pcm_frame(frame);
        }
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
}
```

- [ ] **Step 3: Add concealment record helper**

Add:

```rust
fn record_concealment(
    media_ingress: &MediaIngressRecorder,
    concealer: &mut ServerMediaPcmConcealer,
    event: crate::ServerMediaConcealmentRequired,
) {
    if let Some(frame) = concealer.conceal(&event) {
        media_ingress.record_pcm_frame(frame);
        return;
    }

    media_ingress.record_decode_failure(ServerMediaDecodeFailure {
        track_id: event.track_id,
        sequence_number: event.sequence_number,
        rtp_timestamp: event.rtp_timestamp,
        error: CONCEALMENT_UNAVAILABLE_ERROR.to_owned(),
    });
}
```

- [ ] **Step 4: Instantiate concealer per audio task**

In `stack.rs`, import `ServerMediaPcmConcealer`, add:

```rust
let mut concealer = ServerMediaPcmConcealer::default();
```

beside the decoder and jitter buffer, then call:

```rust
handle_audio_rtp_packet(
    &media_ingress,
    &mut decoder,
    &mut jitter_buffer,
    &mut concealer,
    packet,
);
```

- [ ] **Step 5: Verify existing jitter and malformed tests still pass**

Run:

```bash
cargo test -p lyre-webrtc stack_jitter_buffer_tests::answer_remote_offer_records_loss_when_gap_exceeds_jitter_depth
cargo test -p lyre-webrtc stack_media_tests::answer_remote_offer_records_decode_failure_for_malformed_audio_rtp
```

Expected: the malformed packet test passes unchanged. The existing one-gap jitter loss test is expected to fail until Task 3 removes the old decode-failure expectation because a decoded baseline now lets loss produce PCM.

## Task 3: Add Stack PLC Integration Tests

**Files:**
- Create: `crates/lyre-webrtc/src/stack_pcm_plc_tests.rs`
- Modify: `crates/lyre-webrtc/src/lib.rs`
- Modify: `crates/lyre-webrtc/src/stack_jitter_buffer_tests.rs`

- [ ] **Step 1: Register dedicated PLC test module**

In `crates/lyre-webrtc/src/lib.rs`, add:

```rust
#[cfg(test)]
mod stack_pcm_plc_tests;
```

- [ ] **Step 2: Add PLC stack tests**

Create `crates/lyre-webrtc/src/stack_pcm_plc_tests.rs`:

```rust
use crate::{
    stack::WebRtcStack,
    stack_audio_ingress::CONCEALMENT_UNAVAILABLE_ERROR,
    test_support::{
        encoded_opus_payload_for_test, opus_rtp_packet_for_test,
        server_media_offer_with_valid_opus_sender,
    },
    ServerMediaAnswer, ServerMediaIceCandidate, WebRtcPeerConnectionHandle,
    SERVER_MEDIA_OPUS_FRAME_SIZE,
};
use std::{sync::Arc, time::Duration};
use webrtc::media_stream::track_local::{static_rtp::TrackLocalStaticRTP, TrackLocal};

async fn connected_opus_server() -> (WebRtcPeerConnectionHandle, Arc<TrackLocalStaticRTP>) {
    let server = WebRtcStack::new().create_peer_connection().await.unwrap();
    let offer = server_media_offer_with_valid_opus_sender().await;
    let answer_sdp = server
        .answer_remote_offer(offer.offer_sdp.clone())
        .await
        .unwrap();
    let answer = ServerMediaAnswer {
        room_id: lyre_core::RoomId::default_room(),
        user_id: lyre_core::UserId::from_external("user_01"),
        audio_track_id: "audio-main".to_owned(),
        sdp: answer_sdp,
        state: crate::ServerMediaSessionState::Negotiating,
    };

    for candidate in offer.remote_candidates().await {
        server.add_remote_ice_candidate(candidate).await.unwrap();
    }
    let connected = offer
        .accept_answer(
            &answer,
            server
                .local_ice_candidates()
                .into_iter()
                .map(|candidate| ServerMediaIceCandidate {
                    room_id: answer.room_id.clone(),
                    user_id: answer.user_id.clone(),
                    candidate: candidate.candidate,
                    sdp_mid: candidate.sdp_mid,
                    sdp_mline_index: candidate.sdp_mline_index,
                    username_fragment: candidate.username_fragment,
                })
                .collect(),
        )
        .await;

    (server, connected.track())
}

async fn write_opus_packets(
    track: &Arc<TrackLocalStaticRTP>,
    packets: impl IntoIterator<Item = (u16, u32, Vec<u8>)>,
) {
    for (sequence, timestamp, payload) in packets {
        let _ = track
            .write_rtp(opus_rtp_packet_for_test(sequence, timestamp, payload))
            .await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn missing_packet_after_decoded_baseline_produces_synthetic_pcm_frame() {
    let (server, track) = connected_opus_server().await;
    let payload = encoded_opus_payload_for_test();

    write_opus_packets(
        &track,
        [
            (30, 28_800),
            (32, 30_720),
            (33, 31_680),
            (34, 32_640),
            (35, 33_600),
        ]
        .map(|(sequence, timestamp)| (sequence, timestamp, payload.clone())),
    )
    .await;

    let mut frames = Vec::new();
    for _ in 0..100 {
        frames.extend(server.drain_pcm_frames());
        if let Some(frame) = frames.iter().find(|frame| frame.sequence_number == 31) {
            assert_eq!(frame.rtp_timestamp, 29_760);
            assert_eq!(frame.sample_rate_hz, 48_000);
            assert_eq!(frame.channels, 1);
            assert_eq!(frame.samples.len(), SERVER_MEDIA_OPUS_FRAME_SIZE);
            assert!(frame.samples.iter().any(|sample| sample.abs() > 0.0));
            assert!(server
                .drain_decode_failures()
                .iter()
                .all(|failure| failure.sequence_number != 31));
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("server did not synthesize PCM for missing RTP packet");
}

#[tokio::test]
async fn multiple_missing_packets_produce_multiple_synthetic_pcm_frames() {
    let (server, track) = connected_opus_server().await;
    let payload = encoded_opus_payload_for_test();

    write_opus_packets(
        &track,
        [
            (40, 40_000),
            (43, 42_880),
            (44, 43_840),
            (45, 44_800),
            (46, 45_760),
        ]
        .map(|(sequence, timestamp)| (sequence, timestamp, payload.clone())),
    )
    .await;

    let mut frames = Vec::new();
    for _ in 0..100 {
        frames.extend(server.drain_pcm_frames());
        let loss_frames = frames
            .iter()
            .filter(|frame| matches!(frame.sequence_number, 41 | 42))
            .map(|frame| (frame.sequence_number, frame.rtp_timestamp, frame.samples.len()))
            .collect::<Vec<_>>();
        if loss_frames == vec![
            (41, 40_960, SERVER_MEDIA_OPUS_FRAME_SIZE),
            (42, 41_920, SERVER_MEDIA_OPUS_FRAME_SIZE),
        ] {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("server did not synthesize PCM for multiple missing RTP packets");
}

#[tokio::test]
async fn missing_packet_without_decoded_baseline_records_decode_failure() {
    let (server, track) = connected_opus_server().await;
    let payload = encoded_opus_payload_for_test();

    write_opus_packets(
        &track,
        [
            (30, 28_800, Vec::new()),
            (32, 30_720, payload.clone()),
            (33, 31_680, payload.clone()),
            (34, 32_640, payload.clone()),
            (35, 33_600, payload),
        ],
    )
    .await;

    let mut failures = Vec::new();
    for _ in 0..100 {
        failures.extend(server.drain_decode_failures());
        let saw_malformed_baseline = failures
            .iter()
            .any(|failure| failure.sequence_number == 30 && failure.error == "Input packet empty");
        let saw_loss_without_baseline = failures.iter().any(|failure| {
            failure.sequence_number == 31
                && failure.rtp_timestamp == 29_760
                && failure.error == CONCEALMENT_UNAVAILABLE_ERROR
        });
        if saw_malformed_baseline && saw_loss_without_baseline {
            assert!(server
                .drain_pcm_frames()
                .iter()
                .all(|frame| frame.sequence_number != 31));
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("server did not keep decode failure when no PLC baseline exists");
}

#[tokio::test]
async fn malformed_real_packet_does_not_seed_plc_state() {
    let (server, track) = connected_opus_server().await;
    let payload = encoded_opus_payload_for_test();

    write_opus_packets(
        &track,
        [
            (50, 48_000, Vec::new()),
            (52, 49_920, payload.clone()),
            (53, 50_880, payload.clone()),
            (54, 51_840, payload.clone()),
            (55, 52_800, payload),
        ],
    )
    .await;

    let mut failures = Vec::new();
    let mut frames = Vec::new();
    for _ in 0..100 {
        failures.extend(server.drain_decode_failures());
        frames.extend(server.drain_pcm_frames());
        let saw_malformed = failures
            .iter()
            .any(|failure| failure.sequence_number == 50 && failure.error == "Input packet empty");
        let saw_loss_without_baseline = failures.iter().any(|failure| {
            failure.sequence_number == 51
                && failure.rtp_timestamp == 48_960
                && failure.error == CONCEALMENT_UNAVAILABLE_ERROR
        });
        let decoded_after_loss = frames.iter().any(|frame| frame.sequence_number == 52);
        let no_synthetic_loss = frames.iter().all(|frame| frame.sequence_number != 51);
        if saw_malformed && saw_loss_without_baseline && decoded_after_loss && no_synthetic_loss {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("malformed real packet incorrectly seeded PLC state");
}
```

- [ ] **Step 3: Update old jitter loss tests**

In `stack_jitter_buffer_tests.rs`, remove or update the three tests that expect `CONCEALMENT_UNAVAILABLE_ERROR` after a valid decoded baseline:

- `answer_remote_offer_records_loss_when_gap_exceeds_jitter_depth`
- `answer_remote_offer_records_multiple_loss_failures_with_incrementing_timestamps`
- `answer_remote_offer_records_wrapped_loss_timestamp`

Keep the out-of-order and duplicate tests unchanged. Loss failure behavior is now covered in `stack_pcm_plc_tests.rs` only for the no-baseline and malformed-baseline cases.

- [ ] **Step 4: Verify stack PLC tests**

Run:

```bash
cargo test -p lyre-webrtc stack_pcm_plc_tests
cargo test -p lyre-webrtc stack_jitter_buffer_tests
```

Expected: pass.

## Task 4: Implementation Review, Docs, Final Verification, Commit

**Files:**
- Modify after implementation review approval: `MEMORY.md`
- Modify after implementation review approval: `docs/roadmap.md`

- [ ] **Step 1: Run pre-review verification**

Run:

```bash
cargo fmt --all --check
cargo test -p lyre-webrtc pcm_concealment
cargo test -p lyre-webrtc stack_pcm_plc_tests
cargo test -p lyre-webrtc stack_jitter_buffer_tests
cargo test -p lyre-webrtc stack_media_tests::answer_remote_offer_records_decode_failure_for_malformed_audio_rtp
```

- [ ] **Step 2: Independent implementation review**

Dispatch a fresh reviewer with the approved spec, this plan, diff, and verification output. Required verdict:

```text
VERDICT: APPROVE
```

Do not update docs or commit until implementation review approves.

- [ ] **Step 3: Update MEMORY.md**

Append:

```markdown
## 2026-06-15 Server Media PCM PLC

- Added deterministic Lyre-owned PCM packet loss concealment for server-media ingress.
- Missing RTP packets after a decoded baseline now produce 48 kHz mono 960-sample synthetic PCM fallback frames using a faded copy of the previous frame.
- This is not Opus-native PLC or FEC; missing packets before a usable baseline still record an internal decode failure.
```

- [ ] **Step 4: Update docs/roadmap.md**

Move `Add real PCM packet loss concealment synthesis for server media ingress.` from Next to Completed as:

```markdown
- Deterministic PCM packet loss concealment synthesis for server media ingress after jitter-buffer loss detection.
```

Keep remaining Next items unchanged.

- [ ] **Step 5: Final verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path Cargo.toml --workspace
cd frontend && npm test -- --run
cd frontend && npm run typecheck
cd frontend && npm run lint
cd frontend && npm run build
git diff --check
```

Expected: all pass. Next build may print the existing Node localStorage experimental warning; it is acceptable if the command exits 0.

- [ ] **Step 6: Stage only current increment files**

Stage:

```bash
git add \
  MEMORY.md \
  docs/roadmap.md \
  docs/superpowers/specs/2026-06-15-server-media-pcm-plc-design.md \
  docs/superpowers/plans/2026-06-15-server-media-pcm-plc.md \
  crates/lyre-webrtc/src/lib.rs \
  crates/lyre-webrtc/src/pcm_concealment.rs \
  crates/lyre-webrtc/src/stack.rs \
  crates/lyre-webrtc/src/stack_audio_ingress.rs \
  crates/lyre-webrtc/src/stack_jitter_buffer_tests.rs \
  crates/lyre-webrtc/src/stack_pcm_plc_tests.rs
```

Do not stage existing unrelated untracked SDD artifacts:

```text
docs/superpowers/plans/2026-06-15-processed-audio-webrtc-egress.md
docs/superpowers/plans/2026-06-15-server-media-runtime-pump.md
docs/superpowers/specs/2026-06-15-processed-audio-webrtc-egress-design.md
docs/superpowers/specs/2026-06-15-server-media-runtime-pump-design.md
```

- [ ] **Step 7: Commit and push**

Use Lore commit format. Example intent:

```text
Synthesize PCM for recoverable server media packet loss

Constraint: SDD gates required approved spec, approved plan, independent implementation review, docs updates, and fresh verification before commit.
Rejected: Opus-native PLC | current opus-rs decoder path does not expose PLC/FEC.
Confidence: high
Scope-risk: moderate
Directive: Do not describe this as Opus PLC or FEC; it is a deterministic Lyre PCM fallback.
Tested: cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path Cargo.toml --workspace; frontend npm test -- --run; npm run typecheck; npm run lint; npm run build; git diff --check
Not-tested: Subjective audio quality under real network packet loss
```

Push to the current branch after committing.
