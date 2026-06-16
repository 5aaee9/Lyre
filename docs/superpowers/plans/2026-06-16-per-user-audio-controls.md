# Per-User Audio Controls Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add per-user playback mute and 0-150% volume controls backed by server relay source-user subscriptions.

**Architecture:** Store local playback preferences in the existing Zustand settings store. Add a server media subscription request to the media relay control plane so each recipient subscribes to a set of source users. Before answering a browser server-media offer, emit one WebRTC outbound track per currently subscribed source user and let the frontend route each source user's track into its own Web Audio gain path.

**Tech Stack:** Rust `lyre-core`, `lyre-web`, `lyre-webrtc`; Axum REST and WebRPC documentation types; Next.js/React frontend; Zustand persistence; Vitest and Rust nextest.

---

## File Structure

- Modify `crates/lyre-core/src/media.rs` and `crates/lyre-core/src/media_tests.rs`: add relay subscription DTO/state plus default subscribe-all and filtering helpers.
- Modify `crates/lyre-web/src/api.rs`, `crates/lyre-web/src/api_media_tests.rs`, `crates/lyre-web/src/media_egress.rs`, `crates/lyre-web/src/media_egress_tests.rs`, `crates/lyre-web/src/raw_opus_webrtc_egress_pump.rs`, and raw/processed egress pump tests: expose the subscription route and use it in fanout.
- Modify `crates/lyre-webrtc/src/egress.rs`, `crates/lyre-webrtc/src/stack.rs`, `crates/lyre-webrtc/src/negotiation.rs`, `crates/lyre-web/src/api_server_media_state.rs`, `crates/lyre-web/src/processed_audio_webrtc_egress_pump.rs`, `crates/lyre-web/src/raw_opus_webrtc_egress_pump.rs`, and their tests: create subscribed source-user outbound tracks before answer generation and route outgoing packets by source user onto those tracks.
- Modify `proto/lyre.ridl` and align `frontend/src/lib/lyre.gen.ts`: run the generator when available; if generation fails, manually update the generated TypeScript contract output and record the generator failure in verification.
- Modify `frontend/src/lib/api.ts` and `frontend/src/lib/api.test.ts`: add `updateMediaRelaySubscriptions` using the aligned generated contract types where applicable.
- Modify `frontend/src/lib/settings-store.ts` and `frontend/src/lib/settings-store.test.ts`: add user audio preferences.
- Modify `frontend/src/lib/server-media-audio.ts` and `frontend/src/lib/server-media-audio.test.ts`: map source-user tracks to per-user Web Audio gain paths and apply settings.
- Modify `frontend/src/app/room/[roomId]/room-client.tsx`, `frontend/src/app/room/[roomId]/room-client.test.tsx`, and test utilities: render controls, update subscriptions, and reconnect server-media playback when needed.
- Modify `docs/api-contracts.md`: document the subscription REST/WebRPC contract.
- Modify `docs/roadmap.md`: record the completed feature after implementation approval.

## Task 1: Relay Subscription State

**Files:**
- Modify: `crates/lyre-core/src/media.rs`
- Modify: `crates/lyre-core/src/media_tests.rs`

- [x] **Step 1: Add failing tests for subscription state**

Add tests proving:
- new relay rooms default to all sources subscribed,
- updating recipient subscriptions stores a sorted source list,
- `is_source_subscribed(room, recipient, source)` returns true by default when no explicit subscription exists,
- it returns false when the recipient explicitly excludes that source,
- removing a participant removes their subscription entry and removes them from other recipients' source sets.

Run:

```bash
cargo test -p lyre-core media_subscriptions
```

Expected before implementation: tests fail because subscription types and methods do not exist.

- [x] **Step 2: Implement minimal subscription state**

Add these public DTOs in `media.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateMediaRelaySubscriptionsRequest {
    pub user_id: UserId,
    pub source_user_ids: Vec<UserId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaRelaySubscriptions {
    pub room_id: RoomId,
    pub user_id: UserId,
    pub source_user_ids: Vec<UserId>,
}
```

