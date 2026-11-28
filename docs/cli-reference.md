# CLI & Slash Commands Reference

## CLI Usage (`crates/lazybug-cli/src/cli.rs`)

```sh
lazybug [OPTIONS] [--] [@files...] [prompt...]
lazybug <login|logout|auth|usage|mcp> [OPTIONS]
```

Key flags:
- `-h, --help` / `-V, --version` — print help or version and exit
- `-p, --print` — one-shot non-interactive run (also auto-enabled when piped)
- `--mode <text|json|rpc>` — text TUI, JSONL event stream, or JSONL RPC server
- `--provider <vibe|deepseek|agy>` / `--model <provider/id[:thinking]>`
- `--thinking <off|minimal|low|medium|high|xhigh|max>`
- `--agent-mode <normal|plan|execute|ask|rea>` / `--tools <csv>`
- `--yolo` — auto-approve non-blocked tools
- `-c, --continue` / `-r, --resume [id]` / `--session <path>` / `--session-dir <dir>` / `--fork <id>` / `--no-session` / `-n, --name <name>`
- `--list-models [query]` — list live catalog models
- `--export <session.jsonl> [out.md|out.html]` — offline session export

## Interactive Slash Commands

| Group | Commands |
|-------|----------|
| Models & Settings | `/model`, `/mode`, `/thinking`, `/config`, `/theme`, `/term`, `/yolo`, `/policy`, `/login`, `/logout`, `/usage`, `/auth` |
| Sessions & Context | `/new`, `/clear`, `/resume`, `/rename`, `/status`, `/context`, `/rewind`, `/clone`, `/compact`, `/handover` |
| Export, RAG & Utilities | `/export`, `/copy`, `/report`, `/suggestion`, `/memory`, `/loop`, `/plugins`, `/reload-plugins`, `/trust`, `/reload`, `/skills`, `/mcp`, `/help`, `/changelog`, `/quit` |
