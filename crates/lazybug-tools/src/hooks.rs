//! Declarative lifecycle hooks (`PreToolUse`, `PostToolUse`, `Stop`) loaded from `hooks.json`.

use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Deserialize)]
pub struct HookRule {
    #[serde(default = "default_matcher")]
    pub matcher: String,
    pub command: String,
}

fn default_matcher() -> String {
    "*".to_string()
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct HooksFile {
    #[serde(rename = "PreToolUse", default)]
    pub pre_tool_use: Vec<HookRule>,
    #[serde(rename = "PostToolUse", default)]
    pub post_tool_use: Vec<HookRule>,
    #[serde(rename = "Stop", default)]
    pub stop: Vec<HookRule>,
}

#[derive(Debug, Clone, Default)]
pub struct HookRegistry {
    pub pre_tool_use: Vec<HookRule>,
    pub post_tool_use: Vec<HookRule>,
    pub stop: Vec<HookRule>,
}

impl HookRegistry {
    /// Load user `~/.lazybug/hooks.json` and optionally project `.lazybug/hooks.json`.
    pub fn load_default(cwd: &Path, project_trusted: bool) -> Self {
        let mut paths = Vec::new();
        if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            paths.push(PathBuf::from(home).join(".lazybug").join("hooks.json"));
        }
        if project_trusted {
            paths.push(cwd.join(".lazybug").join("hooks.json"));
        }
        Self::load_from_paths(&paths)
    }

    pub fn load_from_paths(paths: &[PathBuf]) -> Self {
        let mut reg = Self::default();
        for p in paths {
            let Ok(raw) = fs::read_to_string(p) else {
                continue;
            };
            let Ok(parsed) = serde_json::from_str::<HooksFile>(&raw) else {
                continue;
            };
            reg.pre_tool_use.extend(parsed.pre_tool_use);
            reg.post_tool_use.extend(parsed.post_tool_use);
            reg.stop.extend(parsed.stop);
        }
        reg
    }

    /// Run matching `PreToolUse` hooks. Non-zero exit blocks the tool call with stdout/stderr.
    pub fn run_pre_tool(&self, tool_name: &str, args_json: &str) -> Result<(), String> {
        for rule in &self.pre_tool_use {
            if !matcher_matches(&rule.matcher, tool_name) {
                continue;
            }
            let (ok, output) = exec_hook(&rule.command, "PreToolUse", tool_name, args_json);
            if !ok {
                let msg = if output.trim().is_empty() {
                    format!("Blocked by PreToolUse hook `{}`", rule.command)
                } else {
                    format!("Blocked by PreToolUse hook: {}", output.trim())
                };
                return Err(msg);
            }
        }
        Ok(())
    }

    /// Run matching `PostToolUse` hooks and return any non-empty output to append.
    pub fn run_post_tool(&self, tool_name: &str, args_json: &str) -> Option<String> {
        let mut notes = Vec::new();
        for rule in &self.post_tool_use {
            if !matcher_matches(&rule.matcher, tool_name) {
                continue;
            }
            let (_ok, output) = exec_hook(&rule.command, "PostToolUse", tool_name, args_json);
            let trimmed = output.trim();
            if !trimmed.is_empty() {
                notes.push(trimmed.to_string());
            }
        }
        if notes.is_empty() {
            None
        } else {
            Some(notes.join("\n"))
        }
    }

    /// Run `Stop` hooks once at the end of a turn.
    pub fn run_stop(&self) {
        for rule in &self.stop {
            let _ = exec_hook(&rule.command, "Stop", "", "{}");
        }
    }
}

/// Match pipe-separated tool names or `*` wildcard (e.g. `"write|edit"`, `"*"`, `"bash"`).
pub fn matcher_matches(matcher: &str, tool_name: &str) -> bool {
    let m = matcher.trim();
    if m.is_empty() || m == "*" {
        return true;
    }
    m.split('|')
        .map(str::trim)
        .any(|part| part == "*" || part.eq_ignore_ascii_case(tool_name))
}

fn exec_hook(command: &str, event: &str, tool_name: &str, args_json: &str) -> (bool, String) {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("powershell");
        c.args(["-NoProfile", "-Command", command]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-c", command]);
        c
    };
    let out = cmd
        .env("LAZYBUG_HOOK_EVENT", event)
        .env("LAZYBUG_TOOL_NAME", tool_name)
        .env("LAZYBUG_TOOL_ARGS", args_json)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();
    match out {
        Ok(o) => {
            let mut combined = String::from_utf8_lossy(&o.stdout).to_string();
            let err = String::from_utf8_lossy(&o.stderr);
            if !err.trim().is_empty() {
                if !combined.is_empty() {
                    combined.push('\n');
                }
                combined.push_str(&err);
            }
            (o.status.success(), combined)
        }
        Err(e) => (false, format!("hook spawn failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_pipe_and_wildcard_patterns() {
        assert!(matcher_matches("*", "bash"));
        assert!(matcher_matches("write|edit", "edit"));
        assert!(matcher_matches("write | edit", "WRITE"));
        assert!(!matcher_matches("write|edit", "read"));
    }

    #[test]
    fn parses_hooks_json_file() {
        let dir = std::env::temp_dir().join("lazybug-hooks-test");
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("hooks.json");
        fs::write(
            &file,
            r#"{"PreToolUse":[{"matcher":"bash","command":"echo ok"}],"Stop":[{"command":"echo stop"}]}"#,
        )
        .unwrap();
        let reg = HookRegistry::load_from_paths(std::slice::from_ref(&file));
        assert_eq!(reg.pre_tool_use.len(), 1);
        assert_eq!(reg.stop.len(), 1);
        let _ = fs::remove_file(file);
    }
}
