pub mod codemode;
pub mod truncate;
pub mod hooks;
pub mod memory;

use le_vibe_pybridge::{PolicyPort, ToolDefinition};
use le_vibe_schema::validate_object_args;
use regex::{Regex, RegexBuilder};
use serde_json::{Value, json};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use thiserror::Error;

const MAX_LINES: usize = 2000;
const MAX_BYTES: usize = 50 * 1024;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("policy denied: {0}")]
    PolicyDenied(String),
    #[error("policy requires approval: {0}")]
    PolicyAsk(String),
    #[error("schema error: {0}")]
    Schema(#[from] le_vibe_schema::SchemaError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("edit error: {0}")]
    Edit(String),
    /// A tool-level failure whose text is meant for the model verbatim (e.g. `exit_code=3`, or an
    /// out-of-range read offset). Rendered as an error card, not a normal result.
    #[error("{0}")]
    Tool(String),
    #[error("unknown tool: {0}")]
    UnknownTool(String),
}

pub fn builtin_tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "read".to_string(),
            description: "Read a UTF-8 file with optional 1-indexed offset and line limit".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["path"],
                "properties": {
                    "path": { "type": "string" },
                    "offset": { "type": "integer" },
                    "limit": { "type": "integer" }
                }
            }),
        },
        ToolDefinition {
            name: "write".to_string(),
            description: "Create or overwrite a UTF-8 file".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["path", "content"],
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "edit".to_string(),
            description: "Apply one or more exact non-overlapping text replacements to a file".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["path", "edits"],
                "properties": {
                    "path": { "type": "string" },
                    "edits": { "type": "array" }
                }
            }),
        },
        ToolDefinition {
            name: "grep".to_string(),
            description: "Search for a regex/text pattern in files under a path".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["pattern"],
                "properties": {
                    "pattern": { "type": "string" },
                    "path": { "type": "string" },
                    "case_insensitive": { "type": "boolean" },
                    "glob": { "type": "string" },
                    "context": { "type": "integer" }
                }
            }),
        },
        ToolDefinition {
            name: "find".to_string(),
            description: "Find files by path/filename pattern under a directory".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "pattern": { "type": "string" },
                    "max_depth": { "type": "integer" }
                }
            }),
        },
        ToolDefinition {
            name: "ls".to_string(),
            description: "List files and directories under a path".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "bash".to_string(),
            description: "Execute a bash command".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["command"],
                "properties": {
                    "command": { "type": "string" },
                    "timeout_secs": { "type": "integer" }
                }
            }),
        },
        ToolDefinition {
            name: "powershell".to_string(),
            description: "Execute a PowerShell command".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["command"],
                "properties": {
                    "command": { "type": "string" },
                    "timeout_secs": { "type": "integer" }
                }
            }),
        },
        ToolDefinition {
            name: "todo".to_string(),
            description: "Manage a multi-step task list: create/update/list/get/delete/clear. Status: pending -> in_progress -> completed.".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["action"],
                "properties": {
                    "action": { "type": "string", "enum": ["create", "update", "list", "get", "delete", "clear"] },
                    "id": { "type": "integer" },
                    "subject": { "type": "string" },
                    "description": { "type": "string" },
                    "activeForm": { "type": "string" },
                    "status": { "type": "string", "enum": ["pending", "in_progress", "completed", "deleted"] },
                    "blockedBy": { "type": "array", "items": { "type": "integer" } },
                    "addBlockedBy": { "type": "array", "items": { "type": "integer" } },
                    "removeBlockedBy": { "type": "array", "items": { "type": "integer" } },
                    "includeDeleted": { "type": "boolean" },
                    "tasks": { "type": "array" }
                }
            }),
        },
        ToolDefinition {
            name: "ask_user_question".to_string(),
            description: "Ask 1-4 structured clarifying questions; each option needs a label and description. Do not author 'Other' or 'Type something.' - it is appended automatically.".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["questions"],
                "properties": {
                    "questions": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 4,
                        "items": {
                            "type": "object",
                            "required": ["header", "question", "options"],
                            "properties": {
                                "header": { "type": "string", "maxLength": 16 },
                                "question": { "type": "string" },
                                "multiSelect": { "type": "boolean" },
                                "options": {
                                    "type": "array",
                                    "minItems": 2,
                                    "maxItems": 4,
                                    "items": {
                                        "type": "object",
                                        "required": ["label", "description"],
                                        "properties": {
                                            "label": { "type": "string", "maxLength": 60 },
                                            "description": { "type": "string" },
                                            "preview": { "type": "string" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }),
        },
        ToolDefinition {
            name: "web_search".to_string(),
            description: "Search the web (prefer `queries` with 2-4 varied angles). Returns titles, URLs, snippets.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "queries": { "type": "array", "items": { "type": "string" } },
                    "numResults": { "type": "integer" }
                }
            }),
        },
        ToolDefinition {
            name: "fetch_content".to_string(),
            description: "Fetch a page and extract readable Markdown (default), or the exact body with mode='raw'.".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["url"],
                "properties": {
                    "url": { "type": "string" },
                    "mode": { "type": "string", "enum": ["readable", "raw"] }
                }
            }),
        },
        ToolDefinition {
            name: "retrieve_output".to_string(),
            description: "Read exact lines from an earlier compressed tool output by ref id (e.g. 'out_1'), by query or offset/limit.".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["id"],
                "properties": {
                    "id": { "type": "string" },
                    "query": { "type": "string" },
                    "offset": { "type": "integer" },
                    "limit": { "type": "integer" }
                }
            }),
        },
        ToolDefinition {
            name: "memory_search".to_string(),
            description: "Search memory (default scope='memory') or the repo, BM25 + vector. Returns inline text or compact stubs.".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": { "type": "string" },
                    "limit": { "type": "integer" },
                    "scope": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "memory_write".to_string(),
            description: "Upsert a durable decision/constraint/preference: 3-6 synonym keywords, stable key for upsert.".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["content"],
                "properties": {
                    "content": { "type": "string" },
                    "title": { "type": "string" },
                    "keywords": { "type": "array" },
                    "key": { "type": "string" },
                    "target": { "type": "string" },
                    "scope": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "memory_read".to_string(),
            description: "Read full memory entries by batch ids[] / id, or read long_term/daily/list.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "ids": { "type": "array" },
                    "id": { "type": "string" },
                    "target": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "memory_delete".to_string(),
            description: "Delete an obsolete memory entry by id or key from MEMORY.md.".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["id_or_key"],
                "properties": {
                    "id_or_key": { "type": "string" },
                    "scope": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "spawn_agent".to_string(),
            description: "Run an isolated read-only subagent with a clean context window; returns a concise summary.".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["task"],
                "properties": {
                    "task": { "type": "string", "description": "Focused research or analysis task for the isolated subagent" }
                }
            }),
        },
        ToolDefinition {
            name: "load_skill".to_string(),
            description: "Load a skill's full SKILL.md by name when it is relevant to the task.".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["name"],
                "properties": {
                    "name": { "type": "string", "description": "Skill name from the <skills> index (e.g. diagram-design, reverse-engineer)" }
                }
            }),
        },
        ToolDefinition {
            name: "codemode".to_string(),
            description: "Run one Python script that batches tools.<name>(**kwargs) and prints only the distilled answer. Globals: tools, text(v), print, store(k,v), load(k), json, re, Path.".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["script"],
                "properties": {
                    "script": { "type": "string", "description": "Raw Python script calling tools.<name>(**kwargs) and text()/print()" },
                    "timeout_ms": { "type": "integer", "description": "Optional timeout in milliseconds (default 60000)" }
                }
            }),
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TodoItem {
    pub id: u64,
    pub subject: String,
    pub description: String,
    pub active_form: Option<String>,
    pub status: String,
    pub blocked_by: Vec<u64>,
}

