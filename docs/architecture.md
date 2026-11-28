# lazybug Architecture

`lazybug` splits responsibilities cleanly between a **9-crate Rust harness** (`crates/*`) and a **unified Python engine** (`python/lazybug/`).

```text
+----------------------------------------------------------------------+
|  Rust TUI & CLI (crates/lazybug-cli, crates/lazybug-tui)             |
|  - Dirty-driven 30fps ratatui loop, modals, Markdown, editor         |
|  - Non-blocking worker thread (mpsc::channel)                        |
+----------------------------------|-----------------------------------+
                                   |
                                   v
+----------------------------------------------------------------------+
|  Rust Core (agent, tools, durable, mcp, schema, telemetry)           |
|  - Turn loop, Esc cancellation, 429/5xx retry, mode/model sanitize   |
|  - Native tools (read/write/edit/grep/find/ls/bash/powershell/todo)  |
|  - Append-only JSONL DAG + stdio MCP manager                         |
+----------------------------------|-----------------------------------+
                                   | Stdio NDJSON (PYTHONUTF8=1)
                                   v
+----------------------------------------------------------------------+
|  Python Engine (python/lazybug/)                                     |
|  - providers/ (vibe, deepseek, agy — streaming, auth, catalog, usage)|
|  - policy/ (PermissionStore, policy.json loader)                     |
|  - extensions/ (ExtensionHost hooks, custom tools, SKILL.md)         |
+----------------------------------------------------------------------+
```

## Crate Map

| Crate | Responsibility |
|-------|----------------|
| `lazybug-schema` | JSON Schema validation for tool arguments |
| `lazybug-telemetry` | In-memory span & timing recorder |
| `lazybug-pybridge` | Stdio NDJSON/JSON-RPC bridge (`PythonBridge`, `ProviderPort`, `PolicyPort`, `ExtensionPort`) |
| `lazybug-durable` | Append-only JSONL session DAG (`SessionStore`, branching, compaction boundary, fork/clone, tree & label projection, rewind checkpoints) |
| `lazybug-tools` | Native coding tools (`truncate` owns the 2000-line/50 KB caps)  (`read`, `write`, `edit`, `grep`, `find`, `ls`, `bash`, `powershell`, `todo`, `ask_user_question`) |
| `lazybug-mcp` | Stdio JSON-RPC 2.0 MCP client & `McpManager` (`mcp__<server>__<tool>`) |
| `lazybug-agent` | Event-driven turn loop (`run_turn_loop_cancellable`), retry, context lifecycle (`overflow`, `compact`), and slash-command builders (`workflows`: Proof Loop, Goldfish reviewer, skill distillation, compaction instructions) |
| `lazybug-tui` | Modular `ratatui` UI (`theme`, `mode`, `commands`, `state`, `editor`, `markdown`, `transcript`, `modal`, `input`, `export`) |
| `lazybug-cli` | Binary entry point (`cli`, `settings`, `worker`, `rpc`, `main`) |

### Where logic belongs (Thin-CLI rule)

The binary crates dispatch and render; they do not own policy, prompts, or graph walks.

| Concern | Owner |
| --- | --- |
| Prompt text the model sees (Proof Loop, Goldfish, distillation, compaction/handover handoff) | `lazybug-agent::workflows` |
| Reply evaluation (`PROOF_VERIFIED` / `PROOF_BLOCKED` / attempt cap) | `lazybug-agent::workflows::evaluate_proof_reply` |
| Refusing an unverified stop | `lazybug-agent::verification::VerificationGate` |
| When to compact, what to keep, overflow recovery | `lazybug-agent::{compact, overflow}` |
| Session DAG: branches, compaction boundary, fork, tree rows, labels, rewind checkpoints | `lazybug-durable` |
| Strings, layout, colours, key hints | `lazybug-tui` |

Thin-CLI Sweep P0 moved those out of `main.rs` / `worker.rs`; P1 added `VerificationGate`.

### VerificationGate

`lazybug-agent::verification` tracks two facts per prompt:

- `did_mutate` — a successful `edit` / `write` / `multiedit` / `apply_patch` changed the tree.
- `did_verify` — a `bash` / `powershell` / `shell` / `exec` call **after** that mutation reported
  `exit_code=0`. A later mutation clears it again.

When the model tries to stop with `did_mutate && !did_verify`, the harness appends
`NUDGE_MESSAGE` and continues the turn instead of accepting the answer. It nudges **once** per
prompt and then gives up — a gate that can block forever turns a forgetful model into an infinite
loop, which is worse than an unverified answer.

Notes:
- The success signal is the shell `exit_code=`, **not** the tool's `is_error` flag: `run_shell`
  returns `Ok` with `exit_code=N` in the body, so a red test run is a "successful" tool call.
- Read-only modes (`plan`, `ask`) never arm the gate.
- `settings.json` → `verify_edits` (default `true`) disables it; the config modal exposes the same
  toggle as **Verify Edits**.
