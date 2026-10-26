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
    process_pcm_frame_as_track(runtime, key, frame.track_id.clone(), frame)
}

fn process_pcm_frame_as_track(
    runtime: &WebMediaRuntime,
    key: &ServerMediaSessionKey,
    track_id: String,
    frame: ServerMediaPcmFrame,
) -> Result<(), MediaRelayError> {
    runtime.process_frame(audio_frame_as_track(key, track_id, frame))
}

fn audio_frame_as_track(
    key: &ServerMediaSessionKey,
    track_id: String,
    frame: ServerMediaPcmFrame,
) -> AudioFrame {
    AudioFrame {
        room_id: key.room_id.clone(),
        user_id: key.user_id.clone(),
        track_id,
        sample_rate_hz: frame.sample_rate_hz,
        channels: frame.channels,
        sequence: u64::from(frame.sequence_number),
        rtp_timestamp: Some(frame.rtp_timestamp),
        samples: frame.samples,
    }
}

pub fn process_pcm_frame_batch(
    runtime: &WebMediaRuntime,
    key: &ServerMediaSessionKey,
    frames: Vec<ServerMediaPcmFrame>,
) -> Result<usize, MediaRelayError> {
    runtime.process_frames(frames.into_iter().map(|frame| {
        let track_id = frame.track_id.clone();
        audio_frame_as_track(key, track_id, frame)
    }))
}

pub fn process_pcm_frames(
    runtime: &WebMediaRuntime,
    negotiator: &ServerMediaNegotiator,
    key: &ServerMediaSessionKey,
) -> Result<usize, MediaRelayError> {
    let frames = negotiator.drain_pcm_frames(key);
    if runtime.uses_processed_audio(&key.room_id) {
        let _ = negotiator.drain_rtp_packets(key);
    }
    if frames.is_empty() {
        return Ok(0);
    }

    // The session can be closed between draining frames and reading its status.
    let Some(status) = negotiator.session_status(key) else {
        return Ok(0);
    };
    let track_id = status.audio_track_id;
    runtime.process_frames(
        frames
            .into_iter()
            .map(|frame| audio_frame_as_track(key, track_id.clone(), frame)),
    )
}