#[derive(Debug, Default)]
pub struct TodoStore {
    next_id: u64,
    items: Vec<TodoItem>,
}

impl TodoStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn items(&self) -> &[TodoItem] {
        &self.items
    }

    fn has_cycle(&self, from_id: u64, target_id: u64) -> bool {
        if from_id == target_id {
            return true;
        }
        let mut stack = vec![target_id];
        let mut visited = std::collections::HashSet::new();
        while let Some(cur) = stack.pop() {
            if !visited.insert(cur) {
                continue;
            }
            if cur == from_id {
                return true;
            }
            if let Some(item) = self.items.iter().find(|i| i.id == cur) {
                for &dep in &item.blocked_by {
                    stack.push(dep);
                }
            }
        }
        false
    }
}

fn parse_id_array(val: Option<&Value>) -> Vec<u64> {
    val.and_then(Value::as_array)
        .map(|arr| arr.iter().filter_map(Value::as_u64).collect())
        .unwrap_or_default()
}

pub fn execute_todo_tool(store: &mut TodoStore, args: &Value) -> Result<String, ToolError> {
    let action = args
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::Edit("todo.action must be a string".into()))?;
    match action {
        "create" => {
            if let Some(tasks) = args.get("tasks").and_then(Value::as_array) {
                if tasks.is_empty() {
                    return Err(ToolError::Edit("todo.tasks must be non-empty".into()));
                }
                // Validate whole batch before committing.
                for t in tasks {
                    let sub = t.get("subject").and_then(Value::as_str).unwrap_or("").trim();
                    if sub.is_empty() {
                        return Err(ToolError::Edit("each todo task requires a non-empty subject".into()));
                    }
                    for dep in parse_id_array(t.get("blockedBy")) {
                        if !store.items.iter().any(|i| i.id == dep && i.status != "deleted") {
                            return Err(ToolError::Edit(format!("blockedBy id #{dep} does not exist")));
                        }
                    }
                }
                let mut created = Vec::new();
                for t in tasks {
                    let subject = t.get("subject").and_then(Value::as_str).unwrap_or("").trim().to_string();
                    let description = t.get("description").and_then(Value::as_str).unwrap_or("").to_string();
                    let active_form = t
                        .get("activeForm")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(ToString::to_string);
                    let mut blocked_by = parse_id_array(t.get("blockedBy"));
                    blocked_by.sort_unstable();
                    blocked_by.dedup();
                    store.next_id += 1;
                    let id = store.next_id;
                    store.items.push(TodoItem {
                        id,
                        subject: subject.clone(),
                        description,
                        active_form,
                        status: "pending".to_string(),
                        blocked_by,
                    });
                    created.push(format!("Created #{id}: {subject} (pending)"));
                }
                return Ok(created.join("\n"));
            }
            let subject = args
                .get("subject")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| ToolError::Edit("todo.subject required for create".into()))?
                .to_string();
            let description = args.get("description").and_then(Value::as_str).unwrap_or("").to_string();
            let active_form = args
                .get("activeForm")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string);
            let mut blocked_by = parse_id_array(args.get("blockedBy"));
            blocked_by.sort_unstable();
            blocked_by.dedup();
            for &dep in &blocked_by {
                if !store.items.iter().any(|i| i.id == dep && i.status != "deleted") {
                    return Err(ToolError::Edit(format!("blockedBy id #{dep} does not exist")));
                }
            }
            store.next_id += 1;
            let id = store.next_id;
            store.items.push(TodoItem {
                id,
                subject: subject.clone(),
                description,
                active_form,
                status: "pending".to_string(),
                blocked_by,
            });
            Ok(format!("Created #{id}: {subject} (pending)"))
        }
        "update" => {
            let id = args
                .get("id")
                .and_then(Value::as_u64)
                .ok_or_else(|| ToolError::Edit("todo.id required for update".into()))?;
            let add_deps = parse_id_array(args.get("addBlockedBy"));
            let rem_deps = parse_id_array(args.get("removeBlockedBy"));
            for &dep in &add_deps {
                if !store.items.iter().any(|i| i.id == dep) {
                    return Err(ToolError::Edit(format!("addBlockedBy id #{dep} does not exist")));
                }
                if store.has_cycle(id, dep) {
                    return Err(ToolError::Edit(format!(
                        "Dependency cycle rejected: #{id} cannot be blocked by #{dep}"
                    )));
                }
            }
            if let Some(st) = args.get("status").and_then(Value::as_str)
                && !matches!(st, "pending" | "in_progress" | "completed" | "deleted")
            {
                return Err(ToolError::Edit(format!("Invalid todo status: {st}")));
            }
            let item = store
                .items
                .iter_mut()
                .find(|i| i.id == id)
                .ok_or_else(|| ToolError::Edit(format!("todo #{id} not found")))?;
            let prev = item.status.clone();
            let mut changed = false;
            if let Some(st) = args.get("status").and_then(Value::as_str) {
                item.status = st.to_string();
                changed = true;
            }
            if let Some(sub) = args.get("subject").and_then(Value::as_str) {
                item.subject = sub.to_string();
                changed = true;
            }
            if let Some(desc) = args.get("description").and_then(Value::as_str) {
                item.description = desc.to_string();
                changed = true;
            }
            if let Some(af) = args.get("activeForm").and_then(Value::as_str) {
                item.active_form = (!af.trim().is_empty()).then(|| af.trim().to_string());
                changed = true;
            }
            if !add_deps.is_empty() {
                item.blocked_by.extend(add_deps);
                item.blocked_by.sort_unstable();
                item.blocked_by.dedup();
                changed = true;
            }
            if !rem_deps.is_empty() {
                item.blocked_by.retain(|d| !rem_deps.contains(d));
                changed = true;
            }
            if !changed {
                return Err(ToolError::Edit(
                    "todo update requires at least one mutable field (status, subject, description, activeForm, addBlockedBy, removeBlockedBy)".into(),
                ));
            }
            Ok(format!("Updated #{id} ({prev} -> {})", item.status))
        }
        "delete" => {
            let id = args
                .get("id")
                .and_then(Value::as_u64)
                .ok_or_else(|| ToolError::Edit("todo.id required for delete".into()))?;
            let item = store
                .items
                .iter_mut()
                .find(|i| i.id == id)
                .ok_or_else(|| ToolError::Edit(format!("todo #{id} not found")))?;
            item.status = "deleted".to_string();
            Ok(format!("Deleted #{id}"))
        }
        "clear" => {
            store.items.clear();
            Ok("Cleared all todos.".to_string())
        }
        "get" => {
            let id = args
                .get("id")
                .and_then(Value::as_u64)
                .ok_or_else(|| ToolError::Edit("todo.id required for get".into()))?;
            let item = store
                .items
                .iter()
                .find(|i| i.id == id)
                .ok_or_else(|| ToolError::Edit(format!("todo #{id} not found")))?;
            let mut out = format!("#{} [{}] {}", item.id, item.status, item.subject);
            if let Some(af) = &item.active_form {
                out.push_str(&format!("\nactiveForm: {af}"));
            }
            if !item.blocked_by.is_empty() {
                out.push_str(&format!("\nblockedBy: {:?}", item.blocked_by));
            }
            if !item.description.is_empty() {
                out.push_str(&format!("\n{}", item.description));
            }
            Ok(out)
        }
        _ => {
            let filter_status = args.get("status").and_then(Value::as_str);
            let include_deleted = args
                .get("includeDeleted")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let lines: Vec<String> = store
                .items
                .iter()
                .filter(|i| include_deleted || i.status != "deleted")
                .filter(|i| filter_status.is_none_or(|s| i.status == s))
                .map(|i| {
                    let dep_tag = if i.blocked_by.is_empty() {
                        String::new()
                    } else {
                        format!(" (blockedBy: {:?})", i.blocked_by)
                    };
                    format!("#{} [{}] {}{dep_tag}", i.id, i.status, i.subject)
                })
                .collect();
            if lines.is_empty() {
                Ok("No todos.".to_string())
            } else {
                Ok(lines.join("\n"))
            }
        }
    }
}

