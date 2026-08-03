//! Durable maildir-style message spool — the interim persistence
//! until `alo-store` (M5) exists, designed for one-way migration
//! into it.
//!
//! Layout: `tmp/` for in-progress writes, `new/` for accepted
//! messages. An entry is `<id>.eml` (message content, `Received:`
//! already stamped) plus `<id>.json` (the [`Envelope`] sidecar). Both
//! are written and fsynced in `tmp/`, then renamed into `new/` —
//! the `.json` rename LAST, so its presence is the commit marker: a
//! crash at any point leaves either garbage in `tmp/` or a complete
//! entry, never a half-visible message.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::envelope::Envelope;

/// Durable spool rooted at a directory.
#[derive(Debug)]
pub struct Spool {
    root: PathBuf,
    seq: AtomicU64,
}

impl Spool {
    /// Opens (creating if needed) a spool at `root`.
    ///
    /// # Errors
    /// I/O errors creating `tmp/` or `new/`.
    pub fn new(root: impl Into<PathBuf>) -> std::io::Result<Self> {
        let root = root.into();
        std::fs::create_dir_all(root.join("tmp"))?;
        std::fs::create_dir_all(root.join("new"))?;
        std::fs::create_dir_all(root.join("cur"))?;
        let spool = Self {
            root,
            seq: AtomicU64::new(0),
        };
        spool.recover_claims()?;
        spool.reap_crash_garbage()?;
        Ok(spool)
    }