Add `subscriptions: DashMap<UserId, DashMap<UserId, ()>>` to `MediaRelayRoomState`.

Add methods:

```rust
pub fn update_subscriptions(
    &self,
    room_id: RoomId,
    request: UpdateMediaRelaySubscriptionsRequest,
) -> Result<MediaRelaySubscriptions, MediaRelayError>

pub fn subscriptions(
    &self,
    room_id: &RoomId,
    user_id: &UserId,
) -> Result<MediaRelaySubscriptions, MediaRelayError>

pub fn is_source_subscribed(
    &self,
    room_id: &RoomId,
    recipient_id: &UserId,
    source_user_id: &UserId,
) -> Result<bool, MediaRelayError>
```

Validate that the relay is active, the recipient exists, and every requested source user exists. Sort returned source user IDs. In `remove_participant`, remove the participant's subscription map and remove that user from all other maps.

- [x] **Step 3: Verify core tests**

Run:

```bash
cargo test -p lyre-core media_subscriptions
```

Expected: subscription tests pass.

## Task 2: Subscription API And Fanout Filtering

**Files:**
- Modify: `crates/lyre-web/src/api.rs`
- Modify: `crates/lyre-web/src/api_media_tests.rs`
- Modify: `crates/lyre-web/src/media_egress.rs`
- Modify: `crates/lyre-web/src/media_egress_tests.rs`
- Modify: `crates/lyre-web/src/raw_opus_webrtc_egress_pump.rs`
- Modify: `crates/lyre-web/src/raw_opus_webrtc_egress_pump_tests.rs` if present; otherwise add coverage in existing pump tests.
- Modify: `proto/lyre.ridl`

- [x] **Step 1: Add failing API and fanout tests**

Add tests proving:
- `POST /api/rooms/{room_id}/media-relay/subscriptions` requires bearer auth for `user_id`,
- it rejects unknown source users,
- it accepts an empty source list,
- it sorts and deduplicates source users in the response,
- it returns `{ room_id, user_id, source_user_ids }`,
- processed fanout excludes unsubscribed source-recipient pairs,
- raw Opus forwarding excludes unsubscribed source-recipient pairs.

Run:

```bash
cargo test -p lyre-web subscriptions
```

Expected before implementation: tests fail because route/filtering does not exist.

- [x] **Step 2: Implement route and filtering**

In `api.rs`, add:

```rust
.route(
    "/api/rooms/{room_id}/media-relay/subscriptions",
    post(update_media_relay_subscriptions),
)
```

The handler parses `RoomId`, authorizes `request.user_id`, and returns `state.media_relays.update_subscriptions(room_id, request)`.

In `media_egress.rs`, filter participants with:

```rust
self.relays
    .is_source_subscribed(&frame.room_id, &participant.user_id, &frame.user_id)?
```

In `raw_opus_webrtc_egress_pump.rs`, check the same subscription before sending a source packet to a recipient.

Update `proto/lyre.ridl` with DTOs and a documented RPC for subscription updates. Regenerate `frontend/src/lib/lyre.gen.ts` with:

```bash
npm --prefix frontend run generate:webrpc
```

If generation fails for an environment reason, manually align `frontend/src/lib/lyre.gen.ts` with the RIDL additions and preserve the generator error for the final verification report.

- [x] **Step 3: Verify API/fanout tests**

Run:

```bash
cargo test -p lyre-web subscriptions
```

Expected: new API and filtering tests pass.

## Task 3: Per-Source WebRTC Egress Tracks

**Files:**
- Modify: `crates/lyre-webrtc/src/egress.rs`
- Modify: `crates/lyre-webrtc/src/stack.rs`
- Modify: `crates/lyre-webrtc/src/negotiation.rs`
- Modify: `crates/lyre-webrtc/src/session.rs` if session status/debug types need to carry subscribed sources
- Modify: `crates/lyre-web/src/api_server_media_state.rs`
- Modify: `crates/lyre-web/src/processed_audio_webrtc_egress_pump.rs`
- Modify: `crates/lyre-web/src/raw_opus_webrtc_egress_pump.rs`
- Modify: `crates/lyre-webrtc/src/negotiation_tests.rs`
- Modify: `crates/lyre-webrtc/src/stack_egress_tests.rs`
- Modify: `crates/lyre-web/src/processed_audio_webrtc_egress_pump_tests.rs`
- Modify: raw Opus pump tests if present, otherwise add focused coverage in an existing `lyre-web` test module.

