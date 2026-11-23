//! Durable `.engram` persistence.
//!
//! Every mutation is a transaction: take an exclusive advisory lock on a sidecar lock file,
//! re-read the document from disk, apply the change, validate, and atomically replace the
//! file (write temp + fsync + rename). The Engram app and any number of `engram-mcp`
//! processes can therefore write the same file without losing each other's updates.

use crate::error::{EngramError, Result};
use crate::schema::{EngramDocument, FORMAT_MAGIC};
use crate::time::now_rfc3339;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Store {
    path: PathBuf,
}

/// A document together with the fingerprint of the bytes it was parsed from.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub document: EngramDocument,
    pub fingerprint: u64,
}

impl Store {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Directory containing the `.engram` file; relative repo roots resolve against it.
    pub fn base_dir(&self) -> PathBuf {
        self.path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Creates a new `.engram` file. Fails if one already exists at the path.
    pub fn create(&self, project_name: &str) -> Result<EngramDocument> {
        let _lock = self.lock()?;
        if self.path.exists() {
            return Err(EngramError::Conflict(format!("{} already exists", self.path.display())));
        }
        let mut doc = EngramDocument::new(project_name.trim());
        doc.validate()?;
        doc.revision = 1;
        self.write_atomic(&serialize(&doc)?)?;
        Ok(doc)
    }

    pub fn load(&self) -> Result<Snapshot> {
        let bytes = fs::read(&self.path).map_err(|e| EngramError::io(&self.path, e))?;
        Ok(Snapshot {
            document: parse(&self.path, &bytes)?,
            fingerprint: fingerprint(&bytes),
        })
    }

    /// Fingerprint of the bytes currently on disk, or `None` if the file is missing.
    pub fn disk_fingerprint(&self) -> Option<u64> {
        fs::read(&self.path).ok().map(|b| fingerprint(&b))
    }

    /// Runs `f` against the latest on-disk document under an exclusive lock and persists the
    /// result if it changed. Nothing is written when `f` fails or leaves the document as is.
    pub fn transact<T>(&self, f: impl FnOnce(&mut EngramDocument) -> Result<T>) -> Result<(T, Snapshot)> {
        let _lock = self.lock()?;
        let Snapshot { document: original, fingerprint: fp } = self.load()?;
        let mut doc = original.clone();
        let out = f(&mut doc)?;
        if doc == original {
            return Ok((out, Snapshot { document: doc, fingerprint: fp }));
        }
        doc.validate()?;
        doc.revision = original.revision + 1;
        doc.project.updated_at = now_rfc3339();
        let bytes = serialize(&doc)?;
        self.write_atomic(&bytes)?;
        Ok((out, Snapshot { document: doc, fingerprint: fingerprint(&bytes) }))
    }

    /// Replaces the whole document, refusing if the file moved past `expected_revision`.
    pub fn replace(&self, mut doc: EngramDocument, expected_revision: Option<u64>) -> Result<Snapshot> {
        self.transact(move |current| {
            if let Some(expected) = expected_revision {
                if current.revision != expected {
                    return Err(EngramError::Conflict(format!(
                        "file is at revision {} but the edit was based on revision {expected}",
                        current.revision
                    )));
                }
            }
            doc.revision = current.revision;
            doc.project.updated_at = current.project.updated_at.clone();
            *current = doc;
            Ok(())
        })
        .map(|(_, snap)| snap)
    }

    fn lock(&self) -> Result<LockGuard> {
        let name = self
            .path
            .file_name()
            .ok_or_else(|| EngramError::Invalid(format!("not a file path: {}", self.path.display())))?
            .to_string_lossy();
        let lock_path = self.base_dir().join(format!(".{name}.lock"));
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(|e| EngramError::io(&lock_path, e))?;
        file.lock().map_err(|e| EngramError::io(&lock_path, e))?;
        Ok(LockGuard(file))
    }

    fn write_atomic(&self, bytes: &[u8]) -> Result<()> {
        let dir = self.base_dir();
        let name = self.path.file_name().unwrap_or_default().to_string_lossy();
        let tmp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
        let result = (|| {
            let mut f = File::create(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
            if let Ok(meta) = fs::metadata(&self.path) {
                fs::set_permissions(&tmp, meta.permissions())?;
            }
            fs::rename(&tmp, &self.path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result.map_err(|e| EngramError::io(&self.path, e))
    }
}

struct LockGuard(File);

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

pub fn parse(path: &Path, bytes: &[u8]) -> Result<EngramDocument> {
    let doc: EngramDocument = serde_json::from_slice(bytes).map_err(|e| EngramError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    if doc.format != FORMAT_MAGIC {
        return Err(EngramError::Parse {
            path: path.to_path_buf(),
            message: format!("not an engram document (format = \"{}\")", doc.format),
        });
    }
    doc.validate().map_err(|e| EngramError::Parse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;
    Ok(doc)
}

/// Canonical on-disk form: two-space indented JSON with a trailing newline.
pub fn serialize(doc: &EngramDocument) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(doc)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// FNV-1a 64; stable across runs and platforms (unlike `DefaultHasher`).
pub fn fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{EntryStatus, MemoryCategory, MemoryEntry};

    fn entry(key: &str) -> MemoryEntry {
        MemoryEntry {
            key: key.into(),
            category: MemoryCategory::Rule,
            status: EntryStatus::Active,
            pinned: false,
            content: "line one\nline two".into(),
            tags: vec![],
            files: vec![],
            supersedes: None,
            superseded_by: None,
            created_at: now_rfc3339(),
            updated_at: now_rfc3339(),
        }
    }

    #[test]
    fn create_load_transact_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("p.engram"));
        let doc = store.create("Demo").unwrap();
        assert_eq!(doc.revision, 1);
        assert!(store.create("Demo").is_err(), "create must not clobber");

        let (_, snap) = store.transact(|d| Ok(d.memory.push(entry("a")))).unwrap();
        assert_eq!(snap.document.revision, 2);
        let loaded = store.load().unwrap();
        assert_eq!(loaded.document, snap.document);
        assert_eq!(loaded.fingerprint, snap.fingerprint);

        let raw = fs::read_to_string(store.path()).unwrap();
        assert!(raw.ends_with("}\n"));
        assert!(raw.contains("\"line one\",\n"), "multi-line text is stored as lines:\n{raw}");
    }

    #[test]
    fn noop_and_failed_transactions_do_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("p.engram"));
        store.create("Demo").unwrap();
        let before = store.disk_fingerprint();
        store.transact(|_| Ok(())).unwrap();
        assert_eq!(store.disk_fingerprint(), before);
        let err = store.transact(|d| {
            d.memory.push(entry("a"));
            Err::<(), _>(EngramError::Invalid("boom".into()))
        });
        assert!(err.is_err());
        assert_eq!(store.disk_fingerprint(), before);
        // Validation failures abort the write too.
        assert!(store.transact(|d| Ok(d.memory.extend([entry("a"), entry("a")]))).is_err());
        assert_eq!(store.load().unwrap().document.memory.len(), 0);
    }

    #[test]
    fn replace_detects_stale_revision() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("p.engram"));
        let doc = store.create("Demo").unwrap();
        store.transact(|d| Ok(d.memory.push(entry("from-agent")))).unwrap();
        let err = store.replace(doc.clone(), Some(doc.revision)).unwrap_err();
        assert_eq!(err.code(), "conflict");
        let snap = store.replace(doc, None).unwrap();
        assert_eq!(snap.document.revision, 3);
        assert!(snap.document.memory.is_empty());
    }

    #[test]
    fn concurrent_writers_never_lose_updates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("p.engram");
        Store::new(&path).create("Demo").unwrap();
        let handles: Vec<_> = (0..8)
            .map(|t| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let store = Store::new(path);
                    for i in 0..10 {
                        store.transact(|d| Ok(d.memory.push(entry(&format!("t{t}-{i}"))))).unwrap();
                    }
                })
            })
            .collect();
        handles.into_iter().for_each(|h| h.join().unwrap());
        let doc = Store::new(&path).load().unwrap().document;
        assert_eq!(doc.memory.len(), 80);
        assert_eq!(doc.revision, 81);
    }

    #[test]
    fn rejects_foreign_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.engram");
        fs::write(&path, br#"{"format":"other","version":1,"project":{"name":"x","created_at":"","updated_at":""}}"#).unwrap();
        assert_eq!(Store::new(&path).load().unwrap_err().code(), "parse");
    }
}
