# Listen-Only No-Microphone Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let browsers with no usable microphone join Lyre room audio in listen-only mode while preserving server-relay-only audio semantics.

**Architecture:** Add explicit empty-track media relay participants on the server, define audio sources as participants with registered audio tracks, then have the frontend register no-mic clients as listen-only participants and negotiate a receive-only WebRTC audio m-line. Keep normal microphone startup unchanged and keep non-missing media errors visible.

**Tech Stack:** Rust `lyre-core`/`lyre-web`, Axum REST, WebRPC RIDL/generated TypeScript client, Next.js React frontend, WebRTC APIs, Vitest, cargo nextest.

---

## File Structure

- `crates/lyre-core/src/media.rs`: add `register_participant()` and audio-source filtering for subscriptions.
- `crates/lyre-core/src/media_tests.rs`: cover empty-track participant registration and source filtering.
- `crates/lyre-web/src/api.rs`: add REST `POST /api/rooms/{room_id}/media-relay/participants`.
- `crates/lyre-web/src/api_media_tests.rs`: cover REST participant registration.
- `proto/lyre.ridl`: add `RegisterMediaParticipant`.
- `crates/lyre-web/src/webrpc/dto.rs`: add WebRPC DTOs for participant registration.
- `crates/lyre-web/src/webrpc/handlers.rs`: add WebRPC handler.
- `crates/lyre-web/src/webrpc/mod.rs`: route `/rpc/Lyre/RegisterMediaParticipant`.
- `crates/lyre-web/src/webrpc_tests/media_relay.rs`: cover WebRPC wrapper shape.
- `frontend/src/lib/lyre.gen.ts`: regenerate WebRPC client/types after RIDL change.
- `frontend/src/lib/api.ts`: add `registerMediaParticipant()` REST helper.
- `frontend/src/lib/api.test.ts`: cover participant helper serialization and generated contract shape.
- `frontend/src/lib/webrtc.ts`: export missing-input classifier and add receive-only peer-connection option.
- `frontend/src/lib/webrtc.test.ts`: cover receive-only transceiver behavior and exported classifier.
- `frontend/src/lib/server-media-audio.ts`: pass listen-only mode to peer connection creation.
- `frontend/src/lib/server-media-audio.test.ts`: ensure existing tests can construct sessions after input type changes if needed.
- `frontend/src/app/room/[roomId]/room-client.tsx`: implement listen-only startup, source filtering, and local VAD skip.
- `frontend/src/app/room/[roomId]/room-view.tsx`: render listen-only current-user state and disable local mute.
- `frontend/src/app/room/[roomId]/room-client-test-utils.ts`: extend mocks for `registerMediaParticipant`, empty streams, and `addTransceiver`.
- `frontend/src/app/room/[roomId]/room-client.test.tsx`: cover listen-only, permission-denied, and source filtering behavior.
- `frontend/messages/en-US.json` and `frontend/messages/zh-CN.json`: add Room messages.
- `docs/api-contracts.md`, `docs/media-architecture.md`, and `docs/roadmap.md`: document the new endpoint and completed behavior.

## Task 1: Core Media Relay Participant Semantics

**Files:**
- Modify: `crates/lyre-core/src/media.rs`
- Modify: `crates/lyre-core/src/media_tests.rs`

- [ ] **Step 1: Add failing tests for empty-track participants and audio-source filtering**

Add tests in `crates/lyre-core/src/media_tests.rs` near the existing participant/subscription tests:

```rust
#[test]
fn registering_participant_requires_active_relay() {
    let registry = MediaRelayRegistry::new();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("listener");

    assert_eq!(
        registry.register_participant(room_id.clone(), user_id.clone()),
        Err(MediaRelayError::Inactive { room_id })
    );
}

#[test]
fn registering_participant_creates_empty_track_listener() {
    let registry = MediaRelayRegistry::new();
    let room_id = RoomId::default_room();
    let user_id = UserId::from_external("listener");
    registry.start(room_id.clone(), StartMediaRelayRequest::default());

    let status = registry
        .register_participant(room_id.clone(), user_id.clone())
        .unwrap();

    assert_eq!(status.participants.len(), 1);
    assert_eq!(status.participants[0].user_id, user_id);
    assert!(status.participants[0].tracks.is_empty());
    assert_eq!(
        registry
            .subscriptions(&room_id, &UserId::from_external("listener"))
            .unwrap()
            .source_user_ids,
        Vec::<UserId>::new()
    );
}

#[test]
fn media_subscriptions_default_only_to_audio_track_sources() {
    let registry = MediaRelayRegistry::new();
    let room_id = RoomId::default_room();
    registry.start(room_id.clone(), StartMediaRelayRequest::default());
    registry
        .register_participant(room_id.clone(), UserId::from_external("listener"))
        .unwrap();
    registry
        .register_participant(room_id.clone(), UserId::from_external("silent"))
        .unwrap();
    registry
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: UserId::from_external("speaker"),
                track_id: "audio-main".to_owned(),
                kind: MediaTrackKind::Audio,
            },
        )
        .unwrap();
    registry
        .register_track(
            room_id.clone(),
            RegisterMediaTrackRequest {
                user_id: UserId::from_external("video_only"),
                track_id: "video-main".to_owned(),
                kind: MediaTrackKind::Video,
            },
        )
        .unwrap();

    let subscriptions = registry
        .subscriptions(&room_id, &UserId::from_external("listener"))
        .unwrap();

    assert_eq!(user_id_strings(&subscriptions.source_user_ids), ["speaker"]);
}

#[test]
fn media_subscriptions_reject_empty_track_sources() {
    let registry = MediaRelayRegistry::new();
    let room_id = RoomId::default_room();
    registry.start(room_id.clone(), StartMediaRelayRequest::default());
    registry
        .register_participant(room_id.clone(), UserId::from_external("listener"))
        .unwrap();
    registry
        .register_participant(room_id.clone(), UserId::from_external("silent"))
        .unwrap();

    assert_eq!(
        registry.update_subscriptions(
            room_id.clone(),
            UpdateMediaRelaySubscriptionsRequest {
                user_id: UserId::from_external("listener"),
                source_user_ids: vec![UserId::from_external("silent")],
            },
        ),
        Err(MediaRelayError::ParticipantNotFound {
            room_id,
            user_id: UserId::from_external("silent"),
        })
    );
}

#[test]
fn media_subscriptions_do_not_treat_empty_track_participants_as_subscribed_sources() {
    let registry = MediaRelayRegistry::new();
    let room_id = RoomId::default_room();
    registry.start(room_id.clone(), StartMediaRelayRequest::default());
    registry
        .register_participant(room_id.clone(), UserId::from_external("listener"))
        .unwrap();
    registry
        .register_participant(room_id.clone(), UserId::from_external("silent"))
        .unwrap();

    assert!(!registry
        .is_source_subscribed(
            &room_id,
            &UserId::from_external("listener"),
            &UserId::from_external("silent"),
        )
        .unwrap());
}
```

- [ ] **Step 2: Run focused core tests and verify failure**

Run:

```bash
cargo test -p lyre-core -- --nocapture
```

Expected: compile/test failure because `register_participant()` is not implemented and source filtering still treats all participants as sources.

- [ ] **Step 3: Implement core participant registration and source filtering**

In `crates/lyre-core/src/media.rs`:

- Add `pub fn register_participant(&self, room_id: RoomId, user_id: UserId) -> Result<MediaRelayRoomStatus, MediaRelayError>`.
- Require an active relay exactly like `register_track()`.
- Insert `user_id` into `room.participants` with an empty `DashMap`.
- Add a private helper such as `participant_has_audio_track(&DashMap<String, MediaTrackKind>) -> bool`.
- Update default source collection to include only remote participants whose track map contains `MediaTrackKind::Audio`.
- Update explicit source validation in `update_subscriptions()` so a present participant with no audio track returns `MediaRelayError::ParticipantNotFound`.
- Update `is_source_subscribed()` so a present participant with no audio track returns `Ok(false)`.

- [ ] **Step 4: Run focused core tests and verify pass**

Run:

```bash
cargo test -p lyre-core -- --nocapture
```

Expected: focused tests pass.

## Task 2: REST and WebRPC Participant Contract

**Files:**
- Modify: `crates/lyre-web/src/api.rs`
- Modify: `crates/lyre-web/src/api_media_tests.rs`
- Modify: `proto/lyre.ridl`
- Modify: `crates/lyre-web/src/webrpc/dto.rs`
- Modify: `crates/lyre-web/src/webrpc/handlers.rs`
- Modify: `crates/lyre-web/src/webrpc/mod.rs`
- Modify: `crates/lyre-web/src/webrpc_tests/media_relay.rs`
- Modify: `frontend/src/lib/lyre.gen.ts`

- [ ] **Step 1: Add failing REST/WebRPC tests and RIDL contract changes**

