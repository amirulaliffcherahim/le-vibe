//! Shell tool contract parity with pi's `bash.ts`:
//! - a non-zero exit is an **error** (so the UI renders an error card), while the body still starts
//!   with the verbatim `exit_code=N` line that `VerificationGate` parses;
//! - a timeout kills the whole process tree rather than only the shell;
//! - truncation keeps the **tail** and names the spill file holding the full output.

use le_vibe_tools::execute_builtin_tool_approved;
use serde_json::json;

fn shell() -> &'static str {
    if cfg!(windows) { "powershell" } else { "bash" }
}

fn run(command: &str) -> Result<String, String> {
    let mut args = json!({ "command": command });
    if !cfg!(windows) {
        let _ = &mut args;
    }
    execute_builtin_tool_approved(shell(), &args).map_err(|e| e.to_string())
}

fn run_with_timeout(command: &str, timeout_secs: u64) -> Result<String, String> {
    let args = json!({ "command": command, "timeout_secs": timeout_secs });
    execute_builtin_tool_approved(shell(), &args).map_err(|e| e.to_string())
}

#[test]
fn zero_exit_is_ok_and_keeps_the_exit_code_line() {
    let out = match run("exit 0") {
        Ok(out) => out,
        Err(err) => panic!("a zero exit must succeed: {err}"),
    };
    assert!(out.starts_with("exit_code=0"), "{out}");
}

#[test]
fn non_zero_exit_is_an_error_but_keeps_the_exit_code_body() {
    let err = run("exit 3").expect_err("a failing command must be a tool error");
    assert!(
        err.starts_with("exit_code=3"),
        "the verbatim exit_code line must survive so VerificationGate can parse it: {err}"
    );
    assert!(err.contains("Command exited with code 3"), "{err}");
    assert!(
        !err.contains("exit_code=0"),
        "a failure must never look verified: {err}"
    );
}

#[test]
fn failing_command_still_includes_its_output() {
    let err = if cfg!(windows) {
        run("Write-Output 'boom-detail'; exit 7").expect_err("must fail")
    } else {
        run("echo boom-detail; exit 7").expect_err("must fail")
    };
    assert!(err.starts_with("exit_code=7"), "{err}");
    assert!(err.contains("boom-detail"), "output must not be swallowed: {err}");
}

#[test]
fn timeout_kills_the_process_tree_not_just_the_shell() {
    // The child writes its pid, then both parent and child sleep far past the timeout. If only the
    // shell were killed the child would survive, which is the bug pi fixed in #11613.
    let marker = std::env::temp_dir().join("le_vibe_tree_kill_pid.txt");
    let _ = std::fs::remove_file(&marker);
    let child = if cfg!(windows) {
        format!(
            "$p = Start-Process -PassThru -NoNewWindow powershell -ArgumentList '-NoProfile','-Command','Start-Sleep -Seconds 30'; Set-Content -Path '{}' -Value $p.Id; Start-Sleep -Seconds 30",
            marker.display()
        )
    } else {
        format!(
            "sh -c 'sleep 30' & echo $! > '{}'; sleep 30",
            marker.display()
        )
    };

    let err = run_with_timeout(&child, 1).expect_err("timeout must be an error");
    assert!(err.contains("timed out"), "{err}");

    // Give the tree kill a moment, then check the grandchild is gone.
    std::thread::sleep(std::time::Duration::from_millis(1_200));
    let pid = std::fs::read_to_string(&marker)
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if !pid.is_empty() {
        let alive = if cfg!(windows) {
            std::process::Command::new("tasklist")
                .args(["/FI", &format!("PID eq {pid}")])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid))
                .unwrap_or(false)
        } else {
            std::process::Command::new("kill")
                .args(["-0", &pid])
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        };
        assert!(!alive, "grandchild pid {pid} survived the timeout");
    }
    let _ = std::fs::remove_file(&marker);
}

#[test]
fn large_output_keeps_the_tail_and_names_the_spill_file() {
    let command = if cfg!(windows) {
        "1..4000 | ForEach-Object { \"line$_\" }"
    } else {
        "seq 1 4000"
    };
    let out = run(command).expect("shell ok");
    assert!(out.contains("line4000") || out.contains("\n4000"), "tail must survive:\n{out}");
    assert!(
        !out.contains("line1\n"),
        "head must be dropped so the newest lines fit:\n{out}"
    );
    assert!(
        out.contains("Full output:"),
        "the notice must name the spill file:\n{out}"
    );
}
