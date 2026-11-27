# TUI Internals (`crates/le-vibe-tui`)

## Pi Parity Contract (P0)

`le-vibe-tui` is a strict leaf crate: it owns every user-visible string, layout rule and colour.
`le-vibe-agent` and `le-vibe-cli` only forward plain values, so a formatting change never touches
the agent loop.

| Concern | Owner | Parity rule |
| --- | --- | --- |
| Tool card header | `transcript::tool_call_header` | `read <path>:<start>-<end>`, `<shell> <command> (timeout Ns)`, `<name> key=json ...`; collapsed `SKILL.md` reads become `[skill] <name> (ctrl+o to expand)` with no body |
| Tool duration | `transcript::format_tool_duration` | `4.2s`, then `1m 30s`, then `2h 3m 4s` |
| Preview budget | `render_block_wrapped` | 10 lines (40 expanded); shell tools keep the **last** 5 and say `... (N earlier lines, ctrl+o to expand)`, other tools keep the **first** and say `... (N more lines, ...)` |
| Status strip | `state::StatusKind` | `Working` / `Retrying (a/max) in Ns... (esc to cancel)` with a live countdown / `Compacting context...` / `Auto-compacting...` / `Context overflow detected, Auto-compacting...` / `Summarizing branch...`; only retries use the warning colour |
| Footer counters | `draw_footer` | `↑ ↓ R<cacheRead> W<cacheWrite> ↺hit% ⚡t/s $cost ◔ctx`; every zero-valued counter is omitted, and `◔ ?/1.0M (auto)` holds until the next turn reports a prompt size |

`le-vibe-agent` emits `ToolCallStart { args_json }`, `ToolCallEnd { duration_ms }` and
`Retry { attempt, max_attempts, delay_ms }`; nothing in the agent layer formats them. The status
strip's row is always allocated, so idle → busy never reflows the editor or footer
(`idle_status_reserves_height` in `tests/render_tests.rs`).

## Module Layout

- `theme.rs` — 9 full-app palettes (`pi`, `owl-cyan`, `rose-pine`, `kanagawa`, `tokyo-night`, `catppuccin`, `nord`, `gruvbox`, `dracula`); `pi` uses Pi's `dark.json` okhsl tokens converted to sRGB and is the default
- `mode.rs` — `AgentMode`, `ALL_SHELLS`, `build_system_prompt`
- `commands.rs` — `BUILTIN_SLASH_COMMANDS` & prefix filter
- `state.rs` — `TuiState`, `TranscriptBlock`, `ActiveModal`, `TuiAction`, `TerminalGuard` (bracketed paste + mouse capture)
- `markdown.rs` — line-by-line Markdown renderer (headings, fenced code cards, inline `` `code` ``, `**bold**`, lists, blockquotes)
- `editor.rs` — auto-wrapping 1–5 line editor (`wrap_editor_lines`) with UTF-8 char-boundary cursor movement
- `transcript.rs` — sleepy owl header, Pi-style full-width tinted user turns (`ThemeId::user_surface`), dim italic thinking, tinted tool cards keyed by status (pending/success/error), live Todo strip, 3-line `pi-footer` with context pressure colors at 70%/90%
- `modal.rs` — scroll-windowed, width-clamped modals (`/model` live type-to-filter, colored diff preview in `ApprovalModal`, `HelpModal`, `ChangelogModal`, `TreePicker`, `SessionPicker`, `UsageModal`)
- `input.rs` — keyboard, mouse wheel scroll, bracketed paste, and persistent prompt history (`~/.le-vibe/history.txt`)
- `export.rs` — `export_session_markdown` and self-contained dark-theme `export_session_html`
- `probe.rs` — headless rendering (`render_frame`) and key-spec parsing (`key_from_spec`) for `le-vibe tui-probe`

## Event Loop & Smoothness (`crates/le-vibe-cli/src/main.rs`)

- **Dirty-driven redraw**: `terminal.draw` only runs when worker events arrive, input events occur, or the spinner/scroll animation is active.
- **Non-blocking worker**: all LLM streaming, catalog sync, usage queries, shell commands, and MCP calls run on `spawn_tui_worker`.
- **Steering & Cancel**: `Esc` triggers `Arc<AtomicBool>` cancellation mid-turn; typing + `Enter` while busy queues follow-up prompts (`queued_prompts`).

## Headless Probe (`le-vibe tui-probe`)

Agents and tests cannot attach to a real terminal, so the probe drives the same `handle_key_event` /
`draw_frame` code paths against a ratatui `TestBackend` and prints the result as plain text.

```bash
le-vibe tui-probe --demo --size 100x30                            # one frame of seeded content
printf 'type /mo\nframe\n' | le-vibe tui-probe --demo --script -   # slash popup after typing
printf 'modal approval\nframe\n' | le-vibe tui-probe --demo --script -
le-vibe tui-probe --demo --key Enter --key Up --json              # NDJSON frames + state
```

Script verbs: `type <text>`, `key <spec>` (`Enter`, `Esc`, `Tab`, `Ctrl+G`, `Alt+Up`, `F12`, `a`),
`wheel up|down`, `resize <WxH>`, `modal <name>`, `busy on|off`, `queue <text>`, `steer <text>`,
`frame`, `state`. After a frame the probe prints `[caret] col=N row=N` when the frame asked for a
visible cursor.

What the probe cannot show: ConPTY/terminal-driver behaviour (raw mode, mouse capture, SGR leaks,
`ResyncBackend` cursor repair, real resize events). Those need a real terminal; use tmux on a Linux
host for key-level reproduction and Windows Terminal for ConPTY-specific reports.