In `crates/lyre-web/src/api_media_tests.rs`, add a test after `media_relay_register_track_requires_active_relay`:

```rust
#[tokio::test]
async fn media_relay_register_participant_returns_empty_track_listener() {
    let app = router(AppState::default());
    let (user_id, access_token) = join_for_test(app.clone(), "Alice").await;
    app.clone()
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/media-relay/start",
            "{}".to_owned(),
            &access_token,
        ))
        .await
        .unwrap();

    let response = app
        .oneshot(post_json_with_auth(
            "/api/rooms/DEFAULT/media-relay/participants",
            serde_json::json!({ "user_id": user_id }).to_string(),
            &access_token,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response).await;
    assert_eq!(body["participants"][0]["user_id"], user_id);
    assert!(body["participants"][0]["tracks"].as_array().unwrap().is_empty());
}
```

In `crates/lyre-web/src/webrpc_tests/media_relay.rs`, after `StartMediaRelay`, call `RegisterMediaParticipant` and assert the wrapper:

```rust
    let participant_request = serde_json::json!({
        "roomID": "DEFAULT",
        "userID": user_id,
    });
    let participant = app
        .clone()
        .oneshot(rpc_post_auth("RegisterMediaParticipant", participant_request, token))
        .await
        .unwrap();
    let participant = body_json(participant).await;
    assert_eq!(participant["mediaRelay"]["participants"][0]["userID"], user_id);
    assert!(participant["mediaRelay"]["participants"][0]["tracks"]
        .as_array()
        .unwrap()
        .is_empty());
```

Update `proto/lyre.ridl`:

```ridl
struct RegisterMediaParticipantInput
  - userID: string
```

and add service method near `RegisterMediaTrack`:

```ridl
  # Documents POST /api/rooms/{room_id}/media-relay/participants; REST fetch remains the runtime transport in this increment.
  - RegisterMediaParticipant(roomID: string, userID: string) => (mediaRelay: MediaRelayRoomStatus)
```

- [ ] **Step 2: Run focused API tests and verify failure**

Run:

```bash
cargo test -p lyre-web api_media_tests::media_relay_register_participant_returns_empty_track_listener -- --nocapture
cargo test -p lyre-web webrpc_tests::media_relay::webrpc_media_relay_methods_use_auth_and_wrapper_shapes -- --nocapture
```

Expected: route/RPC compile or request failure because participant registration is not wired.

- [ ] **Step 3: Implement REST and WebRPC wiring**

In `crates/lyre-web/src/api.rs`:

- Add a route for `/api/rooms/{room_id}/media-relay/participants`.
- Add a request type if needed:

```rust
#[derive(Debug, Deserialize)]
struct RegisterMediaParticipantRequest {
    user_id: lyre_core::UserId,
}
```

- Add `register_media_participant()` handler using `authorize_room_user()` and `state.media_relays.register_participant()`.

In `crates/lyre-web/src/webrpc/dto.rs`, add:

```rust
#[derive(Debug, Deserialize)]
pub struct RegisterMediaParticipantRequest {
    #[serde(rename = "roomID")]
    pub room_id: String,
    #[serde(rename = "userID")]
    pub user_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterMediaParticipantResponse {
    pub media_relay: MediaRelayRoomStatus,
}
```

In `crates/lyre-web/src/webrpc/handlers.rs`, add handler equivalent to `register_media_track()` but calling `register_participant()`.

In `crates/lyre-web/src/webrpc/mod.rs`, add:

```rust
.route(
    "/rpc/Lyre/RegisterMediaParticipant",
    post(register_media_participant),
)
```

- [ ] **Step 4: Regenerate WebRPC TypeScript client**

Run:

```bash
cd frontend && npm run generate:webrpc
```

Expected: `frontend/src/lib/lyre.gen.ts` gains `registerMediaParticipant` request/response types and method.

- [ ] **Step 5: Run focused API tests and verify pass**

Run:

```bash
cargo test -p lyre-web api_media_tests::media_relay_register_participant_returns_empty_track_listener -- --nocapture
cargo test -p lyre-web webrpc_tests::media_relay::webrpc_media_relay_methods_use_auth_and_wrapper_shapes -- --nocapture
```

Expected: focused REST/WebRPC tests pass.

## Task 3: Frontend API and WebRTC Receive-Only Boundary

**Files:**
- Modify: `frontend/src/lib/api.ts`
- Modify: `frontend/src/lib/api.test.ts`
- Modify: `frontend/src/lib/webrtc.ts`
- Modify: `frontend/src/lib/webrtc.test.ts`
- Modify: `frontend/src/lib/server-media-audio.ts`
- Modify: `frontend/src/app/room/[roomId]/room-client-test-utils.ts`

