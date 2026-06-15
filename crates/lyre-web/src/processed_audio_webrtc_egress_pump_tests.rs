use crate::{
    api::AppState,
    media_egress::ProcessedAudioEgressFanout,
    media_runtime::WebMediaRuntime,
    processed_audio_webrtc_egress_pump::{
        ProcessedAudioWebRtcEgressPump, ProcessedAudioWebRtcEgressSender,
    },
};
use async_trait::async_trait;
use lyre_core::{
    AudioFrame, MediaTrackKind, NoiseCancellationConfig, NoiseProvider, RegisterMediaTrackRequest,
    RoomId, StartMediaRelayRequest, StopMediaRelayRequest, UserId,
};
use lyre_webrtc::{
    ServerMediaEgressError, ServerMediaIceCandidate, ServerMediaNegotiator, ServerMediaOffer,
    ServerMediaOpusDecoder, ServerMediaProcessedAudioFrame, ServerMediaRtpPacket,
    ServerMediaSessionKey, ServerMediaSessionRegistry, WebRtcStack,
};
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

fn egress_pump() -> ProcessedAudioWebRtcEgressPump {
    let relays = Arc::new(lyre_core::MediaRelayRegistry::new());
    let runtime = Arc::new(WebMediaRuntime::new(Arc::clone(&relays)));
    let fanout = Arc::new(ProcessedAudioEgressFanout::new(relays));
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = Arc::new(ServerMediaNegotiator::new(WebRtcStack::new(), sessions));
    ProcessedAudioWebRtcEgressPump::new(runtime, fanout, negotiator)
}

#[derive(Debug)]
struct BlockingSender {
    blocked_user_id: UserId,
    blocked_sequences: Mutex<Vec<u64>>,
    fast_sequences: Mutex<Vec<u64>>,
    notify: Notify,
}

#[async_trait]
impl ProcessedAudioWebRtcEgressSender for BlockingSender {
    async fn send_processed_audio_frame(
        &self,
        key: &ServerMediaSessionKey,
        frame: ServerMediaProcessedAudioFrame,
    ) -> Result<usize, ServerMediaEgressError> {
        if key.user_id == self.blocked_user_id {
            self.blocked_sequences
                .lock()
                .expect("test sender sequence lock must not be poisoned")
                .push(frame.sequence);
            self.notify.notify_waiters();
            std::future::pending::<()>().await;
            unreachable!("test sender intentionally never completes");
        }
        self.fast_sequences
            .lock()
            .expect("test sender sequence lock must not be poisoned")
            .push(frame.sequence);
        self.notify.notify_waiters();
        Ok(1)
    }
}

fn relay_context() -> (
    Arc<lyre_core::MediaRelayRegistry>,
    Arc<WebMediaRuntime>,
    Arc<ProcessedAudioEgressFanout>,
) {
    let relays = Arc::new(lyre_core::MediaRelayRegistry::new());
    let room_id = RoomId::default_room();
    relays.start(room_id.clone(), StartMediaRelayRequest::default());
    for user_id in ["source", "blocked", "fast"] {
        relays
            .register_track(
                room_id.clone(),
                RegisterMediaTrackRequest {
                    user_id: UserId::from_external(user_id),
                    track_id: "audio-main".to_owned(),
                    kind: MediaTrackKind::Audio,
                },
            )
            .unwrap();
    }
    let runtime = Arc::new(WebMediaRuntime::new(Arc::clone(&relays)));
    let fanout = Arc::new(ProcessedAudioEgressFanout::new(Arc::clone(&relays)));
    (relays, runtime, fanout)
}

#[tokio::test]
async fn app_state_start_and_stop_manage_egress_pump() {
    let state = AppState::default();
    let room_id = RoomId::default_room();

    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    assert_eq!(state.processed_audio_webrtc_egress_pump_count(), 1);
    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    assert_eq!(state.processed_audio_webrtc_egress_pump_count(), 1);

    state.stop_media_relay(
        room_id,
        StopMediaRelayRequest {
            user_id: UserId::from_external("user_01"),
        },
    );
    assert_eq!(state.processed_audio_webrtc_egress_pump_count(), 0);
}

#[tokio::test]
async fn close_server_media_sessions_stops_egress_pump() {
    let state = AppState::default();
    let room_id = RoomId::default_room();

    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    assert_eq!(state.processed_audio_webrtc_egress_pump_count(), 1);
    state.close_server_media_sessions_for_room(&room_id);
    assert_eq!(state.processed_audio_webrtc_egress_pump_count(), 0);
}