fn truncate_output(text: &str) -> String {
    let mut lines: Vec<&str> = text.lines().collect();
    let total_lines = lines.len();
    let truncated_lines = total_lines > MAX_LINES;
    if truncated_lines {
        lines.truncate(MAX_LINES);
    }
    let mut out = lines.join("\n");
    let mut truncated_bytes = false;
    if out.len() > MAX_BYTES {
        let mut idx = MAX_BYTES;
        while !out.is_char_boundary(idx) && idx > 0 {
            idx -= 1;
        }
        out.truncate(idx);
        truncated_bytes = true;
    }
    if truncated_lines || truncated_bytes {
        out.push_str("\n...[output truncated]");
    }
    out
}

fn run_read(args: &Value) -> Result<String, ToolError> {
    let path = args.get("path").and_then(Value::as_str).expect("validated");
    let raw = fs::read_to_string(path)?;
    let all_lines: Vec<&str> = raw.lines().collect();
    let total = all_lines.len();
    let start_line = args
        .get("offset")
        .and_then(Value::as_u64)
        .map(|v| v.saturating_sub(1) as usize)
        .unwrap_or(0);
    // pi errors instead of returning an empty string, so the model learns the file is shorter than
    // it assumed. An empty file with no offset is not an error.
    if start_line >= total && start_line > 0 {
        return Err(ToolError::Tool(format!(
            "Offset {} is beyond end of file ({} lines total)",
            start_line + 1,
            total
        )));
    }
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(truncate::DEFAULT_MAX_LINES);

    let selected: Vec<String> = all_lines
        .iter()
        .enumerate()
        .skip(start_line)
        .take(limit)
        .map(|(idx, line)| format!("{}: {line}", idx + 1))
        .collect();
    let joined = selected.join("
");
    let cut = truncate::truncate_head(&joined, limit, truncate::DEFAULT_MAX_BYTES);

    if cut.first_line_exceeds_limit {
        let first_bytes = selected.first().map(String::len).unwrap_or(0);
        // The hint must name a command that exists on this host: suggesting `sed`/`head` on
        // Windows primes a Unix reflex that burns a whole turn on "term not recognized".
        let how = if cfg!(windows) {
            format!(
                "PowerShell: (Get-Content -LiteralPath '{path}')[{idx}] or Python: open(r'{path}', encoding='utf-8').read().splitlines()[{idx}]",
                idx = start_line,
            )
        } else {
            format!(
                "bash: sed -n '{line}p' {path} | head -c {max}",
                line = start_line + 1,
                max = truncate::DEFAULT_MAX_BYTES,
            )
        };
        return Ok(format!(
            "[Line {line} is {size}, exceeds {limit} limit. Read it with {how}]",
            line = start_line + 1,
            size = truncate::format_size(first_bytes),
            limit = truncate::format_size(truncate::DEFAULT_MAX_BYTES),
        ));
    }

    let shown = cut.output_lines;
    let mut out = cut.content;
    if start_line + shown < total {
        let first = start_line + 1;
        let last = start_line + shown;
        let limit_note = if cut.truncated_by == Some(truncate::TruncatedBy::Bytes) {
            format!(
                " ({} limit)",
                truncate::format_size(truncate::DEFAULT_MAX_BYTES)
            )
        } else {
            String::new()
        };
        out.push_str(&format!(
            "

[Showing lines {first}-{last} of {total}{limit_note}. Use offset={} to continue.]",
            last + 1
        ));
    }
    Ok(out)
}

fn run_write(args: &Value) -> Result<String, ToolError> {
    let path = args.get("path").and_then(Value::as_str).expect("validated");
    let content = args.get("content").and_then(Value::as_str).expect("validated");
    // Writes share the same per-file queue as edits: a write landing between an edit's read and
    // write would otherwise be dropped.
    let lock = mutation_lock(path);
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(format!("Wrote {} bytes to {path}", content.len()))
}

/// Normalise every shape a model uses for edits into a single `Vec<Value>` of
/// `{oldText, newText}` pairs.
///
/// Models send `edits` as an array, as a JSON **string**, as one object instead of a one-element
/// array, or as the legacy top-level `oldText`/`newText` pair — sometimes a mix. Rejecting any of
/// those burns a whole turn on a formatting mistake, so all of them are accepted.
fn normalize_edits(args: &Value) -> Result<Vec<Value>, ToolError> {
    let mut out: Vec<Value> = Vec::new();
    match args.get("edits") {
        Some(Value::Array(items)) => out.extend(items.iter().cloned()),
        Some(Value::String(raw)) => {
            let parsed: Value = serde_json::from_str(raw).map_err(|e| {
                ToolError::Edit(format!("edits was a string but not valid JSON: {e}"))
            })?;
            match parsed {
                Value::Array(items) => out.extend(items),
                obj @ Value::Object(_) => out.push(obj),
                _ => return Err(ToolError::Edit("edits must be an array of edits".into())),
            }
        }
        Some(obj @ Value::Object(_)) => out.push(obj.clone()),
        Some(Value::Null) | None => {}
        Some(_) => return Err(ToolError::Edit("edits must be an array of edits".into())),
    }

    // Legacy pair: appended after the array so "merge nearby changes" still applies.
    if let (Some(old), Some(new)) = (
        args.get("oldText").and_then(Value::as_str),
        args.get("newText").and_then(Value::as_str),
    ) {
        out.push(json!({ "oldText": old, "newText": new }));
    }

    if out.is_empty() {
        return Err(ToolError::Edit(
            "no edits supplied: pass edits[] or oldText/newText".into(),
        ));
    }
    Ok(out)
}

/// Serialises mutations per canonical path (pi's `file-mutation-queue.ts`).
///
/// `edit` is read-modify-write, so two concurrent edits to one file would both read the same
/// original and the later write would drop the earlier change. Different files keep separate locks
/// and stay parallel.
fn mutation_lock(path: &str) -> std::sync::Arc<std::sync::Mutex<()>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    static QUEUES: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
    let map = QUEUES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = map.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    Arc::clone(
        guard
            .entry(canonical)
            .or_insert_with(|| Arc::new(Mutex::new(()))),
    )
}