- [ ] **Step 1: Add failing frontend API and WebRTC tests**

In `frontend/src/lib/api.test.ts`, import `registerMediaParticipant` and add:

```ts
  it("serializes media relay participant registration request body", async () => {
    await registerMediaParticipant("DEFAULT", "user_a", "token_a");

    expect(fetch).toHaveBeenCalledWith("https://api.example.test/api/rooms/DEFAULT/media-relay/participants", {
      method: "POST",
      headers: { authorization: "Bearer token_a", "content-type": "application/json" },
      body: JSON.stringify({ user_id: "user_a" })
    });
  });
```

Update generated contract samples in the same file with `RegisterMediaParticipantRequest`/`Response` only if the generated client exports those names in `frontend/src/lib/lyre.gen.ts`.

In `frontend/src/lib/webrtc.test.ts`:

- Add an `addTransceiver` mock to `MockPeerConnection`.
- Add:

```ts
  it("adds a receive-only audio transceiver for listen-only empty streams", () => {
    const emptyStream = {
      getAudioTracks: () => []
    } as unknown as MediaStream;

    createPeerConnection(
      [{ urls: ["stun:stun.example:3478"], username: null, credential: null }],
      emptyStream,
      { receiveOnlyAudio: true }
    );

    expect(addTrack).not.toHaveBeenCalled();
    expect(addTransceiver).toHaveBeenCalledWith("audio", { direction: "recvonly" });
  });

  it("does not add a receive-only transceiver when local audio tracks exist", () => {
    createPeerConnection(
      [{ urls: ["stun:stun.example:3478"], username: null, credential: null }],
      stream,
      { receiveOnlyAudio: true }
    );

    expect(addTrack).toHaveBeenCalledWith({ id: "track" }, stream);
    expect(addTransceiver).not.toHaveBeenCalled();
  });
```

Add exported missing-input classifier tests:

```ts
  it("classifies missing microphone errors narrowly", () => {
    expect(isMissingAudioInputError(Object.assign(new Error("missing"), { name: "NotFoundError" }))).toBe(true);
    expect(isMissingAudioInputError(Object.assign(new Error("missing"), {
      name: "OverconstrainedError",
      constraint: "deviceId"
    }))).toBe(true);
    expect(isMissingAudioInputError(Object.assign(new Error("denied"), { name: "NotAllowedError" }))).toBe(false);
  });
```

- [ ] **Step 2: Run focused frontend tests and verify failure**

Run:

```bash
cd frontend && pnpm test src/lib/api.test.ts src/lib/webrtc.test.ts --run
```

Expected: failures because the API helper, WebRTC option, and exported classifier are not implemented.

- [ ] **Step 3: Implement frontend API and WebRTC boundary**

In `frontend/src/lib/api.ts`, add:

```ts
export async function registerMediaParticipant(
  roomId: string,
  userId: string,
  accessToken: string
): Promise<MediaRelayRoomStatus> {
  const response = await fetch(`${mediaRelayUrl(roomId)}/participants`, {
    method: "POST",
    headers: { ...bearerHeaders(accessToken), "content-type": "application/json" },
    body: JSON.stringify({ user_id: userId })
  });
  return jsonOrThrow(response, "failed to register media participant");
}
```

In `frontend/src/lib/webrtc.ts`:

- Rename/export the private missing device classifier as `export function isMissingAudioInputError(error: unknown): boolean`.
- Keep `openLocalAudioStream()` using that classifier.
- Add:

```ts
type PeerConnectionOptions = {
  receiveOnlyAudio?: boolean;
};
```

- Change `createPeerConnection(iceServers, stream)` to accept `options: PeerConnectionOptions = {}`.
- Add `connection.addTransceiver("audio", { direction: "recvonly" })` only when `options.receiveOnlyAudio` is true and `stream.getAudioTracks().length === 0`.

In `frontend/src/lib/server-media-audio.ts`, add `listenOnly?: boolean` to `ServerMediaAudioSessionInput` and pass:

```ts
this.peer = createPeerConnection(input.iceServers, input.stream, { receiveOnlyAudio: input.listenOnly });
```

In `frontend/src/app/room/[roomId]/room-client-test-utils.ts`, add `registerMediaParticipant` to `apiMocks` and API module mock, reset it in `beforeEach`, and add `addTransceiver = vi.fn()` to `MockPeerConnection`.

