use le_vibe_durable::SessionStore;
use le_vibe_pybridge::PythonBridge;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn self_repo_path() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string())
}

pub(crate) fn cwd_string() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string())
}

pub(crate) fn resolve_python_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("LE_VIBE_PYTHON_DIR") {
        return PathBuf::from(dir);
    }
    let cwd_python = PathBuf::from("python");
    if cwd_python.exists() {
        return cwd_python;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("python"))
        .unwrap_or(cwd_python)
}

pub(crate) fn resolve_python_exe(python_dir: &Path) -> String {
    let venv_win = python_dir.join(".venv").join("Scripts").join("python.exe");
    if venv_win.exists() {
        return venv_win.to_string_lossy().to_string();
    }
    let venv_unix = python_dir.join(".venv").join("bin").join("python");
    if venv_unix.exists() {
        return venv_unix.to_string_lossy().to_string();
    }
    "python".to_string()
}

pub(crate) fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].clone())
}

/// The model a provider starts on: whatever its live catalog recommends.
/// Nothing is hardcoded, so a provider that is not connected yields no model.
pub(crate) fn default_model_for(bridge: &mut PythonBridge, provider: &str) -> String {
    bridge.default_model(provider).unwrap_or_default()
}

#[allow(dead_code)]
pub(crate) fn provider_arg(args: &[String]) -> String {
    arg_value(args, "--provider").unwrap_or_else(|| "vibe".to_string())
}


pub(crate) fn current_git_branch() -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let b = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if b.is_empty() { None } else { Some(b) }
}

pub(crate) fn current_repo_name() -> String {
    if let Ok(out) = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        && out.status.success()
    {
        let root = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Some(name) = Path::new(&root).file_name().and_then(|s| s.to_str())
            && !name.is_empty()
        {
            return name.to_string();
        }
    }
    std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().and_then(|s| s.to_str()).map(ToString::to_string))
        .unwrap_or_else(|| ".".to_string())
}

pub(crate) fn summarize_git_status() -> String {
    let Ok(out) = Command::new("git")
        .args(["status", "--porcelain=v2", "--branch", "--untracked-files=normal"])
        .output()
    else {
        return String::new();
    };
    if !out.status.success() {
        return String::new();
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let (mut staged, mut unstaged, mut untracked, mut conflicts, mut ahead, mut behind) =
        (0u32, 0u32, 0u32, 0u32, 0u32, 0u32);

    for line in text.lines() {
        if let Some(ab) = line.strip_prefix("# branch.ab ") {
            let mut parts = ab.split_whitespace();
            if let (Some(a), Some(b)) = (parts.next(), parts.next()) {
                ahead = a.trim_start_matches('+').parse().unwrap_or(0);
                behind = b.trim_start_matches('-').parse().unwrap_or(0);
            }
        } else if line.starts_with("? ") {
            untracked += 1;
        } else if line.starts_with("u ") {
            conflicts += 1;
        } else if line.starts_with("1 ") || line.starts_with("2 ") {
            let bytes = line.as_bytes();
            if bytes.len() >= 4 {
                if bytes[2] != b'.' {
                    staged += 1;
                }
                if bytes[3] != b'.' {
                    unstaged += 1;
                }
            }
        }
    }

    let mut badges = Vec::new();
    if conflicts > 0 {
        badges.push(format!("!{conflicts}"));
    }
    if staged > 0 {
        badges.push(format!("+{staged}"));
    }
    if unstaged > 0 {
        badges.push(format!("~{unstaged}"));
    }
    if untracked > 0 {
        badges.push(format!("?{untracked}"));
    }
    if ahead > 0 {
        badges.push(format!("↑{ahead}"));
    }
    if behind > 0 {
        badges.push(format!("↓{behind}"));
    }
    if badges.is_empty() {
        String::new()
    } else {
        format!("[{}]", badges.join(" "))
    }
}

pub(crate) fn format_cwd() -> String {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from);
    if let Some(h) = home
        && let Ok(rel) = cwd.strip_prefix(&h)
    {
        let rel_str = rel.to_string_lossy();
        return if rel_str.is_empty() {
            "~".to_string()
        } else {
            format!("~\\{rel_str}")
        };
    }
    cwd.to_string_lossy().to_string()
}