fn run_edit(args: &Value) -> Result<String, ToolError> {
    let path = args.get("path").and_then(Value::as_str).expect("validated");
    let edits = normalize_edits(args)?;
    let lock = mutation_lock(path);
    let _guard = lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let original = fs::read_to_string(path)?;
    let mut spans: Vec<(usize, usize, String)> = Vec::with_capacity(edits.len());

    for (idx, item) in edits.iter().enumerate() {
        let old_text = item
            .get("oldText")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Edit(format!("edits[{idx}].oldText must be a string")))?;
        let new_text = item
            .get("newText")
            .and_then(Value::as_str)
            .ok_or_else(|| ToolError::Edit(format!("edits[{idx}].newText must be a string")))?;
        let matches: Vec<(usize, &str)> = original.match_indices(old_text).collect();
        if matches.is_empty() {
            return Err(ToolError::Edit(format!("edits[{idx}].oldText not found in {path}")));
        }
        if matches.len() > 1 {
            return Err(ToolError::Edit(format!(
                "edits[{idx}].oldText matched {} times; must be unique",
                matches.len()
            )));
        }
        let start = matches[0].0;
        let end = start + old_text.len();
        spans.push((start, end, new_text.to_string()));
    }

    spans.sort_by_key(|s| s.0);
    for pair in spans.windows(2) {
        if pair[0].1 > pair[1].0 {
            return Err(ToolError::Edit("overlapping edits are not allowed".into()));
        }
    }

    let mut updated = String::with_capacity(original.len());
    let mut cursor = 0;
    for (start, end, replacement) in &spans {
        updated.push_str(&original[cursor..*start]);
        updated.push_str(replacement);
        cursor = *end;
    }
    updated.push_str(&original[cursor..]);
    fs::write(path, &updated)?;
    Ok(format!("Applied {} edit(s) to {path}", spans.len()))
}