- [x] **Step 1: Add failing per-source track tests**

Add tests proving:
- source user IDs encode into `lyre-user:<encoded>:audio` track IDs,
- answering a server-media offer with a subscribed source list creates one outbound track per source before generating the answer,
- sending processed frames for two source users writes to the corresponding pre-created tracks,
- raw Opus packets also route through the correct source-user track,
- packets for a source user that was not part of the negotiated subscription are dropped or return a typed missing-source-track error instead of using a shared track.

Run:

```bash
cargo test -p lyre-webrtc egress_source
```

Expected before implementation: tests fail because egress creation and send methods do not accept source user IDs.

- [x] **Step 2: Implement source-user egress routing**

Change `ServerMediaEgress` from a single track/encoder to a map keyed by `lyre_core::UserId`. Keep one encoder per source user. Add a helper:

```rust
pub fn server_media_source_track_id(source_user_id: &lyre_core::UserId) -> String
```

Use a percent-encoding implementation local to the crate for non-unreserved bytes.

Change egress construction or setup to accept a source-user list before answer generation, and change send methods to accept `source_user_id: &UserId`:

```rust
send_processed_audio_frame(&self, source_user_id: &UserId, frame: ServerMediaProcessedAudioFrame)
send_opus_rtp_packet(&self, source_user_id: &UserId, packet: ServerMediaEgressRtpPacket)
```

Because tracks must be added before the answer, add source tracks during `answer_remote_offer` setup or immediately before `create_answer`. Change `ServerMediaNegotiator::answer_offer` to receive the subscribed source user IDs from the web layer, and have `api_server_media_state.rs` retrieve them from `MediaRelayRegistry` before calling the negotiator. Do not add tracks lazily after negotiation.

Update `ServerMediaNegotiator`, `processed_audio_webrtc_egress_pump.rs`, and `raw_opus_webrtc_egress_pump.rs` callers to pass source user IDs into the egress send methods.

- [x] **Step 3: Verify WebRTC tests**

Run:

```bash
cargo test -p lyre-webrtc egress_source
cargo test -p lyre-web processed_audio_webrtc_egress_pump
cargo test -p lyre-web raw_opus
cargo test -p lyre-web server_media
```

Expected: per-source egress tests pass and compile-affected `lyre-web` server-media/pump tests pass.

## Task 4: Frontend API And Settings Store

**Files:**
- Modify: `frontend/src/lib/api.ts`
- Modify: `frontend/src/lib/api.test.ts`
- Modify: `frontend/src/lib/settings-store.ts`
- Modify: `frontend/src/lib/settings-store.test.ts`

- [x] **Step 1: Add failing frontend tests**

Add Vitest tests proving:
- `updateMediaRelaySubscriptions("DEFAULT", "user_a", ["user_b"], "token_a")` POSTs to `/api/rooms/DEFAULT/media-relay/subscriptions` with bearer auth and snake_case body,
- default settings contain `userAudio: {}`,
- updating one user's audio settings persists under `lyre.settings`,
- volume is clamped to 0 and 150.

Run:

```bash
npm --prefix frontend test -- --run frontend/src/lib/api.test.ts frontend/src/lib/settings-store.test.ts
```

Expected before implementation: tests fail.

- [x] **Step 2: Implement API wrapper and store fields**

Add `UserAudioSettings`, `defaultUserAudioSettings`, `setUserAudioSettings`, and `clearUserAudioSettings` to `settings-store.ts`.

Add `updateMediaRelaySubscriptions` to `api.ts`:

```ts
export async function updateMediaRelaySubscriptions(
  roomId: string,
  userId: string,
  sourceUserIds: string[],
  accessToken: string
): Promise<MediaRelaySubscriptions>
```

