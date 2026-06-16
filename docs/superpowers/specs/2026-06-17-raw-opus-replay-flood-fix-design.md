# Raw Opus Replay Flood Fix Design

## Problem

When server-side noise processing is off, Lyre relays raw Opus RTP packets from each source to subscribed recipients. The current raw Opus egress pump tracks forwarding progress per source. If any subscribed recipient fails to receive a packet, the source-level cursor does not advance, so later pump iterations replay the same historical packets to every recipient that did succeed.

This can turn a normal 50 packets-per-second Opus stream into thousands of repeated packets per second for healthy recipients when a stale or missing recipient remains subscribed.

## Scope

Fix only the raw Opus WebRTC egress replay behavior in `crates/lyre-web/src/raw_opus_webrtc_egress_pump.rs` and add regression coverage in the existing server media egress tests.

Out of scope:
- Opus bitrate tuning.
- Processed-audio egress behavior.
- Browser WebRTC client behavior.
- Participant/session cleanup policy changes.
- Audio mixing or topology changes.

## Design

Keep raw Opus forwarding progress per source, but make the cursor represent packets observed by the pump rather than packets successfully delivered to every recipient. For real-time voice, old RTP packets should be dropped instead of retried. If one recipient is missing or fails, that recipient loses those packets; successful recipients must not see historical replay, and future pump ticks should only consider newer source packets.

The pump should continue using the existing received RTP packet snapshots and the existing subscription lookup. It should keep current subscription semantics: only subscribed non-source recipients receive packets, and unsubscribed recipients receive none.

## Acceptance Criteria

- A missing or failing subscribed recipient does not cause another subscribed recipient to receive duplicate historical raw Opus packets.
- Raw Opus packets that cannot be sent during their pump iteration are dropped rather than retried.
- Existing raw Opus forwarding still sends source packets to subscribed recipients.
- Existing unsubscribed-recipient behavior remains unchanged.
- The fix is covered by a regression test that fails with the old source-level cursor behavior.
- No new dependencies or broad refactors are introduced.

## Verification

Run the targeted Rust test that exercises raw Opus replay behavior, then run the crate/server media related tests affected by the change. Final verification should include formatting and clippy per repository guidance.
