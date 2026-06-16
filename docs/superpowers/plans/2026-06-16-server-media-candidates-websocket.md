# Server-Media Candidates WebSocket Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move browser runtime server-media ICE candidate exchange from `/server-media/candidates` REST polling to the existing authenticated room WebSocket while keeping REST/WebRPC compatibility APIs.

**Architecture:** Add explicit server-media ICE signal payloads to the existing room signalling schema. The WebSocket handler consumes candidate submissions and candidate-list requests locally instead of forwarding them to peers. The frontend passes the existing room socket into `ServerMediaAudioSession`, sends local candidates and candidate requests over that socket, and routes incoming candidate-list responses back into the active audio session.

**Tech Stack:** Rust `axum` WebSocket handler, `serde` tagged enums, `lyre_webrtc::ServerMediaIceCandidate`, Next.js/React, Vitest, existing REST/WebRPC server-media APIs.

**Reviewed Spec:** `docs/superpowers/specs/2026-06-16-server-media-candidates-websocket-design.md`

---

## File Structure

- Modify `crates/lyre-web/src/signalling.rs`: add server-media payload variants and helper constructors.
- Modify `crates/lyre-web/src/signalling_tests.rs`: assert wire names and field names.
- Modify `crates/lyre-web/src/api.rs`: route server-media WebSocket payloads to `AppState` instead of `PeerHub::forward`.
- Create `crates/lyre-web/src/websocket_server_media_candidates_tests.rs`: focused tests for WebSocket candidate behavior.
- Modify `crates/lyre-web/src/lib.rs`: include the new test module.
- Modify `frontend/src/lib/signalling.ts`: add server-media payload types and encoder helpers.
- Modify `frontend/src/lib/signalling.test.ts`: assert new WebSocket message shapes and presence ignore behavior.
- Modify `frontend/src/lib/server-media-audio.ts`: replace candidate REST add/get calls with WebSocket send/request handling.
- Modify `frontend/src/lib/server-media-audio.test.ts`: replace candidate REST mocks with WebSocket message assertions.
- Modify `frontend/src/app/room/[roomId]/room-client.tsx`: pass the room socket to the server-media audio session and dispatch candidate responses.
- Modify `frontend/src/app/room/[roomId]/room-client-test-utils.ts`: stop mocking candidate REST calls for runtime assertions where no longer needed.
- Modify `frontend/src/app/room/[roomId]/room-client.test.tsx`: assert server-media candidate WebSocket dispatch.
- Modify `proto/lyre.ridl`: update comments only; no generated client update required for comments.
- Modify `docs/roadmap.md`: record the runtime transport change and remaining REST/WebRPC compatibility.

## Task 1: Rust Signalling Schema and Handler

**Files:**
- Modify: `crates/lyre-web/src/signalling.rs`
- Modify: `crates/lyre-web/src/signalling_tests.rs`
- Modify: `crates/lyre-web/src/api.rs`

- [x] **Step 1: Add failing signalling schema tests**

In `crates/lyre-web/src/signalling_tests.rs`, extend `serializes_all_server_payloads` or add a focused test named `serializes_server_media_ice_payloads`:

```rust
#[test]
fn serializes_server_media_ice_payloads() {
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("user_a");
    let candidate = lyre_webrtc::ServerMediaIceCandidate {
        room_id: room_id.clone(),
        user_id: user_id.clone(),
        candidate: "candidate:server".into(),
        sdp_mid: Some("0".into()),
        sdp_mline_index: Some(0),
        username_fragment: Some("ufrag".into()),
    };
    let cases = [
        SignalPayload::ServerMediaIceCandidate {
            candidate: "candidate:local".into(),
            sdp_mid: Some("0".into()),
            sdp_mline_index: Some(0),
            username_fragment: Some("ufrag".into()),
        },
        SignalPayload::ServerMediaIceCandidatesRequest,
        SignalPayload::ServerMediaIceCandidates {
            candidates: vec![candidate],
        },
    ];

    for payload in cases {
        let message = SignalMessage::new(
            room_id.clone(),
            user_id.clone(),
            Some(user_id.clone()),
            payload,
        );
        let json = serde_json::to_value(message).unwrap();
        assert_eq!(json["type"], json["payload"]["type"]);
        let round_trip: SignalMessage = serde_json::from_value(json).unwrap();
        assert_eq!(round_trip.room_id, room_id);
        assert_eq!(round_trip.sender_id, user_id);
    }
}
```

