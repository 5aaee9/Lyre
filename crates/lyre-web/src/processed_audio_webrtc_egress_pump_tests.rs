use crate::{
    api::AppState,
    media_egress::ProcessedAudioEgressFanout,
    media_runtime::WebMediaRuntime,
    processed_audio_webrtc_egress_pump::{
        ProcessedAudioWebRtcEgressPump, ProcessedAudioWebRtcEgressSender,
    },
};
use async_trait::async_trait;
use lyre_core::media::UpdateMediaRelaySubscriptionsRequest;
use lyre_core::{
    AudioFrame, MediaRelayStatus, MediaTrackKind, NoiseCancellationConfig, NoiseProvider,
    RegisterMediaTrackRequest, RoomId, StartMediaRelayRequest, StopMediaRelayRequest, UserId,
};
use lyre_webrtc::{
    test_support::encoded_opus_payload_for_test, ServerMediaConnectionStateSnapshot,
    ServerMediaEgressError, ServerMediaIceCandidate, ServerMediaNegotiator, ServerMediaOffer,
    ServerMediaOpusDecoder, ServerMediaProcessedAudioFrame, ServerMediaRtpPacket,
    ServerMediaSessionKey, ServerMediaSessionRegistry, WebRtcStack,
};
use std::{
    io::Write,
    sync::{Arc, Mutex},
};
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

#[derive(Debug)]
struct RecipientLifecycleSender {
    removed_user_id: UserId,
    deliveries: Mutex<Vec<(UserId, u64)>>,
    blocked_send_started: Notify,
    blocked_send_cancelled: Notify,
}

#[derive(Debug)]
struct ReadinessFailingSender {
    attempted_source_user_ids: Mutex<Vec<UserId>>,
}

struct NotifyOnDrop<'a>(&'a Notify);

#[derive(Clone)]
struct CapturedWriter(Arc<Mutex<Vec<u8>>>);

impl Write for CapturedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("captured log lock must not be poisoned")
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for NotifyOnDrop<'_> {
    fn drop(&mut self) {
        self.0.notify_waiters();
    }
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

#[async_trait]
impl ProcessedAudioWebRtcEgressSender for RecipientLifecycleSender {
    async fn send_processed_audio_frame(
        &self,
        key: &ServerMediaSessionKey,
        _source_user_id: &UserId,
        frame: ServerMediaProcessedAudioFrame,
    ) -> Result<usize, ServerMediaEgressError> {
        self.deliveries
            .lock()
            .expect("test sender deliveries lock must not be poisoned")
            .push((key.user_id.clone(), frame.sequence));
        if key.user_id == self.removed_user_id {
            let _notify_when_cancelled = NotifyOnDrop(&self.blocked_send_cancelled);
            self.blocked_send_started.notify_waiters();
            std::future::pending::<()>().await;
            unreachable!("test sender intentionally never completes");
        }
        Ok(1)
    }

    fn connection_state(
        &self,
        _key: &ServerMediaSessionKey,
    ) -> Option<ServerMediaConnectionStateSnapshot> {
        None
    }
}

