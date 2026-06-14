use crate::{
    MediaRelayError, MediaRelayRegistry, MediaTrackKind, NoiseCancellationConfig, RoomId, UserId,
};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub struct AudioFrame {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub track_id: String,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub sequence: u64,
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessedAudioFrame {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub track_id: String,
    pub sample_rate_hz: u32,
    pub channels: u16,
    pub sequence: u64,
    pub samples: Vec<f32>,
    pub noise: NoiseCancellationConfig,
}

pub trait AudioFrameProcessor: Send + Sync + 'static {
    fn process(&self, frame: &AudioFrame, noise: &NoiseCancellationConfig) -> Vec<f32>;
}

#[derive(Debug, Default)]
pub struct PassthroughAudioFrameProcessor;

impl AudioFrameProcessor for PassthroughAudioFrameProcessor {
    fn process(&self, frame: &AudioFrame, _noise: &NoiseCancellationConfig) -> Vec<f32> {
        frame.samples.clone()
    }
}

pub trait ProcessedAudioSink: Send + Sync + 'static {
    fn publish(&self, frame: ProcessedAudioFrame);
}

#[derive(Debug)]
pub struct MediaRuntime<P, S> {
    relays: Arc<MediaRelayRegistry>,
    processor: P,
    sink: S,
}

