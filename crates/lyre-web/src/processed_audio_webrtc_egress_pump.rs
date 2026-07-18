use crate::{media_egress::ProcessedAudioEgressFanout, media_runtime::WebMediaRuntime};
use async_trait::async_trait;
use dashmap::DashMap;
use lyre_core::{RoomId, UserId};
use lyre_webrtc::{
    ServerMediaConnectionStateSnapshot, ServerMediaEgressError, ServerMediaNegotiator,
    ServerMediaProcessedAudioFrame, ServerMediaSessionKey,
};
use std::{collections::HashSet, error::Error, sync::Arc};
use tokio::sync::{broadcast, mpsc, Notify};
use tokio_util::sync::CancellationToken;

const PER_RECIPIENT_EGRESS_QUEUE_CAPACITY: usize = 1;

#[derive(Debug)]
pub struct ProcessedAudioWebRtcEgressPump<S = ServerMediaNegotiator> {
    tasks: Arc<DashMap<RoomId, ProcessedAudioWebRtcEgressPumpTask>>,
    runtime: Arc<WebMediaRuntime>,
    fanout: Arc<ProcessedAudioEgressFanout>,
    sender: Arc<S>,
}

#[derive(Debug)]
struct ProcessedAudioWebRtcEgressPumpTask {
    id: Arc<()>,
    token: CancellationToken,
    pending_recipient_removals: Arc<PendingRecipientRemovals>,
    #[cfg(test)]
    completed: CancellationToken,
}

#[derive(Debug)]
struct PendingRecipientRemovals {
    user_ids: DashMap<UserId, ()>,
    notify: Notify,
}

struct RecipientWorker {
    sender: mpsc::Sender<EgressDelivery>,
    token: CancellationToken,
}

struct EgressDelivery {
    key: ServerMediaSessionKey,
    source_user_id: UserId,
    frame: ServerMediaProcessedAudioFrame,
}

#[async_trait]
pub trait ProcessedAudioWebRtcEgressSender: Send + Sync + 'static {
    async fn send_processed_audio_frame(
        &self,
        key: &ServerMediaSessionKey,
        source_user_id: &UserId,
        frame: ServerMediaProcessedAudioFrame,
    ) -> Result<usize, ServerMediaEgressError>;

    fn connection_state(
        &self,
        key: &ServerMediaSessionKey,
    ) -> Option<ServerMediaConnectionStateSnapshot>;
}

#[async_trait]
impl ProcessedAudioWebRtcEgressSender for ServerMediaNegotiator {
    async fn send_processed_audio_frame(
        &self,
        key: &ServerMediaSessionKey,
        source_user_id: &UserId,
        frame: ServerMediaProcessedAudioFrame,
    ) -> Result<usize, ServerMediaEgressError> {
        ServerMediaNegotiator::send_processed_audio_frame(self, key, source_user_id, frame).await
    }

    fn connection_state(
        &self,
        key: &ServerMediaSessionKey,
    ) -> Option<ServerMediaConnectionStateSnapshot> {
        ServerMediaNegotiator::connection_state(self, key)
    }
}

