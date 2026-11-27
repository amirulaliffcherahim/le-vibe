# le-vibe Architecture

`le-vibe` splits responsibilities cleanly between a **9-crate Rust harness** (`crates/*`) and a **unified Python engine** (`python/le_vibe/`).

```text
+----------------------------------------------------------------------+
|  Rust TUI & CLI (crates/le-vibe-cli, crates/le-vibe-tui)             |
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
|  Python Engine (python/le_vibe/)                                     |
|  - providers/ (vibe, deepseek, agy — streaming, auth, catalog, usage)|
|  - policy/ (PermissionStore, policy.json loader)                     |
|  - extensions/ (ExtensionHost hooks, custom tools, SKILL.md)         |
+----------------------------------------------------------------------+
```

## Crate Map

| Crate | Responsibility |
|-------|----------------|
| `le-vibe-schema` | JSON Schema validation for tool arguments |
| `le-vibe-telemetry` | In-memory span & timing recorder |
| `le-vibe-pybridge` | Stdio NDJSON/JSON-RPC bridge (`PythonBridge`, `ProviderPort`, `PolicyPort`, `ExtensionPort`) |
| `le-vibe-durable` | Append-only JSONL session DAG (`SessionStore`, branching, compaction boundary, fork/clone, tree & label projection, rewind checkpoints) |
| `le-vibe-tools` | Native coding tools (`read`, `write`, `edit`, `grep`, `find`, `ls`, `bash`, `powershell`, `todo`, `ask_user_question`) |
| `le-vibe-mcp` | Stdio JSON-RPC 2.0 MCP client & `McpManager` (`mcp__<server>__<tool>`) |
| `le-vibe-agent` | Event-driven turn loop (`run_turn_loop_cancellable`), retry, context lifecycle (`overflow`, `compact`), and slash-command builders (`workflows`: Proof Loop, Goldfish reviewer, skill distillation, compaction instructions) |
| `le-vibe-tui` | Modular `ratatui` UI (`theme`, `mode`, `commands`, `state`, `editor`, `markdown`, `transcript`, `modal`, `input`, `export`) |
| `le-vibe-cli` | Binary entry point (`cli`, `settings`, `worker`, `rpc`, `main`) |

### Where logic belongs (Thin-CLI rule)

The binary crates dispatch and render; they do not own policy, prompts, or graph walks.

| Concern | Owner |
| --- | --- |
| Prompt text the model sees (Proof Loop, Goldfish, distillation, compaction/handover handoff) | `le-vibe-agent::workflows` |
| Reply evaluation (`PROOF_VERIFIED` / `PROOF_BLOCKED` / attempt cap) | `le-vibe-agent::workflows::evaluate_proof_reply` |
| Refusing an unverified stop | `le-vibe-agent::verification::VerificationGate` |
| When to compact, what to keep, overflow recovery | `le-vibe-agent::{compact, overflow}` |
| Session DAG: branches, compaction boundary, fork, tree rows, labels, rewind checkpoints | `le-vibe-durable` |
| Strings, layout, colours, key hints | `le-vibe-tui` |

Thin-CLI Sweep P0 moved those out of `main.rs` / `worker.rs`; P1 added `VerificationGate`.

### VerificationGate

`le-vibe-agent::verification` tracks two facts per prompt:

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
