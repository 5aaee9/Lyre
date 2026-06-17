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
    test_support::encoded_opus_payload_for_test, ServerMediaConnectionStateSnapshot,
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
    fast_rtp_timestamps: Mutex<Vec<Option<u32>>>,
    fast_source_user_ids: Mutex<Vec<UserId>>,
    notify: Notify,
}

#[derive(Debug)]
struct FailingPeerSender {
    attempts: Mutex<Vec<u64>>,
    notify: Notify,
}

#[async_trait]
impl ProcessedAudioWebRtcEgressSender for FailingPeerSender {
    async fn send_processed_audio_frame(
        &self,
        _key: &ServerMediaSessionKey,
        _source_user_id: &UserId,
        frame: ServerMediaProcessedAudioFrame,
    ) -> Result<usize, ServerMediaEgressError> {
        self.attempts
            .lock()
            .expect("test sender attempts lock must not be poisoned")
            .push(frame.sequence);
        self.notify.notify_waiters();
        Err(ServerMediaEgressError::PeerMissing {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external("fast"),
        })
    }

    fn connection_state(
        &self,
        _key: &ServerMediaSessionKey,
    ) -> Option<ServerMediaConnectionStateSnapshot> {
        Some(ServerMediaConnectionStateSnapshot::failed_for_test())
    }
}

#[async_trait]
impl ProcessedAudioWebRtcEgressSender for BlockingSender {
    async fn send_processed_audio_frame(
        &self,
        key: &ServerMediaSessionKey,
        source_user_id: &UserId,
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
        self.fast_rtp_timestamps
            .lock()
            .expect("test sender RTP timestamp lock must not be poisoned")
            .push(frame.rtp_timestamp);
        self.fast_source_user_ids
            .lock()
            .expect("test sender source lock must not be poisoned")
            .push(source_user_id.clone());
        self.notify.notify_waiters();
        Ok(1)
    }

    fn connection_state(
        &self,
        _key: &ServerMediaSessionKey,
    ) -> Option<ServerMediaConnectionStateSnapshot> {
        None
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
    assert_eq!(state.raw_opus_webrtc_egress_pump_count(), 1);
    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    assert_eq!(state.raw_opus_webrtc_egress_pump_count(), 1);

    state.stop_media_relay(
        room_id,
        StopMediaRelayRequest {
            user_id: UserId::from_external("user_01"),
        },
    );
    assert_eq!(state.raw_opus_webrtc_egress_pump_count(), 0);
}

#[tokio::test]
async fn leaving_last_participant_stops_processed_egress_pump() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap()
        .user;

    state.start_media_relay(
        room_id.clone(),
        StartMediaRelayRequest {
            noise: Some(NoiseCancellationConfig {
                provider: NoiseProvider::Dpdfnet,
                ..NoiseCancellationConfig::default()
            }),
        },
    );
    assert_eq!(state.processed_audio_webrtc_egress_pump_count(), 1);

    state
        .leave_room_persisted(&room_id, &user.id)
        .await
        .unwrap();

    assert_eq!(state.processed_audio_webrtc_egress_pump_count(), 0);
}

#[tokio::test]
async fn leaving_last_participant_stops_raw_opus_egress_pump() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap()
        .user;

    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    assert_eq!(state.raw_opus_webrtc_egress_pump_count(), 1);

    state
        .leave_room_persisted(&room_id, &user.id)
        .await
        .unwrap();

    assert_eq!(state.raw_opus_webrtc_egress_pump_count(), 0);
}

#[tokio::test]
async fn close_server_media_sessions_stops_egress_pump() {
    let state = AppState::default();
    let room_id = RoomId::default_room();

    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    assert_eq!(state.raw_opus_webrtc_egress_pump_count(), 1);
    state.close_server_media_sessions_for_room(&room_id);
    assert_eq!(state.raw_opus_webrtc_egress_pump_count(), 0);
}

