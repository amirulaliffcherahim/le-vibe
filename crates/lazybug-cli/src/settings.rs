use lazybug_durable::SessionStore;
use lazybug_pybridge::PythonBridge;
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
    if let Ok(dir) = std::env::var("LAZYBUG_PYTHON_DIR") {
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
    home.join(".lazybug").join("settings.json")
}

pub(crate) fn project_settings_file() -> PathBuf {
    PathBuf::from(".lazybug").join("settings.json")
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
            // A list of strings (`"tools": ["read", "grep"]`) joins to the same comma form
            // the `--tools` flag uses, so both spellings parse identically downstream.
            Value::Array(items) => items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(","),
            _ => continue,
        };
        if !s.trim().is_empty() {
            out.insert(k, s.trim().to_string());
        }
    }
    out
}

/// The `tools` setting: a pinned tool set that replaces the mode default, so a session only pays
/// the schema bytes it actually uses. Accepts a JSON array or the comma form `--tools` takes;
/// the flag on the command line still wins over it.
pub(crate) fn pinned_tools() -> Option<Vec<String>> {
    parse_tool_list(load_layered_settings().0.get("tools")?)
}

fn parse_tool_list(raw: &str) -> Option<Vec<String>> {
    let tools: Vec<String> = raw
        .split([',', ' '])
        .map(|s| s.trim().trim_matches(['"', '[', ']']).to_string())
        .filter(|s| !s.is_empty())
        .collect();
    (!tools.is_empty()).then_some(tools)
}

/// Loads user (`~/.lazybug/settings.json`) then project (`.lazybug/settings.json`),
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
    home.join(".lazybug").join("scoped-models.json")
}

pub(crate) fn load_scope() -> Vec<(String, String)> {
    let parse_rows = |arr: &Vec<Value>| -> Vec<(String, String)> {
        arr.iter()
            .filter_map(|row| {
                let provider = row.get("provider")?.as_str()?.to_string();
                let id = row.get("id")?.as_str()?.to_string();
                Some((provider, id))
            })
            .collect()
    };
    if let Ok(raw) = fs::read_to_string(settings_file())
        && let Ok(val) = serde_json::from_str::<Value>(&raw)
        && let Some(rows) = val.get("scoped_models").and_then(Value::as_array)
    {
        return parse_rows(rows);
    }
    let Ok(raw) = fs::read_to_string(scope_file()) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&raw) else {
        return Vec::new();
    };
    value
        .get("models")
        .and_then(Value::as_array)
        .map(parse_rows)
        .unwrap_or_default()
}

pub(crate) fn save_scope(scope: &[(String, String)]) {
    let path = settings_file();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut obj = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Map<String, Value>>(&s).ok())
        .unwrap_or_default();
    let rows: Vec<Value> = scope
        .iter()
        .map(|(provider, id)| json!({ "provider": provider, "id": id }))
        .collect();
    obj.insert("scoped_models".to_string(), Value::Array(rows));
    let _ = fs::write(
        path,
        serde_json::to_string_pretty(&Value::Object(obj)).unwrap_or_default() + "\n",
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
        paths.push(PathBuf::from(home).join(".lazybug").join("mcp.json"));
    }
    paths.push(PathBuf::from(".lazybug").join("mcp.json"));
    paths
}

pub(crate) fn default_skill_dirs() -> Vec<String> {
    let mut dirs = vec![
        ".lazybug/skill".to_string(),
        ".lazybug/skills".to_string(),
        ".agents/skill".to_string(),
        ".agents/skills".to_string(),
    ];
    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        let h = PathBuf::from(home);
        dirs.push(h.join(".lazybug").join("skill").display().to_string());
        dirs.push(h.join(".lazybug").join("skills").display().to_string());
        dirs.push(h.join(".agents").join("skill").display().to_string());
        dirs.push(h.join(".agents").join("skills").display().to_string());
    }
    dirs
}

pub(crate) fn history_file() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".lazybug").join("history.txt")
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
    let path = home.join(".lazybug").join("policy.json");
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
    home.join(".lazybug").join("sessions")
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


