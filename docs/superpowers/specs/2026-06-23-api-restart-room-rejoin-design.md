# API Restart Room Rejoin Design

## Problem

Room clients persist the anonymous room session in `sessionStorage` and reuse its `user.id` plus `accessToken` when the signalling WebSocket reconnects. After the API process restarts without restoring that in-memory token, the browser keeps reconnecting with credentials the API can no longer validate. The user remains stuck in the reconnecting state instead of joining the room again.

## Goals

- When a stored room session can no longer authenticate after an API restart, the client discards it, joins the room again, stores the fresh session, and reconnects signalling/audio.
- The recovery applies to WebSocket authentication failures and to authenticated media/API startup calls that fail because the stored access token is no longer valid.
- Explicit Leave must continue to clear the stored session and leave the room normally.
- Component unmount must remain local cleanup only and must not call leave-room or server-media cleanup endpoints.

## Non-Goals

- Do not add a backend compatibility fallback for old tokens.
- Do not add long-lived production session storage.
- Do not reintroduce peer-to-peer or mesh audio behavior.
- Do not change the visible room UI except for existing status text during recovery.

## Design

`RoomClient` will centralize room entry around a `joinFreshRoom()` helper and a `recoverExpiredSession()` helper. Initial entry still prefers a valid stored session for fast reconnects. If a WebSocket closes before opening, or if authenticated room/media setup reports an unauthorized response, `recoverExpiredSession()` clears `lyre.roomSession`, closes local audio/socket state, joins the room with the current nickname/noise settings, writes the new session, updates `currentUser`, `accessToken`, and `room`, then opens a new signalling socket.

The reconnect scheduler must always use the latest session reference, not the stale session captured when the component first mounted. A session recovery in progress must not launch overlapping joins. If another reconnect request arrives while recovery is running, it reuses the same recovery path once the in-flight work finishes.

Unauthorized detection will be client-side and minimal: errors whose message ends in `: 401` are treated as expired room sessions because this frontend's API helpers format authenticated failures that way. WebSocket close events cannot expose the HTTP upgrade status in browsers, so a socket that closes before `onopen` after using a stored session is treated as a stale-session candidate and recovers by rejoining.

Audio startup already serializes reconnects through existing refs. During session recovery, local audio sessions close and the new socket open path restarts server relay audio through the existing automatic audio startup effect.

## Acceptance Criteria

- A test covers a stored session whose first WebSocket closes before opening: the client clears the stale token, calls `joinRoom`, stores the new token, creates a new WebSocket, and reaches connected/server-relay audio state.
- A test covers an authenticated media startup call returning `: 401`: the client clears the stale token, rejoins, stores the new token, and retries signalling/audio with the new user/token.
- The existing stored-session websocket reconnect test still proves normal close-after-open reconnects reuse the same session without rejoining.
- Existing leave and unmount cleanup tests continue to pass.
- Frontend tests for `RoomClient` pass.

## Docs Impact

Update `docs/roadmap.md` Completed with the client recovery behavior after the implementation is approved.
