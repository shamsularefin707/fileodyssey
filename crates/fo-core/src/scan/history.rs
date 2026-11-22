//! Scan history: a JSON list in the app's data folder, newest first.
//! Deleting an entry removes its files only when they're inside the store's
//! own `files/` folder (pages kept for review); files the user saved
//! elsewhere are theirs and stay.

use fo_types::{ScanKind, ScanRecord};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Most entries kept; older ones are dropped.
pub const MAX_RECORDS: usize = 500;

pub struct History {
    dir: PathBuf,
    records: Vec<ScanRecord>,
}

impl History {
    /// Opens (or starts) the history in `dir`. A missing or unreadable file
    /// starts an empty history.
    pub fn open(dir: &Path) -> History {
        let records = std::fs::read(dir.join("history.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        History {
            dir: dir.to_path_buf(),
            records,
        }
    }

    /// Where scan files owned by the history live.
    pub fn files_dir(&self) -> PathBuf {
        self.dir.join("files")
    }

    pub fn list(&self) -> Vec<ScanRecord> {
        self.records.clone()
    }

    fn save(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| e.to_string())?;
        let tmp = self.dir.join("history.json.tmp");
        let json = serde_json::to_vec(&self.records).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, self.dir.join("history.json")).map_err(|e| e.to_string())
    }

    pub fn add(&mut self, kind: ScanKind) -> Result<ScanRecord, String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        // Unique even for several scans in one second.
        let mut n = now.as_nanos();
        while self.records.iter().any(|r| r.id == format!("{n:x}")) {
            n += 1;
        }
        let rec = ScanRecord {
            id: format!("{n:x}"),
            at: now.as_secs(),
            kind,
        };
        self.records.insert(0, rec.clone());
        for old in self.records.split_off(self.records.len().min(MAX_RECORDS)) {
            self.remove_owned_files(&old);
        }
        self.save()?;
        Ok(rec)
    }

    fn remove_owned_files(&self, r: &ScanRecord) {
        let ScanKind::Document { files } = &r.kind else {
            return;
        };
        let Ok(own) = self.files_dir().canonicalize() else {
            return;
        };
        for f in files {
            if let Ok(p) = f.canonicalize() {
                if p.starts_with(&own) {
                    let _ = std::fs::remove_file(&p);
                    // Drop the scan's folder once it's empty.
                    if let Some(parent) = p.parent().filter(|d| *d != own) {
                        let _ = std::fs::remove_dir(parent);
                    }
                }
            }
        }
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        if let Some(i) = self.records.iter().position(|r| r.id == id) {
            let r = self.records.remove(i);
            self.remove_owned_files(&r);
            self.save()?;
        }
        Ok(())
    }

    pub fn clear(&mut self) -> Result<(), String> {
        for r in std::mem::take(&mut self.records) {
            self.remove_owned_files(&r);
        }
        self.save()
    }
}