#[async_trait]
impl ProcessedAudioWebRtcEgressSender for ReadinessFailingSender {
    async fn send_processed_audio_frame(
        &self,
        key: &ServerMediaSessionKey,
        source_user_id: &UserId,
        _frame: ServerMediaProcessedAudioFrame,
    ) -> Result<usize, ServerMediaEgressError> {
        self.attempted_source_user_ids
            .lock()
            .expect("test sender source lock must not be poisoned")
            .push(source_user_id.clone());
        Err(ServerMediaEgressError::PeerMissing {
            room_id: key.room_id.clone(),
            user_id: key.user_id.clone(),
        })
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
async fn departed_source_can_log_empty_fanout_again_after_rejoining() {
    let logs = Arc::new(Mutex::new(Vec::<u8>::new()));
    let writer = CapturedWriter(Arc::clone(&logs));
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let relays = Arc::new(lyre_core::MediaRelayRegistry::new());
    let room_id = RoomId::default_room();
    let source_user_id = UserId::from_external("source");
    relays.start(room_id.clone(), StartMediaRelayRequest::default());
    relays
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: source_user_id.clone(),
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
    let runtime = Arc::new(WebMediaRuntime::new(Arc::clone(&relays)));
    let fanout = Arc::new(ProcessedAudioEgressFanout::new(Arc::clone(&relays)));
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = Arc::new(ServerMediaNegotiator::new(WebRtcStack::new(), sessions));
    let pump = ProcessedAudioWebRtcEgressPump::new(Arc::clone(&runtime), fanout, negotiator);
    pump.start(room_id.clone());

    runtime
        .process_frame(audio_frame(room_id.clone(), source_user_id.clone()))
        .unwrap();
    wait_for_log_count(&logs, "processed audio WebRTC egress has no recipients", 1).await;

    relays
        .remove_participant(room_id.clone(), &source_user_id)
        .unwrap();
    pump.remove_recipient(&room_id, &source_user_id);
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    relays
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: source_user_id.clone(),
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
    runtime
        .process_frame(audio_frame(room_id.clone(), source_user_id))
        .unwrap();

    wait_for_log_count(&logs, "processed audio WebRTC egress has no recipients", 2).await;
    pump.stop_and_wait_for_test(&room_id).await;
}

async fn wait_for_log_count(logs: &Arc<Mutex<Vec<u8>>>, message: &str, expected: usize) {
    for _ in 0..100 {
        let count = String::from_utf8(
            logs.lock()
                .expect("captured log lock must not be poisoned")
                .clone(),
        )
        .expect("captured logs must be UTF-8")
        .matches(message)
        .count();
        if count >= expected {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("expected at least {expected} occurrences of {message:?}");
}

fn many_source_relay_context(
    source_count: usize,
) -> (
    Arc<lyre_core::MediaRelayRegistry>,
    Arc<WebMediaRuntime>,
    Arc<ProcessedAudioEgressFanout>,
    Vec<UserId>,
) {
    let relays = Arc::new(lyre_core::MediaRelayRegistry::new());
    let room_id = RoomId::default_room();
    let recipient_user_id = UserId::from_external("recipient");
    relays.start(room_id.clone(), StartMediaRelayRequest::default());
    relays
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: recipient_user_id.clone(),
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
    let source_user_ids = (0..source_count)
        .map(|index| UserId::from_external(format!("source-{index}")))
        .collect::<Vec<_>>();
    for source_user_id in &source_user_ids {
        relays
            .register_track(
                room_id.clone(),
                RegisterMediaTrackRequest {
                    user_id: source_user_id.clone(),
                    track_id: "audio-main".to_owned(),
                    kind: MediaTrackKind::Audio,
                },
            )
            .unwrap();
        relays
            .update_subscriptions(
                room_id.clone(),
                UpdateMediaRelaySubscriptionsRequest {
                    user_id: source_user_id.clone(),
                    source_user_ids: Vec::new(),
                },
            )
            .unwrap();
    }
    relays
        .update_subscriptions(
            room_id,
            UpdateMediaRelaySubscriptionsRequest {
                user_id: recipient_user_id,
                source_user_ids: source_user_ids.clone(),
            },
        )
        .unwrap();
    let runtime = Arc::new(WebMediaRuntime::new(Arc::clone(&relays)));
    let fanout = Arc::new(ProcessedAudioEgressFanout::new(Arc::clone(&relays)));
    (relays, runtime, fanout, source_user_ids)
}

#[tokio::test]
async fn recipient_worker_logs_send_start_once_for_many_sources() {
    let logs = Arc::new(Mutex::new(Vec::<u8>::new()));
    let writer = CapturedWriter(Arc::clone(&logs));
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let (_relays, runtime, fanout, source_user_ids) = many_source_relay_context(8);
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

    for (index, source_user_id) in source_user_ids.into_iter().enumerate() {
        runtime
            .process_frame(AudioFrame {
                sequence: index as u64,
                ..audio_frame(room_id.clone(), source_user_id)
            })
            .unwrap();
        for _ in 0..100 {
            if sender
                .fast_source_user_ids
                .lock()
                .expect("test sender source lock must not be poisoned")
                .len()
                > index
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    pump.stop_and_wait_for_test(&room_id).await;
    let output = String::from_utf8(
        logs.lock()
            .expect("captured log lock must not be poisoned")
            .clone(),
    )
    .expect("captured logs must be UTF-8");
    assert_eq!(
        output
            .matches("processed audio WebRTC egress send started")
            .count(),
        1
    );
}

#[tokio::test]
async fn recipient_worker_logs_each_readiness_failure_kind_once_for_many_sources() {
    let logs = Arc::new(Mutex::new(Vec::<u8>::new()));
    let writer = CapturedWriter(Arc::clone(&logs));
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || writer.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::DEBUG)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let (_relays, runtime, fanout, source_user_ids) = many_source_relay_context(8);
    let sender = Arc::new(ReadinessFailingSender {
        attempted_source_user_ids: Mutex::new(Vec::new()),
    });
    let pump =
        ProcessedAudioWebRtcEgressPump::new(Arc::clone(&runtime), fanout, Arc::clone(&sender));
    let room_id = RoomId::default_room();
    pump.start(room_id.clone());

    for (index, source_user_id) in source_user_ids.into_iter().enumerate() {
        runtime
            .process_frame(AudioFrame {
                sequence: index as u64,
                ..audio_frame(room_id.clone(), source_user_id)
            })
            .unwrap();
        for _ in 0..100 {
            if sender
                .attempted_source_user_ids
                .lock()
                .expect("test sender source lock must not be poisoned")
                .len()
                > index
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    pump.stop_and_wait_for_test(&room_id).await;
    let output = String::from_utf8(
        logs.lock()
            .expect("captured log lock must not be poisoned")
            .clone(),
    )
    .expect("captured logs must be UTF-8");
    assert_eq!(
        output
            .matches("processed audio WebRTC egress peer is not ready")
            .count(),
        1
    );
}

#[tokio::test]
async fn app_state_start_and_stop_manage_egress_pump() {
    let state = AppState::default();
    let room_id = RoomId::default_room();

    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    assert_eq!(state.raw_opus_webrtc_egress_pump_count(), 1);
    state.start_media_relay(room_id.clone(), StartMediaRelayRequest::default());
    assert_eq!(state.raw_opus_webrtc_egress_pump_count(), 1);

    state
        .stop_media_relay(
            room_id,
            StopMediaRelayRequest {
                user_id: UserId::from_external("user_01"),
            },
        )
        .await;
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
async fn leaving_last_participant_removes_relay_and_clears_processed_room_resources() {
    let state = AppState::default();
    let room_id = RoomId::default_room();
    let user = state
        .join_room_persisted(room_id.clone(), Default::default())
        .await
        .unwrap()
        .user;

    start_relay_with_provider(&state, &room_id, NoiseProvider::Rnnoise);
    let mut processed_frames = state.subscribe_processed_media_frames(&room_id);

    state
        .leave_room_persisted(&room_id, &user.id)
        .await
        .unwrap();

    assert!(!state.media_relays.contains_room(&room_id));
    assert_eq!(
        state.media_relays.status(room_id.clone()).status,
        MediaRelayStatus::Inactive
    );
    assert_eq!(state.processed_audio_webrtc_egress_pump_count(), 0);
    assert_eq!(state.raw_opus_webrtc_egress_pump_count(), 0);
    assert_eq!(state.server_media_runtime_pump_count(), 0);
    assert!(matches!(
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            processed_frames.recv()
        )
        .await,
        Ok(Err(tokio::sync::broadcast::error::RecvError::Closed))
    ));
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
    state.close_server_media_sessions_for_room(&room_id).await;
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
async fn closing_processed_audio_room_removes_finished_egress_pump_task() {
    let (_relays, runtime, fanout) = relay_context();
    let sessions = Arc::new(ServerMediaSessionRegistry::new());
    let negotiator = Arc::new(ServerMediaNegotiator::new(WebRtcStack::new(), sessions));
    let pump = ProcessedAudioWebRtcEgressPump::new(Arc::clone(&runtime), fanout, negotiator);
    let room_id = RoomId::default_room();

    pump.start(room_id.clone());
    assert_eq!(pump.task_count(), 1);
    runtime.clear_room(&room_id);

    for _ in 0..100 {
        if pump.task_count() == 0 {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    panic!("finished processed-audio egress pump task remained registered");
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
async fn removing_recipient_cancels_its_worker_while_other_recipient_keeps_receiving() {
    let (relays, runtime, fanout) = relay_context();
    let removed_user_id = UserId::from_external("blocked");
    let fast_user_id = UserId::from_external("fast");
    let sender = Arc::new(RecipientLifecycleSender {
        removed_user_id: removed_user_id.clone(),
        deliveries: Mutex::new(Vec::new()),
        blocked_send_started: Notify::new(),
        blocked_send_cancelled: Notify::new(),
    });
    let pump =
        ProcessedAudioWebRtcEgressPump::new(Arc::clone(&runtime), fanout, Arc::clone(&sender));
    let room_id = RoomId::default_room();
    pump.start(room_id.clone());

    let blocked_send_started = sender.blocked_send_started.notified();
    runtime.process_frame(sequenced_audio_frame(1)).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), blocked_send_started)
        .await
        .expect("removed recipient send must start");
    for _ in 0..100 {
        if sender
            .deliveries
            .lock()
            .expect("test sender deliveries lock must not be poisoned")
            .contains(&(fast_user_id.clone(), 1))
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    relays
        .remove_participant(room_id.clone(), &removed_user_id)
        .unwrap();
    let blocked_send_cancelled = sender.blocked_send_cancelled.notified();
    pump.remove_recipient(&room_id, &removed_user_id);
    tokio::time::timeout(std::time::Duration::from_secs(1), blocked_send_cancelled)
        .await
        .expect("removed recipient worker must cancel its in-flight send");

    runtime.process_frame(sequenced_audio_frame(2)).unwrap();
    for _ in 0..100 {
        if sender
            .deliveries
            .lock()
            .expect("test sender deliveries lock must not be poisoned")
            .contains(&(fast_user_id.clone(), 2))
        {
            pump.stop_and_wait_for_test(&room_id).await;
            let deliveries = sender
                .deliveries
                .lock()
                .expect("test sender deliveries lock must not be poisoned")
                .clone();
            assert_eq!(
                deliveries
                    .iter()
                    .filter(|(user_id, _)| user_id == &removed_user_id)
                    .map(|(_, sequence)| *sequence)
                    .collect::<Vec<_>>(),
                vec![1]
            );
            assert_eq!(
                deliveries
                    .iter()
                    .filter(|(user_id, _)| user_id == &fast_user_id)
                    .map(|(_, sequence)| *sequence)
                    .collect::<Vec<_>>(),
                vec![1, 2]
            );
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    pump.stop_and_wait_for_test(&room_id).await;
    panic!("remaining recipient stopped receiving after peer removal");
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

async fn connect_test_offer_with_opus_payload_type(
    state: &AppState,
    room_id: &RoomId,
    user_id: &str,
    payload_type: u8,
) -> lyre_webrtc::test_support::ServerMediaConnectedOffer {
    let key = ServerMediaSessionKey {
        room_id: room_id.clone(),
        user_id: UserId::from_external(user_id),
    };
    let offer =
        lyre_webrtc::test_support::server_media_offer_with_opus_payload_type(payload_type).await;
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
    let source = connect_test_offer(&state, &room_id, "source").await;
    let recipient = connect_test_offer(&state, &room_id, "recipient").await;

    state
        .process_media_frame(audio_frame(
            room_id.clone(),
            UserId::from_external("source"),
        ))
        .unwrap();

    for _ in 0..150 {
        let packets = recipient.received_remote_rtp_packets();
        if let Some(packet) = packets.first() {
            assert!(source.received_remote_rtp_packets().is_empty());
            assert_eq!(packet.header.payload_type, 111);
            assert!(!packet.payload.is_empty());
            assert!(decoded_peak_from_rtp_payload(&packet.payload) > 0.0);
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
async fn server_relay_off_noise_normalizes_source_dynamic_payload_type_for_recipient() {
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
    let source = connect_test_offer_with_opus_payload_type(&state, &room_id, "source", 109).await;
    let recipient =
        connect_test_offer_with_opus_payload_type(&state, &room_id, "recipient", 109).await;

    source.send_valid_opus_packets(1).await;

    for _ in 0..150 {
        if let Some(packet) = recipient.received_remote_rtp_packets().first() {
            assert_eq!(packet.header.payload_type, 109);
            assert_eq!(packet.payload, encoded_opus_payload_for_test());
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("raw relay dropped source RTP negotiated with dynamic Opus payload type 109");
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
    let subscribed = connect_test_offer(&state, &room_id, "subscribed").await;

    source.send_valid_opus_packets(1).await;

    for _ in 0..150 {
        if !subscribed.received_remote_rtp_packets().is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
            assert_eq!(subscribed.received_remote_rtp_packets().len(), 1);
            assert!(source.received_remote_rtp_packets().is_empty());
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("server relay audio RTP did not reach subscribed recipient peer connection");
}

#[tokio::test]
async fn switching_noise_off_does_not_replay_preexisting_source_rtp() {
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
    let source = connect_test_offer(&state, &room_id, "source").await;
    let recipient = connect_test_offer(&state, &room_id, "recipient").await;

    source.send_valid_opus_packets(5).await;
    for _ in 0..150 {
        if !recipient.received_remote_rtp_packets().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    let received_before_switch = recipient.received_remote_rtp_packets().len();
    assert!(received_before_switch > 0);

    state
        .update_media_relay_settings(
            room_id.clone(),
            lyre_core::UpdateMediaRelaySettingsRequest {
                user_id: UserId::from_external("source"),
                noise: NoiseCancellationConfig {
                    provider: NoiseProvider::Off,
                    ..NoiseCancellationConfig::default()
                },
            },
        )
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;
    assert_eq!(
        recipient.received_remote_rtp_packets().len(),
        received_before_switch
    );

    source.send_valid_opus_packets(1).await;
    for _ in 0..150 {
        if recipient.received_remote_rtp_packets().len() > received_before_switch {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("server relay did not forward fresh source RTP after switching noise off");
}

#[tokio::test]
async fn switching_noise_on_after_raw_relay_and_source_reconnect_keeps_audio_flowing() {
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

    source.send_valid_opus_packets(5).await;
    for _ in 0..150 {
        if !recipient.received_remote_rtp_packets().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let received_before_switch = recipient.received_remote_rtp_packets().len();
    assert!(received_before_switch > 0);

    state
        .update_media_relay_settings(
            room_id.clone(),
            lyre_core::UpdateMediaRelaySettingsRequest {
                user_id: UserId::from_external("source"),
                noise: NoiseCancellationConfig {
                    provider: NoiseProvider::Rnnoise,
                    intensity: 0.5,
                    voice_activity_threshold: 0.35,
                    ..NoiseCancellationConfig::default()
                },
            },
        )
        .unwrap();

    let source = connect_test_offer(&state, &room_id, "source").await;
    source.send_valid_opus_packets(20).await;
    for _ in 0..150 {
        if recipient.received_remote_rtp_packets().len() > received_before_switch {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    panic!("server relay did not forward processed audio after switching noise on");
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
