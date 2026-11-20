use le_vibe_pybridge::{PolicyPort, ToolDefinition};
use le_vibe_schema::validate_object_args;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::Command;
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
            description: "Search for a pattern in files under a path".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["pattern"],
                "properties": {
                    "pattern": { "type": "string" },
                    "path": { "type": "string" }
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
                    "command": { "type": "string" }
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
                    "command": { "type": "string" }
                }
            }),
        },
        ToolDefinition {
            name: "todo".to_string(),
            description: "Manage a multi-step task list (create, update, list, get, delete, clear)".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["action"],
                "properties": {
                    "action": { "type": "string" },
                    "id": { "type": "integer" },
                    "subject": { "type": "string" },
                    "description": { "type": "string" },
                    "status": { "type": "string" },
                    "tasks": { "type": "array" }
                }
            }),
        },
        ToolDefinition {
            name: "ask_user_question".to_string(),
            description: "Ask the user 1-4 structured clarifying questions with 2-4 options each".to_string(),
            parameters: json!({
                "type": "object",
                "required": ["questions"],
                "properties": {
                    "questions": { "type": "array" }
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
    pub status: String,
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
}

pub fn execute_todo_tool(store: &mut TodoStore, args: &Value) -> Result<String, ToolError> {
    let action = args
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::Edit("todo.action must be a string".into()))?;
    match action {
        "create" => {
            if let Some(tasks) = args.get("tasks").and_then(Value::as_array) {
                let mut created = Vec::new();
                for t in tasks {
                    let subject = t
                        .get("subject")
                        .and_then(Value::as_str)
                        .unwrap_or("Untitled task")
                        .to_string();
                    let description = t
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    store.next_id += 1;
                    let id = store.next_id;
                    store.items.push(TodoItem {
                        id,
                        subject: subject.clone(),
                        description,
                        status: "pending".to_string(),
                    });
                    created.push(format!("Created #{id}: {subject} (pending)"));
                }
                return Ok(created.join("\n"));
            }
            let subject = args
                .get("subject")
                .and_then(Value::as_str)
                .unwrap_or("Untitled task")
                .to_string();
            let description = args
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            store.next_id += 1;
            let id = store.next_id;
            store.items.push(TodoItem {
                id,
                subject: subject.clone(),
                description,
                status: "pending".to_string(),
            });
            Ok(format!("Created #{id}: {subject} (pending)"))
        }
        "update" => {
            let id = args
                .get("id")
                .and_then(Value::as_u64)
                .ok_or_else(|| ToolError::Edit("todo.id required for update".into()))?;
            let item = store
                .items
                .iter_mut()
                .find(|i| i.id == id)
                .ok_or_else(|| ToolError::Edit(format!("todo #{id} not found")))?;
            let prev = item.status.clone();
            if let Some(st) = args.get("status").and_then(Value::as_str) {
                item.status = st.to_string();
            }
            if let Some(sub) = args.get("subject").and_then(Value::as_str) {
                item.subject = sub.to_string();
            }
            if let Some(desc) = args.get("description").and_then(Value::as_str) {
                item.description = desc.to_string();
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
            Ok(format!(
                "#{} [{}] {}\n{}",
                item.id, item.status, item.subject, item.description
            ))
        }
        _ => {
            let filter_status = args.get("status").and_then(Value::as_str);
            let lines: Vec<String> = store
                .items
                .iter()
                .filter(|i| i.status != "deleted")
                .filter(|i| filter_status.is_none_or(|s| i.status == s))
                .map(|i| format!("#{} [{}] {}", i.id, i.status, i.subject))
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
    let offset = args
        .get("offset")
        .and_then(Value::as_u64)
        .map(|v| v.saturating_sub(1) as usize)
        .unwrap_or(0);
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(MAX_LINES);
    let slice: Vec<&str> = all_lines.into_iter().skip(offset).take(limit).collect();
    Ok(truncate_output(&slice.join("\n")))
}

fn run_write(args: &Value) -> Result<String, ToolError> {
    let path = args.get("path").and_then(Value::as_str).expect("validated");
    let content = args.get("content").and_then(Value::as_str).expect("validated");
    if let Some(parent) = Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(format!("Wrote {} bytes to {path}", content.len()))
}

fn run_edit(args: &Value) -> Result<String, ToolError> {
    let path = args.get("path").and_then(Value::as_str).expect("validated");
    let edits = args
        .get("edits")
        .and_then(Value::as_array)
        .ok_or_else(|| ToolError::Edit("edits must be an array".into()))?;

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

fn grep_dir(root: &Path, pattern: &str, out: &mut Vec<String>) -> Result<(), ToolError> {
    if root.is_file() {
        if let Ok(content) = fs::read_to_string(root) {
            for (idx, line) in content.lines().enumerate() {
                if line.contains(pattern) {
                    out.push(format!("{}:{}: {}", root.display(), idx + 1, line));
                    if out.len() >= MAX_LINES {
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
        if matches!(name_str.as_ref(), ".git" | "node_modules" | "target" | ".venv") {
            continue;
        }
        grep_dir(&entry.path(), pattern, out)?;
        if out.len() >= MAX_LINES {
            break;
        }
    }
    Ok(())
}

fn run_grep(args: &Value) -> Result<String, ToolError> {
    let pattern = args.get("pattern").and_then(Value::as_str).expect("validated");
    let root = args.get("path").and_then(Value::as_str).unwrap_or(".");
    let mut hits = Vec::new();
    grep_dir(Path::new(root), pattern, &mut hits)?;
    if hits.is_empty() {
        Ok("No matches found.".to_string())
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

fn run_shell(shell: &str, args: &Value) -> Result<String, ToolError> {
    let cmd = args.get("command").and_then(Value::as_str).expect("validated");
    let output = if shell == "powershell" {
        Command::new("powershell")
            .args(["-NoProfile", "-Command", cmd])
            .output()?
    } else {
        Command::new("bash").args(["-c", cmd]).output()?
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = if stderr.is_empty() {
        stdout.to_string()
    } else if stdout.is_empty() {
        stderr.to_string()
    } else {
        format!("{stdout}\n{stderr}")
    };
    let code = output.status.code().unwrap_or(-1);
    Ok(truncate_output(&format!("exit_code={code}\n{combined}")))
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
    match name {
        "read" => run_read(args),
        "write" => run_write(args),
        "edit" => run_edit(args),
        "grep" => run_grep(args),
        "ls" => run_ls(args),
        "bash" => run_shell("bash", args),
        "powershell" => run_shell("powershell", args),
        _ => Err(ToolError::UnknownTool(name.to_string())),
    }
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
        assert_eq!(read_slice, "line2\nLINE_THREE");

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
}