pub(crate) fn settings_file() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".le-vibe").join("settings.json")
}

pub(crate) fn project_settings_file() -> PathBuf {
    PathBuf::from(".le-vibe").join("settings.json")
}

fn read_settings_map(path: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Ok(raw) = fs::read_to_string(path) else {
        return out;
    };
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&raw) else {
        return out;
    };
    for (k, v) in map {
        let s = match v {
            Value::String(s) => s,
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            _ => continue,
        };
        if !s.trim().is_empty() {
            out.insert(k, s.trim().to_string());
        }
    }
    out
}

/// Loads user (`~/.le-vibe/settings.json`) then project (`.le-vibe/settings.json`),
/// returning `(values, origins)` where origin is `"user"` or `"project"`.
pub(crate) fn load_layered_settings() -> (BTreeMap<String, String>, BTreeMap<String, String>) {
    let mut values = BTreeMap::new();
    let mut origins = BTreeMap::new();
    for (k, v) in read_settings_map(&settings_file()) {
        values.insert(k.clone(), v);
        origins.insert(k, "user".to_string());
    }
    for (k, v) in read_settings_map(&project_settings_file()) {
        values.insert(k.clone(), v);
        origins.insert(k, "project".to_string());
    }
    (values, origins)
}

pub(crate) fn save_setting_for_scope(scope: u8, key: &str, value: &str) -> &'static str {
    let (path, label) = match scope % 3 {
        0 => (Some(settings_file()), "user"),
        1 => (Some(project_settings_file()), "project"),
        _ => (None, "session"),
    };
    if let Some(path) = path {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut obj = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Map<String, Value>>(&s).ok())
            .unwrap_or_default();
        obj.insert(key.to_string(), json!(value));
        let _ = fs::write(
            path,
            serde_json::to_string_pretty(&Value::Object(obj)).unwrap_or_default() + "\n",
        );
    }
    label
}

pub(crate) fn save_setting(key: &str, value: &str) {
    let _ = save_setting_for_scope(0, key, value);
}

pub(crate) fn run_shell_command(shell: &str, command: &str) -> io::Result<std::process::Output> {
    let lower = shell.to_lowercase();
    if lower == "powershell" || lower == "pwsh" || lower.ends_with("powershell.exe") || lower.ends_with("pwsh.exe") {
        Command::new(shell)
            .args(["-NoProfile", "-Command", command])
            .output()
    } else if lower == "cmd" || lower.ends_with("cmd.exe") {
        Command::new(shell).args(["/C", command]).output()
    } else {
        Command::new(shell).args(["-c", command]).output()
    }
}

pub(crate) fn scope_file() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".le-vibe").join("scoped-models.json")
}

pub(crate) fn load_scope() -> Vec<(String, String)> {
    let Ok(raw) = fs::read_to_string(scope_file()) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return Vec::new();
    };
    value
        .get("models")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let provider = row.get("provider")?.as_str()?.to_string();
                    let id = row.get("id")?.as_str()?.to_string();
                    Some((provider, id))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn save_scope(scope: &[(String, String)]) {
    let path = scope_file();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let rows: Vec<Value> = scope
        .iter()
        .map(|(provider, id)| json!({ "provider": provider, "id": id }))
        .collect();
    let _ = fs::write(
        path,
        serde_json::to_string_pretty(&json!({ "models": rows })).unwrap_or_default() + "
",
    );
}

pub(crate) fn project_sessions_dir() -> PathBuf {
    let cwd = cwd_string();
    let slug: String = cwd
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = slug.trim_matches('-');
    let dir_name = if trimmed.is_empty() { "default" } else { trimmed };
    sessions_dir().join(dir_name)
}

pub(crate) fn default_mcp_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        paths.push(PathBuf::from(home).join(".le-vibe").join("mcp.json"));
    }
    paths.push(PathBuf::from(".le-vibe").join("mcp.json"));
    paths
}

pub(crate) fn history_file() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".le-vibe").join("history.txt")
}

pub(crate) fn load_history() -> Vec<String> {
    let Ok(raw) = fs::read_to_string(history_file()) else {
        return Vec::new();
    };
    let mut lines: Vec<String> = raw
        .lines()
        .map(|l| l.replace("\\n", "\n"))
        .filter(|l| !l.trim().is_empty())
        .collect();
    if lines.len() > 500 {
        lines.drain(..lines.len() - 500);
    }
    lines
}

