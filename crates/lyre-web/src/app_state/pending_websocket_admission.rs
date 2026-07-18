use super::AppState;
use crate::{error::ApiError, signalling::PeerSender};
use anyhow::Context;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use lyre_core::{JoinRoomRequest, RoomAccessToken, RoomId, RoomSnapshot, UserId};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio_util::sync::CancellationToken;

const PENDING_WEBSOCKET_ADMISSION_LEASE: Duration = Duration::from_secs(30);
const MAX_PENDING_WEBSOCKET_ADMISSIONS: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PendingWebSocketAdmissionKey {
    room_id: RoomId,
    user_id: UserId,
}

#[derive(Debug)]
struct PendingWebSocketAdmission {
    cancellation: CancellationToken,
    expires_at: DateTime<Utc>,
}

impl PendingWebSocketAdmission {
    fn new(expires_at: DateTime<Utc>) -> Self {
        Self {
            cancellation: CancellationToken::new(),
            expires_at,
        }
    }
}

#[derive(Debug)]
pub(super) struct PendingWebSocketAdmissions {
    leases: DashMap<PendingWebSocketAdmissionKey, Arc<PendingWebSocketAdmission>>,
    limit: AtomicUsize,
}

impl Default for PendingWebSocketAdmissions {
    fn default() -> Self {
        Self {
            leases: DashMap::new(),
            limit: AtomicUsize::new(MAX_PENDING_WEBSOCKET_ADMISSIONS),
        }
    }
}

impl PendingWebSocketAdmissions {
    fn is_full(&self) -> bool {
        self.leases.len() >= self.limit.load(Ordering::Relaxed)
    }

    fn insert(
        &self,
        key: PendingWebSocketAdmissionKey,
        expires_at: DateTime<Utc>,
    ) -> Arc<PendingWebSocketAdmission> {
        let lease = Arc::new(PendingWebSocketAdmission::new(expires_at));
        if let Some(previous) = self.leases.insert(key, Arc::clone(&lease)) {
            previous.cancellation.cancel();
        }
        lease
    }

    #[cfg(test)]
    fn current(
        &self,
        key: &PendingWebSocketAdmissionKey,
    ) -> Option<Arc<PendingWebSocketAdmission>> {
        self.leases.get(key).map(|lease| Arc::clone(lease.value()))
    }

    fn is_current(
        &self,
        key: &PendingWebSocketAdmissionKey,
        lease: &Arc<PendingWebSocketAdmission>,
    ) -> bool {
        self.leases
            .get(key)
            .is_some_and(|current| Arc::ptr_eq(current.value(), lease))
    }

    fn cancel(&self, key: &PendingWebSocketAdmissionKey) {
        if let Some((_, lease)) = self.leases.remove(key) {
            lease.cancellation.cancel();
        }
    }

    #[cfg(test)]
    fn set_limit_for_tests(&self, limit: usize) {
        self.limit.store(limit, Ordering::Relaxed);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.leases.len()
    }
}

impl AppState {
    pub async fn join_room_persisted(
        &self,
        room_id: RoomId,
        request: JoinRoomRequest,
    ) -> Result<lyre_core::JoinRoomResponse, ApiError> {
        let _guard = self.room_state_persistence_lock.lock().await;
        if self.pending_websocket_admissions.is_full() {
            return Err(ApiError::PendingWebSocketAdmissionsFull);
        }
        let expires_at = Utc::now()
            + chrono::Duration::from_std(PENDING_WEBSOCKET_ADMISSION_LEASE)
                .expect("pending websocket admission lease must fit chrono duration");
        let persistence = self.room_state_persistence.lock().await.clone();
        let response = if let Some(persistence) = persistence {
            let rollback = self.registry.to_persisted();
            let response = self.registry.join_pending_websocket_admission(
                room_id.clone(),
                request,
                expires_at,
            );
            if let Err(error) = persistence.save_registry(&self.registry) {
                self.registry.replace_with_persisted(rollback);
                self.metrics.record_persistence_failure();
                return Err(ApiError::from(error));
            }
            response
        } else {
            self.registry
                .join_pending_websocket_admission(room_id.clone(), request, expires_at)
        };
        self.track_pending_websocket_admission(room_id, response.user.id.clone(), expires_at);
        self.metrics.record_join();
        Ok(response)
    }

