use nnnoiseless::DenoiseState;

pub const RNNOISE_SAMPLE_RATE_HZ: u32 = 48_000;
pub const RNNOISE_CHANNELS: u16 = 1;
pub const RNNOISE_FRAME_SIZE: usize = DenoiseState::FRAME_SIZE;
pub const PCM_F32_TO_I16_SCALE: f32 = i16::MAX as f32;

#[derive(Debug, Clone, PartialEq)]
pub struct RnnoiseFrameOutput {
    pub samples: Vec<f32>,
    pub voice_activity_probability: f32,
}

pub struct RnnoiseFrameProcessor {
    state: Box<DenoiseState<'static>>,
}

impl RnnoiseFrameProcessor {
    pub fn new() -> Self {
        Self {
            state: DenoiseState::new(),
        }
    }

    pub fn process_samples(&mut self, samples: &[f32]) -> RnnoiseFrameOutput {
        debug_assert!(!samples.is_empty());
        debug_assert!(samples.len().is_multiple_of(RNNOISE_FRAME_SIZE));

        let mut output_samples = Vec::with_capacity(samples.len());
        let mut vad_total = 0.0;
        let mut chunks = 0;

        for chunk in samples.chunks_exact(RNNOISE_FRAME_SIZE) {
            let mut output = vec![0.0; RNNOISE_FRAME_SIZE];
            let scaled_input = chunk
                .iter()
                .map(|sample| sample * PCM_F32_TO_I16_SCALE)
                .collect::<Vec<_>>();
            vad_total += self.state.process_frame(&mut output, &scaled_input);
            output_samples.extend(
                output
                    .into_iter()
                    .map(|sample| sample / PCM_F32_TO_I16_SCALE),
            );
            chunks += 1;
        }

        RnnoiseFrameOutput {
            samples: output_samples,
            voice_activity_probability: vad_total / chunks as f32,
        }
    }
}

impl Default for RnnoiseFrameProcessor {
    fn default() -> Self {
        Self::new()
    }
}
