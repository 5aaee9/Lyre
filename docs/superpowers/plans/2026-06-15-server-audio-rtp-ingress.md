# Server Audio RTP Ingress Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the first verified server-side WebRTC audio RTP ingress boundary, exposed only through Lyre-owned DTO snapshots.

**Architecture:** Keep concrete `webrtc` crate types inside `crates/lyre-webrtc`. First move `stack.rs` tests into a separate test module so production media ingress work never pushes `stack.rs` over the 400 LOC limit. Then add a focused media ingress recorder module, wire the peer connection event handler to record remote audio tracks and RTP packets, and surface read-only snapshots through `WebRtcPeerConnectionHandle`, `ServerMediaNegotiator`, and internal `lyre-web::AppState` methods without adding public raw-packet REST endpoints.

**Tech Stack:** Rust, Tokio, `webrtc 0.20.0-alpha.1`, Axum app state tests, `cargo nextest`.

---

## Verified WebRTC API Surface

Use the installed `webrtc 0.20.0-alpha.1` surface already present in `Cargo.lock`:

```rust
use webrtc::media_stream::{
    MediaStreamTrack,
    track_local::{TrackLocal, static_rtp::TrackLocalStaticRTP},
    track_remote::{TrackRemote, TrackRemoteEvent},
};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
    RTCIceCandidateInit, RTCIceGatheringState, RTCPeerConnectionIceEvent,
    RTCSessionDescription,
};
use rtc::peer_connection::configuration::media_engine::{MediaEngine, MIME_TYPE_OPUS};
use webrtc::rtp_transceiver::{
    RTCRtpTransceiverDirection, RTCRtpTransceiverInit,
};
use rtc::rtp;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodecParameters, RTCRtpCodingParameters,
    RTCRtpEncodingParameters, RtpCodecKind,
};
```

Important API details:

- `webrtc` does not publicly expose all low-level RTP/codec structs needed by this increment; `lyre-webrtc` must add a direct `rtc = "0.20.0-alpha.1"` dependency and keep `rtc::` usage isolated inside `crates/lyre-webrtc`.
- `rtc::rtp::Packet.payload` is `bytes::Bytes`; `lyre-webrtc` must add `bytes = "1"` for test packet construction.
- `PeerConnectionBuilder::new().with_media_engine(media_engine).with_handler(handler).with_udp_addrs(...).build().await`
- `PeerConnection::add_transceiver_from_kind(RtpCodecKind::Audio, Some(RTCRtpTransceiverInit { direction: RTCRtpTransceiverDirection::Recvonly, ..Default::default() })).await`
- `TrackRemote` metadata methods are async through the inherited `Track` trait: `track.track_id().await`, `track.kind().await`, `track.ssrcs().await`, `track.codec(ssrc).await`.
- `TrackRemote::poll().await` yields `TrackRemoteEvent::OnRtpPacket(rtc::rtp::Packet)`.
- `TrackLocalStaticRTP::new(MediaStreamTrack::new(...))` creates a local RTP sender track for tests.
- `TrackLocalStaticRTP::write_rtp(packet).await` takes the RTP packet by value.

## File Structure

- Create `crates/lyre-webrtc/src/stack_tests.rs`: move the existing `stack.rs` unit tests here before adding media ingress code.
- Create `crates/lyre-webrtc/src/media_ingress.rs`: Lyre-owned DTOs and shared in-memory media ingress recorder.
- Modify `Cargo.toml`: add workspace dependency `rtc = "0.20.0-alpha.1"` to match the installed `webrtc` dependency.
- Modify `Cargo.toml`: add workspace dependency `bytes = "1"` for RTP packet payload construction.
- Modify `crates/lyre-webrtc/Cargo.toml`: add `rtc.workspace = true` and `bytes.workspace = true`.
- Modify `crates/lyre-webrtc/src/stack.rs`: register Opus receive support, add audio recvonly transceiver for server peer connections, wire `PeerConnectionHandler::on_track` to the media ingress recorder, and expose handle snapshot methods.
- Modify `crates/lyre-webrtc/src/lib.rs`: export the DTO types and declare test modules.
- Modify `crates/lyre-webrtc/src/negotiation.rs`: add room/user keyed snapshot methods.
- Modify `crates/lyre-webrtc/src/negotiation_tests.rs`: add keyed snapshot and cleanup tests.
- Modify `crates/lyre-web/src/api.rs`: add internal `AppState` snapshot methods.
- Modify `crates/lyre-web/src/api_server_media_tests.rs`: add AppState passthrough coverage and assert no raw RTP REST endpoint exists.
- Keep all changed Rust files below 400 LOC throughout the implementation, not only after cleanup.

