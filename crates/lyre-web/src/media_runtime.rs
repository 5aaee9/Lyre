use dashmap::DashMap;
use lyre_core::{
    AudioFrame, MediaRelayError, MediaRelayRegistry, MediaRuntime, ProcessedAudioFrame,
    ProcessedAudioSink, RoomId,
};
use lyre_noise_cancelling::NoiseCancellingAudioFrameProcessor;
use std::{fmt, sync::Arc};

#[derive(Debug, Clone, Default)]
pub struct RecordingProcessedAudioSink {
    frames: Arc<DashMap<RoomId, Vec<ProcessedAudioFrame>>>,
}

impl RecordingProcessedAudioSink {
    pub fn frames_for_room(&self, room_id: &RoomId) -> Vec<ProcessedAudioFrame> {
        self.frames
            .get(room_id)
            .map(|frames| frames.clone())
            .unwrap_or_default()
    }

    pub fn clear_room(&self, room_id: &RoomId) {
        self.frames.remove(room_id);
    }
}

impl ProcessedAudioSink for RecordingProcessedAudioSink {
    fn publish(&self, frame: ProcessedAudioFrame) {
        self.frames
            .entry(frame.room_id.clone())
            .or_default()
            .push(frame);
    }
}

pub struct WebMediaRuntime {
    runtime: MediaRuntime<NoiseCancellingAudioFrameProcessor, RecordingProcessedAudioSink>,
    sink: RecordingProcessedAudioSink,
}

impl WebMediaRuntime {
    pub fn new(relays: Arc<MediaRelayRegistry>) -> Self {
        let sink = RecordingProcessedAudioSink::default();
        let runtime = MediaRuntime::new(
            relays,
            NoiseCancellingAudioFrameProcessor::default(),
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
}

impl fmt::Debug for WebMediaRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WebMediaRuntime")
            .finish_non_exhaustive()
    }
}
