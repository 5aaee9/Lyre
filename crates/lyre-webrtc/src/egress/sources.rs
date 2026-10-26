use super::ServerMediaEgress;
use lyre_core::UserId;
use std::{error::Error, sync::Arc};
use webrtc::{media_stream::track_local::TrackLocal, peer_connection::PeerConnection};

impl ServerMediaEgress {
    /// Makes the peer connection send exactly `source_user_ids`: removes the send tracks of
    /// departed sources and adds tracks for new ones. Removed transceivers are reused for new
    /// sources, so the caller must renegotiate afterwards.
    pub(crate) async fn sync_sources(
        &self,
        peer_connection: &dyn PeerConnection,
        source_user_ids: &[UserId],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let (removed, unbound) = {
            let mut sources = self.write_sources();
            let departed = sources
                .keys()
                .filter(|source_user_id| !source_user_ids.contains(source_user_id))
                .cloned()
                .collect::<Vec<_>>();
            let removed = departed
                .iter()
                .filter_map(|source_user_id| sources.remove(source_user_id))
                .collect::<Vec<_>>();
            for source_user_id in source_user_ids {
                if !sources.contains_key(source_user_id) {
                    let source = self.new_source(source_user_id)?;
                    sources.insert(source_user_id.clone(), source);
                }
            }
            let unbound = sources
                .values()
                .filter(|source| source.sender.get().is_none())
                .cloned()
                .collect::<Vec<_>>();
            (removed, unbound)
        };
        for source in removed {
            if let Some(sender) = source.sender.get() {
                peer_connection.remove_track(sender).await?;
            }
        }
        for source in unbound {
            let sender = peer_connection
                .add_track(Arc::clone(&source.track) as Arc<dyn TrackLocal>)
                .await?;
            let _ = source.sender.set(sender);
        }
        Ok(())
    }
}
