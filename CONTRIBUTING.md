# Contributing to lazybug

Contributions to **lazybug** are welcome.

## Architecture
- **Rust Harness (`crates/*`)**: `schema`, `telemetry`, `pybridge`, `durable`, `tools`, `mcp`, `agent`, `tui`, `cli`.
- **Python Engine (`python/lazybug/*`)**: `providers` (`vibe`, `deepseek`, `agy`), `policy`, `extensions`, and `bridge.py`.

## Before Opening a PR
1. Run Python tests:
   ```bash
   uv run --project python --with pytest --with pytest-asyncio --with httpx --with pyyaml --with pydantic --with keyring pytest python/tests
   ```
2. Run Rust lint and tests:
   ```bash
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```
3. Keep changes focused, tested, and free of hardcoded credentials or model lists.