## Task 1: Split Stack Tests Before Media Changes

**Files:**
- Create: `crates/lyre-webrtc/src/stack_tests.rs`
- Modify: `crates/lyre-webrtc/src/stack.rs`
- Modify: `crates/lyre-webrtc/src/lib.rs`
- Test: `crates/lyre-webrtc/src/stack_tests.rs`

- [x] **Step 1: Move the existing test module**

Move the whole existing `#[cfg(test)] mod tests { ... }` block from the bottom of `crates/lyre-webrtc/src/stack.rs` into `crates/lyre-webrtc/src/stack_tests.rs`.

In the moved test file, replace `super::` references with `crate::stack::` or `crate::` as appropriate. For example:

```rust
#[tokio::test]
async fn create_peer_connection_returns_lyre_handle() {
    let handle = crate::stack::WebRtcStack::new()
        .create_peer_connection()
        .await
        .unwrap();

    assert_eq!(
        std::any::type_name_of_val(&handle),
        "lyre_webrtc::stack::WebRtcPeerConnectionHandle"
    );
}
```

- [x] **Step 2: Declare the test module**

Update `crates/lyre-webrtc/src/lib.rs`:

```rust
pub mod negotiation;
pub mod session;
pub mod stack;

#[cfg(test)]
mod negotiation_tests;
#[cfg(test)]
mod stack_tests;
```

Keep all existing public exports unchanged in this step.

- [x] **Step 3: Run the moved stack tests**

Run:

```bash
cargo test -p lyre-webrtc stack_tests::
```

Expected: PASS with behavior unchanged.

- [x] **Step 4: Check `stack.rs` LOC before continuing**

Run:

```bash
wc -l crates/lyre-webrtc/src/stack.rs
```

Expected: well below 400 lines before adding media ingress code.

## Task 2: Media Ingress DTOs and Recorder

**Files:**
- Create: `crates/lyre-webrtc/src/media_ingress.rs`
- Modify: `crates/lyre-webrtc/src/lib.rs`
- Test: `crates/lyre-webrtc/src/media_ingress.rs`

- [x] **Step 1: Write recorder unit tests**

