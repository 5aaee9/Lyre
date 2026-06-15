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
- `POST /api/rooms/:room_id/media-relay/tracks`
- `GET /api/rooms/:room_id/ws?user_id=...`

## WebRPC

The formal WebRPC schema lives at `proto/lyre.ridl`. The committed generated TypeScript client and types live at `frontend/src/lib/lyre.gen.ts`.

Regenerate the client with:

```bash
cd frontend
npm run generate:webrpc
```

This uses `go run github.com/webrpc/webrpc/cmd/webrpc-gen@v0.36.0`; the first run needs network access for Go module download.

The Rust API serves generated-client-compatible WebRPC calls at `POST /rpc/Lyre/<Method>`. Existing REST routes remain supported and continue to back the current frontend helper layer.