Run: `cargo test -p lyre-web signalling_tests::serializes_server_media_ice_payloads`

Expected: FAIL because the payload variants do not exist yet.

- [x] **Step 2: Add signalling payload variants**

In `crates/lyre-web/src/signalling.rs`, import `lyre_webrtc::ServerMediaIceCandidate`, add these `SignalKind` variants:

```rust
ServerMediaIceCandidate,
ServerMediaIceCandidatesRequest,
ServerMediaIceCandidates,
```

Add these `SignalPayload` variants:

```rust
ServerMediaIceCandidate {
    candidate: String,
    sdp_mid: Option<String>,
    sdp_mline_index: Option<u16>,
    username_fragment: Option<String>,
},
ServerMediaIceCandidatesRequest,
ServerMediaIceCandidates {
    candidates: Vec<ServerMediaIceCandidate>,
},
```

Update `SignalPayload::kind` with matching arms for the three variants.

- [x] **Step 3: Add a same-user response helper**

In `impl SignalMessage` in `crates/lyre-web/src/signalling.rs`, add:

```rust
pub fn to_self(room_id: RoomId, user_id: UserId, payload: SignalPayload) -> Self {
    Self::new(room_id, user_id.clone(), Some(user_id), payload)
}
```

Use this helper later for WebSocket server responses.

- [x] **Step 4: Run signalling tests**

Add explicit assertions before running the full module:

```rust
let message = SignalMessage::new(
    room_id.clone(),
    user_id.clone(),
    Some(user_id.clone()),
    SignalPayload::ServerMediaIceCandidate {
        candidate: "candidate:local".into(),
        sdp_mid: Some("0".into()),
        sdp_mline_index: Some(0),
        username_fragment: Some("ufrag".into()),
    },
);
let json = serde_json::to_value(&message).unwrap();
assert_eq!(json["type"], "server-media-ice-candidate");
assert_eq!(json["payload"]["type"], "server-media-ice-candidate");
assert_eq!(json["payload"]["sdp_mline_index"], 0);
let decoded: SignalMessage = serde_json::from_value(json).unwrap();
assert_eq!(decoded.payload, message.payload);

let request = SignalMessage::to_self(
    room_id.clone(),
    user_id.clone(),
    SignalPayload::ServerMediaIceCandidatesRequest,
);
let request_json = serde_json::to_value(&request).unwrap();
assert_eq!(request_json["type"], "server-media-ice-candidates-request");
assert_eq!(request_json["payload"]["type"], "server-media-ice-candidates-request");
let decoded_request: SignalMessage = serde_json::from_value(request_json).unwrap();
assert_eq!(decoded_request.payload, request.payload);

let response = SignalMessage::to_self(
    room_id.clone(),
    user_id.clone(),
    SignalPayload::ServerMediaIceCandidates {
        candidates: vec![candidate],
    },
);
let response_json = serde_json::to_value(&response).unwrap();
assert_eq!(response_json["type"], "server-media-ice-candidates");
assert_eq!(response_json["payload"]["type"], "server-media-ice-candidates");
assert_eq!(response_json["payload"]["candidates"][0]["sdp_mline_index"], 0);
let decoded_response: SignalMessage = serde_json::from_value(response_json).unwrap();
assert_eq!(decoded_response.payload, response.payload);
```

Run: `cargo test -p lyre-web signalling_tests`

Expected: PASS.

- [x] **Step 5: Route server-media WebSocket payloads locally**

In `crates/lyre-web/src/api.rs`, add imports:

```rust
server_media_ice_diagnostics::{summarize_candidates, ServerMediaIceCandidateSummary},
```

and:

```rust
use lyre_webrtc::{ServerMediaIceCandidate, ServerMediaSessionKey};
```

Add helper functions near `handle_socket`:

```rust
async fn handle_signal_message(
    state: &AppState,
    room_id: &RoomId,
    user_id: &lyre_core::UserId,
    signal: SignalMessage,
) -> Option<SignalMessage> {
    match route_signal_message(room_id, user_id, &signal) {
        Ok(_) => handle_valid_signal_message(state, room_id, user_id, signal).await,
        Err(error) => Some(*error),
    }
}

async fn handle_valid_signal_message(
    state: &AppState,
    room_id: &RoomId,
    user_id: &lyre_core::UserId,
    signal: SignalMessage,
) -> Option<SignalMessage> {
    match signal.payload {
        SignalPayload::ServerMediaIceCandidate {
            candidate,
            sdp_mid,
            sdp_mline_index,
            username_fragment,
        } => {
            let candidate = ServerMediaIceCandidate {
                room_id: room_id.clone(),
                user_id: user_id.clone(),
                candidate,
                sdp_mid,
                sdp_mline_index,
                username_fragment,
            };
            let summary = ServerMediaIceCandidateSummary::from_candidate(&candidate);
            match state.add_server_media_ice_candidate(candidate).await {
                Ok(()) => {
                    tracing::debug!(
                        room_id = %room_id,
                        user_id = %user_id,
                        candidate = ?summary,
                        "server media remote ICE candidate accepted over websocket"
                    );
                    None
                }
                Err(error) => {
                    tracing::debug!(
                        room_id = %room_id,
                        user_id = %user_id,
                        candidate = ?summary,
                        error = %error,
                        "failed to add server media ICE candidate over websocket"
                    );
                    Some(SignalMessage::error(
                        room_id.clone(),
                        user_id.clone(),
                        error.to_string(),
                    ))
                }
            }
        }
        SignalPayload::ServerMediaIceCandidatesRequest => {
            let key = ServerMediaSessionKey {
                room_id: room_id.clone(),
                user_id: user_id.clone(),
            };
            let candidates = state.server_media_ice_candidates(&key);
            let candidate_summaries = summarize_candidates(&candidates);
            tracing::debug!(
                room_id = %room_id,
                user_id = %user_id,
                candidate_count = candidates.len(),
                candidates = ?candidate_summaries,
                "server media local ICE candidates returned over websocket"
            );
            Some(SignalMessage::to_self(
                room_id.clone(),
                user_id.clone(),
                SignalPayload::ServerMediaIceCandidates { candidates },
            ))
        }
        _ => {
            state.peers.forward(signal);
            None
        }
    }
}
```

Then replace the existing `Ok(signal) => match route_signal_message...` block inside `handle_socket` with a call to `handle_signal_message`; when it returns `Some(response)`, serialize and send `response` on `ws_tx`.

- [x] **Step 6: Preserve error-chain logging**

In the failure branch from Step 5, if `ServerMediaNegotiationError` supports alternate formatting through `Display` only, keep `error = %error`. If the error type has source chains available through `anyhow` only after conversion, do not wrap it. Do not log raw candidate strings.

- [x] **Step 7: Run targeted Rust checks**

Run:

```bash
cargo fmt --check
cargo test -p lyre-web signalling_tests
cargo clippy -p lyre-web --all-targets -- -D warnings
```

Expected: all PASS.

## Task 2: Rust WebSocket Candidate Tests

**Files:**
- Create: `crates/lyre-web/src/websocket_server_media_candidates_tests.rs`
- Modify: `crates/lyre-web/src/lib.rs`

- [x] **Step 1: Add the new test module**

In `crates/lyre-web/src/lib.rs`, add:

```rust
#[cfg(test)]
mod websocket_server_media_candidates_tests;
```

Place it next to the existing WebSocket test modules.

- [x] **Step 2: Write WebSocket helper tests**

Create `crates/lyre-web/src/websocket_server_media_candidates_tests.rs` with helpers for:

```rust
use crate::{
    api::AppState,
    signalling::{SignalMessage, SignalPayload},
};
use lyre_core::{RoomId, UserId};
use lyre_webrtc::{ServerMediaOffer, WebRtcStack};

fn ids() -> (RoomId, UserId) {
    (RoomId::default_room(), UserId::from_external("user_a"))
}
```

Test the local handler helper introduced in Task 1. Make that helper `pub(crate)` in `crate::api`; do not add public API surface outside the crate.

- [x] **Step 3: Cover candidate request response**

Add a test named `server_media_candidates_request_returns_candidates_to_same_socket`. It should:

1. Create `AppState::default()`.
2. Use `AppState::default()` without negotiating a session; an empty candidate list is enough to prove request routing and response shape.
3. Send a `SignalMessage::to_self(..., SignalPayload::ServerMediaIceCandidatesRequest)` to the handler helper.
4. Assert the response payload is `SignalPayload::ServerMediaIceCandidates { candidates }`.
5. Assert `recipient_id` is the same user and no peer broadcast is observed through `PeerHub`.

- [x] **Step 4: Cover missing session error for candidate submission**

Add a test named `server_media_candidate_without_session_returns_error_signal`. It should:

1. Create `AppState::default()`.
2. Send `SignalPayload::ServerMediaIceCandidate` for `user_a` without negotiating a session.
3. Assert the response is `SignalPayload::Error { message }`.
4. Assert the message mentions missing or unavailable server-media session, matching the existing error wording without requiring an exact full sentence.

- [x] **Step 5: Cover existing-session candidate submission**

Add `server_media_candidate_with_session_is_accepted_over_websocket`:

1. Negotiate server media using `WebRtcStack::new().create_peer_connection().await.unwrap().create_local_offer_for_test().await.unwrap()`.
2. Send a host ICE candidate payload through the helper.
3. Assert response is `None`.

Use this local helper inside the test module:

```rust
async fn offer_sdp() -> String {
    let offerer = WebRtcStack::new().create_peer_connection().await.unwrap();
    offerer.create_local_offer_for_test().await.unwrap()
}

async fn negotiate_server_media(state: &AppState, room_id: RoomId, user_id: UserId) {
    state
        .answer_server_media_offer(ServerMediaOffer {
            room_id,
            user_id,
            audio_track_id: "audio-main".to_owned(),
            sdp: offer_sdp().await,
        })
        .await
        .unwrap();
}
```

- [x] **Step 6: Run targeted Rust tests**

Run:

```bash
cargo test -p lyre-web websocket_server_media_candidates_tests
cargo test -p lyre-web api_server_media_tests::server_media_candidate_route_accepts_existing_peer_candidate
cargo test -p lyre-web webrpc_tests::server_media
```

Expected: all PASS. If the WebRPC module filter differs, run `cargo test -p lyre-web webrpc_tests`.

## Task 3: Frontend WebSocket Candidate Runtime

**Files:**
- Modify: `frontend/src/lib/signalling.ts`
- Modify: `frontend/src/lib/signalling.test.ts`
- Modify: `frontend/src/lib/server-media-audio.ts`
- Modify: `frontend/src/lib/server-media-audio.test.ts`
- Modify: `frontend/src/app/room/[roomId]/room-client.tsx`
- Modify: `frontend/src/app/room/[roomId]/room-client-test-utils.ts`
- Modify: `frontend/src/app/room/[roomId]/room-client.test.tsx`

- [x] **Step 1: Add frontend signalling encoders**

In `frontend/src/lib/signalling.ts`, add a local type for server-media ICE candidates compatible with the existing REST shape:

```ts
export type ServerMediaIceCandidateSignal = {
  room_id: string;
  user_id: string;
  candidate: string;
  sdp_mid?: string | null;
  sdp_mline_index?: number | null;
  username_fragment?: string | null;
};
```

Extend `SignalPayload` with:

```ts
| {
    type: "server-media-ice-candidate";
    candidate: string;
    sdp_mid?: string | null;
    sdp_mline_index?: number | null;
    username_fragment?: string | null;
  }
| { type: "server-media-ice-candidates-request" }
| { type: "server-media-ice-candidates"; candidates: ServerMediaIceCandidateSignal[] }
```

Add encoders:

```ts
export function encodeServerMediaIceCandidate(
  roomId: string,
  senderId: string,
  candidate: RTCIceCandidateInit
): SignalMessage {
  return message(roomId, senderId, senderId, {
    type: "server-media-ice-candidate",
    candidate: candidate.candidate ?? "",
    sdp_mid: candidate.sdpMid ?? null,
    sdp_mline_index: candidate.sdpMLineIndex ?? null,
    username_fragment: candidate.usernameFragment ?? null
  });
}

export function encodeServerMediaIceCandidatesRequest(roomId: string, senderId: string): SignalMessage {
  return message(roomId, senderId, senderId, { type: "server-media-ice-candidates-request" });
}
```

