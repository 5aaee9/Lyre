pub mod ids;
pub mod media;
pub mod media_runtime;
pub mod noise;
pub mod room;
pub mod room_persistence;
pub mod webrtc;

#[cfg(test)]
mod media_tests;
#[cfg(test)]
mod room_aggregate_tests;

pub use ids::{RoomId, RoomIdError, UserId, DEFAULT_ROOM_ID};
pub use media::{
    MediaRelayError, MediaRelayMode, MediaRelayParticipant, MediaRelayRegistry,
    MediaRelayRegistryAggregate, MediaRelayRoomStatus, MediaRelayStatus, MediaRelayTrack,
    MediaRelayTrackLookup, MediaTrackKind, RegisterMediaTrackRequest, StartMediaRelayRequest,
    StopMediaRelayRequest,
};
pub use media_runtime::{
    AudioFrame, AudioFrameProcessor, MediaRuntime, PassthroughAudioFrameProcessor,
    ProcessedAudioFrame, ProcessedAudioSink,
};
pub use noise::{supported_noise_providers, DpdfNetConfig, NoiseCancellationConfig, NoiseProvider};
pub use room::{
    JoinRoomRequest, JoinRoomResponse, LeaveRoomRequest, LeaveRoomResponse, PersistedRoom,
    PersistedRoomRegistry, PersistedRoomRegistryError, PersistedRoomUser, RoomAccessError,
    RoomAccessToken, RoomRegistry, RoomRegistryAggregate, RoomSnapshot, UserProfile,
};
pub use webrtc::{
    current_media_topology, default_ice_servers, generate_turn_rest_credentials,
    ice_servers_with_turn_rest_credentials, IceServerConfig, MediaTopology, MediaTopologyMode,
    TurnRestCredentials, TurnRestCredentialsConfig, TurnRestCredentialsError,
};