pub(crate) fn save_history(history: &[String]) {
    let path = history_file();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let start = history.len().saturating_sub(500);
    let body: Vec<String> = history[start..]
        .iter()
        .map(|h| h.replace('\n', "\\n"))
        .collect();
    let _ = fs::write(path, body.join("\n") + "\n");
}

pub(crate) fn trust_cwd_in_policy() -> io::Result<PathBuf> {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let path = home.join(".le-vibe").join("policy.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut root: serde_json::Map<String, Value> = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Map<String, Value>>(&s).ok())
        .unwrap_or_default();
    let cwd = cwd_string();
    let mut folders: Vec<String> = root
        .get("trusted_folders")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();
    if !folders.iter().any(|f| f == &cwd) {
        folders.push(cwd);
    }
    root.insert(
        "trusted_folders".to_string(),
        Value::Array(folders.into_iter().map(Value::String).collect()),
    );
    fs::write(
        &path,
        serde_json::to_string_pretty(&Value::Object(root)).unwrap_or_default() + "\n",
    )?;
    Ok(path)
}

pub(crate) fn scan_workspace_files(root: &Path, cap: usize) -> Vec<String> {
    fn walk(base: &Path, cur: &Path, depth: usize, cap: usize, out: &mut Vec<String>) {
        if depth > 24 || out.len() >= cap {
            return;
        }
        let Ok(entries) = fs::read_dir(cur) else {
            return;
        };
        for entry in entries.flatten() {
            if entry.file_type().map(|ft| ft.is_symlink()).unwrap_or(false) {
                continue;
            }
            let name = entry.file_name();
            let name_s = name.to_string_lossy();
            if matches!(
                name_s.as_ref(),
                ".git" | "target" | "node_modules" | ".venv" | "__pycache__" | "dist" | "build"
            ) {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, depth + 1, cap, out);
            } else if let Ok(rel) = path.strip_prefix(base) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
                if out.len() >= cap {
                    return;
                }
            }
        }
    }
    let mut files = Vec::new();
    walk(root, root, 0, cap, &mut files);
    files.sort();
    files
}

pub(crate) fn sessions_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".le-vibe").join("sessions")
}

pub(crate) fn open_session_with_opts(opts: &crate::cli::CliOptions) -> SessionStore {
    if opts.no_session {
        return SessionStore::in_memory();
    }
    if let Some(path) = &opts.session_path {
        return SessionStore::open(path).unwrap_or_else(|_| SessionStore::in_memory());
    }
    let dir = opts
        .session_dir
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(project_sessions_dir);
    if let Some(fork_id) = &opts.fork_id {
        let src = if Path::new(fork_id).exists() {
            PathBuf::from(fork_id)
        } else {
            dir.join(format!("{fork_id}.jsonl"))
        };
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let dst = dir.join(format!("session-{ts}.jsonl"));
        if let Ok(orig) = SessionStore::open(&src)
            && let Ok(mut forked) = SessionStore::open(&dst)
        {
            for entry in orig.active_branch() {
                let _ = forked.append(entry.role, entry.content);
            }
            return forked;
        }
    }
    if let Some(id) = &opts.resume_id
        && !id.is_empty()
    {
        let p = dir.join(format!("{id}.jsonl"));
        return SessionStore::open(p).unwrap_or_else(|_| SessionStore::in_memory());
    }
    if opts.continue_session || opts.resume_id.as_deref() == Some("") {
        for search_dir in [&dir, &sessions_dir()] {
            if let Ok(entries) = fs::read_dir(search_dir) {
                let mut files: Vec<PathBuf> = entries
                    .filter_map(|e| e.ok().map(|x| x.path()))
                    .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("jsonl"))
                    .collect();
                files.sort();
                if let Some(latest) = files.pop()
                    && let Ok(store) = SessionStore::open(latest)
                {
                    return store;
                }
            }
        }
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let p = dir.join(format!("session-{ts}.jsonl"));
    SessionStore::open(p).unwrap_or_else(|_| SessionStore::in_memory())
}