pub(crate) fn models_cache_file() -> PathBuf {
    settings_file()
        .parent()
        .map(|p| p.join("models-cache.json"))
        .unwrap_or_else(|| PathBuf::from(".lazybug").join("models-cache.json"))
}

pub(crate) fn resolve_initial_model_offline(
    provider_flag: Option<&str>,
    model_flag: Option<&str>,
    layered: &BTreeMap<String, String>,
) -> (String, String) {
    resolve_initial_model_offline_from_cache(
        provider_flag,
        model_flag,
        layered,
        &models_cache_file(),
    )
}

pub(crate) fn resolve_initial_model_offline_from_cache(
    provider_flag: Option<&str>,
    model_flag: Option<&str>,
    layered: &BTreeMap<String, String>,
    cache_path: &Path,
) -> (String, String) {
    let strip_thinking = |raw: &str| -> String {
        if let Some((base, suffix)) = raw.rsplit_once(':')
            && matches!(
                suffix,
                "off" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
            )
        {
            return base.to_string();
        }
        raw.to_string()
    };

    if let Some(m) = model_flag.map(str::trim).filter(|s| !s.is_empty()) {
        let clean = strip_thinking(m);
        if let Some((p, id)) = clean.split_once('/') {
            return (p.trim().to_string(), id.trim().to_string());
        }
        let prov = provider_flag
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or_else(|| layered.get("provider").map(String::as_str))
            .unwrap_or("vibe")
            .to_string();
        return (prov, clean);
    }

    let provider = provider_flag
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| layered.get("provider").map(String::as_str))
        .unwrap_or("vibe")
        .to_string();

    if let Some(saved_m) = layered.get("model").map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let clean = strip_thinking(saved_m);
        if let Some((p, id)) = clean.split_once('/') {
            if p.eq_ignore_ascii_case(&provider) || provider_flag.is_none() {
                return (p.trim().to_string(), id.trim().to_string());
            }
        } else {
            return (provider, clean);
        }
    }

    if let Ok(raw) = fs::read_to_string(cache_path)
        && let Ok(val) = serde_json::from_str::<Value>(&raw)
        && let Some(arr) = val
            .get("models")
            .and_then(|m| m.get(&provider))
            .and_then(Value::as_array)
    {
        if let Some(rec) = arr
            .iter()
            .find(|item| item.get("recommended").and_then(Value::as_bool) == Some(true))
            .and_then(|item| item.get("id").and_then(Value::as_str))
        {
            return (provider, rec.to_string());
        }
        if let Some(first) = arr
            .first()
            .and_then(|item| item.get("id").and_then(Value::as_str))
        {
            return (provider, first.to_string());
        }
    }

    let fallback = match provider.as_str() {
        "deepseek" => "deepseek-flash",
        "agy" => "gemini-4-neon",
        _ => "deepseek-v4-1-flash-1017",
    };
    (provider, fallback.to_string())
}

pub(crate) fn discover_modes() -> Vec<lazybug_tui::ModeSpec> {
    let mut dirs = Vec::new();
    if let Some(base) = settings_file().parent() {
        dirs.push(base.join("modes"));
        dirs.push(base.join("mode"));
        if let Some(home) = base.parent() {
            dirs.push(home.join(".agents").join("modes"));
            dirs.push(home.join(".agents").join("mode"));
        }
    } else {
        dirs.push(PathBuf::from(".lazybug").join("modes"));
        dirs.push(PathBuf::from(".lazybug").join("mode"));
    }
    dirs.push(PathBuf::from(".agents").join("modes"));
    dirs.push(PathBuf::from(".agents").join("mode"));
    dirs.push(PathBuf::from(".lazybug").join("modes"));
    dirs.push(PathBuf::from(".lazybug").join("mode"));
    discover_modes_from_dirs(&dirs)
}

pub(crate) fn discover_modes_from_dirs(dirs: &[PathBuf]) -> Vec<lazybug_tui::ModeSpec> {
    let mut specs = lazybug_tui::ModeSpec::builtins();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|x| x.path()))
            .filter(|p| p.is_file() && p.extension().and_then(|s| s.to_str()) == Some("md"))
            .collect();
        files.sort();
        for path in files {
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let parsed = lazybug_tui::parse_mode_markdown(stem, &raw);
            if let Some(existing) = specs
                .iter_mut()
                .find(|m| m.id.eq_ignore_ascii_case(&parsed.id))
            {
                *existing = parsed;
            } else {
                specs.push(parsed);
            }
        }
    }
    specs
}

