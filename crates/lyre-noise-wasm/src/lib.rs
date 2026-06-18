mod deepfilternet;
mod dpdfnet;
mod rnnoise;

use std::alloc;

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