impl<P, S> MediaRuntime<P, S>
where
    P: AudioFrameProcessor,
    S: ProcessedAudioSink,
{
    pub fn new(relays: Arc<MediaRelayRegistry>, processor: P, sink: S) -> Self {
        Self {
            relays,
            processor,
            sink,
        }
    }

    pub fn process_frame(&self, frame: AudioFrame) -> Result<(), MediaRelayError> {
        let lookup = self
            .relays
            .require_track(&frame.room_id, &frame.user_id, &frame.track_id)?;
        if lookup.kind != MediaTrackKind::Audio {
            return Err(MediaRelayError::UnsupportedTrackKind {
                room_id: frame.room_id,
                user_id: frame.user_id,
                track_id: frame.track_id,
                kind: lookup.kind,
            });
        }
        let samples = self.processor.process(&frame, &lookup.noise);
        self.sink.publish(ProcessedAudioFrame {
            room_id: frame.room_id,
            user_id: frame.user_id,
            track_id: frame.track_id,
            sample_rate_hz: frame.sample_rate_hz,
            channels: frame.channels,
            sequence: frame.sequence,
            samples,
            noise: lookup.noise,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MediaRelayRegistry, NoiseProvider, RegisterMediaTrackRequest, StartMediaRelayRequest,
    };
    use std::sync::Mutex;

    #[derive(Debug, Clone)]
    struct RecordingSink {
        frames: Arc<Mutex<Vec<ProcessedAudioFrame>>>,
    }

    impl RecordingSink {
        fn new() -> Self {
            Self {
                frames: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl ProcessedAudioSink for RecordingSink {
        fn publish(&self, frame: ProcessedAudioFrame) {
            self.frames.lock().unwrap().push(frame);
        }
    }

    #[derive(Debug, Default)]
    struct RecordingProcessor {
        noise: Arc<Mutex<Vec<NoiseCancellationConfig>>>,
    }

    impl AudioFrameProcessor for RecordingProcessor {
        fn process(&self, frame: &AudioFrame, noise: &NoiseCancellationConfig) -> Vec<f32> {
            self.noise.lock().unwrap().push(noise.clone());
            frame.samples.iter().map(|sample| sample * 2.0).collect()
        }
    }

    fn frame(room_id: RoomId, user_id: UserId, track_id: impl Into<String>) -> AudioFrame {
        AudioFrame {
            room_id,
            user_id,
            track_id: track_id.into(),
            sample_rate_hz: 48_000,
            channels: 1,
            sequence: 7,
            samples: vec![0.1, -0.2, 0.3],
        }
    }

    fn active_relays(kind: MediaTrackKind) -> (Arc<MediaRelayRegistry>, RoomId, UserId) {
        let relays = Arc::new(MediaRelayRegistry::new());
        let room_id = RoomId::default_room();
        let user_id = UserId::from_external("user_01");
        relays.start(
            room_id.clone(),
            StartMediaRelayRequest {
                noise: Some(NoiseCancellationConfig {
                    provider: NoiseProvider::Rnnoise,
                    intensity: 0.8,
                    voice_activity_threshold: 0.2,
                }),
            },
        );
        relays
            .register_track(
                room_id.clone(),
                RegisterMediaTrackRequest {
                    user_id: user_id.clone(),
                    track_id: "audio-main".to_owned(),
                    kind,
                },
            )
            .unwrap();
        (relays, room_id, user_id)
    }

    #[test]
    fn inactive_relay_rejects_frame_without_creating_room() {
        let relays = Arc::new(MediaRelayRegistry::new());
        let room_id = RoomId::parse_boundary("UNKNOWN").unwrap();
        let runtime = MediaRuntime::new(
            Arc::clone(&relays),
            PassthroughAudioFrameProcessor,
            RecordingSink::new(),
        );

        assert_eq!(
            runtime.process_frame(frame(
                room_id.clone(),
                UserId::from_external("user_01"),
                "audio-main",
            )),
            Err(MediaRelayError::Inactive {
                room_id: room_id.clone(),
            })
        );
        assert!(!relays.contains_room(&room_id));
    }

    #[test]
    fn active_relay_rejects_unknown_participant() {
        let relays = Arc::new(MediaRelayRegistry::new());
        let room_id = RoomId::default_room();
        relays.start(room_id.clone(), StartMediaRelayRequest::default());
        let runtime = MediaRuntime::new(
            Arc::clone(&relays),
            PassthroughAudioFrameProcessor,
            RecordingSink::new(),
        );
        let user_id = UserId::from_external("user_01");

        assert_eq!(
            runtime.process_frame(frame(room_id.clone(), user_id.clone(), "audio-main")),
            Err(MediaRelayError::ParticipantNotFound { room_id, user_id })
        );
    }

    #[test]
    fn active_relay_rejects_unknown_track() {
        let (relays, room_id, user_id) = active_relays(MediaTrackKind::Audio);
        let runtime = MediaRuntime::new(
            Arc::clone(&relays),
            PassthroughAudioFrameProcessor,
            RecordingSink::new(),
        );

        assert_eq!(
            runtime.process_frame(frame(room_id.clone(), user_id.clone(), "missing-track")),
            Err(MediaRelayError::TrackNotFound {
                room_id,
                user_id,
                track_id: "missing-track".to_owned(),
            })
        );
    }

    #[test]
    fn active_relay_rejects_video_track_for_audio_runtime() {
        let (relays, room_id, user_id) = active_relays(MediaTrackKind::Video);
        let runtime = MediaRuntime::new(
            Arc::clone(&relays),
            PassthroughAudioFrameProcessor,
            RecordingSink::new(),
        );

        assert_eq!(
            runtime.process_frame(frame(room_id.clone(), user_id.clone(), "audio-main")),
            Err(MediaRelayError::UnsupportedTrackKind {
                room_id,
                user_id,
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Video,
            })
        );
    }

    #[test]
    fn processes_registered_audio_track_and_publishes_frame() {
        let (relays, room_id, user_id) = active_relays(MediaTrackKind::Audio);
        let processor = RecordingProcessor::default();
        let processor_noise = Arc::clone(&processor.noise);
        let sink = RecordingSink::new();
        let sink_frames = Arc::clone(&sink.frames);
        let runtime = MediaRuntime::new(Arc::clone(&relays), processor, sink);

        runtime
            .process_frame(frame(room_id.clone(), user_id.clone(), "audio-main"))
            .unwrap();

        let noise = processor_noise.lock().unwrap().clone();
        assert_eq!(noise.len(), 1);
        assert_eq!(noise[0].provider, NoiseProvider::Rnnoise);
        let frames = sink_frames.lock().unwrap().clone();
        assert_eq!(frames.len(), 1);
        assert_eq!(
            frames[0],
            ProcessedAudioFrame {
                room_id,
                user_id,
                track_id: "audio-main".to_owned(),
                sample_rate_hz: 48_000,
                channels: 1,
                sequence: 7,
                samples: vec![0.2, -0.4, 0.6],
                noise: NoiseCancellationConfig {
                    provider: NoiseProvider::Rnnoise,
                    intensity: 0.8,
                    voice_activity_threshold: 0.2,
                },
            }
        );
    }
}