pub(crate) fn subagent_personas_from_modes(
    specs: &[lazybug_tui::ModeSpec],
) -> Vec<lazybug_agent::SubagentPersona> {
    specs
        .iter()
        .filter(|m| m.scope.is_subagent())
        .map(|m| lazybug_agent::SubagentPersona {
            id: m.id.clone(),
            system_prompt: m.body.clone(),
            allowed_tools: m.resolve_allowed_tools(),
            model: m.model.clone(),
            thinking: m.thinking.clone(),
            max_turns: m.max_turns,
            verify_edits: m.verify_edits,
        })
        .collect()
}

#[cfg(test)]
mod pinned_tools_tests {
    use super::parse_tool_list;

    #[test]
    fn accepts_comma_space_and_json_array_forms() {
        let want = vec!["read".to_string(), "grep".to_string()];
        assert_eq!(parse_tool_list("read,grep").unwrap(), want);
        assert_eq!(parse_tool_list(" read , grep ").unwrap(), want);
        assert_eq!(parse_tool_list(r#"["read", "grep"]"#).unwrap(), want);
        assert_eq!(parse_tool_list("read grep").unwrap(), want);
    }

    #[test]
    fn empty_or_blank_pins_nothing() {
        assert_eq!(parse_tool_list(""), None);
        assert_eq!(parse_tool_list("  ,  "), None);
    }

    #[test]
    fn mode_discovery_precedence_project_overrides_global_and_builtin() {
        let root = std::env::temp_dir().join(format!("lazybug_modes_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let global = root.join("global");
        let project = root.join("project");
        std::fs::create_dir_all(&global).unwrap();
        std::fs::create_dir_all(&project).unwrap();

        std::fs::write(
            global.join("plan.md"),
            "---\ndescription: Global plan\n---\nGlobal body.",
        )
        .unwrap();
        std::fs::write(
            project.join("plan.md"),
            "---\ndescription: Project plan\nmodel: vibe/mistral-large-4-0\n---\nProject body.",
        )
        .unwrap();
        std::fs::write(
            project.join("reviewer.md"),
            "---\ndescription: Reviewer\nscope: subagent\ncan_write: false\n---\nAudit diff.",
        )
        .unwrap();

        let specs = super::discover_modes_from_dirs(&[global, project]);
        let plan = specs.iter().find(|m| m.id == "plan").unwrap();
        assert_eq!(plan.description, "Project plan");
        assert_eq!(plan.model.as_deref(), Some("vibe/mistral-large-4-0"));
        assert_eq!(plan.body, "Project body.");

        let rev = specs.iter().find(|m| m.id == "reviewer").unwrap();
        assert_eq!(rev.scope, lazybug_tui::ModeScope::Subagent);
        assert!(!rev.can_write);

        let personas = super::subagent_personas_from_modes(&specs);
        assert!(personas.iter().any(|p| p.id == "reviewer"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_initial_model_offline_reads_models_cache_without_python() {
        let dir = std::env::temp_dir().join(format!("lazybug_cache_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let cache = dir.join("models-cache.json");
        std::fs::write(
            &cache,
            r#"{"models":{"vibe":[{"id":"deepseek-v4-1-flash-1017","recommended":true},{"id":"mistral-large-4-0"}]}}"#,
        )
        .unwrap();

        let (prov, model) = super::resolve_initial_model_offline_from_cache(
            Some("vibe"),
            None,
            &std::collections::BTreeMap::new(),
            &cache,
        );
        assert_eq!(prov, "vibe");
        assert_eq!(model, "deepseek-v4-1-flash-1017");

        // Explicit --model flag wins over cache.
        let (prov2, model2) = super::resolve_initial_model_offline_from_cache(
            None,
            Some("agy/gemini-4-neon"),
            &std::collections::BTreeMap::new(),
            &cache,
        );
        assert_eq!(prov2, "agy");
        assert_eq!(model2, "gemini-4-neon");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
