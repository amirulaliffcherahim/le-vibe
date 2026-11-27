use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
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
    ModeChange,
    ToolCall,
    ToolResult,
    BranchSummary,
    SessionMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionEntry {
    pub id: String,
    pub parent_id: Option<String>,
    #[serde(default = "default_kind")]
    pub kind: EntryKind,
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// pi's `firstKeptEntryId`: the oldest entry a compaction summary keeps verbatim.
    /// Appended at the end of the struct so existing JSONL keeps its field order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_kept_entry_id: Option<String>,
    /// Prompt tokens before a compaction ran (pi's `tokensBefore`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_before: Option<u64>,
    /// pi's `fromId` on a branch summary: the leaf that was abandoned when branching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMetaInfo {
    pub path: PathBuf,
    pub name: Option<String>,
    pub entry_count: usize,
    pub preview: String,
    pub modified_secs: u64,
}

fn default_kind() -> EntryKind {
    EntryKind::Message
}

/// Everything needed to append one entry, including the pi-specific boundary metadata.
struct EntryMetadata {
    kind: EntryKind,
    role: String,
    content: String,
    timestamp: Option<u64>,
    tool_call_id: Option<String>,
    tool_name: Option<String>,
    first_kept_entry_id: Option<String>,
    tokens_before: Option<u64>,
    from_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct SessionStore {
    path: Option<PathBuf>,
    entries: Vec<SessionEntry>,
    leaf_id: Option<String>,
    next_id: usize,
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
                let Ok(line) = line else {
                    continue;
                };
                if line.trim().is_empty() {
                    continue;
                }
                // Tolerate truncated/corrupt tail lines from an unexpected crash.
                if let Ok(entry) = serde_json::from_str::<SessionEntry>(&line) {
                    entries.push(entry);
                }
            }
        }
        let next_id = entries
            .iter()
            .filter_map(|e| e.id.strip_prefix("msg_").and_then(|n| n.parse::<usize>().ok()))
            .max()
            .unwrap_or(0)
            .max(entries.len());
        let leaf_id = entries.last().map(|e: &SessionEntry| e.id.clone());
        Ok(Self {
            path: Some(path_buf),
            entries,
            leaf_id,
            next_id,
        })
    }

    fn append_entry(&mut self, kind: EntryKind, role: String, content: String) -> Result<SessionEntry, SessionError> {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.append_full_entry(kind, role, content, Some(ts), None, None)
    }

    fn append_full_entry(
        &mut self,
        kind: EntryKind,
        role: String,
        content: String,
        timestamp: Option<u64>,
        tool_call_id: Option<String>,
        tool_name: Option<String>,
    ) -> Result<SessionEntry, SessionError> {
        self.append_entry_with_metadata(EntryMetadata {
            kind,
            role,
            content,
            timestamp,
            tool_call_id,
            tool_name,
            first_kept_entry_id: None,
            tokens_before: None,
            from_id: None,
        })
    }

    /// Append one entry with the pi-specific metadata fields set.
    fn append_entry_with_metadata(
        &mut self,
        meta: EntryMetadata,
    ) -> Result<SessionEntry, SessionError> {
        self.next_id += 1;
        let id = format!("msg_{}", self.next_id);
        let parent_id = self.leaf_id.clone();
        let entry = SessionEntry {
            id: id.clone(),
            parent_id,
            kind: meta.kind,
            role: meta.role,
            content: meta.content,
            timestamp: meta.timestamp,
            tool_call_id: meta.tool_call_id,
            tool_name: meta.tool_name,
            first_kept_entry_id: meta.first_kept_entry_id,
            tokens_before: meta.tokens_before,
            from_id: meta.from_id,
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

    /// Append a compaction summary that records pi's `firstKeptEntryId` boundary.
    ///
    /// `first_kept_entry_id` is the oldest entry the summary keeps verbatim, so the kept tail stays
    /// in its original position instead of being copied after the summary.
    pub fn append_compaction_with_keep(
        &mut self,
        summary: impl Into<String>,
        first_kept_entry_id: Option<String>,
        tokens_before: Option<u64>,
    ) -> Result<SessionEntry, SessionError> {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .ok();
        self.append_entry_with_metadata(EntryMetadata {
            kind: EntryKind::CompactionSummary,
            role: "system".to_string(),
            content: summary.into(),
            timestamp: ts,
            tool_call_id: None,
            tool_name: None,
            first_kept_entry_id,
            tokens_before,
            from_id: None,
        })
    }

    /// Branch to `from_id` (or a fresh root when `None`) and record the abandoned leaf as a
    /// `branch_summary` entry, mirroring pi's `branchWithSummary`.
    ///
    /// The stored `from_id` field is the leaf that was abandoned; the `from_id` parameter is the
    /// node to branch *from*.
    pub fn branch_with_summary(
        &mut self,
        from_id: Option<&str>,
        summary: impl Into<String>,
    ) -> Result<SessionEntry, SessionError> {
        if let Some(target) = from_id
            && !self.entries.iter().any(|e| e.id == target)
        {
            return Err(SessionError::UnknownEntry(target.to_string()));
        }
        let abandoned = self.leaf_id.clone();
        self.leaf_id = from_id.map(ToString::to_string);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .ok();
        self.append_entry_with_metadata(EntryMetadata {
            kind: EntryKind::BranchSummary,
            role: "system".to_string(),
            content: summary.into(),
            timestamp: ts,
            tool_call_id: None,
            tool_name: None,
            first_kept_entry_id: None,
            tokens_before: None,
            from_id: abandoned,
        })
    }

    pub fn append_branch_summary(&mut self, summary: impl Into<String>) -> Result<SessionEntry, SessionError> {
        self.append_entry(EntryKind::BranchSummary, "system".to_string(), summary.into())
    }

    pub fn append_tool_call(
        &mut self,
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        arguments_json: impl Into<String>,
    ) -> Result<SessionEntry, SessionError> {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).ok();
        self.append_full_entry(
            EntryKind::ToolCall,
            "assistant".to_string(),
            arguments_json.into(),
            ts,
            Some(tool_call_id.into()),
            Some(tool_name.into()),
        )
    }

    pub fn append_tool_result(
        &mut self,
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        output: impl Into<String>,
    ) -> Result<SessionEntry, SessionError> {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).ok();
        self.append_full_entry(
            EntryKind::ToolResult,
            "tool".to_string(),
            output.into(),
            ts,
            Some(tool_call_id.into()),
            Some(tool_name.into()),
        )
    }

    pub fn set_name(&mut self, name: impl Into<String>) -> Result<SessionEntry, SessionError> {
        self.append_entry(EntryKind::SessionMeta, "system".to_string(), name.into())
    }

    pub fn name(&self) -> Option<&str> {
        self.entries
            .iter()
            .rev()
            .find(|e| e.kind == EntryKind::SessionMeta)
            .map(|e| e.content.as_str())
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn fork_from(&self, entry_id: &str, dest: impl AsRef<Path>) -> Result<Self, SessionError> {
        let mut clone = Self::in_memory();
        clone.entries = self.entries.clone();
        clone.branch_from(entry_id)?;
        let chain = clone.active_branch();
        let mut out = Self::open(dest)?;
        for e in chain {
            out.append_full_entry(
                e.kind,
                e.role,
                e.content,
                e.timestamp,
                e.tool_call_id,
                e.tool_name,
            )?;
        }
        Ok(out)
    }

    pub fn clone_branch(&self, dest: impl AsRef<Path>) -> Result<Self, SessionError> {
        let mut out = Self::open(dest)?;
        for e in self.active_branch() {
            out.append_full_entry(
                e.kind,
                e.role,
                e.content,
                e.timestamp,
                e.tool_call_id,
                e.tool_name,
            )?;
        }
        Ok(out)
    }

    pub fn list_sessions(dir: &Path) -> Vec<SessionMetaInfo> {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut list = Vec::new();
        for item in entries.flatten() {
            let path = item.path();
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }
            let modified_secs = item
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if let Ok(store) = Self::open(&path) {
                let preview = store
                    .entries()
                    .iter()
                    .find(|e| e.kind == EntryKind::Message && e.role == "user")
                    .map(|e| e.content.chars().take(64).collect())
                    .unwrap_or_else(|| "(empty)".to_string());
                list.push(SessionMetaInfo {
                    path,
                    name: store.name().map(ToString::to_string),
                    entry_count: store.entries().len(),
                    preview,
                    modified_secs,
                });
            }
        }
        list.sort_by_key(|b| std::cmp::Reverse(b.modified_secs));
        list
    }

    pub fn append_model_change(&mut self, model_spec: impl Into<String>) -> Result<SessionEntry, SessionError> {
        self.append_entry(EntryKind::ModelChange, "system".to_string(), model_spec.into())
    }

    pub fn append_mode_change(&mut self, mode: impl Into<String>) -> Result<SessionEntry, SessionError> {
        self.append_entry(EntryKind::ModeChange, "system".to_string(), mode.into())
    }

    pub fn branch_from(&mut self, entry_id: &str) -> Result<(), SessionError> {
        if !self.entries.iter().any(|e| e.id == entry_id) {
            return Err(SessionError::UnknownEntry(entry_id.to_string()));
        }
        self.leaf_id = Some(entry_id.to_string());
        Ok(())
    }

    pub fn reset_leaf(&mut self) {
        self.leaf_id = None;
    }

    pub fn leaf_id(&self) -> Option<&str> {
        self.leaf_id.as_deref()
    }

    /// Walks from the active leaf back to root, stopping at the latest compaction summary.
    ///
    /// Kept for compatibility; use [`SessionStore::context_entries`] for the compaction-aware view
    /// that also restores the entries a summary keeps verbatim.
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

    /// Full root→leaf path with no compaction short-circuit (pi's `buildSessionPath`).
    fn full_path(&self) -> Vec<SessionEntry> {
        let by_id: HashMap<&str, &SessionEntry> =
            self.entries.iter().map(|e| (e.id.as_str(), e)).collect();
        let mut chain = Vec::new();
        let mut cursor = self.leaf_id.as_deref();
        while let Some(id) = cursor {
            let Some(entry) = by_id.get(id).copied() else {
                break;
            };
            chain.push(entry.clone());
            cursor = entry.parent_id.as_deref();
        }
        chain.reverse();
        chain
    }

    /// Compaction-aware path, matching pi's `buildContextEntries`.
    ///
    /// The newest compaction on the path is emitted once, followed by the entries it keeps
    /// verbatim (starting at `first_kept_entry_id`, skipping pre-boundary system entries) and then
    /// every entry appended after it. Nothing is copied, so the file has no duplicated history.
    pub fn context_entries(&self) -> Vec<SessionEntry> {
        let path = self.full_path();
        let Some(compaction_idx) = path
            .iter()
            .rposition(|e| e.kind == EntryKind::CompactionSummary)
        else {
            return path;
        };
        let compaction = &path[compaction_idx];
        let mut out = Vec::with_capacity(path.len());
        out.push(compaction.clone());

        if let Some(first_kept) = compaction.first_kept_entry_id.as_deref() {
            let mut found = false;
            for entry in &path[..compaction_idx] {
                if entry.id == first_kept {
                    found = true;
                }
                if found && !(entry.kind == EntryKind::Message && entry.role == "system") {
                    out.push(entry.clone());
                }
            }
        }
        out.extend(path[compaction_idx + 1..].iter().cloned());
        out
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

    #[test]
    fn tool_call_result_roundtrip_fork_clone_and_list_sessions() {
        let dir = std::env::temp_dir().join("le_vibe_durable_phase5_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let s1_path = dir.join("session-1.jsonl");
        let mut s1 = SessionStore::open(&s1_path).unwrap();
        s1.set_name("alpha").unwrap();
        let u1 = s1.append("user", "inspect cargo").unwrap();
        s1.append_tool_call("tc_1", "read", r#"{"path":"Cargo.toml"}"#).unwrap();
        s1.append_tool_result("tc_1", "read", "1: [workspace]").unwrap();
        s1.append("assistant", "done").unwrap();

        let reloaded = SessionStore::open(&s1_path).unwrap();
        assert_eq!(reloaded.name(), Some("alpha"));
        assert!(reloaded.entries().iter().any(|e| e.kind == EntryKind::ToolCall));
        assert!(reloaded.entries().iter().any(|e| e.kind == EntryKind::ToolResult));

        let forked_path = dir.join("session-fork.jsonl");
        let forked = reloaded.fork_from(&u1.id, &forked_path).unwrap();
        assert_eq!(forked.active_branch().len(), 2); // SessionMeta + u1

        let cloned_path = dir.join("session-clone.jsonl");
        let cloned = reloaded.clone_branch(&cloned_path).unwrap();
        assert_eq!(cloned.active_branch().len(), 5);

        let listed = SessionStore::list_sessions(&dir);
        assert_eq!(listed.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
