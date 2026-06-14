use crate::{media_egress::ProcessedAudioEgressFanout, media_runtime::WebMediaRuntime};
use dashmap::DashMap;
use lyre_core::RoomId;
use lyre_webrtc::{ServerMediaNegotiator, ServerMediaProcessedAudioFrame, ServerMediaSessionKey};
use std::sync::Arc;
use tokio::{sync::broadcast, task::JoinHandle};
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub struct ProcessedAudioWebRtcEgressPump {
    tasks: DashMap<RoomId, ProcessedAudioWebRtcEgressPumpTask>,
    runtime: Arc<WebMediaRuntime>,
    fanout: Arc<ProcessedAudioEgressFanout>,
    negotiator: Arc<ServerMediaNegotiator>,
}

#[derive(Debug)]
struct ProcessedAudioWebRtcEgressPumpTask {
    token: CancellationToken,
    handle: JoinHandle<()>,
}

impl ProcessedAudioWebRtcEgressPump {
    pub fn new(
        runtime: Arc<WebMediaRuntime>,
        fanout: Arc<ProcessedAudioEgressFanout>,
        negotiator: Arc<ServerMediaNegotiator>,
    ) -> Self {
        Self {
            tasks: DashMap::new(),
            runtime,
            fanout,
            negotiator,
        }
    }

    pub fn start(&self, room_id: RoomId) {
        self.stop(&room_id);
        let mut frames = self.runtime.subscribe(&room_id);
        let fanout = Arc::clone(&self.fanout);
        let negotiator = Arc::clone(&self.negotiator);
        let token = CancellationToken::new();
        let task_token = token.clone();
        let task_room_id = room_id.clone();
        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = task_token.cancelled() => break,
                    result = frames.recv() => match result {
                        Ok(frame) => {
                            let egress_frames = match fanout.fanout(&frame) {
                                Ok(frames) => frames,
                                Err(error) => {
                                    tracing::warn!(
                                        error = format_args!("{error:#}"),
                                        room_id = %frame.room_id,
                                        user_id = %frame.user_id,
                                        "processed audio WebRTC egress fanout failed"
                                    );
                                    continue;
                                }
                            };
                            for egress in egress_frames {
                                let recipient_id = egress.recipient_id.clone();
                                let key = ServerMediaSessionKey {
                                    room_id: egress.frame.room_id.clone(),
                                    user_id: recipient_id.clone(),
                                };
                                let frame = ServerMediaProcessedAudioFrame {
                                    sequence: egress.frame.sequence,
                                    sample_rate_hz: egress.frame.sample_rate_hz,
                                    channels: egress.frame.channels,
                                    samples: egress.frame.samples,
                                };
                                if let Err(error) =
                                    negotiator.send_processed_audio_frame(&key, frame).await
                                {
                                    tracing::warn!(
                                        error = format_args!("{error:#}"),
                                        room_id = %key.room_id,
                                        source_user_id = %egress.frame.user_id,
                                        recipient_user_id = %recipient_id,
                                        "processed audio WebRTC egress send failed"
                                    );
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                skipped,
                                room_id = %task_room_id,
                                "processed audio WebRTC egress pump lagged"
                            );
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        });
        self.tasks.insert(
            room_id,
            ProcessedAudioWebRtcEgressPumpTask { token, handle },
        );
    }

    pub fn stop(&self, room_id: &RoomId) {
        if let Some((_, task)) = self.tasks.remove(room_id) {
            task.token.cancel();
            tokio::spawn(async move {
                if let Err(error) = task.handle.await {
                    tracing::debug!(
                        error = format_args!("{error:#}"),
                        "processed audio WebRTC egress pump task ended with join error"
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

#[cfg(test)]
mod tests {
    use super::*;
    use lyre_core::MediaRelayRegistry;
    use lyre_webrtc::{ServerMediaSessionRegistry, WebRtcStack};

    fn pump() -> ProcessedAudioWebRtcEgressPump {
        let relays = Arc::new(MediaRelayRegistry::new());
        let runtime = Arc::new(WebMediaRuntime::new(Arc::clone(&relays)));
        let fanout = Arc::new(ProcessedAudioEgressFanout::new(relays));
        let sessions = Arc::new(ServerMediaSessionRegistry::new());
        let negotiator = Arc::new(ServerMediaNegotiator::new(WebRtcStack::new(), sessions));
        ProcessedAudioWebRtcEgressPump::new(runtime, fanout, negotiator)
    }

    #[tokio::test]
    async fn start_replaces_existing_room_task() {
        let pump = pump();
        let room_id = RoomId::default_room();

        pump.start(room_id.clone());
        assert_eq!(pump.task_count(), 1);
        pump.start(room_id.clone());
        assert_eq!(pump.task_count(), 1);

        pump.stop_and_wait_for_test(&room_id).await;
        assert_eq!(pump.task_count(), 0);
    }

    #[tokio::test]
    async fn stop_removes_only_matching_room_task() {
        let pump = pump();
        let room_id = RoomId::default_room();
        let other_room_id = RoomId::parse_boundary("OTHER").unwrap();

        pump.start(room_id.clone());
        pump.start(other_room_id.clone());
        pump.stop(&room_id);

        assert_eq!(pump.task_count(), 1);
        pump.stop_and_wait_for_test(&other_room_id).await;
        assert_eq!(pump.task_count(), 0);
    }

    #[tokio::test]
    async fn stop_waits_for_cancelled_task_to_exit_for_tests() {
        let pump = pump();
        let room_id = RoomId::default_room();

        pump.start(room_id.clone());
        pump.stop_and_wait_for_test(&room_id).await;

        assert_eq!(pump.task_count(), 0);
    }
}
