use dashmap::DashMap;
use lyre_core::{
    AudioFrame, MediaRelayError, MediaRelayRegistry, MediaRuntime, ProcessedAudioFrame,
    ProcessedAudioSink, RoomId,
};
use lyre_noise_cancelling::{
    DeepFilterNetRuntimeConfig, NoiseCancellingAudioFrameProcessor, NoiseModelRuntimeConfig,
};
use std::{fmt, sync::Arc};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Default)]
pub struct ProcessedAudioBroadcaster {
    frames: Arc<DashMap<RoomId, Vec<ProcessedAudioFrame>>>,
    channels: Arc<DashMap<RoomId, broadcast::Sender<ProcessedAudioFrame>>>,
}

const PROCESSED_AUDIO_CHANNEL_CAPACITY: usize = 256;

impl ProcessedAudioBroadcaster {
    pub fn frames_for_room(&self, room_id: &RoomId) -> Vec<ProcessedAudioFrame> {
        self.frames
            .get(room_id)
            .map(|frames| frames.clone())
            .unwrap_or_default()
    }

    pub fn subscribe(&self, room_id: &RoomId) -> broadcast::Receiver<ProcessedAudioFrame> {
        self.sender(room_id).subscribe()
    }

    pub fn clear_room(&self, room_id: &RoomId) {
        self.frames.remove(room_id);
        self.channels.remove(room_id);
    }

    fn sender(&self, room_id: &RoomId) -> broadcast::Sender<ProcessedAudioFrame> {
        self.channels
            .entry(room_id.clone())
            .or_insert_with(|| broadcast::channel(PROCESSED_AUDIO_CHANNEL_CAPACITY).0)
            .clone()
    }
}

impl ProcessedAudioSink for ProcessedAudioBroadcaster {
    fn publish(&self, frame: ProcessedAudioFrame) {
        self.frames
            .entry(frame.room_id.clone())
            .or_default()
            .push(frame.clone());
        let _ = self.sender(&frame.room_id).send(frame);
    }
}

pub struct WebMediaRuntime {
    runtime: MediaRuntime<NoiseCancellingAudioFrameProcessor, ProcessedAudioBroadcaster>,
    sink: ProcessedAudioBroadcaster,
}

impl WebMediaRuntime {
    pub fn new(relays: Arc<MediaRelayRegistry>) -> Self {
        Self::with_deepfilternet_runtime(relays, DeepFilterNetRuntimeConfig::default())
    }

    pub fn with_deepfilternet_runtime(
        relays: Arc<MediaRelayRegistry>,
        deepfilternet_runtime: DeepFilterNetRuntimeConfig,
    ) -> Self {
        Self::with_noise_model_runtime(
            relays,
            NoiseModelRuntimeConfig {
                deepfilternet: deepfilternet_runtime,
                ..NoiseModelRuntimeConfig::default()
            },
        )
    }

    pub fn with_noise_model_runtime(
        relays: Arc<MediaRelayRegistry>,
        model_runtime: NoiseModelRuntimeConfig,
    ) -> Self {
        let sink = ProcessedAudioBroadcaster::default();
        let runtime = MediaRuntime::new(
            relays,
            NoiseCancellingAudioFrameProcessor::with_model_runtime(model_runtime),
            sink.clone(),
        );
        Self { runtime, sink }
    }

    pub fn process_frame(&self, frame: AudioFrame) -> Result<(), MediaRelayError> {
        self.runtime.process_frame(frame)
    }

    pub fn frames_for_room(&self, room_id: &RoomId) -> Vec<ProcessedAudioFrame> {
        self.sink.frames_for_room(room_id)
    }

    pub fn subscribe(&self, room_id: &RoomId) -> broadcast::Receiver<ProcessedAudioFrame> {
        self.sink.subscribe(room_id)
    }

    pub fn clear_room(&self, room_id: &RoomId) {
        self.sink.clear_room(room_id);
    }
}

impl fmt::Debug for WebMediaRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WebMediaRuntime")
            .finish_non_exhaustive()
    }
}
