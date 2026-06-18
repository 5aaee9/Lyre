use lyre_noise_cancelling::dsp::deepfilternet::{
    DeepFilterNetDsp, DEEPFILTERNET_DSP_CHANNELS, DEEPFILTERNET_DSP_DF_BINS,
    DEEPFILTERNET_DSP_DF_ORDER, DEEPFILTERNET_DSP_ERB_BANDS, DEEPFILTERNET_DSP_FRAME_SIZE,
    DEEPFILTERNET_DSP_SAMPLE_RATE_HZ,
};
use std::slice;

#[no_mangle]
pub extern "C" fn lyre_noise_wasm_deepfilternet_new() -> *mut DeepFilterNetDsp {
    Box::into_raw(Box::new(DeepFilterNetDsp::new()))
}

#[no_mangle]
/// # Safety
///
/// `processor` must be a pointer returned by `lyre_noise_wasm_deepfilternet_new` and must not be
/// used after this function returns.
pub unsafe extern "C" fn lyre_noise_wasm_deepfilternet_free(processor: *mut DeepFilterNetDsp) {
    if !processor.is_null() {
        drop(Box::from_raw(processor));
    }
}

#[no_mangle]
pub extern "C" fn lyre_noise_wasm_deepfilternet_sample_rate_hz() -> u32 {
    DEEPFILTERNET_DSP_SAMPLE_RATE_HZ
}

#[no_mangle]
pub extern "C" fn lyre_noise_wasm_deepfilternet_channels() -> u32 {
    DEEPFILTERNET_DSP_CHANNELS
}

#[no_mangle]
pub extern "C" fn lyre_noise_wasm_deepfilternet_frame_size() -> usize {
    DEEPFILTERNET_DSP_FRAME_SIZE
}

#[no_mangle]
pub extern "C" fn lyre_noise_wasm_deepfilternet_erb_bands() -> usize {
    DEEPFILTERNET_DSP_ERB_BANDS
}

#[no_mangle]
pub extern "C" fn lyre_noise_wasm_deepfilternet_df_bins() -> usize {
    DEEPFILTERNET_DSP_DF_BINS
}

#[no_mangle]
pub extern "C" fn lyre_noise_wasm_deepfilternet_df_order() -> usize {
    DEEPFILTERNET_DSP_DF_ORDER
}

#[no_mangle]
/// # Safety
///
/// `processor` must be a live pointer returned by `lyre_noise_wasm_deepfilternet_new`.
/// All pointers must reference contiguous f32 arrays sized by their corresponding len arguments.
pub unsafe extern "C" fn lyre_noise_wasm_deepfilternet_features(
    processor: *mut DeepFilterNetDsp,
    input_ptr: *const f32,
    input_len: usize,
    feat_erb_ptr: *mut f32,
    feat_erb_len: usize,
    feat_spec_ptr: *mut f32,
    feat_spec_len: usize,
) -> bool {
    if processor.is_null()
        || input_ptr.is_null()
        || feat_erb_ptr.is_null()
        || feat_spec_ptr.is_null()
    {
        return false;
    }
    let input = slice::from_raw_parts(input_ptr, input_len);
    let feat_erb = slice::from_raw_parts_mut(feat_erb_ptr, feat_erb_len);
    let feat_spec = slice::from_raw_parts_mut(feat_spec_ptr, feat_spec_len);
    (*processor).extract_features_into(input, feat_erb, feat_spec)
}

#[no_mangle]
/// # Safety
///
/// `processor` must be a live pointer returned by `lyre_noise_wasm_deepfilternet_new`.
/// All pointers must reference contiguous f32 arrays sized by their corresponding len arguments.
pub unsafe extern "C" fn lyre_noise_wasm_deepfilternet_synthesize(
    processor: *mut DeepFilterNetDsp,
    mask_ptr: *const f32,
    mask_len: usize,
    coefs_ptr: *const f32,
    coefs_len: usize,
    output_ptr: *mut f32,
    output_len: usize,
) -> bool {
    if processor.is_null() || mask_ptr.is_null() || coefs_ptr.is_null() || output_ptr.is_null() {
        return false;
    }
    let mask = slice::from_raw_parts(mask_ptr, mask_len);
    let coefs = slice::from_raw_parts(coefs_ptr, coefs_len);
    let output = slice::from_raw_parts_mut(output_ptr, output_len);
    (*processor).synthesize(mask, coefs, output)
}
