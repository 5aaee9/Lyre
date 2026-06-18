use lyre_noise_cancelling::{
    RnnoiseFrameProcessor, RNNOISE_CHANNELS, RNNOISE_FRAME_SIZE, RNNOISE_SAMPLE_RATE_HZ,
};
use std::{alloc, ptr, slice};

#[no_mangle]
pub extern "C" fn lyre_noise_wasm_rnnoise_new() -> *mut RnnoiseFrameProcessor {
    Box::into_raw(Box::new(RnnoiseFrameProcessor::new()))
}

#[no_mangle]
/// # Safety
///
/// `processor` must be a pointer returned by `lyre_noise_wasm_rnnoise_new` and must not be used
/// after this function returns.
pub unsafe extern "C" fn lyre_noise_wasm_rnnoise_free(processor: *mut RnnoiseFrameProcessor) {
    if !processor.is_null() {
        drop(Box::from_raw(processor));
    }
}

#[no_mangle]
pub extern "C" fn lyre_noise_wasm_sample_rate_hz() -> u32 {
    RNNOISE_SAMPLE_RATE_HZ
}

#[no_mangle]
pub extern "C" fn lyre_noise_wasm_channels() -> u32 {
    u32::from(RNNOISE_CHANNELS)
}

#[no_mangle]
pub extern "C" fn lyre_noise_wasm_frame_size() -> usize {
    RNNOISE_FRAME_SIZE
}

#[no_mangle]
pub extern "C" fn lyre_noise_wasm_alloc_f32(len: usize) -> *mut f32 {
    let layout = alloc::Layout::array::<f32>(len).expect("valid f32 allocation layout");
    unsafe { alloc::alloc(layout).cast::<f32>() }
}

#[no_mangle]
/// # Safety
///
/// `ptr` must be a pointer returned by `lyre_noise_wasm_alloc_f32` with the same `len`.
pub unsafe extern "C" fn lyre_noise_wasm_dealloc_f32(ptr: *mut f32, len: usize) {
    if ptr.is_null() {
        return;
    }
    let layout = alloc::Layout::array::<f32>(len).expect("valid f32 allocation layout");
    alloc::dealloc(ptr.cast::<u8>(), layout);
}

#[no_mangle]
/// # Safety
///
/// `processor` must be a live pointer returned by `lyre_noise_wasm_rnnoise_new`.
/// `input_ptr` and `output_ptr` must each reference `len` contiguous `f32` values.
pub unsafe extern "C" fn lyre_noise_wasm_rnnoise_process(
    processor: *mut RnnoiseFrameProcessor,
    input_ptr: *const f32,
    len: usize,
    output_ptr: *mut f32,
) -> f32 {
    if processor.is_null()
        || input_ptr.is_null()
        || output_ptr.is_null()
        || len == 0
        || !len.is_multiple_of(RNNOISE_FRAME_SIZE)
    {
        return f32::NAN;
    }
    let input = slice::from_raw_parts(input_ptr, len);
    let output = (*processor).process_samples(input);
    ptr::copy_nonoverlapping(output.samples.as_ptr(), output_ptr, len);
    output.voice_activity_probability
}
