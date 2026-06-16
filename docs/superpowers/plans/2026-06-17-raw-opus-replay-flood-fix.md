# Raw Opus Replay Flood Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent raw Opus egress from replaying historical RTP packets to healthy recipients when another subscribed recipient is missing or failing.

**Architecture:** Keep the raw Opus pump's source-level forwarding cursor, but advance it after observing a source packet batch even when a recipient send fails. Failed recipient sends drop those packets instead of retrying them, matching real-time voice semantics.

**Tech Stack:** Rust, tokio, lyre-web server media tests, lyre-webrtc test support.

---

### Task 1: Lock Raw Opus Replay Regression

**Files:**
- Modify: `crates/lyre-web/src/processed_audio_webrtc_egress_pump_tests.rs`

- [ ] **Step 1: Add the failing regression test**

Add this test near the existing raw Opus relay tests:

```rust
#[tokio::test]
async fn server_relay_off_noise_does_not_replay_history_when_one_recipient_is_missing() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    register_audio_track(&state, &room_id, "source");
    register_audio_track(&state, &room_id, "subscribed");
    register_audio_track(&state, &room_id, "missing");
    for user_id in ["subscribed", "missing"] {
        state
            .media_relays
            .update_subscriptions(
                room_id.clone(),
                lyre_core::media::UpdateMediaRelaySubscriptionsRequest {
                    user_id: UserId::from_external(user_id),
                    source_user_ids: vec![UserId::from_external("source")],
                },
            )
            .unwrap();
    }
    let source = connect_test_offer(&state, &room_id, "source").await;
    let subscribed_key = ServerMediaSessionKey {
        room_id: room_id.clone(),
        user_id: UserId::from_external("subscribed"),
    };
    let _subscribed = connect_test_offer(&state, &room_id, "subscribed").await;

    source.send_valid_opus_packets(1).await;

    for _ in 0..150 {
        let subscribed = state
            .server_media_peer_connection_for_test(&subscribed_key)
            .unwrap();
        if !subscribed.sent_egress_rtp_packets_for_test().is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
            assert_eq!(subscribed.sent_egress_rtp_packets_for_test().len(), 1);
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("server relay audio RTP did not reach subscribed recipient peer connection");
}
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
cargo test -p lyre-web server_relay_off_noise_does_not_replay_history_when_one_recipient_is_missing -- --nocapture
```

Expected before the implementation fix: FAIL because the healthy subscribed recipient receives the same raw Opus packet more than once.

### Task 2: Drop Failed Raw Opus Sends Without Replaying History

**Files:**
- Modify: `crates/lyre-web/src/raw_opus_webrtc_egress_pump.rs`

- [ ] **Step 1: Advance the source cursor after each observed packet batch**

Keep the existing source-level map:

```rust
HashMap::<ServerMediaSessionKey, usize>::new()
```

Replace the `send_failed` cursor gate with unconditional cursor advancement after the packet loop:

```rust
for packet in packets.into_iter().skip(start) {
    for recipient_key in &recipient_keys {
        if let Err(error) = negotiator
            .send_opus_rtp_packet(
                recipient_key,
                &source_key.user_id,
                ServerMediaEgressRtpPacket {
                    sequence_number: packet.sequence_number,
                    timestamp: packet.timestamp,
                    payload_type: packet.payload_type,
                    payload: packet.payload.clone(),
                },
            )
            .await
        {
            tracing::debug!(
                error = format_args!("{error:#}"),
                room_id = %room_id,
                source_user_id = %source_key.user_id,
                recipient_user_id = %recipient_key.user_id,
                sequence_number = packet.sequence_number,
                "raw Opus WebRTC egress send failed"
            );
        }
    }
}
forwarded.insert(source_key.clone(), packet_count);
```

This intentionally drops packets for failing recipients instead of retrying old audio.

- [ ] **Step 2: Do not add per-recipient backlog state**

Confirm the implementation does not add a `(source, recipient)` backlog cursor or any retry queue. The only retained progress state should remain:

```rust
HashMap<ServerMediaSessionKey, usize>
```

- [ ] **Step 3: Run the targeted test and verify GREEN**

Run:

```bash
cargo test -p lyre-web server_relay_off_noise_does_not_replay_history_when_one_recipient_is_missing -- --nocapture
```

Expected after the implementation fix: PASS.

### Task 3: Verify Existing Raw Opus Behavior

**Files:**
- Test only: `crates/lyre-web/src/processed_audio_webrtc_egress_pump_tests.rs`
- Test only: `crates/lyre-web/src/raw_opus_webrtc_egress_pump.rs`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update roadmap**

Add a concise completed item to `docs/roadmap.md` noting that raw Opus relay now drops failed realtime sends instead of replaying historical RTP packets to healthy recipients.

- [ ] **Step 2: Run focused raw Opus tests**

Run:

```bash
cargo test -p lyre-web server_relay_off_noise -- --nocapture
```

Expected: PASS for raw Opus forwarding and unsubscribed-recipient tests.

- [ ] **Step 3: Run formatting and lint checks**

Run:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets
```

Expected: both commands exit successfully.

- [ ] **Step 4: Run workspace tests**

Run:

```bash
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: workspace tests pass.
