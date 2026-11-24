# le-vibe Development Rules

- **Architecture**: 9-crate Rust workspace (`crates/*`) for harness, TUI, tools, RAG memory, Headroom-lite compression, MCP, session DAG, and CLI. Python (`python/le_vibe/{providers,policy,extensions,bridge.py}`) stays strictly for providers (`vibe`, `deepseek`, `agy`), policy, web tools, and extensions.
- **Zero bloat**: No unnecessary crates, proxies, or heavy runtime dependencies. Keep diffs minimal and surgical.
- **Quality gate (required before PR)**:
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo clippy -p le-vibe-cli --no-default-features -- -D warnings`
  - `cargo test --workspace`
  - `uv run --project python --with pytest --with pytest-asyncio --with httpx --with pydantic pytest python/tests`
- **Release builds**: Always compile official releases with `cargo build --release --no-default-features -p le-vibe-cli` (excludes `dev-debug` event logger & `tui-probe`).
- **Git**: `main` is PR-only. Branch off `main`, stage explicit paths, never force-push or commit personal skills/extensions/secrets.
