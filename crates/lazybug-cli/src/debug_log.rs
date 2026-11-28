//! Structured event log & live debugger store.
//! Compiled out completely in official releases (`cargo build --release --no-default-features`).

#[cfg(feature = "dev-debug")]
mod inner {
    use serde::Serialize;
    use std::collections::VecDeque;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    const MAX_RING_EVENTS: usize = 500;

    #[derive(Debug, Clone, Serialize)]
    pub(crate) struct DebugEvent {
        pub ts_ms: u128,
        pub category: String,
        pub event: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub duration_ms: Option<u64>,
        pub detail: String,
    }

    pub(crate) struct EventLogger {
        path: Option<PathBuf>,
        ring: Mutex<VecDeque<DebugEvent>>,
    }

    impl EventLogger {
        pub(crate) fn new_default() -> Arc<Self> {
            let home = std::env::var_os("USERPROFILE")
                .or_else(|| std::env::var_os("HOME"))
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."));
            let logs_dir = home.join(".lazybug").join("logs");
            Self::prune_old_logs(&logs_dir, 20);
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            Self::open(logs_dir.join(format!("events-{ts}.jsonl")))
        }

        fn prune_old_logs(dir: &Path, keep: usize) {
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            let mut files: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("events-") && n.ends_with(".jsonl"))
                })
                .collect();
            if files.len() <= keep {
                return;
            }
            files.sort();
            for old in &files[..files.len() - keep] {
                let _ = fs::remove_file(old);
            }
        }

        pub(crate) fn open(path: PathBuf) -> Arc<Self> {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            Arc::new(Self {
                path: Some(path),
                ring: Mutex::new(VecDeque::with_capacity(MAX_RING_EVENTS)),
            })
        }

        #[cfg(test)]
        pub(crate) fn in_memory() -> Arc<Self> {
            Arc::new(Self {
                path: None,
                ring: Mutex::new(VecDeque::with_capacity(MAX_RING_EVENTS)),
            })
        }

        pub(crate) fn log(
            &self,
            category: &str,
            event: &str,
            duration_ms: Option<u64>,
            detail: impl Into<String>,
        ) {
            let ts_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let mut detail_str = detail.into();
            if detail_str.len() > 240 {
                let mut idx = 240;
                while idx > 0 && !detail_str.is_char_boundary(idx) {
                    idx -= 1;
                }
                detail_str.truncate(idx);
                detail_str.push('…');
            }
            let rec = DebugEvent {
                ts_ms,
                category: category.to_string(),
                event: event.to_string(),
                duration_ms,
                detail: detail_str,
            };
            if let Some(path) = &self.path
                && let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path)
                && let Ok(line) = serde_json::to_string(&rec)
            {
                let _ = writeln!(file, "{line}");
            }
            if let Ok(mut guard) = self.ring.lock() {
                if guard.len() >= MAX_RING_EVENTS {
                    guard.pop_front();
                }
                guard.push_back(rec);
            }
        }

        pub(crate) fn clear(&self) {
            if let Ok(mut guard) = self.ring.lock() {
                guard.clear();
            }
        }

        #[allow(dead_code)]
        pub(crate) fn path(&self) -> Option<&Path> {
            self.path.as_deref()
        }

        pub(crate) fn formatted_lines(&self, filter: &str, header_summary: &str) -> Vec<String> {
            let mut out = Vec::new();
            let path_str = self
                .path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(in-memory)".to_string());
            out.push(format!("Log file: {path_str}"));
            out.push(header_summary.to_string());
            out.push(format!(
                "Filter: {filter}  (keys: f=cycle filter · c=clear · ↑/↓ scroll · F12/Esc close)"
            ));
            out.push("─".repeat(72));
            if let Ok(guard) = self.ring.lock() {
                let matching: Vec<&DebugEvent> = guard
                    .iter()
                    .rev()
                    .filter(|e| filter == "all" || e.category.eq_ignore_ascii_case(filter))
                    .collect();
                if matching.is_empty() {
                    out.push(" (no matching events yet)".to_string());
                } else {
                    for ev in matching {
                        let dur = ev
                            .duration_ms
                            .map(|d| format!(" [{d}ms]"))
                            .unwrap_or_default();
                        let sec = (ev.ts_ms / 1000) % 86_400;
                        let h = sec / 3600;
                        let m = (sec % 3600) / 60;
                        let s = sec % 60;
                        let ms = ev.ts_ms % 1000;
                        out.push(format!(
                            " {h:02}:{m:02}:{s:02}.{ms:03} [{}] {}{dur} {}",
                            ev.category, ev.event, ev.detail
                        ));
                    }
                }
            }
            out
        }
    }

    pub(crate) const DEBUG_ENABLED: bool = true;
}

#[cfg(not(feature = "dev-debug"))]
mod inner {
    use std::path::Path;
    use std::sync::Arc;

    pub(crate) struct EventLogger;

    impl EventLogger {
        pub(crate) fn new_default() -> Arc<Self> {
            Arc::new(Self)
        }
        #[allow(dead_code)]
        pub(crate) fn in_memory() -> Arc<Self> {
            Arc::new(Self)
        }
        #[inline]
        pub(crate) fn log(
            &self,
            _category: &str,
            _event: &str,
            _duration_ms: Option<u64>,
            _detail: impl Into<String>,
        ) {
        }
        #[allow(dead_code)]
        pub(crate) fn clear(&self) {}
        #[allow(dead_code)]
        pub(crate) fn path(&self) -> Option<&Path> {
            None
        }
        #[allow(dead_code)]
        pub(crate) fn formatted_lines(&self, _filter: &str, _header_summary: &str) -> Vec<String> {
            Vec::new()
        }
    }

    pub(crate) const DEBUG_ENABLED: bool = false;
}

pub(crate) use inner::{DEBUG_ENABLED, EventLogger};

#[cfg(all(test, feature = "dev-debug"))]
mod tests {
    use super::*;

    #[test]
    fn logs_to_ring_and_jsonl_file_and_filters() {
        let dir = std::env::temp_dir().join("lazybug_eventlog_test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("events.jsonl");
        let logger = EventLogger::open(path.clone());
        logger.log("perf", "slow_frame", Some(42), "draw took 42ms");
        logger.log("tool", "call_end", Some(12), "read ok");

        let raw = std::fs::read_to_string(&path).expect("log file created");
        assert!(raw.contains("\"slow_frame\""));
        assert!(raw.contains("\"duration_ms\":42"));

        let perf_lines = logger.formatted_lines("perf", "summary");
        assert!(perf_lines.iter().any(|l| l.contains("slow_frame")));
        assert!(!perf_lines.iter().any(|l| l.contains("call_end")));

        logger.clear();
        let after_clear = logger.formatted_lines("all", "summary");
        assert!(after_clear.iter().any(|l| l.contains("no matching events")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn in_memory_logger_truncates_long_details_on_utf8_boundary() {
        let logger = EventLogger::in_memory();
        logger.log("ui", "burst", None, "🔥".repeat(100));
        let lines = logger.formatted_lines("ui", "summary");
        assert!(lines.iter().any(|l| l.contains('…')));
    }
}