const GREP_MAX_MATCHES: usize = 200;
const FIND_MAX_RESULTS: usize = 500;
const MAX_WALK_DEPTH: usize = 32;

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | "node_modules"
            | "target"
            | ".venv"
            | "__pycache__"
            | ".pytest_cache"
            | "dist"
            | "build"
            | "Cargo.lock"
    )
}

fn simple_glob_match(text: &str, pattern: &str) -> bool {
    let p_bytes = pattern.as_bytes();
    let t_bytes = text.as_bytes();
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut match_idx) = (None, 0);
    while ti < t_bytes.len() {
        if pi < p_bytes.len() && (p_bytes[pi] == b'?' || p_bytes[pi] == t_bytes[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p_bytes.len() && p_bytes[pi] == b'*' {
            star = Some(pi);
            match_idx = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            match_idx += 1;
            ti = match_idx;
        } else {
            return false;
        }
    }
    while pi < p_bytes.len() && p_bytes[pi] == b'*' {
        pi += 1;
    }
    pi == p_bytes.len()
}

fn grep_dir(
    root: &Path,
    re: &Regex,
    glob: Option<&str>,
    context: usize,
    depth: usize,
    out: &mut Vec<String>,
) -> Result<(), ToolError> {
    if depth > MAX_WALK_DEPTH {
        return Ok(());
    }
    if root.is_file() {
        if let Some(g) = glob {
            let name = root.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let path_s = root.to_string_lossy();
            if !simple_glob_match(name, g) && !simple_glob_match(&path_s, g) {
                return Ok(());
            }
        }
        if let Ok(content) = fs::read_to_string(root) {
            let lines: Vec<&str> = content.lines().collect();
            for (idx, line) in lines.iter().enumerate() {
                if re.is_match(line) {
                    if context > 0 {
                        let start = idx.saturating_sub(context);
                        let end = (idx + context + 1).min(lines.len());
                        for (c_idx, c_line) in lines.iter().enumerate().take(end).skip(start) {
                            let sep = if c_idx == idx { ':' } else { '-' };
                            out.push(format!("{}:{}{sep} {}", root.display(), c_idx + 1, c_line));
                        }
                    } else {
                        out.push(format!("{}:{}: {}", root.display(), idx + 1, line));
                    }
                    if out.len() >= GREP_MAX_MATCHES {
                        return Ok(());
                    }
                }
            }
        }
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if should_skip_dir(&name_str)
            || entry.file_type().map(|ft| ft.is_symlink()).unwrap_or(false)
        {
            continue;
        }
        grep_dir(&entry.path(), re, glob, context, depth + 1, out)?;
        if out.len() >= GREP_MAX_MATCHES {
            break;
        }
    }
    Ok(())
}

fn run_grep(args: &Value) -> Result<String, ToolError> {
    let pattern = args.get("pattern").and_then(Value::as_str).expect("validated");
    let root = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let case_insensitive = args
        .get("case_insensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let glob = args.get("glob").and_then(Value::as_str);
    let context = args
        .get("context")
        .and_then(Value::as_u64)
        .map(|c| c.min(5) as usize)
        .unwrap_or(0);

    let re = RegexBuilder::new(pattern)
        .case_insensitive(case_insensitive)
        .build()
        .or_else(|_| {
            RegexBuilder::new(&regex::escape(pattern))
                .case_insensitive(case_insensitive)
                .build()
        })
        .map_err(|e| ToolError::Edit(format!("invalid grep pattern: {e}")))?;

    let mut hits = Vec::new();
    grep_dir(Path::new(root), &re, glob, context, 0, &mut hits)?;
    if hits.is_empty() {
        Ok("No matches found.".to_string())
    } else {
        Ok(truncate_output(&hits.join("\n")))
    }
}

fn find_walk(
    root: &Path,
    pattern: Option<&str>,
    depth: usize,
    max_depth: Option<usize>,
    out: &mut Vec<String>,
) -> Result<(), ToolError> {
    if depth > MAX_WALK_DEPTH || max_depth.is_some_and(|m| depth > m) {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if should_skip_dir(&name_str)
            || entry.file_type().map(|ft| ft.is_symlink()).unwrap_or(false)
        {
            continue;
        }
        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();
        let matches = pattern.is_none_or(|pat| {
            simple_glob_match(&name_str, pat)
                || simple_glob_match(&path_str, pat)
                || path_str.contains(pat)
        });
        if matches {
            let suffix = if path.is_dir() { "/" } else { "" };
            out.push(format!("{path_str}{suffix}"));
            if out.len() >= FIND_MAX_RESULTS {
                return Ok(());
            }
        }
        if path.is_dir() {
            find_walk(&path, pattern, depth + 1, max_depth, out)?;
            if out.len() >= FIND_MAX_RESULTS {
                return Ok(());
            }
        }
    }
    Ok(())
}

fn run_find(args: &Value) -> Result<String, ToolError> {
    let root = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let pattern = args.get("pattern").and_then(Value::as_str);
    let max_depth = args.get("max_depth").and_then(Value::as_u64).map(|d| d as usize);
    let mut hits = Vec::new();
    find_walk(Path::new(root), pattern, 0, max_depth, &mut hits)?;
    hits.sort();
    if hits.is_empty() {
        Ok("No files found.".to_string())
    } else {
        Ok(truncate_output(&hits.join("\n")))
    }
}

fn run_ls(args: &Value) -> Result<String, ToolError> {
    let root = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let mut entries = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let suffix = if entry.path().is_dir() { "/" } else { "" };
        entries.push(format!("{}{suffix}", entry.file_name().to_string_lossy()));
    }
    entries.sort();
    Ok(truncate_output(&entries.join("\n")))
}

fn decode_shell_output(bytes: &[u8]) -> String {
    if bytes.len() >= 2
        && bytes.len().is_multiple_of(2)
        && bytes
            .iter()
            .skip(1)
            .step_by(2)
            .take(16)
            .filter(|&&b| b == 0)
            .count()
            >= 4
    {
        let u16s: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        return String::from_utf16_lossy(&u16s).replace(['\0', '\r'], "");
    }
    String::from_utf8_lossy(bytes).replace(['\0', '\r'], "")
}

/// Kill a process **and its descendants**.
///
/// `Child::kill` only signals the shell itself; on Windows the grandchildren keep the pipes open and
/// keep running, which is what pi fixed in #11613. Windows has no process groups here, so `taskkill
/// /T /F` is the reliable tree kill; Unix gets the process group via `kill -TERM -pid`.
fn kill_process_tree(pid: u32) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-TERM", &format!("-{pid}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = pid;
    }
}