fn audio_frame(room_id: RoomId, user_id: UserId) -> AudioFrame {
    AudioFrame {
        room_id,
        user_id,
        track_id: "audio-main".to_owned(),
        sample_rate_hz: 48_000,
        channels: 1,
        sequence: 1,
        samples: vec![0.1; lyre_webrtc::SERVER_MEDIA_OPUS_FRAME_SIZE],
    }
}

fn sequenced_audio_frame(sequence: u64) -> AudioFrame {
    AudioFrame {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("source"),
        track_id: "audio-main".to_owned(),
        sample_rate_hz: 48_000,
        channels: 1,
        sequence,
        samples: vec![0.1; lyre_webrtc::SERVER_MEDIA_OPUS_FRAME_SIZE],
    }
}

#[tokio::test]
async fn egress_pump_start_replaces_existing_room_task() {
    let pump = egress_pump();
    let room_id = RoomId::default_room();

    pump.start(room_id.clone());
    assert_eq!(pump.task_count(), 1);
    pump.start(room_id.clone());
    assert_eq!(pump.task_count(), 1);

    pump.stop_and_wait_for_test(&room_id).await;
    assert_eq!(pump.task_count(), 0);
}

#[tokio::test]
async fn egress_pump_stop_removes_only_matching_room_task() {
    let pump = egress_pump();
    let room_id = RoomId::default_room();
    let other_room_id = RoomId::parse_boundary("OTHER").unwrap();

    pump.start(room_id.clone());
    pump.start(other_room_id.clone());
    pump.stop(&room_id);

    assert_eq!(pump.task_count(), 1);
    pump.stop_and_wait_for_test(&other_room_id).await;
    assert_eq!(pump.task_count(), 0);
}

#[tokio::test]
async fn egress_pump_stop_waits_for_cancelled_task_to_exit_for_tests() {
    let pump = egress_pump();
    let room_id = RoomId::default_room();

    pump.start(room_id.clone());
    pump.stop_and_wait_for_test(&room_id).await;

    assert_eq!(pump.task_count(), 0);
}

