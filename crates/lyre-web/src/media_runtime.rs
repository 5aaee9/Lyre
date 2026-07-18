use dashmap::DashMap;
use lyre_core::{
    AudioFrame, MediaRelayError, MediaRelayRegistry, MediaRuntime, NoiseProvider,
    ProcessedAudioFrame, ProcessedAudioSink, RoomId,
};
use lyre_noise_cancelling::{
    DeepFilterNetRuntimeConfig, NoiseCancellingAudioFrameProcessor, NoiseModelRuntimeConfig,
};
use std::{fmt, sync::Arc};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Default)]
pub struct ProcessedAudioBroadcaster {
    channels: Arc<DashMap<RoomId, broadcast::Sender<ProcessedAudioFrame>>>,
}

const PROCESSED_AUDIO_CHANNEL_CAPACITY: usize = 256;

impl ProcessedAudioBroadcaster {
    pub fn subscribe(&self, room_id: &RoomId) -> broadcast::Receiver<ProcessedAudioFrame> {
        self.sender(room_id).subscribe()
    }

    pub fn clear_room(&self, room_id: &RoomId) {
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
        let _ = self.sender(&frame.room_id).send(frame);
    }
}

pub struct WebMediaRuntime {
    relays: Arc<MediaRelayRegistry>,
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
            Arc::clone(&relays),
            NoiseCancellingAudioFrameProcessor::with_model_runtime(model_runtime),
            sink.clone(),
        );
        Self {
            relays,
            runtime,
            sink,
        }
    }

    pub fn process_frame(&self, frame: AudioFrame) -> Result<(), MediaRelayError> {
        self.runtime.process_frame(frame)
    }

    pub fn process_frames<I>(&self, frames: I) -> Result<usize, MediaRelayError>
    where
        I: IntoIterator<Item = AudioFrame>,
    {
        self.runtime.process_frames(frames)
    }

    pub fn subscribe(&self, room_id: &RoomId) -> broadcast::Receiver<ProcessedAudioFrame> {
        self.sink.subscribe(room_id)
    }

    pub fn release_source(&self, room_id: &RoomId, user_id: &lyre_core::UserId) {
        self.runtime.release_source(room_id, user_id);
    }

    pub fn clear_room(&self, room_id: &RoomId) {
        self.runtime.clear_room(room_id);
        self.sink.clear_room(room_id);
    }

    pub fn uses_processed_audio(&self, room_id: &RoomId) -> bool {
        self.relays.status(room_id.clone()).noise.provider != NoiseProvider::Off
    }
}

impl fmt::Debug for WebMediaRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WebMediaRuntime")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyre_core::{
        MediaTrackKind, NoiseCancellationConfig, RegisterMediaTrackRequest, StartMediaRelayRequest,
        UserId,
    };
    use lyre_noise_cancelling::{RNNOISE_CHANNELS, RNNOISE_FRAME_SIZE, RNNOISE_SAMPLE_RATE_HZ};

    fn setup_runtime() -> (WebMediaRuntime, RoomId, UserId) {
        let relays = Arc::new(MediaRelayRegistry::new());
        let room_id = RoomId::default_room();
        let user_id = UserId::from_external("user_01");
        relays.start(
            room_id.clone(),
            StartMediaRelayRequest {
                noise: Some(NoiseCancellationConfig {
                    provider: NoiseProvider::Rnnoise,
                    ..NoiseCancellationConfig::default()
                }),
            },
        );
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
        (WebMediaRuntime::new(relays), room_id, user_id)
    }

    fn frame(room_id: &RoomId, user_id: &UserId) -> AudioFrame {
        AudioFrame {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
            track_id: "audio-main".to_owned(),
            sample_rate_hz: RNNOISE_SAMPLE_RATE_HZ,
            channels: RNNOISE_CHANNELS,
            sequence: 1,
            rtp_timestamp: None,
            samples: (0..RNNOISE_FRAME_SIZE)
                .map(|index| ((index as f32) / 24.0).sin() * 0.1)
                .collect(),
        }
    }

    #[tokio::test]
    async fn releasing_a_source_resets_its_noise_processor_state() {
        let (runtime, room_id, user_id) = setup_runtime();
        let mut output = runtime.subscribe(&room_id);
        let frame = frame(&room_id, &user_id);

        runtime.process_frame(frame.clone()).unwrap();
        let first = output.recv().await.unwrap();
        runtime.release_source(&room_id, &user_id);
        runtime.process_frame(frame).unwrap();
        let after_release = output.recv().await.unwrap();

        assert_eq!(after_release.samples, first.samples);
    }

    #[tokio::test]
    async fn clearing_a_room_resets_its_noise_processor_state() {
        let (runtime, room_id, user_id) = setup_runtime();
        let mut output = runtime.subscribe(&room_id);
        let frame = frame(&room_id, &user_id);

        runtime.process_frame(frame.clone()).unwrap();
        let first = output.recv().await.unwrap();
        runtime.clear_room(&room_id);
        let mut output = runtime.subscribe(&room_id);
        runtime.process_frame(frame).unwrap();
        let after_clear = output.recv().await.unwrap();

        assert_eq!(after_clear.samples, first.samples);
    }
}