/// Replace the trailing truncation marker with an actionable tail notice naming the spill file.
fn tail_notice(output: &str, stdout_buf: &str, stderr_buf: &str) -> String {
    let full = if stderr_buf.is_empty() {
        stdout_buf.to_string()
    } else if stdout_buf.is_empty() {
        stderr_buf.to_string()
    } else {
        format!("{stdout_buf}
{stderr_buf}")
    };
    let cut = truncate::truncate_tail(&full, truncate::DEFAULT_MAX_LINES, truncate::DEFAULT_MAX_BYTES);
    if !cut.truncated {
        return output.to_string();
    }
    let spill = truncate::spill_full_output(&full);
    let first = cut.total_lines.saturating_sub(cut.output_lines) + 1;
    let last = cut.total_lines;
    let where_to = spill
        .map(|p| format!(" Full output: {}", p.display()))
        .unwrap_or_default();
    let reason = if cut.last_line_partial {
        format!(
            "[Showing last {} of line {last}.{where_to}]",
            truncate::format_size(cut.content.len())
        )
    } else if cut.truncated_by == Some(truncate::TruncatedBy::Bytes) {
        format!(
            "[Showing lines {first}-{last} of {} ({} limit).{where_to}]",
            cut.total_lines,
            truncate::format_size(truncate::DEFAULT_MAX_BYTES)
        )
    } else {
        format!("[Showing lines {first}-{last} of {}.{where_to}]", cut.total_lines)
    };
    let mut body = cut.content;
    body.push_str("

");
    body.push_str(&reason);
    body
}

fn run_shell_cancellable(
    shell: &str,
    args: &Value,
    cancel: Option<&AtomicBool>,
) -> Result<String, ToolError> {
    let is_cancelled = |flag: Option<&AtomicBool>| flag.is_some_and(|f| f.load(Ordering::SeqCst));
    if is_cancelled(cancel) {
        return Err(ToolError::Tool("Command aborted".to_string()));
    }
    let cmd = args.get("command").and_then(Value::as_str).expect("validated");
    let timeout_secs = args
        .get("timeout_secs")
        .and_then(Value::as_u64)
        .unwrap_or(60)
        .clamp(1, 600);

    let mut command = if shell == "powershell" || shell == "pwsh" {
        let mut c = Command::new(shell);
        c.args(["-NoProfile", "-Command", cmd]);
        c
    } else if shell == "cmd" {
        let mut c = Command::new("cmd");
        c.args(["/C", cmd]);
        c
    } else {
        let mut c = Command::new(shell);
        c.args(["-c", cmd]);
        c
    };
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let started_at = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait()? {
            break st;
        }
        if Instant::now() >= deadline {
            // Kill the whole tree: the shell alone leaves background children writing to our pipes.
            kill_process_tree(child.id());
            let _ = child.kill();
            let _ = child.wait();
            return Err(ToolError::Tool(format!(
                "command timed out after {timeout_secs}s"
            )));
        }
        if is_cancelled(cancel) {
            kill_process_tree(child.id());
            let _ = child.kill();
            let _ = child.wait();
            return Err(ToolError::Tool("Command aborted".to_string()));
        }
        std::thread::sleep(Duration::from_millis(50));
    };

    let mut stdout_bytes = Vec::new();
    let mut stderr_bytes = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_end(&mut stdout_bytes);
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_end(&mut stderr_bytes);
    }
    let stdout_buf = decode_shell_output(&stdout_bytes);
    let stderr_buf = decode_shell_output(&stderr_bytes);
    let code = status.code().unwrap_or(-1);
    let first_err = if code != 0 {
        stderr_buf
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .map(|l| {
                let mut s: String = l.chars().take(200).collect();
                if l.chars().count() > 200 {
                    s.push_str("...");
                }
                s
            })
    } else {
        None
    };
    let combined = if stderr_buf.is_empty() {
        stdout_buf.clone()
    } else if stdout_buf.is_empty() {
        stderr_buf.clone()
    } else {
        format!("{stdout_buf}\n{stderr_buf}")
    };
    let wall = started_at.elapsed().as_secs_f64();
    if code != 0 {
        // pi reports a non-zero exit as a tool **error** so the UI renders an error card instead of a
        // normal result. The body still starts with the verbatim `exit_code=N` line: VerificationGate
        // parses it (and only accepts `exit_code=0` as proof), and tooling relies on that shape.
        let header = match first_err {
            Some(line) => format!("exit_code={code} | stderr: {line}"),
            None => format!("exit_code={code}"),
        };
        let body = if combined.is_empty() {
            header
        } else {
            format!("{header}\n{combined}")
        };
        let body = tail_notice(&body, &stdout_buf, &stderr_buf);
        return Err(ToolError::Tool(format!(
            "{body}\n\nCommand exited with code {code}\nwall_time_seconds={wall:.1}"
        )));
    }
    // `exit_code=N` stays a standalone first line so VerificationGate keeps parsing it; the wall
    // time rides on its own trailing line (pi exposes it as a structured field instead).
    let body = tail_notice(
        &format!("exit_code={code}\n{combined}"),
        &stdout_buf,
        &stderr_buf,
    );
    Ok(format!("{body}\nwall_time_seconds={wall:.1}"))
}

