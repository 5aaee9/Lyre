use crate::{
    ServerMediaConcealmentRequired, ServerMediaPcmFrame, SERVER_MEDIA_OPUS_CHANNELS,
    SERVER_MEDIA_OPUS_FRAME_SIZE, SERVER_MEDIA_OPUS_SAMPLE_RATE_HZ,
};

#[derive(Debug, Default)]
pub struct ServerMediaPcmConcealer {
    last_frame: Option<ServerMediaPcmFrame>,
}

impl ServerMediaPcmConcealer {
    pub fn observe_decoded(&mut self, frame: ServerMediaPcmFrame) {
        if is_usable_seed(&frame) {
            self.last_frame = Some(frame);
        }
    }

    pub fn conceal(
        &mut self,
        event: &ServerMediaConcealmentRequired,
    ) -> Option<ServerMediaPcmFrame> {
        let previous = self.last_frame.as_ref()?;
        let samples = synthesize_samples(&previous.samples);
        let frame = ServerMediaPcmFrame {
            track_id: event.track_id.clone(),
            sequence_number: event.sequence_number,
            rtp_timestamp: event.rtp_timestamp,
            sample_rate_hz: previous.sample_rate_hz,
            channels: previous.channels,
            samples,
        };
        self.last_frame = Some(frame.clone());
        Some(frame)
    }
}

fn is_usable_seed(frame: &ServerMediaPcmFrame) -> bool {
    frame.sample_rate_hz == SERVER_MEDIA_OPUS_SAMPLE_RATE_HZ
        && frame.channels == SERVER_MEDIA_OPUS_CHANNELS
        && !frame.samples.is_empty()
}

fn synthesize_samples(previous: &[f32]) -> Vec<f32> {
    let start = previous.len().saturating_sub(SERVER_MEDIA_OPUS_FRAME_SIZE);
    let seed = previous[start..].iter().rev().copied().collect::<Vec<_>>();

    (0..SERVER_MEDIA_OPUS_FRAME_SIZE)
        .map(|index| {
            let sample = seed[index % seed.len()];
            let fade = 0.60
                * (1.0 - index as f32 / (SERVER_MEDIA_OPUS_FRAME_SIZE.saturating_sub(1)) as f32);
            (sample * fade).clamp(-1.0, 1.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(sequence_number: u16, rtp_timestamp: u32) -> ServerMediaConcealmentRequired {
        ServerMediaConcealmentRequired {
            track_id: "audio-main".to_owned(),
            sequence_number,
            rtp_timestamp,
        }
    }

    fn frame(sequence_number: u16, rtp_timestamp: u32, samples: Vec<f32>) -> ServerMediaPcmFrame {
        ServerMediaPcmFrame {
            track_id: "audio-main".to_owned(),
            sequence_number,
            rtp_timestamp,
            sample_rate_hz: SERVER_MEDIA_OPUS_SAMPLE_RATE_HZ,
            channels: SERVER_MEDIA_OPUS_CHANNELS,
            samples,
        }
    }

    #[test]
    fn returns_none_without_prior_frame() {
        let mut concealer = ServerMediaPcmConcealer::default();

        assert_eq!(concealer.conceal(&event(8, 7_680)), None);
    }

    #[test]
    fn synthesizes_metadata_and_960_samples_from_prior_frame() {
        let mut concealer = ServerMediaPcmConcealer::default();
        let samples = (0..SERVER_MEDIA_OPUS_FRAME_SIZE)
            .map(|index| index as f32 / SERVER_MEDIA_OPUS_FRAME_SIZE as f32)
            .collect::<Vec<_>>();
        concealer.observe_decoded(frame(7, 6_720, samples.clone()));

        let concealed = concealer.conceal(&event(8, 7_680)).unwrap();

        assert_eq!(concealed.track_id, "audio-main");
        assert_eq!(concealed.sequence_number, 8);
        assert_eq!(concealed.rtp_timestamp, 7_680);
        assert_eq!(concealed.sample_rate_hz, SERVER_MEDIA_OPUS_SAMPLE_RATE_HZ);
        assert_eq!(concealed.channels, SERVER_MEDIA_OPUS_CHANNELS);
        assert_eq!(concealed.samples.len(), SERVER_MEDIA_OPUS_FRAME_SIZE);
        assert!(concealed.samples[0] > concealed.samples[SERVER_MEDIA_OPUS_FRAME_SIZE - 1]);
        assert_eq!(concealed.samples[SERVER_MEDIA_OPUS_FRAME_SIZE - 1], 0.0);
        assert!(concealed
            .samples
            .iter()
            .all(|sample| (-1.0..=1.0).contains(sample)));
    }

    #[test]
    fn uses_synthetic_frame_as_seed_for_followup_loss() {
        let mut concealer = ServerMediaPcmConcealer::default();
        concealer.observe_decoded(frame(40, 40_000, vec![0.5; SERVER_MEDIA_OPUS_FRAME_SIZE]));

        let first = concealer.conceal(&event(41, 40_960)).unwrap();
        let second = concealer.conceal(&event(42, 41_920)).unwrap();

        assert_eq!(first.sequence_number, 41);
        assert_eq!(second.sequence_number, 42);
        assert_eq!(second.rtp_timestamp, 41_920);
        assert!(second.samples[0].abs() <= first.samples[0].abs());
    }

    #[test]
    fn repeats_short_prior_frame_to_fill_opus_frame() {
        let mut concealer = ServerMediaPcmConcealer::default();
        concealer.observe_decoded(frame(1, 960, vec![0.25, -0.25]));

        let concealed = concealer.conceal(&event(2, 1_920)).unwrap();

        assert_eq!(concealed.samples.len(), SERVER_MEDIA_OPUS_FRAME_SIZE);
        assert!(concealed.samples.iter().any(|sample| *sample > 0.0));
        assert!(concealed.samples.iter().any(|sample| *sample < 0.0));
    }

    #[test]
    fn invalid_prior_shape_returns_none_without_synthetic_state() {
        let mut concealer = ServerMediaPcmConcealer::default();
        let mut invalid = frame(1, 960, Vec::new());
        invalid.channels = 2;
        concealer.observe_decoded(invalid);

        assert_eq!(concealer.conceal(&event(2, 1_920)), None);

        concealer.observe_decoded(frame(3, 2_880, vec![0.25; SERVER_MEDIA_OPUS_FRAME_SIZE]));
        let concealed = concealer.conceal(&event(4, 3_840)).unwrap();
        assert_eq!(concealed.sequence_number, 4);
        assert_eq!(concealed.samples.len(), SERVER_MEDIA_OPUS_FRAME_SIZE);
    }

    #[test]
    fn invalid_observation_does_not_overwrite_last_valid_frame() {
        let mut concealer = ServerMediaPcmConcealer::default();
        concealer.observe_decoded(frame(1, 960, vec![0.5; SERVER_MEDIA_OPUS_FRAME_SIZE]));
        let mut invalid = frame(2, 1_920, Vec::new());
        invalid.channels = 2;
        concealer.observe_decoded(invalid);

        let concealed = concealer.conceal(&event(3, 2_880)).unwrap();

        assert_eq!(concealed.sequence_number, 3);
        assert_eq!(concealed.samples.len(), SERVER_MEDIA_OPUS_FRAME_SIZE);
        assert!(concealed.samples.iter().any(|sample| sample.abs() > 0.0));
    }
}
