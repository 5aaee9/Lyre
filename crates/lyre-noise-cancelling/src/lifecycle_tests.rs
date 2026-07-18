use lyre_core::{
    AudioFrame, AudioFrameProcessor, NoiseCancellationConfig, NoiseProvider, RoomId, UserId,
};

use crate::{
    NoiseCancellingAudioFrameProcessor, RNNOISE_CHANNELS, RNNOISE_FRAME_SIZE,
    RNNOISE_SAMPLE_RATE_HZ,
};

fn frame() -> AudioFrame {
    AudioFrame {
        room_id: RoomId::default_room(),
        user_id: UserId::from_external("user_01"),
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

fn rnnoise_config(intensity: f32) -> NoiseCancellationConfig {
    NoiseCancellationConfig {
        provider: NoiseProvider::Rnnoise,
        intensity,
        ..NoiseCancellationConfig::default()
    }
}

#[test]
fn changing_source_config_replaces_the_previous_processor_state() {
    let processor = NoiseCancellingAudioFrameProcessor::default();
    let reference = NoiseCancellingAudioFrameProcessor::default();
    let frame = frame();
    let original = rnnoise_config(0.5);

    processor.process(&frame, &original);
    processor.process(&frame, &rnnoise_config(0.6));
    let after_switching_back = processor.process(&frame, &original);
    let fresh = reference.process(&frame, &original);

    assert_eq!(after_switching_back, fresh);
}

#[test]
fn releasing_a_source_discards_its_processor_state() {
    let processor = NoiseCancellingAudioFrameProcessor::default();
    let reference = NoiseCancellingAudioFrameProcessor::default();
    let frame = frame();
    let config = rnnoise_config(0.5);

    processor.process(&frame, &config);
    processor.release_source(&frame.room_id, &frame.user_id);

    assert_eq!(
        processor.process(&frame, &config),
        reference.process(&frame, &config)
    );
}

#[test]
fn clearing_a_room_discards_only_that_rooms_processor_state() {
    let processor = NoiseCancellingAudioFrameProcessor::default();
    let fresh_room_a = NoiseCancellingAudioFrameProcessor::default();
    let room_b_reference = NoiseCancellingAudioFrameProcessor::default();
    let room_a_frame = frame();
    let room_b_frame = AudioFrame {
        room_id: RoomId::parse_boundary("ROOM_B").unwrap(),
        ..room_a_frame.clone()
    };
    let config = rnnoise_config(0.5);

    processor.process(&room_a_frame, &config);
    processor.process(&room_b_frame, &config);
    room_b_reference.process(&room_b_frame, &config);
    processor.clear_room(&room_a_frame.room_id);

    assert_eq!(
        processor.process(&room_a_frame, &config),
        fresh_room_a.process(&room_a_frame, &config)
    );
    assert_eq!(
        processor.process(&room_b_frame, &config),
        room_b_reference.process(&room_b_frame, &config)
    );
}
