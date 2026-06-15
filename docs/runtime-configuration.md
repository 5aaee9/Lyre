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

## DeepFilterNet Runtime

The server-side DeepFilterNet provider currently configures libDF DSP/STFT frame processing, not pretrained DeepFilterNet neural model inference.

Runtime parameters:

- `--deepfilternet-fft-size` / `LYRE_DEEPFILTERNET_FFT_SIZE`, default `960`
- `--deepfilternet-hop-size` / `LYRE_DEEPFILTERNET_HOP_SIZE`, default `480`
- `--deepfilternet-erb-bands` / `LYRE_DEEPFILTERNET_ERB_BANDS`, default `32`
- `--deepfilternet-min-erb-freqs` / `LYRE_DEEPFILTERNET_MIN_ERB_FREQS`, default `2`

Invalid combinations fail startup instead of falling back silently.
