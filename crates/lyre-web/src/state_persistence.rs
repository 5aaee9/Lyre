use anyhow::{Context, Result};
use lyre_core::PersistedRoomRegistry;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Debug, Clone)]
pub struct RoomStatePersistence {
    path: PathBuf,
    #[cfg(test)]
    fail_writes: bool,
}

static WRITE_COUNTER: AtomicU64 = AtomicU64::new(0);

impl RoomStatePersistence {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            #[cfg(test)]
            fail_writes: false,
        }
    }

    #[cfg(test)]
    pub fn always_fail_for_tests(path: PathBuf) -> Self {
        Self {
            path,
            fail_writes: true,
        }
    }

    pub fn load_registry(&self) -> Result<lyre_core::RoomRegistry> {
        if !self.path.exists() {
            if let Some(parent) = self.path.parent() {
                if !parent.as_os_str().is_empty() && !parent.exists() {
                    anyhow::bail!(
                        "Lyre room state parent directory does not exist: {}",
                        parent.display()
                    );
                }
            }
            return Ok(lyre_core::RoomRegistry::new());
        }
        let bytes = std::fs::read(&self.path).with_context(|| {
            format!("failed to read Lyre room state at {}", self.path.display())
        })?;
        let persisted: PersistedRoomRegistry =
            serde_json::from_slice(&bytes).with_context(|| {
                format!("failed to parse Lyre room state at {}", self.path.display())
            })?;
        Ok(lyre_core::RoomRegistry::from_persisted(persisted))
    }

    pub fn save_registry(&self, registry: &lyre_core::RoomRegistry) -> Result<()> {
        #[cfg(test)]
        if self.fail_writes {
            anyhow::bail!("forced Lyre room state write failure for tests");
        }
        let persisted = registry.to_persisted();
        let bytes =
            serde_json::to_vec_pretty(&persisted).context("failed to serialize Lyre room state")?;
        let temp_path = self.temp_path();
        std::fs::write(&temp_path, bytes).with_context(|| {
            format!(
                "failed to write Lyre room state temporary file at {}",
                temp_path.display()
            )
        })?;
        std::fs::rename(&temp_path, &self.path).with_context(|| {
            format!(
                "failed to replace Lyre room state at {}",
                self.path.display()
            )
        })?;
        Ok(())
    }

    fn temp_path(&self) -> PathBuf {
        let counter = WRITE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("lyre-state.json");
        self.path.with_file_name(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            counter
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lyre_core::{JoinRoomRequest, RoomId, RoomRegistry};

    fn unique_state_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "lyre-{name}-{}-{}.json",
            std::process::id(),
            WRITE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn missing_state_file_loads_empty_registry() {
        let persistence = RoomStatePersistence::new(unique_state_path("missing"));

        let registry = persistence.load_registry().unwrap();

        assert!(registry.to_persisted().rooms.is_empty());
    }

    #[test]
    fn missing_parent_directory_is_startup_error_with_context() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "lyre-missing-startup-parent-{}-{}",
            std::process::id(),
            WRITE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        path.push("state.json");
        let persistence = RoomStatePersistence::new(path);

        let error = persistence.load_registry().unwrap_err();

        assert!(format!("{error:#}").contains("Lyre room state parent directory does not exist"));
    }

    #[test]
    fn save_and_load_registry_round_trips_access_tokens() {
        let path = unique_state_path("roundtrip");
        let persistence = RoomStatePersistence::new(path.clone());
        let registry = RoomRegistry::new();
        let joined = registry.join(RoomId::default_room(), JoinRoomRequest::default());

        persistence.save_registry(&registry).unwrap();
        let restored = persistence.load_registry().unwrap();

        assert!(restored
            .validate_access_token(
                &RoomId::default_room(),
                &joined.user.id,
                &joined.access_token,
            )
            .is_ok());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn malformed_state_file_preserves_parse_context() {
        let path = unique_state_path("malformed");
        std::fs::write(&path, "{not json").unwrap();
        let persistence = RoomStatePersistence::new(path.clone());

        let error = persistence.load_registry().unwrap_err();

        assert!(format!("{error:#}").contains("failed to parse Lyre room state"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn missing_parent_directory_is_save_error_with_context() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "lyre-missing-parent-{}-{}",
            std::process::id(),
            WRITE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        path.push("state.json");
        let persistence = RoomStatePersistence::new(path);
        let registry = RoomRegistry::new();

        let error = persistence.save_registry(&registry).unwrap_err();

        assert!(format!("{error:#}").contains("failed to write Lyre room state"));
    }
}
