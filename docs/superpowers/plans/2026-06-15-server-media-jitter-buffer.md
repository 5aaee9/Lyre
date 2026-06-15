# Server Media Jitter Buffer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decode server-media Opus RTP packets in sequence order with bounded jitter buffering and explicit loss detection.

**Architecture:** Add a focused `lyre-webrtc` jitter buffer module that owns sequence ordering, duplicate/stale dropping, and concealment-needed events. Wire the existing WebRTC track loop to decode only buffer-emitted packets and record loss events through existing internal decode-failure snapshots.

**Tech Stack:** Rust, `BTreeMap`, existing `lyre-webrtc` RTP/Opus DTOs, existing Axum/web runtime tests.

**Reviewed Spec:** `docs/superpowers/specs/2026-06-15-server-media-jitter-buffer-design.md`

---

## File Structure

- Create `crates/lyre-webrtc/src/jitter_buffer.rs`: `ServerMediaJitterBuffer`, `ServerMediaJitterBufferOutput`, sequence helpers, and unit tests.
- Modify `crates/lyre-webrtc/src/lib.rs`: expose the new module internally and re-export only DTOs needed by `stack.rs` if required.
- Create `crates/lyre-webrtc/src/stack_audio_ingress.rs`: own audio RTP decode/output handling so `stack.rs` stays below the 400 LOC split threshold.
- Modify `crates/lyre-webrtc/src/stack.rs`: instantiate the jitter buffer per audio track and call the helper instead of inlining new decode logic.
- Modify `crates/lyre-webrtc/src/test_support.rs`: add ordered custom RTP packet helpers for integration tests.
- Create `crates/lyre-webrtc/src/stack_jitter_buffer_tests.rs`: cover out-of-order decode, duplicate dropping, and loss failure.
- Modify `crates/lyre-webrtc/src/lib.rs`: register the dedicated jitter-buffer stack test module.
- Modify after implementation review approval: `MEMORY.md` and `docs/roadmap.md`.

Keep changed Rust files below 400 LOC. Put new jitter-buffer unit tests in the new module instead of expanding `stack.rs`.

## Task 0: SDD Pre-Implementation Gate

**Files:**
- Read: `docs/superpowers/specs/2026-06-15-server-media-jitter-buffer-design.md`
- Read: `docs/superpowers/plans/2026-06-15-server-media-jitter-buffer.md`

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

## Task 1: Add Jitter Buffer Unit

**Files:**
- Create: `crates/lyre-webrtc/src/jitter_buffer.rs`
- Modify: `crates/lyre-webrtc/src/lib.rs`

- [ ] **Step 1: Add module declaration**

In `crates/lyre-webrtc/src/lib.rs`, add:

```rust
mod jitter_buffer;
```

and re-export:

```rust
pub(crate) use jitter_buffer::{ServerMediaJitterBuffer, ServerMediaJitterBufferOutput};
```

- [ ] **Step 2: Add failing jitter buffer tests**

Create `crates/lyre-webrtc/src/jitter_buffer.rs` with tests first:

```rust
use std::collections::BTreeMap;

use crate::{ServerMediaRtpPacket, SERVER_MEDIA_OPUS_FRAME_SIZE};

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(sequence_number: u16, timestamp: u32) -> ServerMediaRtpPacket {
        ServerMediaRtpPacket {
            track_id: "audio-main".to_owned(),
            sequence_number,
            timestamp,
            marker: true,
            payload_type: 111,
            payload: vec![sequence_number as u8],
        }
    }

    fn emitted_sequences(outputs: &[ServerMediaJitterBufferOutput]) -> Vec<u16> {
        outputs
            .iter()
            .filter_map(|output| match output {
                ServerMediaJitterBufferOutput::Packet(packet) => Some(packet.sequence_number),
                ServerMediaJitterBufferOutput::ConcealmentRequired(_) => None,
            })
            .collect()
    }

    #[test]
    fn emits_in_order_packets_immediately() {
        let mut buffer = ServerMediaJitterBuffer::default();

        assert_eq!(emitted_sequences(&buffer.push(packet(7, 960))), vec![7]);
        assert_eq!(emitted_sequences(&buffer.push(packet(8, 1920))), vec![8]);
    }

    #[test]
    fn reorders_packets_once_gap_arrives() {
        let mut buffer = ServerMediaJitterBuffer::default();

        assert_eq!(emitted_sequences(&buffer.push(packet(10, 960))), vec![10]);
        assert!(buffer.push(packet(12, 2880)).is_empty());
        assert_eq!(emitted_sequences(&buffer.push(packet(11, 1920))), vec![11, 12]);
    }

    #[test]
    fn drops_duplicates_and_stale_packets() {
        let mut buffer = ServerMediaJitterBuffer::default();

        assert_eq!(emitted_sequences(&buffer.push(packet(1, 960))), vec![1]);
        assert!(buffer.push(packet(1, 960)).is_empty());
        assert_eq!(emitted_sequences(&buffer.push(packet(2, 1920))), vec![2]);
        assert!(buffer.push(packet(1, 960)).is_empty());
    }

    #[test]
    fn records_concealment_when_gap_exceeds_depth() {
        let mut buffer = ServerMediaJitterBuffer::default();

        assert_eq!(emitted_sequences(&buffer.push(packet(20, 20_000))), vec![20]);
        assert!(buffer.push(packet(22, 21_920)).is_empty());
        assert!(buffer.push(packet(23, 22_880)).is_empty());
        assert!(buffer.push(packet(24, 23_840)).is_empty());
        let outputs = buffer.push(packet(25, 24_800));

        assert!(matches!(
            &outputs[0],
            ServerMediaJitterBufferOutput::ConcealmentRequired(event)
                if event.sequence_number == 21 && event.rtp_timestamp == 20_960
        ));
        assert_eq!(emitted_sequences(&outputs), vec![22, 23, 24, 25]);
    }

    #[test]
    fn handles_sequence_and_timestamp_wraparound() {
        let mut buffer = ServerMediaJitterBuffer::default();

        assert_eq!(
            emitted_sequences(&buffer.push(packet(65_534, u32::MAX - 959))),
            vec![65_534]
        );
        assert_eq!(emitted_sequences(&buffer.push(packet(65_535, 0))), vec![65_535]);
        assert_eq!(emitted_sequences(&buffer.push(packet(0, 960))), vec![0]);
        assert_eq!(emitted_sequences(&buffer.push(packet(1, 1920))), vec![1]);
    }

    #[test]
    fn records_multiple_gap_concealment_events_with_incrementing_timestamps() {
        let mut buffer = ServerMediaJitterBuffer::default();

        assert_eq!(emitted_sequences(&buffer.push(packet(40, 40_000))), vec![40]);
        for (sequence, timestamp) in [(43, 42_880), (44, 43_840), (45, 44_800)] {
            let _ = buffer.push(packet(sequence, timestamp));
        }
        let outputs = buffer.push(packet(46, 45_760));
        let gaps = outputs
            .iter()
            .filter_map(|output| match output {
                ServerMediaJitterBufferOutput::ConcealmentRequired(event) => Some((
                    event.sequence_number,
                    event.rtp_timestamp,
                )),
                ServerMediaJitterBufferOutput::Packet(_) => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(gaps, vec![(41, 40_960), (42, 41_920)]);
        assert_eq!(emitted_sequences(&outputs), vec![43, 44, 45, 46]);
    }

    #[test]
    fn concealment_timestamps_wrap_with_u32_addition() {
        let mut buffer = ServerMediaJitterBuffer::default();

        assert_eq!(
            emitted_sequences(&buffer.push(packet(100, u32::MAX - 479))),
            vec![100]
        );
        for sequence in [102, 103, 104] {
            let _ = buffer.push(packet(sequence, 0));
        }
        let outputs = buffer.push(packet(105, 960));
        assert!(matches!(
            &outputs[0],
            ServerMediaJitterBufferOutput::ConcealmentRequired(event)
                if event.sequence_number == 101 && event.rtp_timestamp == 480
        ));
    }
}
```

- [ ] **Step 3: Run tests and observe failure**

Run:

```bash
cargo test -p lyre-webrtc jitter_buffer
```

Expected before implementation: compile fails because the jitter buffer types are not implemented.

- [ ] **Step 4: Implement jitter buffer**

Add above tests:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerMediaConcealmentRequired {
    pub track_id: String,
    pub sequence_number: u16,
    pub rtp_timestamp: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerMediaJitterBufferOutput {
    Packet(ServerMediaRtpPacket),
    ConcealmentRequired(ServerMediaConcealmentRequired),
}

#[derive(Debug)]
pub struct ServerMediaJitterBuffer {
    pending: BTreeMap<u16, ServerMediaRtpPacket>,
    next_sequence: Option<u16>,
    next_timestamp: Option<u32>,
    max_depth: usize,
    track_id: Option<String>,
}

impl Default for ServerMediaJitterBuffer {
    fn default() -> Self {
        Self {
            pending: BTreeMap::new(),
            next_sequence: None,
            next_timestamp: None,
            max_depth: 3,
            track_id: None,
        }
    }
}

impl ServerMediaJitterBuffer {
    pub fn push(&mut self, packet: ServerMediaRtpPacket) -> Vec<ServerMediaJitterBufferOutput> {
        if self.next_sequence.is_none() {
            self.next_sequence = Some(packet.sequence_number);
            self.next_timestamp = Some(packet.timestamp);
            self.track_id = Some(packet.track_id.clone());
        }
        let next_sequence = self.next_sequence.expect("next sequence initialized");
        if sequence_distance(next_sequence, packet.sequence_number) < 0 {
            return Vec::new();
        }
        self.pending.entry(packet.sequence_number).or_insert(packet);
        self.drain_ready()
    }

    fn drain_ready(&mut self) -> Vec<ServerMediaJitterBufferOutput> {
        let mut outputs = Vec::new();
        while let Some(sequence) = self.next_sequence {
            if let Some(packet) = self.pending.remove(&sequence) {
                self.next_sequence = Some(sequence.wrapping_add(1));
                self.next_timestamp =
                    Some(packet.timestamp.wrapping_add(SERVER_MEDIA_OPUS_FRAME_SIZE as u32));
                outputs.push(ServerMediaJitterBufferOutput::Packet(packet));
                continue;
            }
            if self.pending.len() <= self.max_depth {
                break;
            }
            let rtp_timestamp = self.next_timestamp.unwrap_or_default();
            outputs.push(ServerMediaJitterBufferOutput::ConcealmentRequired(
                ServerMediaConcealmentRequired {
                    track_id: self.track_id.clone().unwrap_or_default(),
                    sequence_number: sequence,
                    rtp_timestamp,
                },
            ));
            self.next_sequence = Some(sequence.wrapping_add(1));
            self.next_timestamp = Some(rtp_timestamp.wrapping_add(SERVER_MEDIA_OPUS_FRAME_SIZE as u32));
        }
        outputs
    }
}

fn sequence_distance(from: u16, to: u16) -> i16 {
    to.wrapping_sub(from) as i16
}
```

- [ ] **Step 5: Verify unit tests**

Run:

```bash
cargo test -p lyre-webrtc jitter_buffer
```

Expected: pass.

## Task 2: Wire Jitter Buffer Into WebRTC Track Decode

**Files:**
- Create: `crates/lyre-webrtc/src/stack_audio_ingress.rs`
- Modify: `crates/lyre-webrtc/src/lib.rs`
- Modify: `crates/lyre-webrtc/src/stack.rs`

- [ ] **Step 1: Register audio ingress helper module**

In `crates/lyre-webrtc/src/lib.rs`, add:

```rust
mod stack_audio_ingress;
```

- [ ] **Step 2: Add packet decode helper**

Create `crates/lyre-webrtc/src/stack_audio_ingress.rs` with:

```rust
use tracing::warn;

use crate::{
    media_ingress::MediaIngressRecorder, ServerMediaDecodeError, ServerMediaDecodeFailure,
    ServerMediaJitterBuffer, ServerMediaJitterBufferOutput, ServerMediaOpusDecoder,
    ServerMediaRtpPacket,
};

pub(crate) const CONCEALMENT_UNAVAILABLE_ERROR: &str =
    "packet loss concealment required but not available with current Opus decoder";

pub(crate) fn handle_audio_rtp_packet(
    media_ingress: &MediaIngressRecorder,
    decoder: &mut ServerMediaOpusDecoder,
    jitter_buffer: &mut ServerMediaJitterBuffer,
    packet: ServerMediaRtpPacket,
) {
    media_ingress.record_rtp_packet(packet.clone());
    for output in jitter_buffer.push(packet) {
        match output {
            ServerMediaJitterBufferOutput::Packet(packet) => {
                decode_packet(media_ingress, decoder, packet);
            }
            ServerMediaJitterBufferOutput::ConcealmentRequired(event) => {
                media_ingress.record_decode_failure(ServerMediaDecodeFailure {
                    track_id: event.track_id,
                    sequence_number: event.sequence_number,
                    rtp_timestamp: event.rtp_timestamp,
                    error: CONCEALMENT_UNAVAILABLE_ERROR.to_owned(),
                });
            }
        }
    }
}

fn decode_packet(
    media_ingress: &MediaIngressRecorder,
    decoder: &mut ServerMediaOpusDecoder,
    packet: ServerMediaRtpPacket,
) {
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
}
```

- [ ] **Step 3: Import jitter buffer and helper types**

Add to the crate imports in `stack.rs`:

```rust
stack_audio_ingress::handle_audio_rtp_packet,
ServerMediaJitterBuffer,
```

- [ ] **Step 4: Replace direct decode with helper call**

Inside the audio `tokio::spawn` in `on_track`, add:

```rust
let mut jitter_buffer = ServerMediaJitterBuffer::default();
```

Replace:

```rust
media_ingress.record_rtp_packet(packet.clone());
match decoder.decode_packet(&packet) { ... }
```

with:

```rust
handle_audio_rtp_packet(&media_ingress, &mut decoder, &mut jitter_buffer, packet);
```

This extraction should keep `stack.rs` under the 400 LOC split threshold while preserving existing malformed decode failure behavior.

- [ ] **Step 5: Run existing media tests**

Run:

```bash
cargo test -p lyre-webrtc stack_media_tests::answer_remote_offer_records_audio_track_rtp_packet_and_pcm_frame
cargo test -p lyre-webrtc stack_media_tests::answer_remote_offer_records_decode_failure_for_malformed_audio_rtp
```

Expected: pass.

## Task 3: Add Integration Test Helpers and Stack Jitter Tests

**Files:**
- Modify: `crates/lyre-webrtc/src/test_support.rs`
- Create: `crates/lyre-webrtc/src/stack_jitter_buffer_tests.rs`
- Modify: `crates/lyre-webrtc/src/lib.rs`

- [ ] **Step 1: Add custom RTP helper in test_support**

Change `fn encoded_opus_payload() -> Vec<u8>` to:

```rust
pub fn encoded_opus_payload_for_test() -> Vec<u8> {
    ...
}
```

Keep a private wrapper if existing local code uses `encoded_opus_payload()`:

```rust
fn encoded_opus_payload() -> Vec<u8> {
    encoded_opus_payload_for_test()
}
```

Add:

```rust
pub fn opus_rtp_packet_for_test(
    sequence_number: u16,
    timestamp: u32,
    payload: Vec<u8>,
) -> rtc::rtp::Packet {
    rtc::rtp::Packet {
        header: rtc::rtp::Header {
            version: 2,
            sequence_number,
            timestamp,
            marker: true,
            payload_type: 111,
            ssrc: 1234,
            ..Default::default()
        },
        payload: Bytes::from(payload),
    }
}
```

Update private `test_rtp_packet` to call `opus_rtp_packet_for_test(42, 1234, payload)`.

- [ ] **Step 2: Register stack jitter test module**

In `crates/lyre-webrtc/src/lib.rs`, add:

```rust
#[cfg(test)]
mod stack_jitter_buffer_tests;
```

- [ ] **Step 3: Add stack jitter integration tests**

In `stack_jitter_buffer_tests.rs`, add a helper local to tests:

```rust
async fn connected_opus_server() -> (WebRtcPeerConnectionHandle, Arc<TrackLocalStaticRTP>) {
    ...
}
```

Use the existing setup code from `answer_remote_offer_records_audio_track_rtp_packet_and_pcm_frame`.

Add tests:

```rust
#[tokio::test]
async fn answer_remote_offer_decodes_out_of_order_rtp_in_sequence_order() {
    let (server, track) = connected_opus_server().await;
    let payload = crate::test_support::encoded_opus_payload_for_test();
    let mut decoded = Vec::new();

    let _ = track.write_rtp(crate::test_support::opus_rtp_packet_for_test(10, 9_600, payload.clone())).await;
    let _ = track.write_rtp(crate::test_support::opus_rtp_packet_for_test(12, 11_520, payload.clone())).await;
    let _ = track.write_rtp(crate::test_support::opus_rtp_packet_for_test(11, 10_560, payload)).await;

    for _ in 0..100 {
        decoded.extend(server.drain_pcm_frames());
        let frames = &decoded;
        let sequences = frames.iter().map(|frame| frame.sequence_number).collect::<Vec<_>>();
        if sequences == vec![10, 11, 12] {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("server did not decode out-of-order RTP in sequence order");
}
```

Add duplicate and loss tests with the same helper:

```rust
#[tokio::test]
async fn answer_remote_offer_drops_duplicate_rtp_packets() {
    let (server, track) = connected_opus_server().await;
    let payload = crate::test_support::encoded_opus_payload_for_test();
    let mut decoded = Vec::new();

    let _ = track.write_rtp(crate::test_support::opus_rtp_packet_for_test(20, 19_200, payload.clone())).await;
    let _ = track.write_rtp(crate::test_support::opus_rtp_packet_for_test(20, 19_200, payload.clone())).await;
    let _ = track.write_rtp(crate::test_support::opus_rtp_packet_for_test(21, 20_160, payload)).await;

    for _ in 0..100 {
        decoded.extend(server.drain_pcm_frames());
        let frames = &decoded;
        let sequences = frames.iter().map(|frame| frame.sequence_number).collect::<Vec<_>>();
        if sequences == vec![20, 21] {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            decoded.extend(server.drain_pcm_frames());
            assert_eq!(
                decoded.iter().map(|frame| frame.sequence_number).collect::<Vec<_>>(),
                vec![20, 21]
            );
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("server did not drop duplicate RTP packet");
}

#[tokio::test]
async fn answer_remote_offer_records_loss_when_gap_exceeds_jitter_depth() {
    let (server, track) = connected_opus_server().await;
    let payload = crate::test_support::encoded_opus_payload_for_test();

    for (sequence, timestamp) in [(30, 28_800), (32, 30_720), (33, 31_680), (34, 32_640), (35, 33_600)] {
        let _ = track.write_rtp(crate::test_support::opus_rtp_packet_for_test(sequence, timestamp, payload.clone())).await;
    }

    for _ in 0..100 {
        let failures = server.drain_decode_failures();
        if failures.iter().any(|failure| {
            failure.sequence_number == 31
                && failure.rtp_timestamp == 29_760
                && failure.error == "packet loss concealment required but not available with current Opus decoder"
        }) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("server did not record missing RTP packet as concealment-unavailable failure");
}

#[tokio::test]
async fn answer_remote_offer_records_multiple_loss_failures_with_incrementing_timestamps() {
    let (server, track) = connected_opus_server().await;
    let payload = crate::test_support::encoded_opus_payload_for_test();

    for (sequence, timestamp) in [(40, 40_000), (43, 42_880), (44, 43_840), (45, 44_800), (46, 45_760)] {
        let _ = track.write_rtp(crate::test_support::opus_rtp_packet_for_test(sequence, timestamp, payload.clone())).await;
    }

    let mut failures = Vec::new();
    for _ in 0..100 {
        failures.extend(server.drain_decode_failures());
        let loss_events = failures
            .iter()
            .filter(|failure| failure.error == crate::stack_audio_ingress::CONCEALMENT_UNAVAILABLE_ERROR)
            .map(|failure| (failure.sequence_number, failure.rtp_timestamp))
            .collect::<Vec<_>>();
        if loss_events == vec![(41, 40_960), (42, 41_920)] {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("server did not record multiple missing RTP packets with deterministic timestamps");
}

#[tokio::test]
async fn answer_remote_offer_records_wrapped_loss_timestamp() {
    let (server, track) = connected_opus_server().await;
    let payload = crate::test_support::encoded_opus_payload_for_test();

    for (sequence, timestamp) in [(100, u32::MAX - 479), (102, 0), (103, 0), (104, 0), (105, 960)] {
        let _ = track.write_rtp(crate::test_support::opus_rtp_packet_for_test(sequence, timestamp, payload.clone())).await;
    }

    for _ in 0..100 {
        let failures = server.drain_decode_failures();
        if failures.iter().any(|failure| {
            failure.sequence_number == 101
                && failure.rtp_timestamp == 480
                && failure.error == crate::stack_audio_ingress::CONCEALMENT_UNAVAILABLE_ERROR
        }) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("server did not record wrapped missing RTP timestamp");
}
```

- [ ] **Step 4: Verify integration tests**

Run:

```bash
cargo test -p lyre-webrtc stack_jitter_buffer_tests::answer_remote_offer_decodes_out_of_order_rtp_in_sequence_order
cargo test -p lyre-webrtc stack_jitter_buffer_tests::answer_remote_offer_drops_duplicate_rtp_packets
cargo test -p lyre-webrtc stack_jitter_buffer_tests::answer_remote_offer_records_loss_when_gap_exceeds_jitter_depth
cargo test -p lyre-webrtc stack_jitter_buffer_tests::answer_remote_offer_records_multiple_loss_failures_with_incrementing_timestamps
cargo test -p lyre-webrtc stack_jitter_buffer_tests::answer_remote_offer_records_wrapped_loss_timestamp
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
cargo test -p lyre-webrtc jitter_buffer
cargo test -p lyre-webrtc jitter_buffer::tests::records_multiple_gap_concealment_events_with_incrementing_timestamps
cargo test -p lyre-webrtc jitter_buffer::tests::concealment_timestamps_wrap_with_u32_addition
cargo test -p lyre-webrtc stack_jitter_buffer_tests::answer_remote_offer_decodes_out_of_order_rtp_in_sequence_order
cargo test -p lyre-webrtc stack_jitter_buffer_tests::answer_remote_offer_drops_duplicate_rtp_packets
cargo test -p lyre-webrtc stack_jitter_buffer_tests::answer_remote_offer_records_loss_when_gap_exceeds_jitter_depth
cargo test -p lyre-webrtc stack_jitter_buffer_tests::answer_remote_offer_records_multiple_loss_failures_with_incrementing_timestamps
cargo test -p lyre-webrtc stack_jitter_buffer_tests::answer_remote_offer_records_wrapped_loss_timestamp
```

- [ ] **Step 2: Independent implementation review**

Dispatch a fresh reviewer with the approved spec, this plan, diff, and verification output. Required verdict:

```text
VERDICT: APPROVE
```

Fix and re-review until approved.

- [ ] **Step 3: Update docs**

Update `MEMORY.md` with:

- Server media ingress now has a bounded RTP jitter buffer.
- The buffer reorders packets, drops duplicate/stale packets, and records missing packets as explicit internal failures.
- Real PLC PCM synthesis is still future work because the current Opus decoder API does not expose PLC.

Update `docs/roadmap.md`:

- Move jitter buffering and loss detection to Completed.
- Keep packet loss concealment PCM synthesis in Next.

- [ ] **Step 4: Final verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path Cargo.toml --workspace
cd frontend
npm test -- --run
npm run typecheck
npm run lint
npm run build
cd ..
git diff --check
```

Expected: all pass.

- [ ] **Step 5: Commit and push**

Stage only this increment's files plus this reviewed spec/plan. Leave unrelated untracked SDD artifacts untouched. Create a Lore-format commit and push current branch/upstream.
