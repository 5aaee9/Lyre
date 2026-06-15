# Getting Started

## Backend

Run the API server:

```bash
cargo run -p lyre-app -- serve --host 0.0.0.0 --port 8080
```

Useful variants:

```bash
cargo run -p lyre-app -- serve --ice-server 'stun:stun.l.google.com:19302'
cargo run -p lyre-app -- serve --ice-server 'turn:turn.example:3478|user|pass'
cargo run -p lyre-app -- serve --ice-server 'turn:turn.example:3478' --turn-rest-secret 'shared-secret'
cargo run -p lyre-app -- serve --embedded-turn --turn-rest-secret 'shared-secret'
cargo run -p lyre-app -- config print
```

## Frontend

Run the Next.js frontend:

```bash
cd frontend
npm install
APP_BASE_URL=http://localhost:3000 APP_API_URL=http://localhost:8080 npm run dev
```

Routes:

- `/`
- `/room/[roomId]`
- `/settings`

The settings page stores nickname, preferred room, noise-cancellation settings, and browser audio-processing controls through the Zustand settings store. The room page keeps microphone access behind the `Connect audio` button.