Add this test module at the bottom of the new `crates/lyre-webrtc/src/media_ingress.rs` file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorder_returns_remote_track_and_rtp_snapshots() {
        let recorder = MediaIngressRecorder::default();

        recorder.record_remote_track(ServerMediaRemoteTrack {
            track_id: "audio-1".to_owned(),
            kind: ServerMediaTrackKind::Audio,
            mime_type: Some("audio/opus".to_owned()),
        });
        recorder.record_rtp_packet(ServerMediaRtpPacket {
            track_id: "audio-1".to_owned(),
            sequence_number: 7,
            timestamp: 48_000,
            marker: true,
            payload_type: 111,
            payload: vec![1, 2, 3],
        });

        assert_eq!(
            recorder.remote_tracks(),
            vec![ServerMediaRemoteTrack {
                track_id: "audio-1".to_owned(),
                kind: ServerMediaTrackKind::Audio,
                mime_type: Some("audio/opus".to_owned()),
            }]
        );
        assert_eq!(
            recorder.received_rtp_packets(),
            vec![ServerMediaRtpPacket {
                track_id: "audio-1".to_owned(),
                sequence_number: 7,
                timestamp: 48_000,
                marker: true,
                payload_type: 111,
                payload: vec![1, 2, 3],
            }]
        );
    }
}
```

- [x] **Step 2: Run the focused test and verify it fails**

Run:

```bash
cargo test -p lyre-webrtc media_ingress::tests::recorder_returns_remote_track_and_rtp_snapshots
```

Expected: FAIL because `MediaIngressRecorder`, `ServerMediaRemoteTrack`, `ServerMediaTrackKind`, and `ServerMediaRtpPacket` are not implemented yet.

- [x] **Step 3: Implement the DTOs and recorder**

Add this production code above the tests:

```rust
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServerMediaRemoteTrack {
    pub track_id: String,
    pub kind: ServerMediaTrackKind,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ServerMediaTrackKind {
    Audio,
    Video,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ServerMediaRtpPacket {
    pub track_id: String,
    pub sequence_number: u16,
    pub timestamp: u32,
    pub marker: bool,
    pub payload_type: u8,
    pub payload: Vec<u8>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct MediaIngressRecorder {
    inner: Arc<Mutex<MediaIngressState>>,
}

#[derive(Debug, Default)]
struct MediaIngressState {
    remote_tracks: Vec<ServerMediaRemoteTrack>,
    received_rtp_packets: Vec<ServerMediaRtpPacket>,
}

impl MediaIngressRecorder {
    pub(crate) fn record_remote_track(&self, track: ServerMediaRemoteTrack) {
        self.inner
            .lock()
            .expect("media ingress recorder lock must not be poisoned")
            .remote_tracks
            .push(track);
    }

    pub(crate) fn record_rtp_packet(&self, packet: ServerMediaRtpPacket) {
        self.inner
            .lock()
            .expect("media ingress recorder lock must not be poisoned")
            .received_rtp_packets
            .push(packet);
    }

    pub(crate) fn remote_tracks(&self) -> Vec<ServerMediaRemoteTrack> {
        self.inner
            .lock()
            .expect("media ingress recorder lock must not be poisoned")
            .remote_tracks
            .clone()
    }

    pub(crate) fn received_rtp_packets(&self) -> Vec<ServerMediaRtpPacket> {
        self.inner
            .lock()
            .expect("media ingress recorder lock must not be poisoned")
            .received_rtp_packets
            .clone()
    }
}
```

- [x] **Step 4: Export the DTOs**

Update `crates/lyre-webrtc/src/lib.rs`:

```rust
pub mod media_ingress;
pub mod negotiation;
pub mod session;
pub mod stack;

#[cfg(test)]
mod negotiation_tests;
#[cfg(test)]
mod stack_tests;

pub use media_ingress::{ServerMediaRemoteTrack, ServerMediaRtpPacket, ServerMediaTrackKind};
```

Keep the existing `negotiation`, `session`, and `stack` exports after this new export.

- [x] **Step 5: Run the focused test**

Run:

```bash
cargo test -p lyre-webrtc media_ingress::tests::recorder_returns_remote_track_and_rtp_snapshots
```

Expected: PASS.

## Task 3: Compile-Proven WebRTC RTP Test Helper

**Files:**
- Modify: `Cargo.toml`
- Modify: `crates/lyre-webrtc/Cargo.toml`
- Modify: `crates/lyre-webrtc/src/stack_tests.rs`

- [x] **Step 1: Add direct dependencies for low-level RTP/codec test types**

In the root `Cargo.toml` workspace dependencies, add:

```toml
rtc = "0.20.0-alpha.1"
bytes = "1"
```

In `crates/lyre-webrtc/Cargo.toml`, add:

```toml
rtc.workspace = true
bytes.workspace = true
```

These dependencies are intentionally limited to `lyre-webrtc`; no other crate should import `rtc::`.

- [x] **Step 2: Add a local offerer helper that compiles against installed `webrtc`/`rtc` APIs**

Add these imports and helper to `crates/lyre-webrtc/src/stack_tests.rs`:

```rust
use std::sync::Arc;
use bytes::Bytes;
use rtc::rtp_transceiver::rtp_sender::{
    RTCRtpCodec, RTCRtpCodingParameters, RTCRtpEncodingParameters, RtpCodecKind,
};
use webrtc::media_stream::{
    MediaStreamTrack,
    track_local::{TrackLocal, static_rtp::TrackLocalStaticRTP},
};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler,
};
use rtc::peer_connection::configuration::media_engine::{MediaEngine, MIME_TYPE_OPUS};

#[derive(Clone)]
struct NoopPeerConnectionHandler;

#[async_trait::async_trait]
impl PeerConnectionEventHandler for NoopPeerConnectionHandler {}

async fn opus_offerer() -> (Arc<dyn PeerConnection>, Arc<TrackLocalStaticRTP>) {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .expect("default codecs should register for tests");
    let offerer = PeerConnectionBuilder::new()
        .with_handler(Arc::new(NoopPeerConnectionHandler))
        .with_media_engine(media_engine)
        .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
        .build()
        .await
        .expect("test offerer peer connection should build");
    let track = Arc::new(TrackLocalStaticRTP::new(MediaStreamTrack::new(
        "lyre-test".to_owned(),
        "audio".to_owned(),
        "audio".to_owned(),
        RtpCodecKind::Audio,
        vec![RTCRtpEncodingParameters {
            rtp_coding_parameters: RTCRtpCodingParameters {
                ssrc: Some(1234),
                ..Default::default()
            },
            codec: RTCRtpCodec {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: 48_000,
                channels: 2,
                sdp_fmtp_line: String::new(),
                rtcp_feedback: vec![],
            },
            ..Default::default()
        }],
    )));
    offerer
        .add_track(track.clone())
        .await
        .expect("test offerer should accept audio track");
    (Arc::from(offerer), track)
}

async fn local_description_sdp_with_ice(peer_connection: &Arc<dyn PeerConnection>) -> String {
    for _ in 0..64 {
        if let Some(local_description) = peer_connection.local_description().await {
            if local_description.sdp.contains("a=ice-ufrag:") {
                return local_description.sdp;
            }
        }
        tokio::task::yield_now().await;
    }
    peer_connection.local_description().await.unwrap().sdp
}
```

- [x] **Step 3: Add a compile-proving helper test**

Add this test:

```rust
#[tokio::test]
async fn opus_offerer_helper_creates_media_offer() {
    let (offerer, _track) = opus_offerer().await;

    let offer = offerer.create_offer(None).await.unwrap();
    offerer.set_local_description(offer).await.unwrap();

    let offer_sdp = local_description_sdp_with_ice(&offerer).await;
    assert!(offer_sdp.contains("m=audio"));
    assert!(offer_sdp.contains("opus"));
    assert!(offer_sdp.contains("a=ice-ufrag:"));
}
```

- [x] **Step 4: Run the helper test**

Run:

```bash
cargo test -p lyre-webrtc stack_tests::opus_offerer_helper_creates_media_offer
```

Expected: PASS. If it does not compile, fix the helper against the installed crate API before continuing. Do not change production code in this task.

## Task 4: Peer Connection Audio RTP Ingress

**Files:**
- Modify: `crates/lyre-webrtc/src/stack.rs`
- Modify: `crates/lyre-webrtc/src/stack_tests.rs`

- [x] **Step 1: Add a failing real RTP ingress test**

Add this test to `crates/lyre-webrtc/src/stack_tests.rs`:

```rust
#[tokio::test]
async fn answer_remote_offer_records_audio_track_and_rtp_packet() {
    use std::time::Duration;

    let server = crate::stack::WebRtcStack::new()
        .create_peer_connection()
        .await
        .unwrap();
    let (offerer, track) = opus_offerer().await;

    let offer = offerer.create_offer(None).await.unwrap();
    offerer.set_local_description(offer).await.unwrap();
    let offer_sdp = local_description_sdp_with_ice(&offerer).await;

    let answer_sdp = server.answer_remote_offer(offer_sdp).await.unwrap();
    let answer = webrtc::peer_connection::RTCSessionDescription::answer(answer_sdp).unwrap();
    offerer.set_remote_description(answer).await.unwrap();

    for _ in 0..100 {
        if track.write_rtp(test_rtp_packet(vec![0x11, 0x22, 0x33, 0x44])).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    for _ in 0..100 {
        let packets = server.received_rtp_packets();
        if packets.iter().any(|packet| {
            packet.sequence_number == 42
                && packet.timestamp == 1234
                && packet.marker
                && packet.payload_type == 111
                && packet.payload == vec![0x11, 0x22, 0x33, 0x44]
        }) {
            assert!(server.remote_tracks().iter().any(|track| {
                track.kind == crate::ServerMediaTrackKind::Audio
                    && track.mime_type.as_deref() == Some(MIME_TYPE_OPUS)
            }));
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    panic!("server did not record the sent RTP packet");
}

fn test_rtp_packet(payload: Vec<u8>) -> rtc::rtp::Packet {
    rtc::rtp::Packet {
        header: rtc::rtp::Header {
            sequence_number: 42,
            timestamp: 1234,
            marker: true,
            payload_type: 111,
            ..Default::default()
        },
        payload: Bytes::from(payload),
    }
}
```

- [x] **Step 2: Run the RTP ingress test and verify it fails**

Run:

```bash
cargo test -p lyre-webrtc stack_tests::answer_remote_offer_records_audio_track_and_rtp_packet
```

Expected: FAIL because `WebRtcPeerConnectionHandle` does not yet expose `remote_tracks` or `received_rtp_packets`, and the server does not yet record media ingress.

- [x] **Step 3: Wire media ingress into peer connection creation**

Update `crates/lyre-webrtc/src/stack.rs` imports to include the verified APIs from the top of this plan.

In `WebRtcStack::create_peer_connection`:

```rust
let local_ice_candidates = Arc::new(Mutex::new(Vec::new()));
let media_ingress = MediaIngressRecorder::default();
let handler = Arc::new(PeerConnectionHandler {
    local_ice_candidates: Arc::clone(&local_ice_candidates),
    media_ingress: media_ingress.clone(),
});
let mut media_engine = MediaEngine::default();
media_engine
    .register_codec(
        RTCRtpCodecParameters {
            rtp_codec: RTCRtpCodec {
                mime_type: MIME_TYPE_OPUS.to_owned(),
                clock_rate: 48_000,
                channels: 2,
                sdp_fmtp_line: String::new(),
                rtcp_feedback: vec![],
            },
            payload_type: 111,
            ..Default::default()
        },
        RtpCodecKind::Audio,
    )
    .map_err(|source| WebRtcStackError::CreatePeerConnection {
        source: Box::new(source),
    })?;
let peer_connection = PeerConnectionBuilder::new()
    .with_handler(handler)
    .with_media_engine(media_engine)
    .with_udp_addrs(vec!["127.0.0.1:0".to_owned()])
    .build()
    .await
    .map_err(|source| WebRtcStackError::CreatePeerConnection {
        source: Box::new(source),
    })?;
peer_connection
    .add_transceiver_from_kind(
        RtpCodecKind::Audio,
        Some(RTCRtpTransceiverInit {
            direction: RTCRtpTransceiverDirection::Recvonly,
            ..Default::default()
        }),
    )
    .await
    .map_err(|source| WebRtcStackError::CreatePeerConnection {
        source: Box::new(source),
    })?;
```

Store `media_ingress` on `WebRtcPeerConnectionHandle`.

- [x] **Step 4: Record remote tracks and RTP packets**

Extend `PeerConnectionHandler` with `media_ingress: MediaIngressRecorder`.

Add `on_track`:

```rust
async fn on_track(&self, track: Arc<dyn TrackRemote>) {
    let track_id = track.track_id().await.to_string();
    let kind = match track.kind().await {
        RtpCodecKind::Audio => ServerMediaTrackKind::Audio,
        RtpCodecKind::Video => ServerMediaTrackKind::Video,
        _ => ServerMediaTrackKind::Unknown,
    };
    let mime_type = first_codec_mime_type(&track).await;
    self.media_ingress.record_remote_track(ServerMediaRemoteTrack {
        track_id: track_id.clone(),
        kind: kind.clone(),
        mime_type,
    });

    if kind != ServerMediaTrackKind::Audio {
        return;
    }

    let media_ingress = self.media_ingress.clone();
    tokio::spawn(async move {
        while let Some(event) = track.poll().await {
            if let TrackRemoteEvent::OnRtpPacket(packet) = event {
                media_ingress.record_rtp_packet(ServerMediaRtpPacket {
                    track_id: track_id.clone(),
                    sequence_number: packet.header.sequence_number,
                    timestamp: packet.header.timestamp,
                    marker: packet.header.marker,
                    payload_type: packet.header.payload_type,
                    payload: packet.payload.to_vec(),
                });
            }
        }
    });
}
```

Add this helper in `stack.rs`:

```rust
async fn first_codec_mime_type(track: &Arc<dyn TrackRemote>) -> Option<String> {
    for ssrc in track.ssrcs().await {
        if let Some(codec) = track.codec(ssrc).await {
            return Some(codec.mime_type);
        }
    }
    None
}
```

- [x] **Step 5: Add handle snapshot methods**

Update `WebRtcPeerConnectionHandle`:

```rust
#[derive(Clone)]
pub struct WebRtcPeerConnectionHandle {
    _peer_connection: Arc<dyn PeerConnection>,
    local_ice_candidates: Arc<Mutex<Vec<ServerMediaIceCandidateInit>>>,
    media_ingress: MediaIngressRecorder,
}

impl WebRtcPeerConnectionHandle {
    pub fn remote_tracks(&self) -> Vec<ServerMediaRemoteTrack> {
        self.media_ingress.remote_tracks()
    }

    pub fn received_rtp_packets(&self) -> Vec<ServerMediaRtpPacket> {
        self.media_ingress.received_rtp_packets()
    }
}
```

- [x] **Step 6: Run focused stack tests and LOC check**

Run:

```bash
cargo test -p lyre-webrtc stack_tests::
wc -l crates/lyre-webrtc/src/stack.rs crates/lyre-webrtc/src/stack_tests.rs
```

Expected: tests PASS and both files remain below 400 lines.

## Task 5: Negotiator and AppState Snapshot Queries

**Files:**
- Modify: `crates/lyre-webrtc/src/negotiation.rs`
- Modify: `crates/lyre-webrtc/src/negotiation_tests.rs`
- Modify: `crates/lyre-web/src/api.rs`
- Modify: `crates/lyre-web/src/api_server_media_tests.rs`

- [x] **Step 1: Add failing negotiator tests**

In `crates/lyre-webrtc/src/negotiation_tests.rs`, add tests named:

```rust
#[tokio::test]
async fn server_media_snapshot_queries_return_empty_for_missing_session() {
    let negotiator = test_negotiator();
    let missing_key = ServerMediaSessionKey {
        room_id: RoomId::parse_boundary("DEFAULT").unwrap(),
        user_id: UserId::parse_boundary("missing-user").unwrap(),
    };

    assert!(negotiator.remote_tracks(&missing_key).is_empty());
    assert!(negotiator.received_rtp_packets(&missing_key).is_empty());
}
```

Add a second test that negotiates with the existing helper, calls `negotiator.close(&key)`, then asserts both snapshot methods return empty for that key.

- [x] **Step 2: Run the negotiator tests and verify they fail**

Run:

```bash
cargo test -p lyre-webrtc negotiation_tests::server_media_snapshot_queries
```

Expected: FAIL because `remote_tracks` and `received_rtp_packets` do not exist on `ServerMediaNegotiator`.

- [x] **Step 3: Implement negotiator query methods**

Update imports in `crates/lyre-webrtc/src/negotiation.rs`:

```rust
use crate::{
    ServerMediaIceCandidateInit, ServerMediaRemoteTrack, ServerMediaRtpPacket,
    ServerMediaSessionConfig, ServerMediaSessionKey, ServerMediaSessionRegistry,
    ServerMediaSessionState, WebRtcPeerConnectionHandle, WebRtcStack, WebRtcStackError,
};
```

Add methods:

```rust
pub fn remote_tracks(&self, key: &ServerMediaSessionKey) -> Vec<ServerMediaRemoteTrack> {
    self.peer_connections
        .get(key)
        .map(|peer_connection| peer_connection.remote_tracks())
        .unwrap_or_default()
}

pub fn received_rtp_packets(&self, key: &ServerMediaSessionKey) -> Vec<ServerMediaRtpPacket> {
    self.peer_connections
        .get(key)
        .map(|peer_connection| peer_connection.received_rtp_packets())
        .unwrap_or_default()
}
```

- [x] **Step 4: Add failing AppState passthrough tests**

In `crates/lyre-web/src/api_server_media_tests.rs`, add one test for internal AppState snapshots:

```rust
#[tokio::test]
async fn app_state_server_media_snapshots_are_internal_and_empty_for_missing_session() {
    let state = AppState::default();
    let key = ServerMediaSessionKey {
        room_id: RoomId::parse_boundary("DEFAULT").unwrap(),
        user_id: UserId::parse_boundary("user-1").unwrap(),
    };

    assert!(state.server_media_remote_tracks(&key).is_empty());
    assert!(state.server_media_received_rtp_packets(&key).is_empty());
}
```

Add one HTTP smoke test asserting `GET /api/rooms/DEFAULT/server-media/rtp-packets?user_id=user-1` returns 404.

- [x] **Step 5: Run the AppState tests and verify they fail only on missing methods**

Run:

```bash
cargo test -p lyre-web api_server_media_tests::
```

Expected: FAIL because the AppState methods do not exist. The raw RTP route test should pass once the code compiles.

- [x] **Step 6: Implement AppState passthrough methods**

In `crates/lyre-web/src/api.rs`, import the DTO types:

```rust
use lyre_webrtc::{ServerMediaRemoteTrack, ServerMediaRtpPacket};
```

Add methods:

```rust
pub fn server_media_remote_tracks(
    &self,
    key: &ServerMediaSessionKey,
) -> Vec<ServerMediaRemoteTrack> {
    self.server_media_negotiator.remote_tracks(key)
}

pub fn server_media_received_rtp_packets(
    &self,
    key: &ServerMediaSessionKey,
) -> Vec<ServerMediaRtpPacket> {
    self.server_media_negotiator.received_rtp_packets(key)
}
```

- [x] **Step 7: Run focused tests and LOC check**

Run:

```bash
cargo test -p lyre-webrtc negotiation_tests::
cargo test -p lyre-web api_server_media_tests::
wc -l crates/lyre-webrtc/src/negotiation.rs crates/lyre-web/src/api.rs
```

Expected: tests PASS and both files remain below 400 lines.

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
rg '(^|[^[:alnum:]_])webrtc::' crates --glob '!crates/lyre-webrtc/**'
rg '(^|[^[:alnum:]_])rtc::' crates --glob '!crates/lyre-webrtc/**'
cargo rustdoc -p lyre-webrtc --lib -- -D warnings
wc -l crates/lyre-webrtc/src/stack.rs crates/lyre-webrtc/src/stack_tests.rs crates/lyre-webrtc/src/media_ingress.rs crates/lyre-webrtc/src/negotiation.rs crates/lyre-web/src/api.rs
```

Expected:

- `git diff --check` exits 0.
- `rg '(^|[^[:alnum:]_])webrtc::' crates --glob '!crates/lyre-webrtc/**'` finds no matches, without matching `lyre_webrtc::`.
- `rg '(^|[^[:alnum:]_])rtc::' crates --glob '!crates/lyre-webrtc/**'` finds no matches.
- `cargo rustdoc -p lyre-webrtc --lib -- -D warnings` passes, proving the public library documentation can be generated with the exposed signatures.
- Public exports in `crates/lyre-webrtc/src/lib.rs` include only Lyre-owned DTOs for remote tracks/RTP packets: `ServerMediaRemoteTrack`, `ServerMediaRtpPacket`, and `ServerMediaTrackKind`; no concrete `webrtc` media/track/RTP type is re-exported.
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
- 2026-06-15: Added the server audio RTP ingress boundary in `lyre-webrtc`. Server peer connections now negotiate an Opus recvonly audio path, record remote track metadata, and snapshot incoming audio RTP packet metadata/payload behind Lyre-owned DTOs. Raw RTP snapshots remain internal; Opus decode, decoded PCM noise-cancelling input, processed audio broadcast, and browser playback remain future work.
```

- [x] **Step 2: Update `docs/roadmap.md` after implementation approval**

Move the server audio RTP ingress boundary into the completed section and keep the next TODO focused on Opus decode/PCM conversion and connecting decoded audio to the noise provider runtime. Do not claim RNNoise/DeepFilterNet processes real WebRTC tracks yet.

- [x] **Step 3: Run final docs-inclusive verification**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
cd frontend && npm run generate:webrpc && npm test -- --run && npm run typecheck && npm run lint && npm run build
git diff --check
rg '(^|[^[:alnum:]_])webrtc::' crates --glob '!crates/lyre-webrtc/**'
rg '(^|[^[:alnum:]_])rtc::' crates --glob '!crates/lyre-webrtc/**'
cargo rustdoc -p lyre-webrtc --lib -- -D warnings
wc -l crates/lyre-webrtc/src/stack.rs crates/lyre-webrtc/src/stack_tests.rs crates/lyre-webrtc/src/media_ingress.rs crates/lyre-webrtc/src/negotiation.rs crates/lyre-web/src/api.rs
```

Expected: all commands PASS or produce the expected empty grep result.

- [x] **Step 4: Review final diff**

Run:

```bash
git status --short
git diff --stat
git diff -- Cargo.toml Cargo.lock crates/lyre-webrtc/Cargo.toml crates/lyre-webrtc/src/media_ingress.rs crates/lyre-webrtc/src/stack.rs crates/lyre-webrtc/src/stack_tests.rs crates/lyre-webrtc/src/lib.rs crates/lyre-webrtc/src/negotiation.rs crates/lyre-webrtc/src/negotiation_tests.rs crates/lyre-web/src/api.rs crates/lyre-web/src/api_server_media_tests.rs MEMORY.md docs/roadmap.md docs/superpowers/specs/2026-06-15-server-audio-rtp-ingress-design.md docs/superpowers/plans/2026-06-15-server-audio-rtp-ingress.md
```

Expected: only intended files changed.

- [x] **Step 5: Commit and push**

Commit with Lore protocol:

```bash
git add Cargo.toml Cargo.lock crates/lyre-webrtc/Cargo.toml crates/lyre-webrtc/src/media_ingress.rs crates/lyre-webrtc/src/stack.rs crates/lyre-webrtc/src/stack_tests.rs crates/lyre-webrtc/src/lib.rs crates/lyre-webrtc/src/negotiation.rs crates/lyre-webrtc/src/negotiation_tests.rs crates/lyre-web/src/api.rs crates/lyre-web/src/api_server_media_tests.rs MEMORY.md docs/roadmap.md docs/superpowers/specs/2026-06-15-server-audio-rtp-ingress-design.md docs/superpowers/plans/2026-06-15-server-audio-rtp-ingress.md
git commit -m "Prove server audio RTP can enter Lyre" -m "Constraint: Server-side noise cancellation needs a verified media ingress boundary before Opus decode or PCM processing.
Rejected: Exposing raw RTP over REST | It would create a debugging API rather than product behavior.
Confidence: medium
Scope-risk: moderate
Directive: Keep concrete webrtc crate types isolated inside crates/lyre-webrtc and do not claim server-side noise cancellation consumes real tracks until Opus decode and PCM wiring exist.
Tested: cargo fmt --all --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --manifest-path Cargo.toml --workspace; frontend generate/test/typecheck/lint/build; git diff --check; webrtc/rtc dependency leak checks; cargo rustdoc -p lyre-webrtc --lib -- -D warnings; LOC check
Not-tested: Browser end-to-end server media playback; Opus decode; RNNoise/DeepFilterNet processing of real WebRTC tracks"
git push
```

Expected: commit and push succeed. If push is blocked by credentials, network, branch policy, or remote rejection, report the local commit SHA and exact push error.
