pub mod egress;
pub mod media_ingress;
pub mod negotiation;
pub mod opus_decode;
pub mod session;
pub mod stack;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

#[cfg(test)]
mod negotiation_tests;
#[cfg(test)]
mod stack_egress_tests;
#[cfg(test)]
mod stack_media_tests;
#[cfg(test)]
mod stack_tests;

pub use egress::{
    ServerMediaEgressError, ServerMediaEgressRtpPacket, ServerMediaProcessedAudioFrame,
};
pub use media_ingress::{ServerMediaRemoteTrack, ServerMediaRtpPacket, ServerMediaTrackKind};
pub use negotiation::{
    ServerMediaAnswer, ServerMediaIceCandidate, ServerMediaNegotiationError, ServerMediaNegotiator,
    ServerMediaOffer,
};
pub use opus_decode::{
    ServerMediaDecodeError, ServerMediaDecodeFailure, ServerMediaOpusDecoder, ServerMediaPcmFrame,
    SERVER_MEDIA_OPUS_CHANNELS, SERVER_MEDIA_OPUS_FRAME_SIZE, SERVER_MEDIA_OPUS_SAMPLE_RATE_HZ,
};
pub use session::{
    ServerMediaSessionConfig, ServerMediaSessionKey, ServerMediaSessionRegistry,
    ServerMediaSessionState, ServerMediaSessionStatus,
};
pub use stack::{
    ServerMediaIceCandidateInit, WebRtcPeerConnectionHandle, WebRtcStack, WebRtcStackError,
};