    /// Removes leftovers of interrupted stores: everything in `tmp/`
    /// (writes that never committed) and any `.eml` in `new/` without
    /// its `.json` commit marker (crash between the two renames).
    /// Safe at startup only — nothing else writes while we open.
    fn reap_crash_garbage(&self) -> std::io::Result<()> {
        for entry in std::fs::read_dir(self.root.join("tmp"))? {
            let path = entry?.path();
            tracing::warn!(path = %path.display(), "reaping uncommitted spool write");
            std::fs::remove_file(&path)?;
        }
        for entry in std::fs::read_dir(self.root.join("new"))? {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "eml") {
                let marker = path.with_extension("json");
                if !marker.exists() {
                    tracing::warn!(path = %path.display(), "reaping markerless spool entry");
                    std::fs::remove_file(&path)?;
                }
            }
        }
        Ok(())
    }

    /// Generates a spool id unique within and across process runs:
    /// `epoch-nanos.pid.sequence`.
    pub fn next_id(&self) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        format!("{nanos}.{}.{seq}", std::process::id())
    }

    /// Durably stores a message and its envelope under `id`.
    ///
    /// # Errors
    /// Any I/O failure; the caller replies 451 (transient) and the
    /// message is NOT accepted.
    pub fn store(&self, id: &str, envelope: &Envelope, message: &[u8]) -> std::io::Result<()> {
        let tmp_eml = self.root.join("tmp").join(format!("{id}.eml"));
        let tmp_json = self.root.join("tmp").join(format!("{id}.json"));
        let new_eml = self.root.join("new").join(format!("{id}.eml"));
        let new_json = self.root.join("new").join(format!("{id}.json"));

        write_synced(&tmp_eml, message)?;
        let json = serde_json::to_vec_pretty(envelope).map_err(std::io::Error::other)?;
        write_synced(&tmp_json, &json)?;

        std::fs::rename(&tmp_eml, &new_eml)?;
        std::fs::rename(&tmp_json, &new_json)?; // commit marker
        sync_dir(&self.root.join("new"))?;
        Ok(())
    }

    /// Lists committed entry ids (those whose `.json` marker exists).
    ///
    /// # Errors
    /// I/O errors reading the directory.
    pub fn list(&self) -> std::io::Result<Vec<String>> {
        let mut ids = Vec::new();
        for entry in std::fs::read_dir(self.root.join("new"))? {
            let name = entry?.file_name();
            let name = name.to_string_lossy();
            if let Some(id) = name.strip_suffix(".json") {
                ids.push(id.to_owned());
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Reads a committed entry back from `new/`.
    ///
    /// # Errors
    /// I/O errors, or a sidecar that fails to deserialize.
    pub fn read(&self, id: &str) -> std::io::Result<(Envelope, Vec<u8>)> {
        self.read_from("new", id)
    }

    /// Reads a claimed entry back from `cur/`.
    ///
    /// # Errors
    /// I/O errors, or a sidecar that fails to deserialize.
    pub fn read_claimed(&self, id: &str) -> std::io::Result<(Envelope, Vec<u8>)> {
        self.read_from("cur", id)
    }

    fn read_from(&self, area: &str, id: &str) -> std::io::Result<(Envelope, Vec<u8>)> {
        let json = std::fs::read(self.root.join(area).join(format!("{id}.json")))?;
        let envelope: Envelope = serde_json::from_slice(&json).map_err(std::io::Error::other)?;
        let message = std::fs::read(self.root.join(area).join(format!("{id}.eml")))?;
        Ok((envelope, message))
    }

    /// Claims an entry for the queue: moves it `new/` → `cur/`. The
    /// `.json` moves FIRST so the entry atomically vanishes from
    /// [`Self::list`]; a crash mid-claim is healed by
    /// `recover_claims` at the next open.
    ///
    /// # Errors
    /// I/O errors; a failed claim leaves the entry recoverable.
    pub fn claim(&self, id: &str) -> std::io::Result<()> {
        let new = self.root.join("new");
        let cur = self.root.join("cur");
        std::fs::rename(
            new.join(format!("{id}.json")),
            cur.join(format!("{id}.json")),
        )?;
        std::fs::rename(new.join(format!("{id}.eml")), cur.join(format!("{id}.eml")))?;
        sync_dir(&cur)?;
        Ok(())
    }

    /// Lists claimed entry ids (in `cur/`, `.json` present).
    ///
    /// # Errors
    /// I/O errors reading the directory.
    pub fn list_claimed(&self) -> std::io::Result<Vec<String>> {
        let mut ids = Vec::new();
        for entry in std::fs::read_dir(self.root.join("cur"))? {
            let name = entry?.file_name();
            let name = name.to_string_lossy();
            if let Some(id) = name.strip_suffix(".json")
                && !id.ends_with(".state")
            {
                ids.push(id.to_owned());
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Durably writes the queue-state sidecar (`cur/<id>.state.json`)
    /// via the same tmp-then-rename pattern as message stores.
    ///
    /// # Errors
    /// I/O errors; on failure the previous state file is intact.
    pub fn write_state(&self, id: &str, state: &[u8]) -> std::io::Result<()> {
        let tmp = self.root.join("tmp").join(format!("{id}.state.json"));
        let dst = self.root.join("cur").join(format!("{id}.state.json"));
        write_synced(&tmp, state)?;
        std::fs::rename(&tmp, &dst)?;
        sync_dir(&self.root.join("cur"))
    }

    /// Reads the queue-state sidecar, `None` when not yet written.
    ///
    /// # Errors
    /// I/O errors other than not-found.
    pub fn read_state(&self, id: &str) -> std::io::Result<Option<Vec<u8>>> {
        match std::fs::read(self.root.join("cur").join(format!("{id}.state.json"))) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Removes a fully processed entry (delivered everywhere or
    /// bounced): message, envelope, and state.
    ///
    /// # Errors
    /// I/O errors; removal is idempotent per file.
    pub fn complete(&self, id: &str) -> std::io::Result<()> {
        let cur = self.root.join("cur");
        for name in [
            format!("{id}.state.json"),
            format!("{id}.eml"),
            format!("{id}.json"),
        ] {
            match std::fs::remove_file(cur.join(&name)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        sync_dir(&cur)
    }

    /// Heals interrupted claims: a `.json` in `cur/` whose `.eml` is
    /// still in `new/` (crash between the two renames) gets the move
    /// finished. Runs before garbage reaping so a half-claimed entry
    /// is never mistaken for an orphan.
    fn recover_claims(&self) -> std::io::Result<()> {
        let new = self.root.join("new");
        let cur = self.root.join("cur");
        for entry in std::fs::read_dir(&cur)? {
            let name = entry?.file_name();
            let name = name.to_string_lossy();
            if let Some(id) = name.strip_suffix(".json")
                && !id.ends_with(".state")
            {
                let cur_eml = cur.join(format!("{id}.eml"));
                let new_eml = new.join(format!("{id}.eml"));
                if !cur_eml.exists() && new_eml.exists() {
                    tracing::warn!(%id, "healing interrupted claim");
                    std::fs::rename(&new_eml, &cur_eml)?;
                }
            }
        }
        Ok(())
    }
}

/// Writes content and fsyncs the file before returning.
fn write_synced(path: &Path, content: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)?;
    file.write_all(content)?;
    file.sync_all()
}

/// Fsyncs a directory so renames are durable. Directory handles are
/// not syncable on Windows; there the rename metadata rides on the
/// volume flush behavior, which is the platform's best available
/// guarantee (dev-only platform — production runs on Linux).
fn sync_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(dir)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _keep = dir;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn test_envelope() -> Envelope {
        Envelope {
            helo: "client.example".to_owned(),
            peer: "192.0.2.9:52061".to_owned(),
            mail_from: Some("bob@example.org".to_owned()),
            rcpt_to: vec!["alice@example.com".to_owned()],
            received_at: "2026-07-25T12:00:00Z".to_owned(),
        }
    }

    #[test]
    fn store_then_read_round_trips_and_tmp_is_clean() {
        let dir = tempfile::tempdir().unwrap();
        let spool = Spool::new(dir.path()).unwrap();
        let id = spool.next_id();

        spool
            .store(&id, &test_envelope(), b"Subject: hi\r\n\r\nbody\r\n")
            .unwrap();

        let (envelope, message) = spool.read(&id).unwrap();
        assert_eq!(envelope, test_envelope());
        assert_eq!(message, b"Subject: hi\r\n\r\nbody\r\n");
        assert_eq!(spool.list().unwrap(), vec![id]);
        // Nothing may linger in tmp/ after a successful store.
        assert_eq!(
            std::fs::read_dir(dir.path().join("tmp")).unwrap().count(),
            0
        );
    }

    #[test]
    fn crash_garbage_is_reaped_on_open() {
        let dir = tempfile::tempdir().unwrap();
        // Simulate a committed entry, an uncommitted tmp write, and a
        // markerless .eml (crash between the two renames).
        {
            let spool = Spool::new(dir.path()).unwrap();
            let id = spool.next_id();
            spool.store(&id, &test_envelope(), b"kept\r\n").unwrap();
        }
        std::fs::write(dir.path().join("tmp").join("crash.eml"), b"junk").unwrap();
        std::fs::write(dir.path().join("new").join("orphan.eml"), b"junk").unwrap();

        let spool = Spool::new(dir.path()).unwrap();
        assert_eq!(spool.list().unwrap().len(), 1, "committed entry survives");
        assert_eq!(
            std::fs::read_dir(dir.path().join("tmp")).unwrap().count(),
            0,
            "tmp/ swept"
        );
        assert!(
            !dir.path().join("new").join("orphan.eml").exists(),
            "markerless .eml reaped"
        );
    }

    #[test]
    fn claim_state_complete_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let spool = Spool::new(dir.path()).unwrap();
        let id = spool.next_id();
        spool.store(&id, &test_envelope(), b"msg\r\n").unwrap();

        spool.claim(&id).unwrap();
        assert!(spool.list().unwrap().is_empty(), "gone from new/");
        assert_eq!(spool.list_claimed().unwrap(), vec![id.clone()]);
        let (envelope, message) = spool.read_claimed(&id).unwrap();
        assert_eq!(envelope, test_envelope());
        assert_eq!(message, b"msg\r\n");

        assert!(spool.read_state(&id).unwrap().is_none());
        spool.write_state(&id, b"{\"v\":1}").unwrap();
        assert_eq!(spool.read_state(&id).unwrap().unwrap(), b"{\"v\":1}");

        spool.complete(&id).unwrap();
        assert!(spool.list_claimed().unwrap().is_empty());
        assert!(spool.read_state(&id).unwrap().is_none());
    }

    #[test]
    fn interrupted_claim_is_healed_on_open() {
        let dir = tempfile::tempdir().unwrap();
        let id;
        {
            let spool = Spool::new(dir.path()).unwrap();
            id = spool.next_id();
            spool.store(&id, &test_envelope(), b"msg\r\n").unwrap();
            // Simulate a crash between the two claim renames: json
            // moved to cur/, eml still in new/.
            std::fs::rename(
                dir.path().join("new").join(format!("{id}.json")),
                dir.path().join("cur").join(format!("{id}.json")),
            )
            .unwrap();
        }
        let spool = Spool::new(dir.path()).unwrap();
        assert_eq!(spool.list_claimed().unwrap(), vec![id.clone()]);
        let (_envelope, message) = spool.read_claimed(&id).unwrap();
        assert_eq!(message, b"msg\r\n");
    }

    #[test]
    fn ids_are_unique() {
        let dir = tempfile::tempdir().unwrap();
        let spool = Spool::new(dir.path()).unwrap();
        let a = spool.next_id();
        let b = spool.next_id();
        assert_ne!(a, b);
    }
}
