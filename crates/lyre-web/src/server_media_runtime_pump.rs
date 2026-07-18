use crate::{
    media_runtime::WebMediaRuntime, server_media_lifecycle::ServerMediaLifecycle,
    server_media_runtime,
};
use dashmap::DashMap;
use lyre_core::RoomId;
use lyre_webrtc::{ServerMediaNegotiator, ServerMediaSessionKey};
use std::{fmt, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub const SERVER_MEDIA_RUNTIME_PUMP_INTERVAL: Duration = Duration::from_millis(20);

pub struct ServerMediaRuntimePump {
    tasks: Arc<DashMap<ServerMediaSessionKey, ServerMediaRuntimePumpTask>>,
    runtime: Arc<WebMediaRuntime>,
    negotiator: Arc<ServerMediaNegotiator>,
    lifecycle: Arc<ServerMediaLifecycle>,
    on_terminal_peer: Arc<dyn Fn(ServerMediaSessionKey) + Send + Sync>,
}

impl fmt::Debug for ServerMediaRuntimePump {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerMediaRuntimePump")
            .field("tasks", &self.tasks)
            .field("runtime", &self.runtime)
            .field("negotiator", &self.negotiator)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct ServerMediaRuntimePumpTask {
    id: Arc<()>,
    token: CancellationToken,
    completed: CancellationToken,
}

impl ServerMediaRuntimePump {
    pub fn new(runtime: Arc<WebMediaRuntime>, negotiator: Arc<ServerMediaNegotiator>) -> Self {
        Self::with_terminal_peer_handler(runtime, negotiator, Arc::new(|_| {}))
    }

    pub(crate) fn with_lifecycle(
        runtime: Arc<WebMediaRuntime>,
        negotiator: Arc<ServerMediaNegotiator>,
        lifecycle: Arc<ServerMediaLifecycle>,
    ) -> Self {
        Self::with_lifecycle_and_terminal_peer_handler(
            runtime,
            negotiator,
            lifecycle,
            Arc::new(|_| {}),
        )
    }

    pub fn with_terminal_peer_handler(
        runtime: Arc<WebMediaRuntime>,
        negotiator: Arc<ServerMediaNegotiator>,
        on_terminal_peer: Arc<dyn Fn(ServerMediaSessionKey) + Send + Sync>,
    ) -> Self {
        Self::with_lifecycle_and_terminal_peer_handler(
            runtime,
            negotiator,
            Arc::default(),
            on_terminal_peer,
        )
    }

    fn with_lifecycle_and_terminal_peer_handler(
        runtime: Arc<WebMediaRuntime>,
        negotiator: Arc<ServerMediaNegotiator>,
        lifecycle: Arc<ServerMediaLifecycle>,
        on_terminal_peer: Arc<dyn Fn(ServerMediaSessionKey) + Send + Sync>,
    ) -> Self {
        Self {
            tasks: Arc::new(DashMap::new()),
            runtime,
            negotiator,
            lifecycle,
            on_terminal_peer,
        }
    }

    pub fn start(&self, key: ServerMediaSessionKey) {
        self.stop(&key);
        let runtime = Arc::clone(&self.runtime);
        let negotiator = Arc::clone(&self.negotiator);
        let lifecycle = Arc::clone(&self.lifecycle);
        let on_terminal_peer = Arc::clone(&self.on_terminal_peer);
        let tasks = Arc::clone(&self.tasks);
        let token = CancellationToken::new();
        let task_token = token.clone();
        let task_key = key.clone();
        let task_id = Arc::new(());
        let runtime_task_id = Arc::clone(&task_id);
        let completed = CancellationToken::new();
        self.tasks.insert(
            key.clone(),
            ServerMediaRuntimePumpTask {
                id: Arc::clone(&task_id),
                token,
                completed: completed.clone(),
            },
        );
        let handle = tokio::spawn(async move {
            loop {
                if task_token.is_cancelled() {
                    break;
                }
                if negotiator
                    .connection_state(&task_key)
                    .is_some_and(|state| state.is_terminal_failure())
                {
                    let lifecycle_guard = tokio::select! {
                        () = task_token.cancelled() => break,
                        guard = lifecycle.lock(&task_key.room_id) => guard,
                    };
                    if task_token.is_cancelled()
                        || !task_is_current(&tasks, &task_key, &runtime_task_id)
                    {
                        drop(lifecycle_guard);
                        break;
                    }
                    tracing::info!(
                        room_id = %task_key.room_id,
                        user_id = %task_key.user_id,
                        "server media runtime pump closing terminal peer"
                    );
                    negotiator.close(&task_key);
                    runtime.release_source(&task_key.room_id, &task_key.user_id);
                    on_terminal_peer(task_key.clone());
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
        let tasks = Arc::clone(&self.tasks);
        tokio::spawn(async move {
            if let Err(error) = handle.await {
                tracing::debug!(
                    error = format_args!("{error:#}"),
                    "server media runtime pump task ended with join error"
                );
            }
            tasks.remove_if(&key, |_, task| Arc::ptr_eq(&task.id, &task_id));
            completed.cancel();
        });
    }

    pub fn stop(&self, key: &ServerMediaSessionKey) {
        if let Some((_, task)) = self.tasks.remove(key) {
            task.token.cancel();
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

    pub async fn stop_and_wait(&self, key: &ServerMediaSessionKey) {
        let Some((_, task)) = self.tasks.remove(key) else {
            return;
        };
        task.token.cancel();
        task.completed.cancelled().await;
    }

    pub async fn stop_room_and_wait(&self, room_id: &RoomId) {
        let keys = self
            .tasks
            .iter()
            .filter(|entry| &entry.key().room_id == room_id)
            .map(|entry| entry.key().clone())
            .collect::<Vec<_>>();
        for key in keys {
            self.stop_and_wait(&key).await;
        }
    }

    #[cfg(test)]
    pub async fn stop_and_wait_for_test(&self, key: &ServerMediaSessionKey) {
        self.stop_and_wait(key).await;
    }

    #[cfg(test)]
    pub async fn stop_room_and_wait_for_test(&self, room_id: &RoomId) {
        self.stop_room_and_wait(room_id).await;
    }
}

fn task_is_current(
    tasks: &DashMap<ServerMediaSessionKey, ServerMediaRuntimePumpTask>,
    key: &ServerMediaSessionKey,
    task_id: &Arc<()>,
) -> bool {
    tasks
        .get(key)
        .is_some_and(|task| Arc::ptr_eq(&task.id, task_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyre_core::{MediaRelayRegistry, UserId};
    use lyre_webrtc::{ServerMediaOffer, ServerMediaSessionRegistry, WebRtcStack};
    use tokio::{sync::Notify, time::timeout};

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

    #[tokio::test]
    async fn terminal_runtime_task_removes_its_registered_handle() {
        let relays = Arc::new(MediaRelayRegistry::new());
        let runtime = Arc::new(WebMediaRuntime::new(relays));
        let sessions = Arc::new(ServerMediaSessionRegistry::new());
        let negotiator = Arc::new(ServerMediaNegotiator::new(
            WebRtcStack::new(),
            Arc::clone(&sessions),
        ));
        let terminal = Arc::new(Notify::new());
        let terminal_handler = Arc::clone(&terminal);
        let pump = ServerMediaRuntimePump::with_terminal_peer_handler(
            runtime,
            Arc::clone(&negotiator),
            Arc::new(move |_| terminal_handler.notify_waiters()),
        );
        let key = key("user_01");
        let offerer = WebRtcStack::new().create_peer_connection().await.unwrap();
        let sdp = offerer.create_local_offer_for_test().await.unwrap();
        negotiator
            .answer_offer(ServerMediaOffer {
                room_id: key.room_id.clone(),
                user_id: key.user_id.clone(),
                audio_track_id: "audio-main".to_owned(),
                sdp,
            })
            .await
            .unwrap();

        let terminal_notified = terminal.notified();
        pump.start(key.clone());
        negotiator
            .peer_connection_for_test(&key)
            .unwrap()
            .close()
            .await
            .unwrap();
        timeout(Duration::from_secs(1), terminal_notified)
            .await
            .expect("terminal runtime task must observe the closed peer");

        for _ in 0..100 {
            if !pump.tasks.contains_key(&key) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        panic!("finished server-media runtime pump handle remained registered");
    }
}
