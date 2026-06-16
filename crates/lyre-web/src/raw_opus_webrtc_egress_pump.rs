use dashmap::DashMap;
use lyre_core::{MediaRelayError, NoiseProvider, RoomId};
use lyre_webrtc::{ServerMediaEgressRtpPacket, ServerMediaNegotiator, ServerMediaSessionKey};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

const RAW_OPUS_EGRESS_PUMP_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug)]
pub struct RawOpusWebRtcEgressPump {
    tasks: DashMap<RoomId, RawOpusWebRtcEgressPumpTask>,
    relays: Arc<lyre_core::MediaRelayRegistry>,
    negotiator: Arc<ServerMediaNegotiator>,
}

#[derive(Debug)]
struct RawOpusWebRtcEgressPumpTask {
    token: CancellationToken,
    handle: JoinHandle<()>,
}

impl RawOpusWebRtcEgressPump {
    pub fn new(
        relays: Arc<lyre_core::MediaRelayRegistry>,
        negotiator: Arc<ServerMediaNegotiator>,
    ) -> Self {
        Self {
            tasks: DashMap::new(),
            relays,
            negotiator,
        }
    }

    pub fn start(&self, room_id: RoomId) {
        self.stop(&room_id);
        let relays = Arc::clone(&self.relays);
        let negotiator = Arc::clone(&self.negotiator);
        let token = CancellationToken::new();
        let task_token = token.clone();
        let task_room_id = room_id.clone();
        let handle = tokio::spawn(async move {
            let mut forwarded = HashMap::<ServerMediaSessionKey, usize>::new();
            loop {
                if task_token.is_cancelled() {
                    break;
                }
                if room_uses_raw_opus_relay(&relays, &task_room_id) {
                    forward_new_packets(&relays, &negotiator, &task_room_id, &mut forwarded).await;
                }
                tokio::select! {
                    () = task_token.cancelled() => break,
                    () = tokio::time::sleep(RAW_OPUS_EGRESS_PUMP_INTERVAL) => {}
                }
            }
        });
        self.tasks
            .insert(room_id, RawOpusWebRtcEgressPumpTask { token, handle });
    }

    pub fn stop(&self, room_id: &RoomId) {
        if let Some((_, task)) = self.tasks.remove(room_id) {
            task.token.cancel();
            tokio::spawn(async move {
                if let Err(error) = task.handle.await {
                    tracing::debug!(
                        error = format_args!("{error:#}"),
                        "raw Opus WebRTC egress pump task ended with join error"
                    );
                }
            });
        }
    }

    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    #[cfg(test)]
    pub async fn stop_and_wait_for_test(&self, room_id: &RoomId) {
        let Some((_, task)) = self.tasks.remove(room_id) else {
            return;
        };
        task.token.cancel();
        task.handle.await.unwrap();
    }
}

fn room_uses_raw_opus_relay(relays: &lyre_core::MediaRelayRegistry, room_id: &RoomId) -> bool {
    relays.status(room_id.clone()).noise.provider == NoiseProvider::Off
}

async fn forward_new_packets(
    relays: &lyre_core::MediaRelayRegistry,
    negotiator: &ServerMediaNegotiator,
    room_id: &RoomId,
    forwarded: &mut HashMap<ServerMediaSessionKey, usize>,
) {
    let Ok(participants) = relays.active_participants(room_id) else {
        return;
    };
    let keys = participants
        .iter()
        .map(|participant| ServerMediaSessionKey {
            room_id: room_id.clone(),
            user_id: participant.user_id.clone(),
        })
        .collect::<Vec<_>>();
    for source_key in &keys {
        let packets = negotiator.received_rtp_packets(source_key);
        let packet_count = packets.len();
        let start = forwarded.get(source_key).copied().unwrap_or_default();
        let recipient_keys = match subscribed_recipient_keys(relays, room_id, source_key, &keys) {
            Ok(keys) => keys,
            Err(error) => {
                tracing::debug!(
                    error = format_args!("{error:#}"),
                    room_id = %room_id,
                    source_user_id = %source_key.user_id,
                    "raw Opus WebRTC egress subscription lookup failed"
                );
                continue;
            }
        };
        let mut send_failed = false;
        for packet in packets.into_iter().skip(start) {
            for recipient_key in &recipient_keys {
                if let Err(error) = negotiator
                    .send_opus_rtp_packet(
                        recipient_key,
                        &source_key.user_id,
                        ServerMediaEgressRtpPacket {
                            sequence_number: packet.sequence_number,
                            timestamp: packet.timestamp,
                            payload_type: packet.payload_type,
                            payload: packet.payload.clone(),
                        },
                    )
                    .await
                {
                    tracing::debug!(
                        error = format_args!("{error:#}"),
                        room_id = %room_id,
                        source_user_id = %source_key.user_id,
                        recipient_user_id = %recipient_key.user_id,
                        sequence_number = packet.sequence_number,
                        "raw Opus WebRTC egress send failed"
                    );
                    send_failed = true;
                }
            }
        }
        if !send_failed {
            forwarded.insert(source_key.clone(), packet_count);
        }
    }
}

fn subscribed_recipient_keys(
    relays: &lyre_core::MediaRelayRegistry,
    room_id: &RoomId,
    source_key: &ServerMediaSessionKey,
    keys: &[ServerMediaSessionKey],
) -> Result<Vec<ServerMediaSessionKey>, MediaRelayError> {
    let mut recipient_keys = Vec::new();
    for recipient_key in keys.iter().filter(|key| *key != source_key) {
        if relays.is_source_subscribed(room_id, &recipient_key.user_id, &source_key.user_id)? {
            recipient_keys.push(recipient_key.clone());
        }
    }
    Ok(recipient_keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyre_core::{MediaTrackKind, RegisterMediaTrackRequest, StartMediaRelayRequest, UserId};

    #[test]
    fn subscribed_recipient_keys_returns_registry_errors() {
        let relays = lyre_core::MediaRelayRegistry::new();
        let room_id = RoomId::default_room();
        let source_key = ServerMediaSessionKey {
            room_id: room_id.clone(),
            user_id: UserId::from_external("source"),
        };
        let recipient_key = ServerMediaSessionKey {
            room_id: room_id.clone(),
            user_id: UserId::from_external("recipient"),
        };
        relays.start(room_id.clone(), StartMediaRelayRequest::default());
        relays
            .register_track(
                room_id.clone(),
                RegisterMediaTrackRequest {
                    user_id: source_key.user_id.clone(),
                    track_id: "audio-main".to_owned(),
                    kind: MediaTrackKind::Audio,
                },
            )
            .unwrap();

        assert_eq!(
            subscribed_recipient_keys(
                &relays,
                &room_id,
                &source_key,
                &[source_key.clone(), recipient_key.clone()],
            ),
            Err(MediaRelayError::ParticipantNotFound {
                room_id,
                user_id: recipient_key.user_id,
            })
        );
    }
}
