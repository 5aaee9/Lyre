use std::{
    ffi::{c_int, c_uchar, c_void},
    marker::PhantomData,
};

pub(crate) const APPLICATION_AUDIO: c_int = 2049;
pub(crate) const OK: c_int = 0;

pub(crate) struct Encoder {
    ptr: *mut c_void,
}

pub(crate) struct Decoder {
    ptr: *mut c_void,
}

pub(crate) struct SendOnly<T> {
    inner: T,
    _not_sync: PhantomData<*mut ()>,
}

unsafe impl<T: Send> Send for SendOnly<T> {}

impl<T> SendOnly<T> {
    pub(crate) fn new(inner: T) -> Self {
        Self {
            inner,
            _not_sync: PhantomData,
        }
    }

    pub(crate) fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

unsafe impl Send for Encoder {}
unsafe impl Send for Decoder {}

impl Encoder {
    pub(crate) fn new(
        sample_rate_hz: u32,
        channels: u16,
        application: c_int,
    ) -> Result<Self, String> {
        let mut error = 0;
        let ptr = unsafe {
            opus_encoder_create(
                sample_rate_hz as c_int,
                channels as c_int,
                application,
                &mut error,
            )
        };
        if ptr.is_null() || error != OK {
            return Err(error_message(error));
        }
        Ok(Self { ptr })
    }

    pub(crate) fn encode_float(
        &mut self,
        input: &[f32],
        frame_size: usize,
        output: &mut [u8],
    ) -> Result<usize, String> {
        let encoded = unsafe {
            opus_encode_float(
                self.ptr,
                input.as_ptr(),
                frame_size as c_int,
                output.as_mut_ptr(),
                output.len() as c_int,
            )
        };
        if encoded < 0 {
            return Err(error_message(encoded));
        }
        Ok(encoded as usize)
    }
}

impl Decoder {
    pub(crate) fn new(sample_rate_hz: u32, channels: u16) -> Result<Self, String> {
        let mut error = 0;
        let ptr =
            unsafe { opus_decoder_create(sample_rate_hz as c_int, channels as c_int, &mut error) };
        if ptr.is_null() || error != OK {
            return Err(error_message(error));
        }
        Ok(Self { ptr })
    }

    pub(crate) fn decode_float(
        &mut self,
        input: &[u8],
        frame_size: usize,
        output: &mut [f32],
    ) -> Result<usize, String> {
        let decoded = unsafe {
            opus_decode_float(
                self.ptr,
                input.as_ptr(),
                input.len() as c_int,
                output.as_mut_ptr(),
                frame_size as c_int,
                0,
            )
        };
        if decoded < 0 {
            return Err(error_message(decoded));
        }
        Ok(decoded as usize)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        unsafe { opus_encoder_destroy(self.ptr) };
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe { opus_decoder_destroy(self.ptr) };
    }
}

pub(crate) fn error_message(code: c_int) -> String {
    let ptr = unsafe { opus_strerror(code) };
    if ptr.is_null() {
        return format!("libopus error {code}");
    }
    unsafe { std::ffi::CStr::from_ptr(ptr.cast()) }
        .to_string_lossy()
        .into_owned()
}

#[link(name = "opus")]
extern "C" {
    fn opus_encoder_create(
        fs: c_int,
        channels: c_int,
        application: c_int,
        error: *mut c_int,
    ) -> *mut c_void;

    fn opus_encode_float(
        st: *mut c_void,
        pcm: *const f32,
        frame_size: c_int,
        data: *mut c_uchar,
        max_data_bytes: c_int,
    ) -> c_int;

    fn opus_encoder_destroy(st: *mut c_void);

    fn opus_decoder_create(fs: c_int, channels: c_int, error: *mut c_int) -> *mut c_void;

    fn opus_decode_float(
        st: *mut c_void,
        data: *const c_uchar,
        len: c_int,
        pcm: *mut f32,
        frame_size: c_int,
        decode_fec: c_int,
    ) -> c_int;

    fn opus_decoder_destroy(st: *mut c_void);

    fn opus_strerror(error: c_int) -> *const c_uchar;
}
