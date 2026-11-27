//! `Esc` must be able to stop a running shell command.
//!
//! The cancel flag already flows through the agent; `run_shell` never looked at it, so a runaway
//! command could only be stopped by killing the terminal.

use le_vibe_tools::execute_builtin_tool_approved_with_cancel;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

fn shell() -> &'static str {
    if cfg!(windows) { "powershell" } else { "bash" }
}

#[test]
fn an_already_cancelled_turn_does_not_start_the_command() {
    let cancel = Arc::new(AtomicBool::new(true));
    let started = std::time::Instant::now();
    let err = execute_builtin_tool_approved_with_cancel(
        shell(),
        &json!({ "command": if cfg!(windows) { "Start-Sleep -Seconds 30" } else { "sleep 30" } }),
        &cancel,
    )
    .expect_err("cancelled work must not run");
    assert!(err.to_string().contains("Command aborted"), "{err}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "must abort before running, took {:?}",
        started.elapsed()
    );
}

#[test]
fn cancelling_mid_flight_kills_the_command_promptly() {
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        flag.store(true, Ordering::SeqCst);
    });

    let started = std::time::Instant::now();
    let err = execute_builtin_tool_approved_with_cancel(
        shell(),
        &json!({ "command": if cfg!(windows) { "Start-Sleep -Seconds 30" } else { "sleep 30" } }),
        &cancel,
    )
    .expect_err("aborted command must be an error");
    let elapsed = started.elapsed();
    assert!(err.to_string().contains("Command aborted"), "{err}");
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "abort must interrupt the wait, took {elapsed:?}"
    );
}

#[test]
fn a_finished_command_is_not_reported_as_aborted() {
    let cancel = Arc::new(AtomicBool::new(false));
    let out = execute_builtin_tool_approved_with_cancel(
        shell(),
        &json!({ "command": "exit 0" }),
        &cancel,
    )
    .expect("plain command");
    assert!(out.starts_with("exit_code=0"), "{out}");
    assert!(!out.contains("aborted"), "{out}");
}