Update `reducePresence` to ignore the three server-media payloads.

- [x] **Step 2: Add signalling tests**

In `frontend/src/lib/signalling.test.ts`, import the new encoders and assert:

```ts
expect(
  encodeServerMediaIceCandidate("DEFAULT", "user_a", {
    candidate: "candidate:local",
    sdpMid: "0",
    sdpMLineIndex: 0,
    usernameFragment: "ufrag"
  }).payload
).toEqual({
  type: "server-media-ice-candidate",
  candidate: "candidate:local",
  sdp_mid: "0",
  sdp_mline_index: 0,
  username_fragment: "ufrag"
});

expect(encodeServerMediaIceCandidatesRequest("DEFAULT", "user_a")).toMatchObject({
  type: "server-media-ice-candidates-request",
  recipient_id: "user_a",
  payload: { type: "server-media-ice-candidates-request" }
});
```

Also assert `reducePresence` returns the same state for a `server-media-ice-candidates` payload.

- [x] **Step 3: Update `ServerMediaAudioSession` input and imports**

In `frontend/src/lib/server-media-audio.ts`, remove imports of `addServerMediaIceCandidate` and `getServerMediaIceCandidates`. Keep `answerServerMediaOffer` and import the WebSocket encoders plus `ServerMediaIceCandidateSignal`:

```ts
import {
  encodeServerMediaIceCandidate,
  encodeServerMediaIceCandidatesRequest,
  type SignalMessage,
  type ServerMediaIceCandidateSignal
} from "./signalling";
```

Add `socket: WebSocket` to `ServerMediaAudioSessionInput`.

- [x] **Step 4: Replace REST candidate add/get with WebSocket send/request**

In `ServerMediaAudioSession`, replace `addLocalCandidate` with a synchronous WebSocket send helper:

```ts
private sendLocalCandidate(candidate: RTCIceCandidateInit): void {
  this.sendSignal(encodeServerMediaIceCandidate(this.input.roomId, this.input.userId, candidate));
}
```

Change `flushLocalCandidates` to call `sendLocalCandidate` for queued candidates. Change `sendOrQueueLocalCandidate` to queue before answer and call `sendLocalCandidate` after answer.

Add:

```ts
private requestServerCandidates(): void {
  this.sendSignal(encodeServerMediaIceCandidatesRequest(this.input.roomId, this.input.userId));
}

private sendSignal(signal: SignalMessage): void {
  if (this.input.socket.readyState !== WebSocket.OPEN) {
    this.reportError(new Error("Audio signalling websocket is not connected"));
    return;
  }
  this.input.socket.send(JSON.stringify(signal));
}
```

In `start()`, after `flushLocalCandidates()`, call `this.requestServerCandidates()` and start an interval that calls `requestServerCandidates`.

Add:

```ts
async handleSignal(signal: SignalMessage): Promise<void> {
  if (signal.payload.type !== "server-media-ice-candidates") {
    return;
  }
  for (const candidate of signal.payload.candidates) {
    await this.addServerCandidate(candidate);
  }
}
```

Update `addServerCandidate` and `candidateKey` to use `ServerMediaIceCandidateSignal`.

- [x] **Step 5: Update `server-media-audio.test.ts`**

Remove REST candidate mocks from `server-media-audio.test.ts`. Provide a mock socket:

```ts
function makeSocket() {
  return {
    readyState: WebSocket.OPEN,
    send: vi.fn()
  } as unknown as WebSocket;
}
```

Pass the socket into `makeSession`. Update tests:

- Negotiation test asserts the first candidate request is sent over `socket.send`, then advancing timers sends another request.
- Local candidate test asserts a `server-media-ice-candidate` JSON message is sent.
- Queued candidate test asserts no local candidate message is sent before the answer promise resolves, then one is sent after.
- Deduplication test calls `await session.handleSignal(fullSignalMessage)` twice and asserts `addIceCandidate` once. The full message must include `type`, `room_id`, `sender_id`, `recipient_id`, and `payload: { type: "server-media-ice-candidates", candidates: [...] }`.
- Close test asserts no extra candidate request after `close()`.

- [x] **Step 6: Wire RoomClient to the existing socket**

In `frontend/src/app/room/[roomId]/room-client.tsx`, update `socket.onmessage`:

