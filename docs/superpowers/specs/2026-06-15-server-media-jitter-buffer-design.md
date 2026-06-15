# Server Media Jitter Buffer Design

## Scope

Add a bounded server-media RTP jitter buffer before Opus decode so ingress handles out-of-order, duplicate, and stale packets deterministically.

This increment covers:

- Adding a Lyre-owned jitter buffer inside `lyre-webrtc`.
- Reordering incoming audio RTP packets by 16-bit RTP sequence number before decode.
- Dropping duplicate and stale packets.
- Limiting buffering delay with a small fixed packet depth.
- Recording packet-loss gaps as internal decode failures with explicit context.
- Keeping the existing decoded PCM frame DTO and media runtime API unchanged.

This increment does not synthesize packet loss concealment PCM. The current `opus-rs` public decoder API rejects empty packets and does not expose a PLC call equivalent. Real PLC remains future work unless Lyre switches decoder API, extends `opus-rs`, or adopts a decoder that exposes PLC/FEC.

## Current Behavior

`WebRtcStack::on_track` currently decodes each received Opus RTP packet immediately in arrival order. That means:

- Out-of-order packets produce PCM frames out of sequence.
- Duplicate packets can produce duplicate PCM frames.
- Missing packets are invisible except as sequence jumps in later frames.

## Jitter Buffer Behavior

Add a `ServerMediaJitterBuffer` in `lyre-webrtc`.

The buffer should:

- Accept `ServerMediaRtpPacket` values for one audio track/decoder task.
- Keep packets in a `BTreeMap<u16, ServerMediaRtpPacket>` keyed by RTP sequence number.
- Track `next_sequence: Option<u16>`.
- Track `next_timestamp: Option<u32>`.
- Track a fixed `max_depth` of `3` packets.
- On the first packet, set `next_sequence` to that packet's sequence number and `next_timestamp` to that packet's RTP timestamp.
- Drop packets with sequence numbers older than `next_sequence`.
- Drop duplicates already stored in the pending map.
- Emit ready packets in ascending sequence order whenever `next_sequence` is present.
- If the expected `next_sequence` is missing but the pending map length exceeds `max_depth`, record a `ServerMediaConcealmentRequired` event for the missing sequence and current `next_timestamp`, advance `next_sequence` by one with wrapping addition, advance `next_timestamp` by `SERVER_MEDIA_OPUS_FRAME_SIZE as u32` with wrapping addition, and continue draining ready packets.
- When emitting a real packet, advance `next_sequence` by one with wrapping addition and set `next_timestamp` to the emitted packet timestamp plus `SERVER_MEDIA_OPUS_FRAME_SIZE as u32` with wrapping addition.

Sequence comparisons must handle normal u16 wraparound. Use a helper equivalent to:

```rust
fn sequence_distance(from: u16, to: u16) -> i16 {
    to.wrapping_sub(from) as i16
}
```

Where negative means `to` is older than `from`, zero means same, positive means newer within the normal half-range.

## Concealment Metadata

Add:

```rust
pub struct ServerMediaConcealmentRequired {
    pub track_id: String,
    pub sequence_number: u16,
    pub rtp_timestamp: u32,
}
```

This is an internal signal that a packet was missing long enough for the jitter buffer to move on. Because this increment does not synthesize PLC PCM, `WebRtcStack::on_track` should convert each concealment event into a `ServerMediaDecodeFailure` with:

- `track_id` from the missing sequence's track.
- `sequence_number` set to the missing RTP sequence.
- `rtp_timestamp` set to the jitter buffer's deterministic expected timestamp for that missing sequence.
- `error` set to `packet loss concealment required but not available with current Opus decoder`.

Timestamp rules are deterministic:

- First packet initializes expected sequence and timestamp.
- Every emitted real packet advances expected timestamp to `packet.timestamp.wrapping_add(SERVER_MEDIA_OPUS_FRAME_SIZE as u32)`.
- Every missing packet uses the current expected timestamp in its concealment event, then advances expected timestamp by `SERVER_MEDIA_OPUS_FRAME_SIZE as u32` with wrapping addition.
- Multi-packet gaps produce one concealment event per missing sequence as the buffer depth forces advancement.
- First-packet loss cannot be detected because there is no baseline sequence or timestamp before the first received packet.

This keeps the loss visible in existing internal failure snapshots without adding a public endpoint.

## Integration

In `WebRtcStack::on_track`:

- Continue recording every received RTP packet in `MediaIngressRecorder`.
- Push audio RTP packets into `ServerMediaJitterBuffer`.
- Decode only packets emitted by the buffer.
- Record decode failures for decode errors exactly as today.
- Record concealment-required events as decode failures using the message above.

No REST, WebRPC, or frontend changes are required.

## Testing

Add unit tests in `lyre-webrtc` for the jitter buffer:

- In-order packets are emitted immediately.
- Out-of-order packets are emitted in sequence order once the gap arrives.
- Duplicate packets are dropped.
- Stale packets older than `next_sequence` are dropped.
- When a missing sequence exceeds `max_depth`, the buffer records a concealment-required event and then emits later ready packets.
- Sequence wraparound works for `65534, 65535, 0, 1`.
- Concealment event timestamps advance by 960 samples and wrap with `u32::wrapping_add`.

Add Opus decoder or stack-level tests proving:

- The server records PCM frames in sequence order when RTP packets arrive out of order.
- Duplicate RTP packets do not produce duplicate PCM frames.
- A lost packet produces an internal decode failure with the explicit concealment-unavailable message.
- A lost packet's failure uses the deterministic expected RTP timestamp, including multi-gap and wrapping behavior.

Use internal test helpers only; do not expose raw RTP, jitter, or loss state through REST.

## Documentation

After implementation review approval:

- Update `MEMORY.md` to record that Lyre now has an ingress jitter buffer with loss detection but not PCM PLC synthesis.
- Update `docs/roadmap.md`:
  - Move jitter buffering and loss detection to Completed.
  - Keep real packet loss concealment PCM synthesis in Next.

## Acceptance Criteria

- Opus packets are decoded in RTP sequence order for normal reordering within the buffer depth.
- Duplicate/stale packets do not produce duplicate decoded PCM.
- Missing packets past the jitter depth are recorded as explicit internal failures.
- Loss failure `rtp_timestamp` values follow the deterministic 960-sample wrapping timestamp rule.
- The implementation does not claim real PLC PCM synthesis.
- Rust formatting, clippy, and workspace tests pass.
