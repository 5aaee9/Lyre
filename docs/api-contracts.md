# API and WebRPC Contracts

## REST and WebSocket Routes

- `GET /health`
- `GET /api/rooms/:room_id`
- `POST /api/rooms/:room_id/join`
- `POST /api/rooms/:room_id/leave`
- `GET /api/noise/providers`
- `GET /api/webrtc/ice-servers`
- `GET /api/webrtc/topology`
- `GET /api/rooms/:room_id/media-relay`
- `POST /api/rooms/:room_id/media-relay/start`
- `POST /api/rooms/:room_id/media-relay/stop`
- `POST /api/rooms/:room_id/media-relay/participants`
- `POST /api/rooms/:room_id/media-relay/tracks`
- `POST /api/rooms/:room_id/media-relay/subscriptions`
- `GET /api/rooms/:room_id/ws?user_id=...`

### Media Relay Participants

`POST /api/rooms/:room_id/media-relay/participants` registers one authenticated room user as an active relay participant without publishing any tracks. This is used by clients that join without a usable microphone and should receive room audio in listen-only mode.

The route requires the room bearer token for `user_id`.

Request:

```json
{
  "user_id": "listener_user_id"
}
```

Response:

```json
{
  "room_id": "DEFAULT",
  "status": "active",
  "participants": [
    {
      "user_id": "listener_user_id",
      "tracks": []
    }
  ]
}
```

Registering a participant replaces that user's currently registered relay tracks with an empty track list. Trackless participants can update their own subscriptions and receive remote audio, but they are not audio sources for other listeners.

### Media Relay Subscriptions

`POST /api/rooms/:room_id/media-relay/subscriptions` updates one listener's complete source-user allow-list.

The route requires the room bearer token for `user_id`.

Request:

```json
{
  "user_id": "listener_user_id",
  "source_user_ids": ["source_user_a", "source_user_b"]
}
```

Response:

```json
{
  "room_id": "DEFAULT",
  "user_id": "listener_user_id",
  "source_user_ids": ["source_user_a", "source_user_b"]
}
```

The server validates that the media relay is active, that `user_id` is an active relay participant, and that every source user is an active relay participant with an audio track. `source_user_ids` is sorted and deduplicated in the response. An empty list is valid and means the listener receives no remote relay audio. Repeating the same request is idempotent.

## WebRPC

The formal WebRPC schema lives at `proto/lyre.ridl`. The committed generated TypeScript client and types live at `frontend/src/lib/lyre.gen.ts`.

Regenerate the client with:

```bash
cd frontend
npm run generate:webrpc
```

This uses `go run github.com/webrpc/webrpc/cmd/webrpc-gen@v0.36.0`; the first run needs network access for Go module download.

The Rust API serves generated-client-compatible WebRPC calls at `POST /rpc/Lyre/<Method>`. Existing REST routes remain supported and continue to back the current frontend helper layer.
