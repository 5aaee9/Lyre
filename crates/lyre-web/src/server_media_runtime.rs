use lyre_core::{AudioFrame, MediaRelayError};
use lyre_webrtc::{
    ServerMediaDecodeFailure, ServerMediaNegotiator, ServerMediaPcmFrame, ServerMediaSessionKey,
};

use crate::media_runtime::WebMediaRuntime;

pub fn drain_pcm_frames(
    negotiator: &ServerMediaNegotiator,
    key: &ServerMediaSessionKey,
) -> Vec<ServerMediaPcmFrame> {
    negotiator.drain_pcm_frames(key)
}

pub fn drain_decode_failures(
    negotiator: &ServerMediaNegotiator,
    key: &ServerMediaSessionKey,
) -> Vec<ServerMediaDecodeFailure> {
    negotiator.drain_decode_failures(key)
}

pub fn process_pcm_frame(
    runtime: &WebMediaRuntime,
    key: &ServerMediaSessionKey,
    frame: ServerMediaPcmFrame,
) -> Result<(), MediaRelayError> {
    runtime.process_frame(AudioFrame {
        room_id: key.room_id.clone(),
        user_id: key.user_id.clone(),
        track_id: frame.track_id,
        sample_rate_hz: frame.sample_rate_hz,
        channels: frame.channels,
        sequence: u64::from(frame.sequence_number),
        samples: frame.samples,
    })
}

pub fn process_pcm_frame_batch(
    runtime: &WebMediaRuntime,
    key: &ServerMediaSessionKey,
    frames: Vec<ServerMediaPcmFrame>,
) -> Result<usize, MediaRelayError> {
    let mut processed = 0;
    for frame in frames {
        process_pcm_frame(runtime, key, frame)?;
        processed += 1;
    }
    Ok(processed)
}

pub fn process_pcm_frames(
    runtime: &WebMediaRuntime,
    negotiator: &ServerMediaNegotiator,
    key: &ServerMediaSessionKey,
) -> Result<usize, MediaRelayError> {
    process_pcm_frame_batch(runtime, key, negotiator.drain_pcm_frames(key))
}