pub fn execute_builtin_tool(
    policy: &mut dyn PolicyPort,
    name: &str,
    args: &Value,
) -> Result<String, ToolError> {
    let defs = builtin_tool_definitions();
    let def = defs
        .iter()
        .find(|d| d.name == name)
        .ok_or_else(|| ToolError::UnknownTool(name.to_string()))?;
    validate_object_args(&def.parameters, args)?;

    let target_path = args.get("path").and_then(Value::as_str);
    let command = args.get("command").and_then(Value::as_str);
    let decision = policy
        .evaluate_tool_call(name, command, target_path)
        .map_err(|e| ToolError::PolicyDenied(e.to_string()))?;
    match decision.action.as_str() {
        "deny" => return Err(ToolError::PolicyDenied(decision.reason)),
        "ask" => return Err(ToolError::PolicyAsk(decision.reason)),
        _ => {}
    }

    execute_builtin_tool_approved(name, args)
}

/// Execute a validated built-in tool after user approval.
pub fn execute_builtin_tool_approved(name: &str, args: &Value) -> Result<String, ToolError> {
    execute_builtin_tool_approved_cancellable(name, args, None)
}

/// Same as [`execute_builtin_tool_approved`], but a running shell command is killed (with its whole
/// process tree) as soon as `cancel` flips.
pub fn execute_builtin_tool_approved_with_cancel(
    name: &str,
    args: &Value,
    cancel: &AtomicBool,
) -> Result<String, ToolError> {
    execute_builtin_tool_approved_cancellable(name, args, Some(cancel))
}

/// Cancellation-aware dispatch used by the agent loop, which may have no cancel flag at all.
pub fn execute_builtin_tool_approved_cancellable(
    name: &str,
    args: &Value,
    cancel: Option<&AtomicBool>,
) -> Result<String, ToolError> {
    match name {
        "read" => run_read(args),
        "write" => run_write(args),
        "edit" => run_edit(args),
        "grep" => run_grep(args),
        "find" => run_find(args),
        "ls" => run_ls(args),
        "bash" => run_shell_cancellable("bash", args, cancel),
        "powershell" => run_shell_cancellable("powershell", args, cancel),
        "memory_search" | "memory_write" | "memory_read" | "memory_delete" => {
            memory::execute_memory_tool(name, args).map_err(ToolError::Edit)
        }
        "load_skill" => load_skill_by_name(args),
        _ => Err(ToolError::UnknownTool(name.to_string())),
    }
}

