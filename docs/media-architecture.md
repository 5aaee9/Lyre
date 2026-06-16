# Media Architecture

## Current Topology

`GET /api/webrtc/topology` reports the active media topology. The current topology is peer-to-peer mesh WebRTC with TURN relay support for NAT traversal.

The frontend creates one browser `RTCPeerConnection` per remote room user, reuses one local audio stream, and targets WebRTC offer, answer, and ICE messages with `recipient_id`.

TURN, including the embedded TURN relay, relays encrypted WebRTC packets and cannot run server-side RNNoise or DeepFilterNet by itself.

## Server-Media Path

Server-side noise cancellation requires a media relay or SFU-like path that terminates WebRTC media, decodes audio to PCM, runs `lyre-noise-cancelling`, then re-encodes and broadcasts processed audio.

The media relay REST endpoints expose the room-scoped state for this path:

- `GET /api/rooms/:room_id/media-relay` reports whether the relay is active, intended noise config, and registered participant tracks.
- `POST /api/rooms/:room_id/media-relay/start` activates the room relay and records an optional noise config.
- `POST /api/rooms/:room_id/media-relay/tracks` registers track metadata while active.
- `POST /api/rooms/:room_id/media-relay/stop` deactivates the relay and clears tracks.

`lyre-core` defines a decoded-PCM media runtime boundary. It accepts already-decoded audio frames, requires an active relay and registered audio track without mutating relay state, runs an `AudioFrameProcessor`, and publishes processed PCM to an internal `ProcessedAudioSink`.

`lyre-web::AppState` owns an internal decoded-PCM media runtime wired to the media relay registry and RNNoise-capable processor. Processed frames are stored in an internal in-memory sink for tests and future broadcaster integration.

## Noise Processing

`lyre-noise-cancelling` can run RNNoise-compatible processing for decoded 48 kHz mono PCM frames of 480 samples using `nnnoiseless`. RNNoise returns voice activity detection metadata, but Lyre's `intensity` and `voice_activity_threshold` settings do not alter or suppress output yet.

DeepFilterNet uses the DeepFilterNet3 ONNX `enc.onnx`, `erb_dec.onnx`, and `df_dec.onnx` neural model pipeline through ONNX Runtime. Lyre still uses the Rust `deep_filter` crate for the streaming STFT, feature extraction, ERB masking, deep-filter reconstruction, and ISTFT audio framing around the model inference.

If client-side noise cancellation is added before server-side media relay processing, it should be implemented as Rust compiled to WebAssembly rather than a JavaScript DSP path.