#[tokio::test]
async fn recipient_send_backpressure_does_not_block_other_recipients() {
    let (_relays, runtime, fanout) = relay_context();
    let sender = Arc::new(BlockingSender {
        blocked_user_id: UserId::from_external("blocked"),
        blocked_sequences: Mutex::new(Vec::new()),
        fast_sequences: Mutex::new(Vec::new()),
        notify: Notify::new(),
    });
    let pump =
        ProcessedAudioWebRtcEgressPump::new(Arc::clone(&runtime), fanout, Arc::clone(&sender));
    let room_id = RoomId::default_room();
    pump.start(room_id.clone());

    for sequence in 1..=300 {
        runtime
            .process_frame(sequenced_audio_frame(sequence))
            .unwrap();
    }

    for _ in 0..100 {
        let fast_sequences = sender
            .fast_sequences
            .lock()
            .expect("test sender sequence lock must not be poisoned")
            .clone();
        if fast_sequences.len() >= 2 {
            pump.stop_and_wait_for_test(&room_id).await;
            let blocked_sequences = sender
                .blocked_sequences
                .lock()
                .expect("test sender sequence lock must not be poisoned")
                .clone();
            assert_eq!(blocked_sequences.len(), 1);
            assert!(fast_sequences[1] > 1, "fast_sequences={fast_sequences:?}");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    pump.stop_and_wait_for_test(&room_id).await;
    panic!("recipient egress backpressure blocked other recipients");
}

fn register_audio_track(state: &AppState, room_id: &RoomId, user_id: &str) {
    state
        .media_relays
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: UserId::from_external(user_id),
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
}

fn start_relay_with_provider(state: &AppState, room_id: &RoomId, provider: NoiseProvider) {
    state.start_media_relay(
        room_id.clone(),
        StartMediaRelayRequest {
            noise: Some(NoiseCancellationConfig {
                provider,
                intensity: 0.5,
                voice_activity_threshold: 0.35,
            }),
        },
    );
}

async fn answer_offer(state: &AppState, room_id: &RoomId, user_id: &str) -> ServerMediaSessionKey {
    let offerer = WebRtcStack::new().create_peer_connection().await.unwrap();
    let key = ServerMediaSessionKey {
        room_id: room_id.clone(),
        user_id: UserId::from_external(user_id),
    };
    state
        .answer_server_media_offer(ServerMediaOffer {
            room_id: key.room_id.clone(),
            user_id: key.user_id.clone(),
            audio_track_id: "audio-main".to_owned(),
            sdp: offerer.create_local_offer_for_test().await.unwrap(),
        })
        .await
        .unwrap();
    key
}

async fn connect_test_offer(
    state: &AppState,
    room_id: &RoomId,
    user_id: &str,
) -> lyre_webrtc::test_support::ServerMediaConnectedOffer {
    let key = ServerMediaSessionKey {
        room_id: room_id.clone(),
        user_id: UserId::from_external(user_id),
    };
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
            .add_server_media_ice_candidate(ServerMediaIceCandidate {
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
        .accept_answer(&answer, state.server_media_ice_candidates(&key))
        .await
}

fn decoded_peak_from_rtp_payload(payload: &[u8]) -> f32 {
    let mut decoder = ServerMediaOpusDecoder::new().unwrap();
    let frame = decoder
        .decode_packet(&ServerMediaRtpPacket {
            track_id: "audio-main".to_owned(),
            sequence_number: 0,
            timestamp: 0,
            marker: true,
            payload_type: 111,
            payload: payload.to_vec(),
        })
        .unwrap();
    frame
        .samples
        .iter()
        .map(|sample| sample.abs())
        .fold(0.0, f32::max)
}

#[tokio::test]
async fn processed_audio_frame_is_sent_to_recipient_server_media_peer() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    register_audio_track(&state, &room_id, "source");
    register_audio_track(&state, &room_id, "recipient");
    let source_key = answer_offer(&state, &room_id, "source").await;
    let recipient_key = answer_offer(&state, &room_id, "recipient").await;

    state
        .process_media_frame(audio_frame(
            room_id.clone(),
            UserId::from_external("source"),
        ))
        .unwrap();

    for _ in 0..100 {
        let recipient = state
            .server_media_peer_connection_for_test(&recipient_key)
            .unwrap();
        if !recipient.sent_egress_rtp_packets_for_test().is_empty() {
            let source = state
                .server_media_peer_connection_for_test(&source_key)
                .unwrap();
            assert!(source.sent_egress_rtp_packets_for_test().is_empty());
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("processed audio frame was not sent to recipient server-media peer");
}

#[tokio::test]
async fn server_relay_audio_reaches_recipient_peer_connection() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    register_audio_track(&state, &room_id, "source");
    register_audio_track(&state, &room_id, "recipient");
    let source = connect_test_offer(&state, &room_id, "source").await;
    let recipient = connect_test_offer(&state, &room_id, "recipient").await;

    source.send_valid_opus_packets(100).await;

    for _ in 0..150 {
        if !recipient.received_remote_rtp_packets().is_empty() {
            assert!(source.received_remote_rtp_packets().is_empty());
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("server relay audio RTP did not reach recipient peer connection");
}

async fn server_relay_noise_provider_reaches_recipient_with_audible_payload(
    provider: NoiseProvider,
) {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    start_relay_with_provider(&state, &room_id, provider);
    register_audio_track(&state, &room_id, "source");
    register_audio_track(&state, &room_id, "recipient");
    let source = connect_test_offer(&state, &room_id, "source").await;
    let recipient = connect_test_offer(&state, &room_id, "recipient").await;

    source.send_valid_opus_packets(100).await;

    for _ in 0..150 {
        let packets = recipient.received_remote_rtp_packets();
        if packets.len() >= 5 {
            let peak = packets
                .iter()
                .map(|packet| decoded_peak_from_rtp_payload(&packet.payload))
                .fold(0.0, f32::max);
            assert!(peak > 0.01, "provider={provider:?} peak={peak}");
            assert!(source.received_remote_rtp_packets().is_empty());
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("server relay audio RTP did not reach recipient peer connection");
}

#[tokio::test]
async fn server_relay_rnnoise_audio_reaches_recipient_with_audible_payload() {
    server_relay_noise_provider_reaches_recipient_with_audible_payload(NoiseProvider::Rnnoise)
        .await;
}

#[tokio::test]
async fn server_relay_deepfilternet_audio_reaches_recipient_with_audible_payload() {
    server_relay_noise_provider_reaches_recipient_with_audible_payload(
        NoiseProvider::Deepfilternet,
    )
    .await;
}

#[tokio::test]
async fn server_relay_audio_survives_repeated_room_relay_start() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    register_audio_track(&state, &room_id, "source");
    let source = connect_test_offer(&state, &room_id, "source").await;

    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    register_audio_track(&state, &room_id, "recipient");
    let recipient = connect_test_offer(&state, &room_id, "recipient").await;

    source.send_valid_opus_packets(100).await;

    for _ in 0..150 {
        if !recipient.received_remote_rtp_packets().is_empty() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("server relay audio RTP did not reach recipient after repeated relay start");
}