- [ ] **Step 4: Run focused frontend tests and verify pass**

Run:

```bash
cd frontend && pnpm test src/lib/api.test.ts src/lib/webrtc.test.ts --run
```

Expected: focused frontend API/WebRTC tests pass.

## Task 4: Room Listen-Only Flow and UI

**Files:**
- Modify: `frontend/src/app/room/[roomId]/room-client.tsx`
- Modify: `frontend/src/app/room/[roomId]/room-view.tsx`
- Modify: `frontend/src/app/room/[roomId]/room-client-test-utils.ts`
- Modify: `frontend/src/app/room/[roomId]/room-client.test.tsx`
- Modify: `frontend/messages/en-US.json`
- Modify: `frontend/messages/zh-CN.json`

- [ ] **Step 1: Add failing RoomClient tests**

In `frontend/src/app/room/[roomId]/room-client.test.tsx`, add tests:

```tsx
  it("enters listen-only mode when no microphone is available", async () => {
    getUserMedia.mockRejectedValueOnce(Object.assign(new Error("Requested device not found"), { name: "NotFoundError" }));

    render(<RoomClient roomId="DEFAULT" />);

    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    expect(apiMocks.startMediaRelay).toHaveBeenCalledOnce();
    expect(apiMocks.registerMediaParticipant).toHaveBeenCalledWith("DEFAULT", "user_a", "token_a");
    expect(apiMocks.registerMediaTrack).not.toHaveBeenCalled();
    expect(apiMocks.updateMediaRelaySubscriptions).toHaveBeenCalledWith(
      "DEFAULT",
      "user_a",
      ["user_b", "user_c"],
      "token_a"
    );
    expect(peerConnections[0].addTransceiver).toHaveBeenCalledWith("audio", { direction: "recvonly" });
    expect(voiceActivityMock.instances).toHaveLength(0);
    expect(screen.getByText("Listening without microphone")).toBeInTheDocument();
    expect(screen.getByText("Listen only")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Mute" })).toBeDisabled();
  });

  it("does not enter listen-only mode for microphone permission denial", async () => {
    getUserMedia.mockRejectedValueOnce(Object.assign(new Error("Permission denied"), { name: "NotAllowedError" }));

    render(<RoomClient roomId="DEFAULT" />);

    await waitFor(() => expect(screen.getByText("Permission denied")).toBeInTheDocument());
    expect(apiMocks.registerMediaParticipant).not.toHaveBeenCalled();
    expect(apiMocks.registerMediaTrack).not.toHaveBeenCalled();
    expect(apiMocks.answerServerMediaOffer).not.toHaveBeenCalled();
  });

  it("ignores relay participants without audio tracks as subscription sources", async () => {
    apiMocks.getMediaRelay.mockResolvedValue({
      room_id: "DEFAULT",
      status: "active",
      mode: "media_relay",
      server_side_audio_processing: true,
      server_side_noise_cancelling: true,
      noise: defaultNoiseConfig,
      participants: [
        { user_id: "user_a", tracks: [{ track_id: "audio-main", kind: "audio" }] },
        { user_id: "user_b", tracks: [] },
        { user_id: "user_c", tracks: [{ track_id: "audio-main", kind: "audio" }] }
      ]
    });

    render(<RoomClient roomId="DEFAULT" />);
    await waitFor(() => expect(apiMocks.answerServerMediaOffer).toHaveBeenCalledOnce());

    expect(apiMocks.updateMediaRelaySubscriptions).toHaveBeenCalledWith(
      "DEFAULT",
      "user_a",
      ["user_c"],
      "token_a"
    );
  });
```

- [ ] **Step 2: Run focused RoomClient tests and verify failure**

Run:

```bash
cd frontend && pnpm test 'src/app/room/[roomId]/room-client.test.tsx' --run
```

Expected: new tests fail because listen-only flow and UI props are not implemented.

- [ ] **Step 3: Implement RoomClient listen-only flow**

In `frontend/src/app/room/[roomId]/room-client.tsx`:

- Import `registerMediaParticipant` and `isMissingAudioInputError`.
- Add `const [listenOnly, setListenOnly] = useState(false);`.
- In `refreshRelaySourceIds()`, derive source IDs with:

```ts
const sourceIds = status.participants
  .filter((participant) => participant.tracks.some((track) => track.kind === "audio"))
  .map((participant) => participant.user_id)
  .filter((userId) => userId !== currentUser.id);
```