    pub async fn leave_room_persisted(
        &self,
        room_id: &RoomId,
        user_id: &UserId,
    ) -> Result<lyre_core::LeaveRoomResponse, ApiError> {
        let _guard = self.room_state_persistence_lock.lock().await;
        self.leave_room_persisted_locked(room_id, user_id).await
    }

    async fn leave_room_persisted_locked(
        &self,
        room_id: &RoomId,
        user_id: &UserId,
    ) -> Result<lyre_core::LeaveRoomResponse, ApiError> {
        let persistence = self.room_state_persistence.lock().await.clone();
        let response = if let Some(persistence) = persistence {
            let rollback = self.registry.to_persisted();
            let response = self.registry.leave(room_id, user_id);
            if let Err(error) = persistence.save_registry(&self.registry) {
                self.registry.replace_with_persisted(rollback);
                self.metrics.record_persistence_failure();
                return Err(ApiError::from(error));
            }
            response
        } else {
            self.registry.leave(room_id, user_id)
        };
        self.pending_websocket_admissions
            .cancel(&PendingWebSocketAdmissionKey {
                room_id: room_id.clone(),
                user_id: user_id.clone(),
            });
        if response.removed {
            self.peers.remove_peer(room_id, user_id);
            self.close_departed_user_server_media_state(room_id, user_id)
                .await;
            self.metrics.record_leave();
        }
        Ok(response)
    }

    pub async fn connect_room_socket(
        &self,
        room_id: RoomId,
        user_id: UserId,
        access_token: &RoomAccessToken,
        peer_tx: PeerSender,
    ) -> Result<RoomSnapshot, ApiError> {
        let _guard = self.room_state_persistence_lock.lock().await;
        self.registry
            .validate_access_token(&room_id, &user_id, access_token)
            .map_err(|_| ApiError::Unauthorized)?;
        let rollback = self.registry.to_persisted();
        let was_pending = self
            .registry
            .confirm_websocket_admission(&room_id, &user_id);
        if was_pending {
            if let Some(persistence) = self.room_state_persistence.lock().await.clone() {
                if let Err(error) = persistence.save_registry(&self.registry) {
                    self.registry.replace_with_persisted(rollback);
                    self.metrics.record_persistence_failure();
                    return Err(ApiError::from(error));
                }
            }
        }
        self.pending_websocket_admissions
            .cancel(&PendingWebSocketAdmissionKey {
                room_id: room_id.clone(),
                user_id: user_id.clone(),
            });
        Ok(self
            .peers
            .connect(&self.registry, room_id, user_id, peer_tx))
    }

    pub async fn disconnect_room_socket_if_current(
        &self,
        room_id: &RoomId,
        user_id: &UserId,
        peer_tx: &PeerSender,
    ) -> bool {
        let _guard = self.room_state_persistence_lock.lock().await;
        if !self.peers.remove_peer_sender(room_id, user_id, peer_tx) {
            return false;
        }
        self.disconnect_room_socket_locked(room_id, user_id).await;
        true
    }

    pub async fn disconnect_room_socket(&self, room_id: &RoomId, user_id: &UserId) {
        let _guard = self.room_state_persistence_lock.lock().await;
        self.peers.remove_peer(room_id, user_id);
        self.disconnect_room_socket_locked(room_id, user_id).await;
    }

    async fn disconnect_room_socket_locked(&self, room_id: &RoomId, user_id: &UserId) {
        match self.leave_room_persisted_locked(room_id, user_id).await {
            Ok(response) if response.removed => {
                self.peers.user_left(room_id, user_id);
            }
            Ok(_) => {}
            Err(ApiError::Persistence(error)) => {
                tracing::warn!(
                    error = format_args!("{error:#}"),
                    "failed to leave room after websocket disconnect"
                );
            }
            Err(error) => {
                tracing::warn!(
                    error = ?error,
                    "failed to leave room after websocket disconnect"
                );
            }
        }
    }