Clamp volume in the store action before persisting.

- [x] **Step 3: Verify frontend library tests**

Run:

```bash
npm --prefix frontend test -- --run frontend/src/lib/api.test.ts frontend/src/lib/settings-store.test.ts
```

Expected: frontend API/store tests pass.

## Task 5: Frontend Playback Session And Room UI

**Files:**
- Modify: `frontend/src/lib/server-media-audio.ts`
- Modify: `frontend/src/lib/server-media-audio.test.ts`
- Modify: `frontend/src/app/room/[roomId]/room-client.tsx`
- Modify: `frontend/src/app/room/[roomId]/room-client.test.tsx`
- Modify: `frontend/src/app/room/[roomId]/room-client-test-utils.ts`

- [x] **Step 1: Add failing playback/UI tests**

Add tests proving:
- `ServerMediaAudioSession` creates one Web Audio `MediaStreamAudioSourceNode -> GainNode -> destination` path per valid `lyre-user:<encoded>:audio` remote track,
- invalid track IDs are reported and not played,
- `setUserAudioSettings` applies muted and gain values from 0 through 1.5 to the matching `GainNode`,
- remote room users render Mute/Unmute and 0-150 range controls,
- current user row does not render per-user playback controls,
- persisted muted users are excluded before first server-media connect,
- muting a remote user calls `updateMediaRelaySubscriptions` without recreating local microphone mute state,
- if `updateMediaRelaySubscriptions` fails, the room status shows the error and the session is not recreated,
- when audio is connected, subscription changes close and recreate the server media session through existing offer flow,
- when a new remote user joins, the frontend updates the subscription and recreates connected audio unless that user is persisted muted,
- when a remote user leaves, controls and playback for that user are removed.

Run:

```bash
npm --prefix frontend test -- --run frontend/src/lib/server-media-audio.test.ts frontend/src/app/room/[roomId]/room-client.test.tsx
```

Expected before implementation: tests fail.

- [x] **Step 2: Implement playback/UI**

In `server-media-audio.ts`, parse source user IDs from track IDs and manage a map:

```ts
type RemotePlayback = {
  stream: MediaStream;
  source: MediaStreamAudioSourceNode;
  gain: GainNode;
};
```

Create an `AudioContext` lazily when the first valid remote track arrives. Connect each source through its gain node to `audioContext.destination`. Apply `gain.gain.value = settings.muted ? 0 : settings.volumePercent / 100`, allowing 150% to become `1.5`. Disconnect source and gain nodes and close the session-owned `AudioContext` on `close()`.

In `room-client.tsx`, compute subscribed source IDs from current room users excluding current user and excluding users with `muted: true`. On mute/unmute, persist settings, call `updateMediaRelaySubscriptions`, and if audio is active close/recreate the server-media session with `updateRelay: true`.

- [x] **Step 3: Verify frontend playback/UI tests**

Run:

```bash
npm --prefix frontend test -- --run frontend/src/lib/server-media-audio.test.ts frontend/src/app/room/[roomId]/room-client.test.tsx
```

Expected: playback/UI tests pass.

## Task 6: Docs And Full Verification

**Files:**
- Modify: `docs/api-contracts.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update API docs and roadmap**

Document `POST /api/rooms/{room_id}/media-relay/subscriptions` in `docs/api-contracts.md` with auth, request, response, idempotency, and validation behavior. Add a completed roadmap bullet for per-user playback mute/volume controls with server relay subscriptions.

- [ ] **Step 2: Run formatting and verification**

Run:

```bash
cargo fmt
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --manifest-path "Cargo.toml" --workspace
npm --prefix frontend run lint
npm --prefix frontend run typecheck
npm --prefix frontend test -- --run
```

Expected: all commands exit 0.

- [ ] **Step 3: Review final diff**

Run:

```bash
git status --short
git diff --stat
git diff
```

Expected: changes are limited to the files listed in this plan plus generated WebRPC output if regeneration is used.
