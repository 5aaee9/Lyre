use lyre_core::RoomId;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};
use tokio::sync::{Mutex, MutexGuard};

#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(test)]
use tokio::sync::Notify;

const SERVER_MEDIA_LIFECYCLE_STRIPES: usize = 64;

#[derive(Debug)]
pub(crate) struct ServerMediaLifecycle {
    stripes: [Mutex<()>; SERVER_MEDIA_LIFECYCLE_STRIPES],
    #[cfg(test)]
    offer_commit_gate: OfferCommitGate,
}

impl Default for ServerMediaLifecycle {
    fn default() -> Self {
        Self {
            stripes: std::array::from_fn(|_| Mutex::new(())),
            #[cfg(test)]
            offer_commit_gate: OfferCommitGate::default(),
        }
    }
}

impl ServerMediaLifecycle {
    pub(crate) async fn lock(&self, room_id: &RoomId) -> MutexGuard<'_, ()> {
        self.stripes[room_stripe(room_id)].lock().await
    }

    #[cfg(test)]
    pub(crate) fn pause_next_offer_before_pump(&self) {
        self.offer_commit_gate
            .pause_next
            .store(true, Ordering::SeqCst);
    }

    #[cfg(test)]
    pub(crate) async fn wait_for_offer_before_pump(&self) {
        self.offer_commit_gate.reached.notified().await;
    }

    #[cfg(test)]
    pub(crate) fn resume_offer(&self) {
        self.offer_commit_gate.resume.notify_one();
    }

    #[cfg(test)]
    pub(crate) async fn pause_before_pump_if_requested(&self) {
        if self
            .offer_commit_gate
            .pause_next
            .swap(false, Ordering::SeqCst)
        {
            self.offer_commit_gate.reached.notify_one();
            self.offer_commit_gate.resume.notified().await;
        }
    }
}

fn room_stripe(room_id: &RoomId) -> usize {
    let mut hasher = DefaultHasher::new();
    room_id.hash(&mut hasher);
    hasher.finish() as usize % SERVER_MEDIA_LIFECYCLE_STRIPES
}

#[cfg(test)]
#[derive(Debug, Default)]
struct OfferCommitGate {
    pause_next: AtomicBool,
    reached: Notify,
    resume: Notify,
}
