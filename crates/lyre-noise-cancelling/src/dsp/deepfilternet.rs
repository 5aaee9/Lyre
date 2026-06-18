use df::{band_compr, band_mean_norm_erb, band_unit_norm, Complex32, DFState};

pub const DEEPFILTERNET_DSP_SAMPLE_RATE_HZ: u32 = 48_000;
pub const DEEPFILTERNET_DSP_CHANNELS: u32 = 1;
pub const DEEPFILTERNET_DSP_FRAME_SIZE: usize = 480;
pub const DEEPFILTERNET_DSP_FFT_SIZE: usize = 960;
pub const DEEPFILTERNET_DSP_ERB_BANDS: usize = 32;
pub const DEEPFILTERNET_DSP_MIN_ERB_FREQS: usize = 2;
pub const DEEPFILTERNET_DSP_DF_ORDER: usize = 5;
pub const DEEPFILTERNET_DSP_DF_BINS: usize = 96;

const ALPHA: f32 = 0.99;

pub struct DeepFilterNetFeatures {
    pub feat_erb: Vec<f32>,
    pub feat_spec: Vec<f32>,
}

pub struct DeepFilterNetDsp {
    state: DFState,
    erb_norm_state: Vec<f32>,
    spec_norm_state: Vec<f32>,
    spec_buffer: Vec<Vec<Complex32>>,
    current_spec: Vec<Complex32>,
}

impl DeepFilterNetDsp {
    pub fn new() -> Self {
        let state = DFState::new(
            DEEPFILTERNET_DSP_SAMPLE_RATE_HZ as usize,
            DEEPFILTERNET_DSP_FFT_SIZE,
            DEEPFILTERNET_DSP_FRAME_SIZE,
            DEEPFILTERNET_DSP_ERB_BANDS,
            DEEPFILTERNET_DSP_MIN_ERB_FREQS,
        );
        let freq_size = state.freq_size;
        Self {
            state,
            erb_norm_state: vec![df::MEAN_NORM_INIT[0]; DEEPFILTERNET_DSP_ERB_BANDS],
            spec_norm_state: vec![df::UNIT_NORM_INIT[0]; freq_size],
            spec_buffer: vec![vec![Complex32::ZERO; freq_size]; DEEPFILTERNET_DSP_DF_ORDER],
            current_spec: vec![Complex32::ZERO; freq_size],
        }
    }

    pub fn extract_features(&mut self, input: &[f32]) -> Option<DeepFilterNetFeatures> {
        let mut feat_erb = vec![0.0; DEEPFILTERNET_DSP_ERB_BANDS];
        let mut feat_spec = vec![0.0; DEEPFILTERNET_DSP_DF_BINS * 2];
        self.extract_features_into(input, &mut feat_erb, &mut feat_spec)
            .then_some(DeepFilterNetFeatures {
                feat_erb,
                feat_spec,
            })
    }

    pub fn extract_features_into(
        &mut self,
        input: &[f32],
        feat_erb: &mut [f32],
        feat_spec: &mut [f32],
    ) -> bool {
        if input.len() != DEEPFILTERNET_DSP_FRAME_SIZE
            || feat_erb.len() != DEEPFILTERNET_DSP_ERB_BANDS
            || feat_spec.len() != DEEPFILTERNET_DSP_DF_BINS * 2
        {
            return false;
        }
        self.state.analysis(input, &mut self.current_spec);
        self.spec_buffer.rotate_right(1);
        self.spec_buffer[0].copy_from_slice(&self.current_spec);

        let spec_power = self
            .current_spec
            .iter()
            .map(|bin| bin.re.mul_add(bin.re, bin.im * bin.im))
            .collect::<Vec<_>>();
        band_compr(feat_erb, &spec_power, &self.state.erb);
        band_mean_norm_erb(feat_erb, &mut self.erb_norm_state, ALPHA);

        let mut normalized_spec = self.current_spec.clone();
        band_unit_norm(&mut normalized_spec, &mut self.spec_norm_state, ALPHA);
        complex_prefix_to_interleaved(&normalized_spec, DEEPFILTERNET_DSP_DF_BINS, feat_spec);
        true
    }

    pub fn synthesize(&mut self, mask: &[f32], coefs: &[f32], output: &mut [f32]) -> bool {
        if mask.len() < DEEPFILTERNET_DSP_ERB_BANDS
            || coefs.len() < DEEPFILTERNET_DSP_DF_BINS * DEEPFILTERNET_DSP_DF_ORDER * 2
            || output.len() != DEEPFILTERNET_DSP_FRAME_SIZE
        {
            return false;
        }
        apply_erb_mask(&mut self.current_spec, mask, &self.state.erb);
        apply_deep_filter(&mut self.current_spec, &self.spec_buffer, coefs);
        self.state.synthesis(&mut self.current_spec, output);
        true
    }
}

impl Default for DeepFilterNetDsp {
    fn default() -> Self {
        Self::new()
    }
}

fn complex_prefix_to_interleaved(spec: &[Complex32], bins: usize, output: &mut [f32]) {
    for (index, bin) in spec.iter().take(bins).enumerate() {
        output[index] = bin.re;
        output[index + bins] = bin.im;
    }
}

fn apply_erb_mask(spec: &mut [Complex32], mask: &[f32], erb: &[usize]) {
    let mut offset = 0;
    for (band_size, gain) in erb.iter().zip(mask.iter()) {
        for bin in &mut spec[offset..offset + band_size] {
            *bin *= *gain;
        }
        offset += band_size;
    }
}

fn apply_deep_filter(spec: &mut [Complex32], history: &[Vec<Complex32>], coefs: &[f32]) {
    let max_bins = DEEPFILTERNET_DSP_DF_BINS.min(spec.len());
    if coefs.len() < max_bins * DEEPFILTERNET_DSP_DF_ORDER * 2 {
        return;
    }

    for bin in 0..max_bins {
        let mut enhanced = Complex32::ZERO;
        for (order, frame) in history.iter().enumerate().take(DEEPFILTERNET_DSP_DF_ORDER) {
            let coef_index = (bin * DEEPFILTERNET_DSP_DF_ORDER + order) * 2;
            let coef = Complex32::new(coefs[coef_index], coefs[coef_index + 1]);
            enhanced += frame[bin] * coef;
        }
        spec[bin] = enhanced;
    }
}
