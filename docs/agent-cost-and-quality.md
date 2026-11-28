# Agent cost and output quality

What actually separates a cheap agent run from an expensive one, and what le-vibe does about it.
Numbers are from third-party measurements (Nov 2026) plus first-party measurement of this repo.

## The three cost drivers

A coding agent's bill is `(fixed prefix + transcript) x steps x price`. The model sets the
accuracy ceiling; the harness sets how many tokens you spend reaching it.

1. **Fixed prefix per call** — system prompt + tool schemas, resent on every request.
2. **Transcript growth** — every step resends everything before it, so cost grows quadratically
   with steps unless old tool results are elided.
3. **Failed steps** — failed runs use *more* steps and roughly twice the reasoning of solved runs.

## What the measurements say

| Finding | Number | Source |
| --- | --- | --- |
| Tokens per solved task, same model, 6 harnesses | 17.3k (pi) to 97.4k (Antigravity CLI), 5.6x | harness-efficiency-bench v2 |
| Share of input that is the harness's own prompt/tools | >80% for OpenCode, Codex, Antigravity; <50% Claude Code | same |
| Cache discount on that gap | 5.6x -> 4.0x at Anthropic cache pricing; 3.5x at 2% | same |
| Cost per trial, same models, 5 harnesses | $0.74 to $1.57 (2.1x) with a 2.75pt accuracy spread | Strands, 94k trials |
| OpenAI models: cheapest vs priciest harness | 59% cheaper AND 1.29pt more accurate | same |
| Harness swap vs rerun, same harness | Pass-rate change is within a rerun; bounds 2-5pt | arXiv 2610.04433 |
| Model-harness interaction | OpenCode -8/-9pt on one model, +14pt on another | same |
| Pass@1 across 3 harnesses on SWE-bench Pro v2 | "Pick the harness for the bill, not the score" | Scale Labs / Reflection |
| Turn counts | all six harnesses used 5-9 calls/task — nobody won by stepping less | harness-efficiency-bench v2 |

The consistent conclusion across all of them: **harness choice moves cost 2-4x and accuracy by a
few points; model choice moves accuracy far more.** Optimise the harness for cost, choose the
model for quality.

## le-vibe's position

Measured on this repo (`cargo test -p le-vibe-tui --test prompt_size_probe`):

| Component | Size | Sent |
| --- | --- | --- |
| Static core prompt | 233 words / 1,595 chars (~400 tok) | every call |
| Full system prompt | 713 words / 4,935 chars (~1,250 tok) | every call |
| Tool schemas, 20 tools | 6,238 bytes (~1,560 tok) | every call |
| **Fixed prefix** | **~2,800 tok** | **every call** |

Reference points: pi ~1,474 tok, Claude Code ~1,161 tok. le-vibe's prefix is larger mainly because
it carries 20 tools where pi carries 4. That is a deliberate quality trade — memory, subagents,
codemode, skills and web tools live in the same request.

Modes narrow this: `plan`/`ask`/`rea` send 11-14 tools, `normal`/`cyber` send all 20.

## What le-vibe does to keep cost down

- **Small static prompt, volatile blocks last.** Prefix-stable ordering so KV/prefix caches hit;
  `prompt_contract.rs` asserts this and fails the build if the stable core grows.
- **Compress at append.** Shell/MCP/log/JSON output is crushed once on arrival with head+tail and
  buried-error preservation; `read` stays exact below 24 KB; originals go into a bounded
  `OutputStore` for `retrieve_output`. Compressing at append (not per request) keeps the prefix
  stable, which is what makes caching work.
- **Age stale tool results before each request.** Every step resends the transcript, so a 24 KB
  read kept forever costs ~6k tokens on every later step. The newest six tool results stay
  verbatim (the working set); older bulky ones shrink to a head plus a re-run hint. The session
  DAG keeps the full text — only the outgoing request is shaped.
- **Reserve-based compaction.** Compaction triggers at `window - reserveTokens` (16,384), not a
  percentage, plus one compact-and-retry on silent overflow.
- **One 401 -> one forced refresh -> one replay.** No retry storm on auth failures.

## Measuring a run

Every assistant turn now persists the provider-reported usage into the session log:

```json
{"id":"msg_7","kind":"message","role":"assistant","content":"...","usage":{"input_tokens":1200,"cache_read_tokens":9000,"cache_write_tokens":300,"output_tokens":450}}
```

The footer's counters are live-only; this is what lets a finished session be costed. `usage` is
`Option`al and skipped when absent, so older logs still load.

Tokens per solved task for one session:

```python
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])          # or a glob over ~/.le-vibe/sessions/**/*.jsonl
inp = cache = out = turns = 0
for line in path.read_text(encoding="utf-8").splitlines():
    e = json.loads(line)
    u = e.get("usage") or {}
    if not u:
        continue
    turns += 1
    inp += u.get("input_tokens", 0)
    cache += u.get("cache_read_tokens", 0)
    out += u.get("output_tokens", 0)
print(f"turns={turns} uncached_in={inp} cache_read={cache} out={out} billed_in={inp + cache} total={inp + cache + out}")
```

Compare like with like: same model, same tasks, same 3 runs, one proxy counting every call.
A number from a single self-selected task is not a benchmark — the published spread between
different task sets is larger than the spread between harnesses.

## Rules of thumb when adding to le-vibe

- A new tool costs its schema on **every** call of **every** session. Budget ~300 tok; keep the
  description to the non-obvious contract and let the parameter schema carry the rest.
- Prefer few large steps over many small ones, but never trade a verification step for a shorter
  trajectory — failed runs cost more than the step they saved.
- Anything appended to context should be compressible or retrievable, or it becomes permanent
  freight.
- Measure with `prompt_size_probe` before and after; a prompt or schema change that is not
  measured is a guess.