- In audio startup, replace direct `stream = await openLocalAudioStream();` with a try/catch that creates `new MediaStream()` and sets `listenOnlySession = true` only for `isMissingAudioInputError(error)`.
- On initial relay registration, call `registerMediaParticipant()` when `listenOnlySession` is true; otherwise call `registerMediaTrack()` as today.
- Pass `listenOnly: listenOnlySession` into `ServerMediaAudioSession`.
- Skip `VoiceActivityDetector` when `listenOnlySession` is true.
- Set `setListenOnly(listenOnlySession)` after successful session startup.
- Set status to `listenOnlySession ? "Listening without microphone" : "Server relay audio connected"`.
- Reset `listenOnly` to false on non-socket startup failure, leave, unmount, and normal microphone startup success.
- In `toggleMuted()`, return early when `listenOnly` is true.

- [ ] **Step 4: Implement RoomView listen-only UI and messages**

In `frontend/src/app/room/[roomId]/room-view.tsx`:

- Add `listenOnly: boolean` prop.
- Disable local mute when `!audioStarted || listenOnly`.
- Pass the status label through `translateStatus()` by adding `listeningWithoutMicrophone`.
- Render current user subtitle as `listenOnly ? t("listenOnly") : t("localMicrophone")`.

In `frontend/messages/en-US.json` Room section:

```json
"listeningWithoutMicrophone": "Listening without microphone",
"listenOnly": "Listen only"
```

In `frontend/messages/zh-CN.json` Room section:

```json
"listeningWithoutMicrophone": "无麦克风，仅收听",
"listenOnly": "仅收听"
```

- [ ] **Step 5: Run focused RoomClient tests and verify pass**

Run:

```bash
cd frontend && pnpm test 'src/app/room/[roomId]/room-client.test.tsx' --run
```

Expected: focused room client tests pass.

## Task 5: Documentation, Formatting, and Full Verification

**Files:**
- Modify: `docs/api-contracts.md`
- Modify: `docs/media-architecture.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update API and media docs**

In `docs/api-contracts.md`:

- Add `POST /api/rooms/:room_id/media-relay/participants` to the route list.
- Add a short section explaining it registers the authenticated room user as an active relay participant with no tracks for listen-only receive paths.
- Update the subscriptions section so source users must be relay participants with an audio track.

In `docs/media-architecture.md`:

- Correct the current topology wording to server relay only.
- Add the participants endpoint to the media relay endpoint list.
- State that participants without tracks are listeners, not subscribable audio sources.

In `docs/roadmap.md`:

- Add a Completed bullet for listen-only no-microphone room audio.

- [ ] **Step 2: Run formatters**

Run:

```bash
cargo fmt
cd frontend && pnpm exec eslint --fix src/lib/api.ts src/lib/webrtc.ts src/lib/server-media-audio.ts 'src/app/room/[roomId]/room-client.tsx' 'src/app/room/[roomId]/room-view.tsx' 'src/app/room/[roomId]/room-client-test-utils.ts' src/lib/api.test.ts src/lib/webrtc.test.ts 'src/app/room/[roomId]/room-client.test.tsx'
```

- [ ] **Step 3: Run focused verification**

Run:

```bash
cargo test -p lyre-core -- --nocapture
cargo test -p lyre-web api_media_tests::media_relay_register_participant_returns_empty_track_listener -- --nocapture
cargo test -p lyre-web webrpc_tests::media_relay::webrpc_media_relay_methods_use_auth_and_wrapper_shapes -- --nocapture
cd frontend && pnpm test src/lib/api.test.ts src/lib/webrtc.test.ts 'src/app/room/[roomId]/room-client.test.tsx' --run
```

Expected: all focused tests pass.

- [ ] **Step 4: Run generated client check**

Run:

```bash
cd frontend && npm run generate:webrpc
git diff -- frontend/src/lib/lyre.gen.ts
```

Expected: generated client is up to date and any intended generated diff is present in the working tree.

- [ ] **Step 5: Run full frontend verification**

Run:

```bash
cd frontend && pnpm test -- --run
cd frontend && pnpm run typecheck
cd frontend && pnpm run lint
cd frontend && pnpm run build
```

Expected: all frontend tests, typecheck, lint, and build pass.

- [ ] **Step 6: Run required Rust verification**

Run from repository root:

```bash
cargo clippy --workspace --all-targets
cargo fmt --check
cargo nextest run --manifest-path "Cargo.toml" --workspace
```

Expected: clippy, rustfmt check, and nextest pass.
