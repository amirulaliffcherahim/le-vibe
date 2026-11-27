# reference/ — archived material, read-only, never built

This folder is **not** part of le-vibe. Nothing here is compiled, tested, or imported by the
workspace. It exists so a future session can read the original TypeScript behaviour without
re-downloading it, and so the abandoned port keeps an audit trail.

## What is here

| Path | What | Keep? |
| --- | --- | --- |
| `pi-src/` | upstream `earendil-works/pi` v1.6.0, source only (`faac585`) — 67 packages, 305,834 lines, 1,029 test files | read-only reference. `node_modules` is **not** included; run `npm ci --ignore-scripts` inside it (≈54 s) before `npx vitest` |
| `rust-anything-analyze.md` | 2,300-line analyzer report: 626 units, 342 ready / 171 caution / 6 blocked / 97 keep | historical only — see the deprecation header at its top |
| `crusty_vs_pi_comparison.xlsx` | 8-sheet comparison written mid-port | historical only |
| `.ref/` | the npm-packed `pi@1.6.0` bundle used as a 1.6.0 oracle, an isolated agent dir, and the fixture-record harness | useful if you need to run the original |
| `legacy/` | the first prototype: 7 crates, 8,391 lines, 13 tests | audit trail |
| `rust/` | the second attempt: `DECISIONS.md` (ADR-001..007), `MAP.md`, `GLOSSARY.md`, `RULES.md` + 4 crates (`shared-rs`, `builders-rs`, `idna-pattern`, `crusty-providers`) | audit trail; `DECISIONS.md` is the record of *why* the port stopped |
| `target/` | Rust build dir from the prototype | disposable |
| `rusted-pi-git-archive/.git` | the prototype repo's history (5 commits, `f2a6a1d..e2673ec`), no remote | preserved. To revive: `git --git-dir=reference/rusted-pi-git-archive/.git log` |

## Why the port stopped

`rust-anything` proves a pure function matches TypeScript byte-for-byte — it produced real results
(`builders.ts` 2600/2600 cases, `idna-pattern` 28,260/28,260 answers, all gated). It cannot verify
an agent loop, a TUI, or a session tree, and at 626 units it would have taken months to rebuild
what le-vibe already ships in 23,036 lines of Rust and 4,823 of Python.

The active plan is now **feature-by-feature behavioural parity**: pick one `pi` behaviour, extract
its input/output contract from `pi-src/`, and assert it against le-vibe's existing crates and
Python providers. See `DECISIONS.md` in `rust/` for the decisions that got here.

## Rules for this folder

1. Do not add it to the workspace `Cargo.toml`, do not `cargo build` inside `rust/`, do not import
   from it.
2. Do not resume unit-by-unit porting from `rust-anything-analyze.md`.
3. Reading `pi-src/` to answer "what does pi actually do here?" is the whole point — keep it.
