use lyre_noise_cancelling::dsp::dpdfnet::{
    DpdfNetDsp, DPDFNET_DSP_CHANNELS, DPDFNET_DSP_SAMPLE_RATE_HZ,
};
use std::{ptr, slice};

#[no_mangle]
pub extern "C" fn lyre_noise_wasm_dpdfnet_new(
    window_len: usize,
    hop_size: usize,
) -> *mut DpdfNetDsp {
    DpdfNetDsp::new(window_len, hop_size)
        .map(Box::new)
        .map(Box::into_raw)
        .unwrap_or(ptr::null_mut())
}

#[no_mangle]
/// # Safety
///
/// `processor` must be a pointer returned by `lyre_noise_wasm_dpdfnet_new` and must not be used
/// after this function returns.
pub unsafe extern "C" fn lyre_noise_wasm_dpdfnet_free(processor: *mut DpdfNetDsp) {
    if !processor.is_null() {
        drop(Box::from_raw(processor));
    }
}

#[no_mangle]
pub extern "C" fn lyre_noise_wasm_dpdfnet_sample_rate_hz() -> u32 {
    DPDFNET_DSP_SAMPLE_RATE_HZ
}

#[no_mangle]
pub extern "C" fn lyre_noise_wasm_dpdfnet_channels() -> u32 {
    DPDFNET_DSP_CHANNELS
}

#[no_mangle]
/// # Safety
///
/// `processor` must be a live pointer returned by `lyre_noise_wasm_dpdfnet_new`.
/// `input_ptr` must reference `input_len` contiguous f32 values.
/// `spec_ptr` must reference `spec_len` contiguous f32 values.
pub unsafe extern "C" fn lyre_noise_wasm_dpdfnet_stft(
    processor: *mut DpdfNetDsp,
    input_ptr: *const f32,
    input_len: usize,
    spec_ptr: *mut f32,
    spec_len: usize,
) -> bool {
    if processor.is_null() || input_ptr.is_null() || spec_ptr.is_null() {
        return false;
    }
    let input = slice::from_raw_parts(input_ptr, input_len);
    let spec = slice::from_raw_parts_mut(spec_ptr, spec_len);
    (*processor).process_stft(input, spec)
}

#[no_mangle]
/// # Safety
///
/// `processor` must be a live pointer returned by `lyre_noise_wasm_dpdfnet_new`.
/// `spec_ptr` must reference `spec_len` contiguous f32 values.
/// `output_ptr` must reference `output_len` contiguous f32 values.
pub unsafe extern "C" fn lyre_noise_wasm_dpdfnet_istft(
    processor: *mut DpdfNetDsp,
    spec_ptr: *const f32,
    spec_len: usize,
    output_ptr: *mut f32,
    output_len: usize,
) -> bool {
    if processor.is_null() || spec_ptr.is_null() || output_ptr.is_null() {
        return false;
    }
    let spec = slice::from_raw_parts(spec_ptr, spec_len);
    let output = slice::from_raw_parts_mut(output_ptr, output_len);
    (*processor).process_istft(spec, output)
}
