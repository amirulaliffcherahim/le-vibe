//! Policy-checked Codemode script runner (collapses multi-tool workflows into 1 turn).

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const RUNNER_PY: &str = r#"
import json, re, sys, traceback
from pathlib import Path

_out = []
_store = {}
_last_result = None

def text(val):
    _out.append(val if isinstance(val, str) else json.dumps(val, ensure_ascii=False, separators=(",", ":")))

def store(key, val):
    if val is None:
        _store.pop(str(key), None)
    else:
        _store[str(key)] = val

def load(key):
    return _store.get(str(key))

_POS_KEYS = {
    "read": ("path", "offset", "limit"),
    "ls": ("path",),
    "grep": ("pattern", "path"),
    "find": ("pattern", "path"),
    "write": ("path", "content"),
    "edit": ("path", "old_text", "new_text"),
    "bash": ("command",),
    "powershell": ("command",),
    "load_skill": ("name",),
    "web_search": ("query",),
    "fetch_content": ("url", "mode"),
    "retrieve_output": ("id", "query", "offset", "limit"),
    "memory_search": ("query", "limit", "scope"),
}

class _ToolsProxy:
    def __getattr__(self, name):
        def _call(*args, **kwargs):
            if args:
                keys = _POS_KEYS.get(name, ())
                for k, v in zip(keys, args):
                    kwargs.setdefault(k, v)
            if name in ("read", "write", "edit", "ls") and "path" not in kwargs:
                for alias in ("file", "file_path", "filepath", "filename"):
                    if alias in kwargs:
                        kwargs["path"] = kwargs.pop(alias)
                        break
            if name in ("bash", "powershell") and "command" not in kwargs:
                for alias in ("cmd", "script"):
                    if alias in kwargs:
                        kwargs["command"] = kwargs.pop(alias)
                        break
            sys.stdout.write(json.dumps({"call": name, "args": kwargs}, ensure_ascii=False) + "\n")
            sys.stdout.flush()
            line = sys.stdin.readline()
            if not line:
                raise RuntimeError("codemode host closed")
            resp = json.loads(line)
            if resp.get("error"):
                raise RuntimeError(resp["error"])
            global _last_result
            res = resp.get("result", "")
            _last_result = res
            return res
        return _call

tools = _ToolsProxy()

def _print(*args, **kwargs):
    _out.append(" ".join(str(a) for a in args))

script = sys.stdin.readline()
payload = json.loads(script)
code = payload["script"]
ns = {"tools": tools, "text": text, "print": _print, "store": store, "load": load, "json": json, "re": re, "Path": Path}
try:
    exec(compile(code, "<codemode>", "exec"), ns, ns)
    if not _out and _last_result is not None:
        _out.append(_last_result if isinstance(_last_result, str) else json.dumps(_last_result, ensure_ascii=False, separators=(",", ":")))
    sys.stdout.write(json.dumps({"done": True, "output": "\n".join(_out)}, ensure_ascii=False) + "\n")
    sys.stdout.flush()
except Exception as e:
    tb = traceback.format_exc(limit=4)
    sys.stdout.write(json.dumps({"done": True, "output": "\n".join(_out), "error": f"{e}\n{tb}"}, ensure_ascii=False) + "\n")
    sys.stdout.flush()
"#;

fn find_python() -> String {
    if cfg!(windows) {
        for cand in ["python/.venv/Scripts/python.exe", "python.exe", "python"] {
            if std::path::Path::new(cand).is_file() {
                return cand.to_string();
            }
        }
        "python".to_string()
    } else {
        for cand in ["python/.venv/bin/python", "python3", "python"] {
            if std::path::Path::new(cand).is_file() {
                return cand.to_string();
            }
        }
        "python3".to_string()
    }
}

/// Execute a Python codemode script, routing every `tools.<name>(**kwargs)` call through `on_tool_call`
/// so policy, approvals, and hooks still govern every action.
pub fn run_codemode_script(
    script: &str,
    timeout_ms: u64,
    mut on_tool_call: impl FnMut(&str, Value) -> Result<String, String>,
) -> Result<String, String> {
    let started = Instant::now();
    let deadline = Duration::from_millis(timeout_ms.clamp(1_000, 300_000));
    let py = find_python();
    let mut child = Command::new(&py)
        .args(["-u", "-c", RUNNER_PY])
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn codemode runner ({py}): {e}"))?;

    let mut stdin = child.stdin.take().ok_or("codemode stdin unavailable")?;
    let stdout = child.stdout.take().ok_or("codemode stdout unavailable")?;
    let mut reader = BufReader::new(stdout);

    let init_line = json!({ "script": script }).to_string();
    writeln!(stdin, "{init_line}").map_err(|e| format!("codemode write failed: {e}"))?;
    let _ = stdin.flush();

    let mut line = String::new();
    let mut tool_calls_count = 0usize;
    loop {
        if started.elapsed() > deadline {
            let _ = child.kill();
            return Err(format!("Script timed out after {}ms", deadline.as_millis()));
        }
        line.clear();
        let n = reader
            .read_line(&mut line)
            .map_err(|e| format!("codemode read failed: {e}"))?;
        if n == 0 {
            let _ = child.wait();
            return Err("Codemode script exited without returning a result.".to_string());
        }
        let msg: Value = serde_json::from_str(line.trim())
            .map_err(|e| format!("Invalid codemode protocol frame: {e}"))?;

        if msg.get("done").and_then(Value::as_bool) == Some(true) {
            let _ = child.wait();
            let elapsed = started.elapsed().as_secs_f64();
            let out = msg.get("output").and_then(Value::as_str).unwrap_or("");
            if let Some(err) = msg.get("error").and_then(Value::as_str) {
                let mut combined = format!("Script failed ({elapsed:.2}s, {tool_calls_count} tool call(s)):");
                if !out.is_empty() {
                    combined.push('\n');
                    combined.push_str(out);
                }
                combined.push_str("\nScript error:\n");
                combined.push_str(err);
                return Err(combined);
            }
            return Ok(format!(
                "Script completed ({elapsed:.2}s, {tool_calls_count} tool call(s)):\n{out}"
            ));
        }

        if let Some(tool_name) = msg.get("call").and_then(Value::as_str) {
            tool_calls_count += 1;
            let args = msg.get("args").cloned().unwrap_or_else(|| json!({}));
            let resp = match on_tool_call(tool_name, args) {
                Ok(res) => json!({ "result": res }),
                Err(err) => json!({ "error": err }),
            };
            writeln!(stdin, "{resp}").map_err(|e| format!("codemode reply failed: {e}"))?;
            let _ = stdin.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executes_codemode_script_and_proxies_tools() {
        let out = run_codemode_script(
            "a = tools.echo(msg='hello')\nstore('k', a)\ntext(load('k') + ' world')",
            10_000,
            |name, args| {
                assert_eq!(name, "echo");
                Ok(args["msg"].as_str().unwrap_or("").to_string())
            },
        )
        .expect("codemode ok");
        assert!(out.contains("hello world"));
        assert!(out.contains("1 tool call(s)"));
    }
}
