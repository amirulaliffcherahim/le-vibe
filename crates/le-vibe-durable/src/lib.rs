use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unknown session entry: {0}")]
    UnknownEntry(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    Message,
    CompactionSummary,
    ModelChange,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionEntry {
    pub id: String,
    pub parent_id: Option<String>,
    #[serde(default = "default_kind")]
    pub kind: EntryKind,
    pub role: String,
    pub content: String,
}

fn default_kind() -> EntryKind {
    EntryKind::Message
}

#[derive(Debug, Default)]
pub struct SessionStore {
    path: Option<PathBuf>,
    entries: Vec<SessionEntry>,
    leaf_id: Option<String>,
}

impl SessionStore {
    pub fn in_memory() -> Self {
        Self::default()
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, SessionError> {
        let path_buf = path.as_ref().to_path_buf();
        let mut entries = Vec::new();
        if path_buf.exists() {
            let file = File::open(&path_buf)?;
            for line in BufReader::new(file).lines() {
                let line = line?;
                if !line.trim().is_empty() {
                    entries.push(serde_json::from_str(&line)?);
                }
            }
        }
        let leaf_id = entries.last().map(|e: &SessionEntry| e.id.clone());
        Ok(Self {
            path: Some(path_buf),
            entries,
            leaf_id,
        })
    }

    fn append_entry(&mut self, kind: EntryKind, role: String, content: String) -> Result<SessionEntry, SessionError> {
        let id = format!("msg_{}", self.entries.len() + 1);
        let parent_id = self.leaf_id.clone();
        let entry = SessionEntry {
            id: id.clone(),
            parent_id,
            kind,
            role,
            content,
        };
        if let Some(path) = &self.path {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut file = OpenOptions::new().create(true).append(true).open(path)?;
            let mut line = serde_json::to_string(&entry)?;
            line.push('\n');
            file.write_all(line.as_bytes())?;
        }
        self.leaf_id = Some(id);
        self.entries.push(entry.clone());
        Ok(entry)
    }

    pub fn append(&mut self, role: impl Into<String>, content: impl Into<String>) -> Result<SessionEntry, SessionError> {
        self.append_entry(EntryKind::Message, role.into(), content.into())
    }

    pub fn append_compaction(&mut self, summary: impl Into<String>) -> Result<SessionEntry, SessionError> {
        self.append_entry(EntryKind::CompactionSummary, "system".to_string(), summary.into())
    }

    pub fn branch_from(&mut self, entry_id: &str) -> Result<(), SessionError> {
        if !self.entries.iter().any(|e| e.id == entry_id) {
            return Err(SessionError::UnknownEntry(entry_id.to_string()));
        }
        self.leaf_id = Some(entry_id.to_string());
        Ok(())
    }

    pub fn leaf_id(&self) -> Option<&str> {
        self.leaf_id.as_deref()
    }

    /// Walks from the active leaf back to root, stopping at the latest compaction summary.
    pub fn active_branch(&self) -> Vec<SessionEntry> {
        let by_id: HashMap<&str, &SessionEntry> =
            self.entries.iter().map(|e| (e.id.as_str(), e)).collect();
        let mut chain = Vec::new();
        let mut cursor = self.leaf_id.as_deref();
        while let Some(id) = cursor {
            let Some(entry) = by_id.get(id).copied() else {
                break;
            };
            chain.push(entry.clone());
            if entry.kind == EntryKind::CompactionSummary {
                break;
            }
            cursor = entry.parent_id.as_deref();
        }
        chain.reverse();
        chain
    }

    pub fn entries(&self) -> &[SessionEntry] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_branches_and_compacts_dag() {
        let mut store = SessionStore::in_memory();
        let e1 = store.append("user", "q1").unwrap();
        let e2 = store.append("assistant", "a1").unwrap();
        assert_eq!(e2.parent_id.as_deref(), Some(e1.id.as_str()));

        store.branch_from(&e1.id).unwrap();
        let e3 = store.append("assistant", "a1-alt").unwrap();
        assert_eq!(e3.parent_id.as_deref(), Some(e1.id.as_str()));

        let branch: Vec<String> = store.active_branch().into_iter().map(|e| e.content).collect();
        assert_eq!(branch, vec!["q1", "a1-alt"]);

        store.append_compaction("summary-1").unwrap();
        store.append("user", "q2").unwrap();
        let compacted: Vec<String> = store.active_branch().into_iter().map(|e| e.content).collect();
        assert_eq!(compacted, vec!["summary-1", "q2"]);
    }

    #[test]
    fn disk_reload_preserves_compaction_boundary_and_rejects_unknown_branch() {
        let dir = std::env::temp_dir().join("le_vibe_durable_isolation_test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("session.jsonl");

        {
            let mut store = SessionStore::open(&path).unwrap();
            store.append("user", "secret-pre-compaction").unwrap();
            store.append("assistant", "old-reply").unwrap();
            store.append_compaction("clean-summary").unwrap();
            store.append("user", "fresh-question").unwrap();
            assert!(store.branch_from("nonexistent").is_err());
        }

        let reloaded = SessionStore::open(&path).unwrap();
        let active: Vec<String> = reloaded.active_branch().into_iter().map(|e| e.content).collect();
        assert_eq!(active, vec!["clean-summary", "fresh-question"]);
        assert!(!active.iter().any(|c| c.contains("secret-pre-compaction")));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