```ts
socket.onmessage = (event) => {
  const signal = JSON.parse(event.data as string) as SignalMessage;
  void serverAudioSessionRef.current?.handleSignal(signal).catch((error: unknown) => {
    setStatus(error instanceof Error ? error.message : "Audio connection failed");
  });
  setRoom((current) => {
    const next: PresenceState = reducePresence({ room: current ?? undefined }, signal);
    if (next.error) {
      setStatus(next.error);
    }
    return next.room ?? current;
  });
};
```

When constructing `ServerMediaAudioSession`, pass `socket: socketRef.current`. If no socket exists or it is not open, throw `new Error("Audio signalling websocket is not connected")` before creating the session.

- [x] **Step 7: Update room client tests**

In `frontend/src/app/room/[roomId]/room-client-test-utils.ts`, set `readyState = WebSocket.OPEN` on `MockWebSocket` when it opens.

In `frontend/src/app/room/[roomId]/room-client.test.tsx`:

- Update the existing server relay negotiation test: `send` is now expected to be called for server-media candidate request, not never called.
- Replace the startup failure test that mocked `getServerMediaIceCandidates` with a WebSocket-closed failure test if needed.
- Add a test that starts audio, sends an incoming `server-media-ice-candidates` socket message, and asserts `peerConnections[0].addIceCandidate` receives the server candidate.

- [x] **Step 8: Run frontend targeted tests**

Run:

```bash
npm --prefix frontend run test -- frontend/src/lib/signalling.test.ts frontend/src/lib/server-media-audio.test.ts 'frontend/src/app/room/[roomId]/room-client.test.tsx'
npm --prefix frontend run typecheck
```

Expected: all PASS.

## Task 4: Documentation, Final Verification, Review Prep

**Files:**
- Modify: `proto/lyre.ridl`
- Modify: `docs/roadmap.md`
- Modify: `docs/superpowers/plans/2026-06-16-server-media-candidates-websocket.md`

- [x] **Step 1: Update WebRPC comments**

In `proto/lyre.ridl`, change the candidate method comments from “REST fetch remains the runtime transport in this increment” to wording like:

```ridl
# Compatibility RPC for POST /api/rooms/{room_id}/server-media/candidates; browser runtime ICE exchange uses the room WebSocket.
```

and:

```ridl
# Compatibility RPC for GET /api/rooms/{room_id}/server-media/candidates?user_id={userID}; browser runtime ICE exchange uses the room WebSocket.
```

Do not regenerate `frontend/src/lib/lyre.gen.ts` for comment-only changes.

- [x] **Step 2: Update roadmap**

In `docs/roadmap.md`, add a completed item noting:

- Browser runtime server-media ICE candidates now use the authenticated room WebSocket.
- REST/WebRPC candidate endpoints remain compatibility and test surfaces.
- True server-side candidate push subscriptions remain future work if desired.

- [x] **Step 3: Mark plan checklist**

As each task completes, update this plan's checkboxes from `[ ]` to `[x]` only for steps actually completed.

- [x] **Step 4: Run final verification**

Run:

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p lyre-web signalling_tests
cargo test -p lyre-web websocket_server_media_candidates_tests
cargo test -p lyre-web api_server_media_tests
cargo test -p lyre-web webrpc_tests
npm --prefix frontend run test -- frontend/src/lib/signalling.test.ts frontend/src/lib/server-media-audio.test.ts 'frontend/src/app/room/[roomId]/room-client.test.tsx'
npm --prefix frontend run typecheck
npm --prefix frontend run lint
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: all PASS. If any command fails for an environment reason, capture the exact output and run the next-best narrower command only after understanding the failure.

- [x] **Step 5: Prepare implementation review inputs**

Capture:

```bash
git diff --stat
git diff -- crates/lyre-web/src/signalling.rs crates/lyre-web/src/api.rs crates/lyre-web/src/signalling_tests.rs crates/lyre-web/src/websocket_server_media_candidates_tests.rs crates/lyre-web/src/lib.rs frontend/src/lib/signalling.ts frontend/src/lib/server-media-audio.ts frontend/src/app/room/[roomId]/room-client.tsx proto/lyre.ridl docs/roadmap.md
```

Use this with the reviewed spec and this plan for the required final implementation reviewer.
