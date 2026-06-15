use crate::{media_runtime::WebMediaRuntime, server_media_runtime};
use dashmap::DashMap;
use lyre_core::RoomId;
use lyre_webrtc::{ServerMediaNegotiator, ServerMediaSessionKey};
use std::{sync::Arc, time::Duration};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub const SERVER_MEDIA_RUNTIME_PUMP_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug)]
pub struct ServerMediaRuntimePump {
    tasks: DashMap<ServerMediaSessionKey, ServerMediaRuntimePumpTask>,
    runtime: Arc<WebMediaRuntime>,
    negotiator: Arc<ServerMediaNegotiator>,
}

#[derive(Debug)]
struct ServerMediaRuntimePumpTask {
    token: CancellationToken,
    handle: JoinHandle<()>,
}

impl ServerMediaRuntimePump {
    pub fn new(runtime: Arc<WebMediaRuntime>, negotiator: Arc<ServerMediaNegotiator>) -> Self {
        Self {
            tasks: DashMap::new(),
            runtime,
            negotiator,
        }
    }

    pub fn start(&self, key: ServerMediaSessionKey) {
        self.stop(&key);
        let runtime = Arc::clone(&self.runtime);
        let negotiator = Arc::clone(&self.negotiator);
        let token = CancellationToken::new();
        let task_token = token.clone();
        let task_key = key.clone();
        let handle = tokio::spawn(async move {
            loop {
                if task_token.is_cancelled() {
                    break;
                }
                let process_result = {
                    let runtime = Arc::clone(&runtime);
                    let negotiator = Arc::clone(&negotiator);
                    let key = task_key.clone();
                    tokio::task::spawn_blocking(move || {
                        server_media_runtime::process_pcm_frames(&runtime, &negotiator, &key)
                    })
                    .await
                };
                let process_result = match process_result {
                    Ok(result) => result,
                    Err(error) => {
                        tracing::warn!(
                            error = format_args!("{error:#}"),
                            room_id = %task_key.room_id,
                            user_id = %task_key.user_id,
                            "server media runtime pump blocking task ended with join error"
                        );
                        continue;
                    }
                };
                if let Err(error) = process_result {
                    tracing::warn!(
                        error = format_args!("{error:#}"),
                        room_id = %task_key.room_id,
                        user_id = %task_key.user_id,
                        "server media runtime pump failed to process decoded PCM batch"
                    );
                }
                tokio::select! {
                    () = task_token.cancelled() => break,
                    () = tokio::time::sleep(SERVER_MEDIA_RUNTIME_PUMP_INTERVAL) => {}
                }
            }
        });
        self.tasks
            .insert(key, ServerMediaRuntimePumpTask { token, handle });
    }

    pub fn stop(&self, key: &ServerMediaSessionKey) {
        if let Some((_, task)) = self.tasks.remove(key) {
            task.token.cancel();
            tokio::spawn(async move {
                if let Err(error) = task.handle.await {
                    tracing::debug!(
                        error = format_args!("{error:#}"),
                        "server media runtime pump task ended with join error"
                    );
                }
            });
        }
    }

    pub fn stop_room(&self, room_id: &RoomId) {
        let keys = self
            .tasks
            .iter()
            .filter(|entry| &entry.key().room_id == room_id)
            .map(|entry| entry.key().clone())
            .collect::<Vec<_>>();
        for key in keys {
            self.stop(&key);
        }
    }

    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    #[cfg(test)]
    pub async fn stop_and_wait_for_test(&self, key: &ServerMediaSessionKey) {
        let Some((_, task)) = self.tasks.remove(key) else {
            return;
        };
        task.token.cancel();
        task.handle.await.unwrap();
    }

    #[cfg(test)]
    pub async fn stop_room_and_wait_for_test(&self, room_id: &RoomId) {
        let keys = self
            .tasks
            .iter()
            .filter(|entry| &entry.key().room_id == room_id)
            .map(|entry| entry.key().clone())
            .collect::<Vec<_>>();
        for key in keys {
            self.stop_and_wait_for_test(&key).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyre_core::{MediaRelayRegistry, UserId};
    use lyre_webrtc::{ServerMediaSessionRegistry, WebRtcStack};

    fn pump() -> ServerMediaRuntimePump {
        let relays = Arc::new(MediaRelayRegistry::new());
        let runtime = Arc::new(WebMediaRuntime::new(Arc::clone(&relays)));
        let sessions = Arc::new(ServerMediaSessionRegistry::new());
        let negotiator = Arc::new(ServerMediaNegotiator::new(WebRtcStack::new(), sessions));
        ServerMediaRuntimePump::new(runtime, negotiator)
    }

    fn key(user: &str) -> ServerMediaSessionKey {
        ServerMediaSessionKey {
            room_id: RoomId::default_room(),
            user_id: UserId::from_external(user),
        }
    }

    #[tokio::test]
    async fn start_replaces_existing_task_for_key() {
        let pump = pump();
        let key = key("user_01");

        pump.start(key.clone());
        assert_eq!(pump.task_count(), 1);
        pump.start(key.clone());
        assert_eq!(pump.task_count(), 1);

        pump.stop_and_wait_for_test(&key).await;
        assert_eq!(pump.task_count(), 0);
    }

    #[tokio::test]
    async fn stop_room_removes_only_matching_room_tasks() {
        let pump = pump();
        let default_key = key("user_01");
        let other_key = ServerMediaSessionKey {
            room_id: RoomId::parse_boundary("OTHER").unwrap(),
            user_id: UserId::from_external("user_02"),
        };

        pump.start(default_key.clone());
        pump.start(other_key.clone());
        pump.stop_room(&RoomId::default_room());

        assert_eq!(pump.task_count(), 1);
        pump.stop_and_wait_for_test(&other_key).await;
        assert_eq!(pump.task_count(), 0);
    }

    #[tokio::test]
    async fn stop_waits_for_cancelled_task_to_exit_for_tests() {
        let pump = pump();
        let key = key("user_01");

        pump.start(key.clone());
        pump.stop_and_wait_for_test(&key).await;

        assert_eq!(pump.task_count(), 0);
    }

    #[tokio::test]
    async fn stop_room_waits_for_cancelled_tasks_to_exit_for_tests() {
        let pump = pump();

        pump.start(key("user_01"));
        pump.start(key("user_02"));
        pump.stop_room_and_wait_for_test(&RoomId::default_room())
            .await;

        assert_eq!(pump.task_count(), 0);
    }
}