fn load_skill_by_name(args: &Value) -> Result<String, ToolError> {
    let raw = args
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("");
    if raw.is_empty() || raw.contains("..") || raw.contains('/') || raw.contains('\\') {
        return Err(ToolError::Edit("Invalid skill name".to_string()));
    }
    let mut roots = vec![
        std::path::PathBuf::from(".le-vibe/skills"),
        std::path::PathBuf::from(".pi/skills"),
    ];
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        let h = std::path::PathBuf::from(home);
        roots.push(h.join(".le-vibe").join("skills"));
        roots.push(h.join(".pi").join("agent").join("skills"));
        roots.push(h.join(".agents").join("skills"));
    }
    for root in roots {
        for cand in [
            root.join(raw).join("SKILL.md"),
            root.join(format!("{raw}.md")),
        ] {
            if cand.is_file()
                && let Ok(body) = fs::read_to_string(&cand)
            {
                return Ok(format!(
                    "# Skill: {raw} ({})\n\n{}",
                    cand.display(),
                    body.trim()
                ));
            }
        }
    }
    Err(ToolError::Edit(format!(
        "Skill `{raw}` not found in project or global skill directories."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use le_vibe_pybridge::{BridgeError, PolicyDecision};

    struct AllowAllPolicy;
    impl PolicyPort for AllowAllPolicy {
        fn evaluate_tool_call(
            &mut self,
            _tool_name: &str,
            _command: Option<&str>,
            _target_path: Option<&str>,
        ) -> Result<PolicyDecision, BridgeError> {
            Ok(PolicyDecision {
                action: "allow".into(),
                reason: "test".into(),
            })
        }
    }

    #[test]
    fn write_edit_grep_ls_and_read_roundtrip() {
        let dir = std::env::temp_dir().join("le_vibe_tools_suite_test");
        let _ = fs::remove_dir_all(&dir);
        let file = dir.join("hello.txt");
        let mut policy = AllowAllPolicy;

        execute_builtin_tool(
            &mut policy,
            "write",
            &json!({"path": file.to_string_lossy(), "content": "line1\nline2\nline3\n"}),
        )
        .unwrap();

        execute_builtin_tool(
            &mut policy,
            "edit",
            &json!({
                "path": file.to_string_lossy(),
                "edits": [
                    {"oldText": "line1", "newText": "LINE_ONE"},
                    {"oldText": "line3", "newText": "LINE_THREE"}
                ]
            }),
        )
        .unwrap();

        let read_slice = execute_builtin_tool(
            &mut policy,
            "read",
            &json!({"path": file.to_string_lossy(), "offset": 2, "limit": 2}),
        )
        .unwrap();
        // offset 2 / limit 2 on a 3-line file covers the rest of the file, so no continuation notice.
        assert_eq!(read_slice, "2: line2\n3: LINE_THREE");

        let grep_hits = execute_builtin_tool(
            &mut policy,
            "grep",
            &json!({"pattern": "LINE_ONE", "path": dir.to_string_lossy()}),
        )
        .unwrap();
        assert!(grep_hits.contains("LINE_ONE"));

        let ls_out = execute_builtin_tool(
            &mut policy,
            "ls",
            &json!({"path": dir.to_string_lossy()}),
        )
        .unwrap();
        assert_eq!(ls_out, "hello.txt");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn todo_store_create_update_list_and_clear() {
        let mut store = TodoStore::new();
        let created = execute_todo_tool(
            &mut store,
            &json!({"action": "create", "tasks": [{"subject": "Step 1"}, {"subject": "Step 2"}]}),
        )
        .unwrap();
        assert!(created.contains("Created #1: Step 1"));
        assert!(created.contains("Created #2: Step 2"));

        let upd = execute_todo_tool(&mut store, &json!({"action": "update", "id": 1, "status": "completed"})).unwrap();
        assert!(upd.contains("completed"));

        let listed = execute_todo_tool(&mut store, &json!({"action": "list"})).unwrap();
        assert!(listed.contains("#1 [completed] Step 1"));
        assert!(listed.contains("#2 [pending] Step 2"));
    }

    #[test]
    fn rejects_overlapping_edits_and_policy_ask_blocks_without_approval() {
        struct AskPolicy;
        impl PolicyPort for AskPolicy {
            fn evaluate_tool_call(
                &mut self,
                _tool_name: &str,
                _command: Option<&str>,
                _target_path: Option<&str>,
            ) -> Result<PolicyDecision, BridgeError> {
                Ok(PolicyDecision {
                    action: "ask".into(),
                    reason: "needs user approval".into(),
                })
            }
        }

        let dir = std::env::temp_dir().join("le_vibe_tools_overlap_test");
        let _ = fs::remove_dir_all(&dir);
        let file = dir.join("overlap.txt");
        let mut allow = AllowAllPolicy;
        execute_builtin_tool(
            &mut allow,
            "write",
            &json!({"path": file.to_string_lossy(), "content": "abcdef"}),
        )
        .unwrap();

        let overlap_res = execute_builtin_tool(
            &mut allow,
            "edit",
            &json!({
                "path": file.to_string_lossy(),
                "edits": [
                    {"oldText": "abcd", "newText": "X"},
                    {"oldText": "cdef", "newText": "Y"}
                ]
            }),
        );
        assert!(matches!(overlap_res, Err(ToolError::Edit(_))));

        let mut ask = AskPolicy;
        let gated = execute_builtin_tool(
            &mut ask,
            "write",
            &json!({"path": file.to_string_lossy(), "content": "overwrite"}),
        );
        assert!(matches!(gated, Err(ToolError::PolicyAsk(_))));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_numbers_lines_grep_regex_glob_find_and_timeout() {
        let dir = std::env::temp_dir().join("le_vibe_tools_phase4_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("target")).unwrap();
        let rs_file = dir.join("sample.rs");
        fs::write(&rs_file, "fn Alpha() {}\nfn beta() {}\nfn gamma() {}\n").unwrap();
        fs::write(dir.join("target").join("ignored.rs"), "fn Alpha() {}\n").unwrap();

        let mut allow = AllowAllPolicy;
        let read_out = execute_builtin_tool(
            &mut allow,
            "read",
            &json!({"path": rs_file.to_string_lossy(), "offset": 2, "limit": 1}),
        )
        .unwrap();
        assert!(read_out.contains("2: fn beta() {}"));
        assert!(
            read_out.contains("[Showing lines 2-2 of 3. Use offset=3 to continue.]"),
            "{read_out}"
        );

        let grep_out = execute_builtin_tool(
            &mut allow,
            "grep",
            &json!({
                "path": dir.to_string_lossy(),
                "pattern": "fn alpha",
                "case_insensitive": true,
                "glob": "*.rs"
            }),
        )
        .unwrap();
        assert!(grep_out.contains("sample.rs:1: fn Alpha() {}"));
        assert!(!grep_out.contains("ignored.rs"));

        let find_out = execute_builtin_tool(
            &mut allow,
            "find",
            &json!({"path": dir.to_string_lossy(), "pattern": "*.rs"}),
        )
        .unwrap();
        assert!(find_out.contains("sample.rs"));
        assert!(!find_out.contains("ignored.rs"));

        let timeout_res = execute_builtin_tool(
            &mut allow,
            "powershell",
            &json!({"command": "Start-Sleep -Seconds 5", "timeout_secs": 1}),
        );
        assert!(matches!(timeout_res, Err(ToolError::Tool(msg)) if msg.contains("timed out")));

        let fail_err = execute_builtin_tool(
            &mut allow,
            "powershell",
            &json!({"command": "[Console]::Error.WriteLine('boom-line-1'); exit 3"}),
        )
        .expect_err("non-zero exit must be a tool error");
        let fail_msg = fail_err.to_string();
        assert!(fail_msg.starts_with("exit_code=3 | stderr: boom-line-1"), "{fail_msg}");
        assert!(fail_msg.contains("Command exited with code 3"), "{fail_msg}");

        let _ = fs::remove_dir_all(&dir);
    }
}
