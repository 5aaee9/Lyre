use super::*;
use crate::{
    MediaTrackKind, NoiseCancellationConfig, ProcessedAudioFrame, RegisterMediaTrackRequest,
    StartMediaRelayRequest,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};

struct BlockingLifecycleProcessor {
    entered: mpsc::SyncSender<()>,
    proceed: Mutex<mpsc::Receiver<()>>,
    cached: Arc<AtomicBool>,
}

impl AudioFrameProcessor for BlockingLifecycleProcessor {
    fn process(&self, frame: &AudioFrame, _noise: &NoiseCancellationConfig) -> Vec<f32> {
        self.entered.send(()).unwrap();
        self.proceed.lock().unwrap().recv().unwrap();
        self.cached.store(true, Ordering::SeqCst);
        frame.samples.clone()
    }

    fn release_source(&self, _room_id: &RoomId, _user_id: &UserId) {
        self.cached.store(false, Ordering::SeqCst);
    }
}

struct IgnoringSink;

impl ProcessedAudioSink for IgnoringSink {
    fn publish(&self, _frame: ProcessedAudioFrame) {}
}

struct BatchLifecycleProcessor {
    processed: mpsc::SyncSender<()>,
    cached: Arc<AtomicBool>,
}

impl AudioFrameProcessor for BatchLifecycleProcessor {
    fn process(&self, frame: &AudioFrame, _noise: &NoiseCancellationConfig) -> Vec<f32> {
        self.cached.store(true, Ordering::SeqCst);
        self.processed.send(()).unwrap();
        frame.samples.clone()
    }

    fn release_source(&self, _room_id: &RoomId, _user_id: &UserId) {
        self.cached.store(false, Ordering::SeqCst);
    }
}

fn frame(room_id: RoomId, user_id: UserId) -> AudioFrame {
    AudioFrame {
        room_id,
        user_id,
        track_id: "audio-main".to_owned(),
        sample_rate_hz: 48_000,
        channels: 1,
        sequence: 7,
        rtp_timestamp: None,
        samples: vec![0.1, -0.2, 0.3],
    }
}

#[test]
fn release_waits_for_admitted_frame_before_clearing_processor_state() {
    let relays = Arc::new(MediaRelayRegistry::new());
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    relays.start(room_id.clone(), StartMediaRelayRequest::default());
    relays
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: user_id.clone(),
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (proceed_tx, proceed_rx) = mpsc::channel();
    let cached = Arc::new(AtomicBool::new(false));
    let runtime = Arc::new(MediaRuntime::new(
        Arc::clone(&relays),
        BlockingLifecycleProcessor {
            entered: entered_tx,
            proceed: Mutex::new(proceed_rx),
            cached: Arc::clone(&cached),
        },
        IgnoringSink,
    ));
    let processing_runtime = Arc::clone(&runtime);
    let processing_room_id = room_id.clone();
    let processing_user_id = user_id.clone();
    let processing = std::thread::spawn(move || {
        processing_runtime
            .process_frame(frame(processing_room_id, processing_user_id))
            .unwrap();
    });
    entered_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("frame must enter processor");
    relays
        .remove_participant(room_id.clone(), &user_id)
        .unwrap();

    let cleanup_runtime = Arc::clone(&runtime);
    let cleanup_room_id = room_id.clone();
    let cleanup_user_id = user_id.clone();
    let (cleanup_finished_tx, cleanup_finished_rx) = mpsc::channel();
    let cleanup = std::thread::spawn(move || {
        cleanup_runtime.release_source(&cleanup_room_id, &cleanup_user_id);
        cleanup_finished_tx.send(()).unwrap();
    });

    assert_eq!(
        cleanup_finished_rx.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout),
        "cleanup returned while an admitted frame could still recreate processor state"
    );

    proceed_tx.send(()).unwrap();
    processing.join().unwrap();
    cleanup_finished_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("cleanup must finish after the admitted frame");
    cleanup.join().unwrap();
    assert!(!cached.load(Ordering::SeqCst));

    relays
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: user_id.clone(),
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
    proceed_tx.send(()).unwrap();
    runtime.process_frame(frame(room_id, user_id)).unwrap();
    entered_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("rejoined source must reach the processor");
    assert!(cached.load(Ordering::SeqCst));
}

#[test]
fn release_waits_for_the_entire_admitted_frame_batch() {
    let relays = Arc::new(MediaRelayRegistry::new());
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_01");
    relays.start(room_id.clone(), StartMediaRelayRequest::default());
    relays
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: user_id.clone(),
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
    let (processed_tx, processed_rx) = mpsc::sync_channel(2);
    let cached = Arc::new(AtomicBool::new(false));
    let runtime = Arc::new(MediaRuntime::new(
        relays,
        BatchLifecycleProcessor {
            processed: processed_tx,
            cached: Arc::clone(&cached),
        },
        IgnoringSink,
    ));
    let (frame_tx, frame_rx) = mpsc::channel();
    let batch_runtime = Arc::clone(&runtime);
    let batch = std::thread::spawn(move || batch_runtime.process_frames(frame_rx));

    frame_tx
        .send(frame(room_id.clone(), user_id.clone()))
        .unwrap();
    processed_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("first batch frame must be processed");

    let cleanup_runtime = Arc::clone(&runtime);
    let cleanup_room_id = room_id.clone();
    let cleanup_user_id = user_id.clone();
    let (cleanup_finished_tx, cleanup_finished_rx) = mpsc::channel();
    let cleanup = std::thread::spawn(move || {
        cleanup_runtime.release_source(&cleanup_room_id, &cleanup_user_id);
        cleanup_finished_tx.send(()).unwrap();
    });

    assert_eq!(
        cleanup_finished_rx.recv_timeout(Duration::from_millis(50)),
        Err(mpsc::RecvTimeoutError::Timeout),
        "cleanup returned between two frames from one admitted batch"
    );

    frame_tx.send(frame(room_id, user_id)).unwrap();
    drop(frame_tx);
    processed_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("second batch frame must be processed");
    assert_eq!(batch.join().unwrap().unwrap(), 2);
    cleanup_finished_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("cleanup must finish after the admitted batch");
    cleanup.join().unwrap();
    assert!(!cached.load(Ordering::SeqCst));
}