impl<S> ProcessedAudioWebRtcEgressPump<S>
where
    S: ProcessedAudioWebRtcEgressSender,
{
    pub fn new(
        runtime: Arc<WebMediaRuntime>,
        fanout: Arc<ProcessedAudioEgressFanout>,
        sender: Arc<S>,
    ) -> Self {
        Self {
            tasks: Arc::new(DashMap::new()),
            runtime,
            fanout,
            sender,
        }
    }

    pub fn start(&self, room_id: RoomId) {
        self.stop(&room_id);
        let mut frames = self.runtime.subscribe(&room_id);
        let fanout = Arc::clone(&self.fanout);
        let sender = Arc::clone(&self.sender);
        let token = CancellationToken::new();
        let task_token = token.clone();
        let task_room_id = room_id.clone();
        let pending_recipient_removals = Arc::new(PendingRecipientRemovals {
            user_ids: DashMap::new(),
            notify: Notify::new(),
        });
        let task_pending_recipient_removals = Arc::clone(&pending_recipient_removals);
        let task_id = Arc::new(());
        #[cfg(test)]
        let completed = CancellationToken::new();
        self.tasks.insert(
            room_id.clone(),
            ProcessedAudioWebRtcEgressPumpTask {
                id: Arc::clone(&task_id),
                token,
                pending_recipient_removals,
                #[cfg(test)]
                completed: completed.clone(),
            },
        );
        let handle = tokio::spawn(async move {
            let mut logged_empty_fanout = HashSet::<UserId>::new();
            let recipient_workers = DashMap::<UserId, RecipientWorker>::new();
            loop {
                tokio::select! {
                    () = task_token.cancelled() => break,
                    () = task_pending_recipient_removals.notify.notified() => {
                        let recipient_ids = task_pending_recipient_removals
                            .user_ids
                            .iter()
                            .map(|entry| entry.key().clone())
                            .collect::<Vec<_>>();
                        for recipient_id in recipient_ids {
                            task_pending_recipient_removals.user_ids.remove(&recipient_id);
                            logged_empty_fanout.remove(&recipient_id);
                            if let Some((_, worker)) = recipient_workers.remove(&recipient_id) {
                                worker.token.cancel();
                            }
                        }
                    }
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
                            if egress_frames.is_empty()
                                && logged_empty_fanout.insert(frame.user_id.clone())
                            {
                                tracing::info!(
                                    room_id = %frame.room_id,
                                    source_user_id = %frame.user_id,
                                    track_id = %frame.track_id,
                                    "processed audio WebRTC egress has no recipients"
                                );
                            }
                            for egress in egress_frames {
                                let recipient_id = egress.recipient_id.clone();
                                let key = ServerMediaSessionKey {
                                    room_id: egress.frame.room_id.clone(),
                                    user_id: recipient_id.clone(),
                                };
                                let frame = ServerMediaProcessedAudioFrame {
                                    sequence: egress.frame.sequence,
                                    rtp_timestamp: egress.frame.rtp_timestamp,
                                    sample_rate_hz: egress.frame.sample_rate_hz,
                                    channels: egress.frame.channels,
                                    samples: egress.frame.samples,
                                };
                                let delivery = EgressDelivery {
                                    key,
                                    source_user_id: egress.frame.user_id,
                                    frame,
                                };
                                let worker = recipient_workers
                                    .entry(recipient_id.clone())
                                    .or_insert_with(|| {
                                        spawn_recipient_worker(
                                            Arc::clone(&sender),
                                            task_token.clone(),
                                            recipient_id.clone(),
                                        )
                                    });
                                let worker_sender = worker.sender.clone();
                                drop(worker);
                                match worker_sender.try_send(delivery) {
                                    Ok(()) => {}
                                    Err(mpsc::error::TrySendError::Full(_)) => {}
                                    Err(mpsc::error::TrySendError::Closed(_)) => {
                                        if let Some((_, worker)) = recipient_workers.remove(&recipient_id) {
                                            worker.token.cancel();
                                        }
                                    }
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
            for worker in &recipient_workers {
                worker.token.cancel();
            }
        });
        let tasks = Arc::clone(&self.tasks);
        tokio::spawn(async move {
            if let Err(error) = handle.await {
                tracing::debug!(
                    error = format_args!("{error:#}"),
                    "processed audio WebRTC egress pump task ended with join error"
                );
            }
            tasks.remove_if(&room_id, |_, task| Arc::ptr_eq(&task.id, &task_id));
            #[cfg(test)]
            completed.cancel();
        });
    }

    pub fn remove_recipient(&self, room_id: &RoomId, user_id: &UserId) {
        if let Some(task) = self.tasks.get(room_id) {
            task.pending_recipient_removals
                .user_ids
                .insert(user_id.clone(), ());
            task.pending_recipient_removals.notify.notify_one();
        }
    }

    pub fn stop(&self, room_id: &RoomId) {
        if let Some((_, task)) = self.tasks.remove(room_id) {
            task.token.cancel();
        }
    }

    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    #[cfg(test)]
    pub(crate) async fn stop_and_wait_for_test(&self, room_id: &RoomId) {
        let Some((_, task)) = self.tasks.remove(room_id) else {
            return;
        };
        task.token.cancel();
        task.completed.cancelled().await;
    }
}

fn spawn_recipient_worker<S>(
    sender: Arc<S>,
    room_token: CancellationToken,
    recipient_id: UserId,
) -> RecipientWorker
where
    S: ProcessedAudioWebRtcEgressSender,
{
    let (tx, mut rx) = mpsc::channel::<EgressDelivery>(PER_RECIPIENT_EGRESS_QUEUE_CAPACITY);
    let token = room_token.child_token();
    let worker_token = token.clone();
    tokio::spawn(async move {
        let mut logged_egress_started = false;
        let mut logged_readiness_failures = HashSet::<EgressReadinessFailureKind>::new();
        loop {
            tokio::select! {
                () = worker_token.cancelled() => break,
                delivery = rx.recv() => {
                    let Some(delivery) = delivery else {
                        break;
                    };
                    let result = tokio::select! {
                        () = worker_token.cancelled() => break,
                        result = sender.send_processed_audio_frame(
                            &delivery.key,
                            &delivery.source_user_id,
                            delivery.frame,
                        ) => result,
                    };
                    match result {
                        Ok(packet_count) => {
                            if !logged_egress_started {
                                logged_egress_started = true;
                                tracing::info!(
                                    packet_count,
                                    room_id = %delivery.key.room_id,
                                    source_user_id = %delivery.source_user_id,
                                    recipient_user_id = %recipient_id,
                                    "processed audio WebRTC egress send started"
                                );
                            }
                        }
                        Err(error) => {
                            let connection_state = sender.connection_state(&delivery.key);
                            let terminal_failure = connection_state
                                .as_ref()
                                .is_some_and(|state| state.is_terminal_failure());
                            if warns_for_send_failure(&error, connection_state.as_ref()) {
                                tracing::warn!(
                                    error = format_args!("{error:#}"),
                                    error_source = ?error.source().map(|source| source.to_string()),
                                    peer_connection_state = ?connection_state
                                        .as_ref()
                                        .map(|state| state.peer_connection_state),
                                    ice_connection_state = ?connection_state
                                        .as_ref()
                                        .map(|state| state.ice_connection_state),
                                    room_id = %delivery.key.room_id,
                                    source_user_id = %delivery.source_user_id,
                                    recipient_user_id = %recipient_id,
                                    "processed audio WebRTC egress send failed"
                                );
                            } else if let Some(failure) = EgressReadinessFailureKind::from_error(&error) {
                                let first_readiness_failure = logged_readiness_failures.insert(failure);
                                if first_readiness_failure {
                                    tracing::debug!(
                                        error = format_args!("{error:#}"),
                                        peer_connection_state = ?connection_state
                                            .as_ref()
                                            .map(|state| state.peer_connection_state),
                                        ice_connection_state = ?connection_state
                                            .as_ref()
                                            .map(|state| state.ice_connection_state),
                                        room_id = %delivery.key.room_id,
                                        source_user_id = %delivery.source_user_id,
                                        recipient_user_id = %recipient_id,
                                        "processed audio WebRTC egress peer is not ready"
                                    );
                                }
                            }
                            if terminal_failure {
                                break;
                            }
                        }
                    }
                }
            }
        }
    });
    RecipientWorker { sender: tx, token }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum EgressReadinessFailureKind {
    PeerMissing,
    SourceNotNegotiated,
}

impl EgressReadinessFailureKind {
    fn from_error(error: &ServerMediaEgressError) -> Option<Self> {
        match error {
            ServerMediaEgressError::PeerMissing { .. } => Some(Self::PeerMissing),
            ServerMediaEgressError::SourceNotNegotiated { .. } => Some(Self::SourceNotNegotiated),
            ServerMediaEgressError::InvalidSampleRate { .. }
            | ServerMediaEgressError::InvalidChannels { .. }
            | ServerMediaEgressError::InvalidFrameSize { .. }
            | ServerMediaEgressError::EncoderInit { .. }
            | ServerMediaEgressError::Encode { .. }
            | ServerMediaEgressError::InvalidPayloadType { .. }
            | ServerMediaEgressError::WriteRtp { .. } => None,
        }
    }
}

pub(crate) fn warns_for_send_failure(
    error: &ServerMediaEgressError,
    connection_state: Option<&ServerMediaConnectionStateSnapshot>,
) -> bool {
    connection_state.is_some_and(ServerMediaConnectionStateSnapshot::is_terminal_failure)
        || EgressReadinessFailureKind::from_error(error).is_none()
}
