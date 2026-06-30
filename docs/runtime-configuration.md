# Runtime Configuration

## ICE Servers

`LYRE_ICE_SERVERS` accepts a semicolon-separated list using:

```text
url[,url...][|username|credential]
```

Configured TURN usernames and credentials are returned to browsers by `GET /api/webrtc/ice-servers`. Use scoped, rotated, low-lifetime TURN credentials instead of privileged long-lived secrets.

## TURN REST Credentials

TURN REST credentials can be generated for configured `turn:` and `turns:` ICE servers with `--turn-rest-secret` or `LYRE_TURN_REST_SECRET`.

Optional settings:

- `--turn-rest-ttl-seconds` / `LYRE_TURN_REST_TTL_SECONDS`
- `--turn-rest-identity` / `LYRE_TURN_REST_IDENTITY`

The shared secret is never returned to browsers. The endpoint returns only short-lived usernames and HMAC-SHA1 credentials using the existing ICE server response shape, so `proto/lyre.ridl` does not need a separate schema change for this behavior.

## Embedded TURN Relay

An embedded UDP TURN relay can be enabled with `--embedded-turn` or `LYRE_EMBEDDED_TURN=true`.

Required:

- `--turn-rest-secret` / `LYRE_TURN_REST_SECRET`

Optional settings:

- `--embedded-turn-listen` / `LYRE_EMBEDDED_TURN_LISTEN`, default `0.0.0.0:3478`
- `--embedded-turn-external` / `LYRE_EMBEDDED_TURN_EXTERNAL`, default `127.0.0.1:3478`
- `--embedded-turn-realm` / `LYRE_EMBEDDED_TURN_REALM`
- `--embedded-turn-port-range` / `LYRE_EMBEDDED_TURN_PORT_RANGE`

`--embedded-turn-external` must be an IP socket address, not a hostname. Port ranges use inclusive `<start>..<end>` syntax within `49152..65535`.

When embedded TURN is enabled and no `--ice-server` / `LYRE_ICE_SERVERS` is configured, Lyre advertises `turn:<embedded-turn-external>` through `/api/webrtc/ice-servers`. Explicit ICE server configuration disables this auto-injection.

The embedded TURN runtime uses the MIT `turn-server` crate from the `turn-rs` project. The relay validates the HMAC credential but does not enforce the timestamp embedded in TURN REST usernames, so keep TURN credential TTL short.

## Server-Media Public IP

When Lyre runs behind a VPC, NAT, or cloud private interface, server-media WebRTC host ICE candidates may otherwise advertise private addresses. Set `--server-media-public-ip <ip>` or `LYRE_SERVER_MEDIA_PUBLIC_IP=<ip>` to rewrite the advertised server-media host candidate IP returned to browsers.

This changes only the ICE candidate address exposed to clients. The server still binds its WebRTC UDP socket on the local interface.

Set `--server-media-port-range <start>..<end>` or `LYRE_SERVER_MEDIA_PORT_RANGE=<start>..<end>` to restrict server-media WebRTC UDP sockets to firewall-open ports. When embedded TURN is enabled and no server-media range is set, Lyre reuses `--embedded-turn-port-range` / `LYRE_EMBEDDED_TURN_PORT_RANGE`.

## DeepFilterNet Runtime

The server-side DeepFilterNet provider runs the DeepFilterNet3 ONNX model through ONNX Runtime. The model directory must contain:

- `enc.onnx`
- `erb_dec.onnx`
- `df_dec.onnx`

Runtime parameters:

- `--deepfilternet-model-dir` / `LYRE_DEEPFILTERNET_MODEL_DIR`, default `deepfilternet/onnx`
- `--deepfilternet-intra-threads` / `LYRE_DEEPFILTERNET_INTRA_THREADS`, default `1`
- `--deepfilternet-inter-threads` / `LYRE_DEEPFILTERNET_INTER_THREADS`, default `1`

The local `deepfilternet/` model directory is ignored by git so downloaded ONNX artifacts stay out of the source tree. Invalid runtime thread counts fail startup instead of falling back silently.

The Nix `lyre-api` package sets `LYRE_DEEPFILTERNET_MODEL_DIR` and `LYRE_DPDFNET_MODEL_DIR` to packaged store paths when those variables are unset. Explicit CLI flags or environment variables still override those defaults.

The packaged model directories are:

- `${lyre-noise-models}/share/lyre/models/deepfilternet/onnx`
- `${lyre-noise-models}/share/lyre/models/dpdfnet/onnx`

The DPDFNet model directory must contain one ONNX file per supported server-side model, for example `dpdfnet2_48khz_hr.onnx` for the default DPDFNet settings.

## Client-Side Noise Models

When browser-side noise cancellation is enabled, the frontend uses the same Rust WASM DSP crate as the server-side implementation boundary and runs ONNX inference with `onnxruntime-web`'s WebAssembly backend.

The frontend build copies the matching ONNX Runtime Web asset from `onnxruntime-web` to:

- `/ort/ort-wasm-simd-threaded.wasm`

Client model files are served from `frontend/public/models/` at these paths and cached in the browser Cache API:

- `/models/deepfilternet/enc.onnx`
- `/models/deepfilternet/erb_dec.onnx`
- `/models/deepfilternet/df_dec.onnx`
- `/models/dpdfnet/dpdfnet2_48khz_hr.onnx`
- `/models/dpdfnet/dpdfnet2_48khz_hr.json`
- `/models/dpdfnet/dpdfnet8_48khz_hr.onnx`
- `/models/dpdfnet/dpdfnet8_48khz_hr.json`

The DPDFNet JSON manifest must contain the initial ONNX runtime state derived from the model metadata:

```json
{
  "initialState": [0.0]
}
```

The actual `initialState` array must match the selected ONNX model's state input length. Browser-side DPDFNet currently accepts only the 48 kHz HR models; 16 kHz DPDFNet models remain server-side because the browser path does not include resampling.