    fn track_pending_websocket_admission(
        &self,
        room_id: RoomId,
        user_id: UserId,
        expires_at: DateTime<Utc>,
    ) {
        self.track_pending_websocket_admission_on(
            room_id,
            user_id,
            expires_at,
            &tokio::runtime::Handle::current(),
        );
    }

    pub(super) fn restore_pending_websocket_admissions(
        &self,
        pending: Vec<(RoomId, UserId, DateTime<Utc>)>,
    ) -> anyhow::Result<()> {
        if pending.is_empty() {
            return Ok(());
        }
        let runtime = tokio::runtime::Handle::try_current()
            .context("restoring pending websocket admissions requires a Tokio runtime")?;
        for (room_id, user_id, expires_at) in pending {
            self.track_pending_websocket_admission_on(room_id, user_id, expires_at, &runtime);
        }
        Ok(())
    }

    fn track_pending_websocket_admission_on(
        &self,
        room_id: RoomId,
        user_id: UserId,
        expires_at: DateTime<Utc>,
        runtime: &tokio::runtime::Handle,
    ) {
        let key = PendingWebSocketAdmissionKey { room_id, user_id };
        let lease = self
            .pending_websocket_admissions
            .insert(key.clone(), expires_at);
        let state = self.clone();
        runtime.spawn(async move {
            let mut delay = (lease.expires_at - Utc::now())
                .to_std()
                .unwrap_or(Duration::ZERO);
            loop {
                tokio::select! {
                    _ = lease.cancellation.cancelled() => return,
                    _ = tokio::time::sleep(delay) => {}
                }
                match state.expire_pending_websocket_admission(&key, &lease).await {
                    Ok(_) => return,
                    Err(ApiError::Persistence(error)) => {
                        tracing::warn!(
                            error = format_args!("{error:#}"),
                            room_id = %key.room_id,
                            user_id = %key.user_id,
                            "failed to persist expired websocket admission cleanup; retrying"
                        );
                        delay = PENDING_WEBSOCKET_ADMISSION_LEASE;
                    }
                    Err(error) => {
                        tracing::warn!(
                            error = ?error,
                            room_id = %key.room_id,
                            user_id = %key.user_id,
                            "failed to expire websocket admission lease"
                        );
                        return;
                    }
                }
            }
        });
    }

    async fn expire_pending_websocket_admission(
        &self,
        key: &PendingWebSocketAdmissionKey,
        lease: &Arc<PendingWebSocketAdmission>,
    ) -> Result<bool, ApiError> {
        let _guard = self.room_state_persistence_lock.lock().await;
        if !self.pending_websocket_admissions.is_current(key, lease) {
            return Ok(false);
        }
        let response = self
            .leave_room_persisted_locked(&key.room_id, &key.user_id)
            .await?;
        if response.removed {
            self.peers.user_left(&key.room_id, &key.user_id);
        }
        Ok(response.removed)
    }

    #[cfg(test)]
    pub async fn expire_pending_websocket_admission_for_tests(
        &self,
        room_id: &RoomId,
        user_id: &UserId,
    ) -> Result<bool, ApiError> {
        let key = PendingWebSocketAdmissionKey {
            room_id: room_id.clone(),
            user_id: user_id.clone(),
        };
        let Some(lease) = self.pending_websocket_admissions.current(&key) else {
            return Ok(false);
        };
        self.expire_pending_websocket_admission(&key, &lease).await
    }

    #[cfg(test)]
    pub fn set_pending_websocket_admission_limit_for_tests(&self, limit: usize) {
        self.pending_websocket_admissions.set_limit_for_tests(limit);
    }

    #[cfg(test)]
    pub fn pending_websocket_admission_count_for_tests(&self) -> usize {
        self.pending_websocket_admissions.len()
    }
}
