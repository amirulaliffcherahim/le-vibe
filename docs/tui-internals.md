# TUI Internals (`crates/le-vibe-tui`)

## Module Layout

- `theme.rs` — 6 full-app palettes (`owl-cyan`, `nord`, `tokyo-night`, `catppuccin`, `gruvbox`, `dracula`)
- `mode.rs` — `AgentMode`, `ALL_SHELLS`, `build_system_prompt`
- `commands.rs` — `BUILTIN_SLASH_COMMANDS` & prefix filter
- `state.rs` — `TuiState`, `TranscriptBlock`, `ActiveModal`, `TuiAction`, `TerminalGuard` (bracketed paste + mouse capture)
- `markdown.rs` — line-by-line Markdown renderer (headings, fenced code cards, inline `` `code` ``, `**bold**`, lists, blockquotes)
- `editor.rs` — auto-wrapping 1–5 line editor (`wrap_editor_lines`) with UTF-8 char-boundary cursor movement
- `transcript.rs` — sleepy owl header, shaded cards, collapsible thinking (`Ctrl+T`) and tools (`Ctrl+O`), live Todo strip, 3-line `pi-footer`
- `modal.rs` — scroll-windowed, width-clamped modals (`/model` live type-to-filter, colored diff preview in `ApprovalModal`, `HelpModal`, `ChangelogModal`, `TreePicker`, `SessionPicker`, `UsageModal`)
- `input.rs` — keyboard, mouse wheel scroll, bracketed paste, and persistent prompt history (`~/.le-vibe/history.txt`)
- `export.rs` — `export_session_markdown` and self-contained dark-theme `export_session_html`

## Event Loop & Smoothness (`crates/le-vibe-cli/src/main.rs`)

- **Dirty-driven redraw**: `terminal.draw` only runs when worker events arrive, input events occur, or the spinner/scroll animation is active.
- **Non-blocking worker**: all LLM streaming, catalog sync, usage queries, shell commands, and MCP calls run on `spawn_tui_worker`.
- **Steering & Cancel**: `Esc` triggers `Arc<AtomicBool>` cancellation mid-turn; typing + `Enter` while busy queues follow-up prompts (`queued_prompts`).