fn audio_frame(room_id: RoomId, user_id: UserId) -> AudioFrame {
    AudioFrame {
        room_id,
        user_id,
        track_id: "audio-main".to_owned(),
        sample_rate_hz: 48_000,
        channels: 1,
        sequence: 1,
        rtp_timestamp: None,
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
        rtp_timestamp: None,
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
        fast_rtp_timestamps: Mutex::new(Vec::new()),
        fast_source_user_ids: Mutex::new(Vec::new()),
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

#[tokio::test]
async fn failed_recipient_peer_stops_egress_worker_after_first_failure() {
    let (_relays, runtime, fanout) = relay_context();
    let sender = Arc::new(FailingPeerSender {
        attempts: Mutex::new(Vec::new()),
        notify: Notify::new(),
    });
    let pump =
        ProcessedAudioWebRtcEgressPump::new(Arc::clone(&runtime), fanout, Arc::clone(&sender));
    let room_id = RoomId::default_room();
    pump.start(room_id.clone());

    runtime.process_frame(sequenced_audio_frame(1)).unwrap();
    for _ in 0..100 {
        if !sender
            .attempts
            .lock()
            .expect("test sender attempts lock must not be poisoned")
            .is_empty()
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    runtime.process_frame(sequenced_audio_frame(2)).unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    pump.stop_and_wait_for_test(&room_id).await;
    assert_eq!(
        *sender
            .attempts
            .lock()
            .expect("test sender attempts lock must not be poisoned"),
        vec![1, 1]
    );
}

#[test]
fn transient_egress_readiness_errors_are_not_warn_failures() {
    assert!(
        !crate::processed_audio_webrtc_egress_pump::warns_for_send_failure(
            &ServerMediaEgressError::PeerMissing {
                room_id: RoomId::default_room(),
                user_id: UserId::from_external("recipient"),
            },
            None,
        )
    );
    assert!(
        !crate::processed_audio_webrtc_egress_pump::warns_for_send_failure(
            &ServerMediaEgressError::SourceNotNegotiated {
                source_user_id: UserId::from_external("source"),
            },
            Some(&ServerMediaConnectionStateSnapshot::default()),
        )
    );
    assert!(
        crate::processed_audio_webrtc_egress_pump::warns_for_send_failure(
            &ServerMediaEgressError::SourceNotNegotiated {
                source_user_id: UserId::from_external("source"),
            },
            Some(&ServerMediaConnectionStateSnapshot::failed_for_test()),
        )
    );
}

#[tokio::test]
async fn egress_pump_forwards_source_rtp_timestamp_metadata() {
    let (_relays, runtime, fanout) = relay_context();
    let sender = Arc::new(BlockingSender {
        blocked_user_id: UserId::from_external("unused"),
        blocked_sequences: Mutex::new(Vec::new()),
        fast_sequences: Mutex::new(Vec::new()),
        fast_rtp_timestamps: Mutex::new(Vec::new()),
        fast_source_user_ids: Mutex::new(Vec::new()),
        notify: Notify::new(),
    });
    let pump =
        ProcessedAudioWebRtcEgressPump::new(Arc::clone(&runtime), fanout, Arc::clone(&sender));
    let room_id = RoomId::default_room();
    pump.start(room_id.clone());

    runtime
        .process_frame(AudioFrame {
            rtp_timestamp: Some(48_000),
            ..sequenced_audio_frame(7)
        })
        .unwrap();

    for _ in 0..100 {
        let timestamps = sender
            .fast_rtp_timestamps
            .lock()
            .expect("test sender RTP timestamp lock must not be poisoned")
            .clone();
        if timestamps.len() >= 2 {
            pump.stop_and_wait_for_test(&room_id).await;
            assert_eq!(timestamps, vec![Some(48_000), Some(48_000)]);
            assert_eq!(
                sender
                    .fast_source_user_ids
                    .lock()
                    .expect("test sender source lock must not be poisoned")
                    .clone(),
                vec![
                    UserId::from_external("source"),
                    UserId::from_external("source")
                ]
            );
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    pump.stop_and_wait_for_test(&room_id).await;
    panic!("recipient egress did not receive source RTP timestamp metadata");
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
                ..NoiseCancellationConfig::default()
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
        .answer_server_media_offer_with_subscriptions(ServerMediaOffer {
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
        .answer_server_media_offer_with_subscriptions(ServerMediaOffer {
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
    start_relay_with_provider(&state, &room_id, NoiseProvider::Rnnoise);
    register_audio_track(&state, &room_id, "source");
    register_audio_track(&state, &room_id, "recipient");
    state
        .media_relays
        .update_subscriptions(
            room_id.clone(),
            lyre_core::media::UpdateMediaRelaySubscriptionsRequest {
                user_id: UserId::from_external("recipient"),
                source_user_ids: vec![UserId::from_external("source")],
            },
        )
        .unwrap();
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
            assert_eq!(
                recipient.sent_egress_rtp_packets_for_test()[0].payload_type,
                111
            );
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
    state
        .media_relays
        .update_subscriptions(
            room_id.clone(),
            lyre_core::media::UpdateMediaRelaySubscriptionsRequest {
                user_id: UserId::from_external("recipient"),
                source_user_ids: vec![UserId::from_external("source")],
            },
        )
        .unwrap();
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

#[tokio::test]
async fn server_relay_off_noise_forwards_opus_payload_without_transcoding() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    register_audio_track(&state, &room_id, "source");
    register_audio_track(&state, &room_id, "recipient");
    state
        .media_relays
        .update_subscriptions(
            room_id.clone(),
            lyre_core::media::UpdateMediaRelaySubscriptionsRequest {
                user_id: UserId::from_external("recipient"),
                source_user_ids: vec![UserId::from_external("source")],
            },
        )
        .unwrap();
    let source = connect_test_offer(&state, &room_id, "source").await;
    let recipient = connect_test_offer(&state, &room_id, "recipient").await;
    let expected_payload = encoded_opus_payload_for_test();

    source.send_valid_opus_packets(1).await;

    for _ in 0..150 {
        let packets = recipient.received_remote_rtp_packets();
        if let Some(packet) = packets.first() {
            assert_eq!(packet.payload, expected_payload);
            assert!(source.received_remote_rtp_packets().is_empty());
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("server relay did not forward source Opus payload");
}

#[tokio::test]
async fn server_relay_off_noise_excludes_unsubscribed_raw_opus_recipient() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    register_audio_track(&state, &room_id, "source");
    register_audio_track(&state, &room_id, "subscribed");
    register_audio_track(&state, &room_id, "unsubscribed");
    state
        .media_relays
        .update_subscriptions(
            room_id.clone(),
            lyre_core::media::UpdateMediaRelaySubscriptionsRequest {
                user_id: UserId::from_external("subscribed"),
                source_user_ids: vec![UserId::from_external("source")],
            },
        )
        .unwrap();
    state
        .media_relays
        .update_subscriptions(
            room_id.clone(),
            lyre_core::media::UpdateMediaRelaySubscriptionsRequest {
                user_id: UserId::from_external("unsubscribed"),
                source_user_ids: Vec::new(),
            },
        )
        .unwrap();
    let source = connect_test_offer(&state, &room_id, "source").await;
    let subscribed = connect_test_offer(&state, &room_id, "subscribed").await;
    let unsubscribed = connect_test_offer(&state, &room_id, "unsubscribed").await;

    source.send_valid_opus_packets(10).await;

    for _ in 0..150 {
        if !subscribed.received_remote_rtp_packets().is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            assert!(source.received_remote_rtp_packets().is_empty());
            assert!(unsubscribed.received_remote_rtp_packets().is_empty());
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("server relay audio RTP did not reach subscribed recipient peer connection");
}

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

async fn server_relay_noise_provider_reaches_recipient_with_audible_payload(
    provider: NoiseProvider,
) {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    start_relay_with_provider(&state, &room_id, provider);
    register_audio_track(&state, &room_id, "source");
    register_audio_track(&state, &room_id, "recipient");
    state
        .media_relays
        .update_subscriptions(
            room_id.clone(),
            lyre_core::media::UpdateMediaRelaySubscriptionsRequest {
                user_id: UserId::from_external("recipient"),
                source_user_ids: vec![UserId::from_external("source")],
            },
        )
        .unwrap();
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
async fn server_relay_dpdfnet_audio_reaches_recipient_with_audible_payload() {
    server_relay_noise_provider_reaches_recipient_with_audible_payload(NoiseProvider::Dpdfnet)
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
