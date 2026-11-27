> **DEPRECATED 2026-11-26.** The unit-by-unit port stopped at ADR-007. This report is reference
> only: do **not** resume porting from it. The active plan is feature-by-feature behavioural
> parity against le-vibe (`Cargo.toml` workspace + `python/le_vibe/providers/`). See
> `reference/rust/DECISIONS.md` and `reference/README.md`.

# Rust migration plan: rusted-pi (TypeScript/JavaScript)

68 packages, 1710 source files (305834 lines), 1447 test files, 126 UI files, grouped into 616 units.

| Ready | Caution | Blocked | Keep in source language |
| ---: | ---: | ---: | ---: |
| 342 | 171 | 6 | 97 |

## Start here

Best first ports: safe to port (readiness) and worth porting (loops and string work, tests, size).

| # | Unit | Lines | Priority | Readiness | Tests | Bridge | Main risks |
| ---: | --- | ---: | ---: | ---: | ---: | --- | --- |
| 1 | `pi-src/apps/p2/plugins/search/query.ts` | 20 | 93 | 100 | 1 | napi-rs | String offsets and slicing (2) |
| 2 | `pi-src/apps/p2/plugins/file-index/rank.ts` | 47 | 93 | 94 | 2 | napi-rs | String offsets and slicing (6); Insertion-ordered maps and object keys (1) |
| 3 | `pi-src/packages/telemetry/src/render.ts` | 57 | 93 | 93 | 3 | napi-rs | String offsets and slicing (2); Insertion-ordered maps and object keys (3); Number semantics (1) |
| 4 | `pi-src/packages/coding-agent/src/bun/` | 31 | 91 | 94 | 1 | napi-rs | String offsets and slicing (3); Insertion-ordered maps and object keys (1); Exceptions (1) |
| 5 | `pi-src/apps/p2/plugins/models/scope.ts` | 54 | 90 | 91 | 1 | napi-rs | String offsets and slicing (4); Regexes built at runtime (1); Insertion-ordered maps and object keys (1) |
| 6 | `pi-src/packages/tui/src/terminal-colors.ts` | 72 | 90 | 87 | 1 | napi-rs | String offsets and slicing (8); Number semantics (5) |
| 7 | `pi-src/apps/p2/plugins/tree/toc.ts` | 109 | 89 | 100 | 1 | napi-rs | Insertion-ordered maps and object keys (7) |
| 8 | `pi-src/apps/p2/plugins/voice/speech-text.ts` | 106 | 89 | 78 | 1 | napi-rs | String offsets and slicing (10); Regex features the `regex` crate lacks (4); Insertion-ordered maps and object keys (1) |
| 9 | `pi-src/apps/p2/plugins/projects/place.ts` | 20 | 88 | 100 | 1 | napi-rs | - |
| 10 | `pi-src/packages/tui/src/fuzzy.ts` | 114 | 87 | 92 | 1 | napi-rs | String offsets and slicing (3); Callback-shaped APIs (1) |

### `pi-src/apps/p2/plugins/search/query.ts`

20 lines in 1 file, wave 0, priority 93, readiness 100, bridge **napi-rs**.

- Leaf: imports no other unit.
- 1 test file imports it directly.
- String offsets and slicing x2: `pi-src/apps/p2/plugins/search/query.ts:17`, `pi-src/apps/p2/plugins/search/query.ts:26`

### `pi-src/apps/p2/plugins/file-index/rank.ts`

47 lines in 1 file, wave 18, priority 93, readiness 94, bridge **napi-rs**.

- Imports `pi-src/apps/p2/src/api/v1/`, `pi-src/packages/tui/src/index.ts`: port them first, or port them together.
- 2 test files import it directly.
- String offsets and slicing x6: `pi-src/apps/p2/plugins/file-index/rank.ts:7`, `pi-src/apps/p2/plugins/file-index/rank.ts:47`, `pi-src/apps/p2/plugins/file-index/rank.ts:54`
- Insertion-ordered maps and object keys x1: `pi-src/apps/p2/plugins/file-index/rank.ts:52`

### `pi-src/packages/telemetry/src/render.ts`

57 lines in 1 file, wave 1, priority 93, readiness 93, bridge **napi-rs**.

- Imports `pi-src/packages/telemetry/src/index.ts`: port it first, or port them together.
- 3 test files import it directly.
- String offsets and slicing x2: `pi-src/packages/telemetry/src/render.ts:9`
- Insertion-ordered maps and object keys x3: `pi-src/packages/telemetry/src/render.ts:18`, `pi-src/packages/telemetry/src/render.ts:29`, `pi-src/packages/telemetry/src/render.ts:31`
- Number semantics x1: `pi-src/packages/telemetry/src/render.ts:14`
- JSON round-trips x1: `pi-src/packages/telemetry/src/render.ts:19`

### `pi-src/packages/coding-agent/src/bun/`

31 lines in 4 files, wave 17, priority 91, readiness 94, bridge **napi-rs**.

- 1 npm dependencies not in the crate table: check them by hand.
- Imports `pi-src/packages/ai/src/bedrock-provider.ts`, `pi-src/packages/ai/src/bun-oauth.ts`, `pi-src/packages/ai/src/compat.ts`, `pi-src/packages/coding-agent/src/cli.ts`, `pi-src/packages/coding-agent/src/config.ts`: port them first, or port them together.
- 1 test file imports it directly.
- String offsets and slicing x3: `pi-src/packages/coding-agent/src/bun/restore-sandbox-env.ts:28`, `pi-src/packages/coding-agent/src/bun/restore-sandbox-env.ts:30`
- Insertion-ordered maps and object keys x1: `pi-src/packages/coding-agent/src/bun/restore-sandbox-env.ts:23`
- Exceptions x1: `pi-src/packages/coding-agent/src/bun/restore-sandbox-env.ts:25`

### `pi-src/apps/p2/plugins/models/scope.ts`

54 lines in 1 file, wave 18, priority 90, readiness 91, bridge **napi-rs**.

- Imports `pi-src/apps/p2/plugins/models/contract.ts`: port it first, or port them together.
- 1 test file imports it directly.
- String offsets and slicing x4: `pi-src/apps/p2/plugins/models/scope.ts:41`, `pi-src/apps/p2/plugins/models/scope.ts:42`, `pi-src/apps/p2/plugins/models/scope.ts:43`
- Regexes built at runtime x1: `pi-src/apps/p2/plugins/models/scope.ts:16`
- Insertion-ordered maps and object keys x1: `pi-src/apps/p2/plugins/models/scope.ts:6`

## Order by package

Packages come after the workspace packages they use. Inside a package, port by wave: a unit's imports are in earlier waves.

### 1. pi-monorepo (`pi-src`, node, 15 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/vitest.base.ts` | ready | 94 | 98 | napi-rs | - |
| 17 | `pi-src/scripts/` | caution | 7637 | 0 | napi-rs | String offsets and slicing (179); Regex features the `regex` crate lacks (5) |

### 2. agent (`pi-src/apps/p2/plugins/agent`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/agent/activity.tsx` | keep | 43 | 97 | keep | - |
| 19 | `pi-src/apps/p2/plugins/agent/web.tsx` | keep | 8 | 100 | keep | - |
| 21 | `pi-src/apps/p2/plugins/agent/tool.ts` | caution | 155 | 82 | napi-rs | String offsets and slicing (3); Async and promises (19) |
| 22 | `pi-src/apps/p2/plugins/agent/worker.ts` | ready | 20 | 96 | napi-rs | Reflection and prototypes (2); Exceptions (1) |

### 3. agent-plugins (`pi-src/apps/p2/plugins/agent-plugins`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/agent-plugins/worker.ts` | ready | 25 | 95 | napi-rs | Async and promises (2); JSON round-trips (1) |

### 4. counter (`pi-src/apps/p2/plugins/agent-plugins/skills/writing-p2-plugins/example/counter`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/agent-plugins/skills/writing-p2-plugins/example/counter/contract.ts` | ready | 14 | 95 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/agent-plugins/skills/writing-p2-plugins/example/counter/web.tsx` | keep | 48 | 99 | keep | - |
| 19 | `pi-src/apps/p2/plugins/agent-plugins/skills/writing-p2-plugins/example/counter/worker.ts` | ready | 37 | 94 | napi-rs | Async and promises (5) |
| 19 | `pi-src/apps/p2/plugins/agent-plugins/skills/writing-p2-plugins/example/counter/server.ts` | ready | 104 | 87 | napi-rs | Async and promises (10); Insertion-ordered maps and object keys (1) |

### 5. artifacts (`pi-src/apps/p2/plugins/artifacts`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/apps/p2/plugins/artifacts/bridge.ts` | ready | 111 | 94 | napi-rs | JSON round-trips (1) |
| 0 | `pi-src/apps/p2/plugins/artifacts/local-item.ts` | keep | 19 | 91 | keep | Insertion-ordered maps and object keys (2); Callback-shaped APIs (1) |
| 1 | `pi-src/apps/p2/plugins/artifacts/frame.ts` | ready | 10 | 90 | napi-rs | String offsets and slicing (3) |
| 18 | `pi-src/apps/p2/plugins/artifacts/contract.ts` | ready | 107 | 95 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/artifacts/inline-size.ts` | keep | 16 | 92 | keep | Insertion-ordered maps and object keys (1); Callback-shaped APIs (1) |
| 19 | `pi-src/apps/p2/plugins/artifacts/tui.ts` | ready | 194 | 98 | napi-rs | Async and promises (2); Callback-shaped APIs (2) |
| 19 | `pi-src/apps/p2/plugins/artifacts/use-artifacts.ts` | ready | 11 | 95 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/artifacts/viewer.ts` | ready | 23 | 92 | napi-rs | String offsets and slicing (1); JSON round-trips (2) |
| 19 | `pi-src/apps/p2/plugins/artifacts/commands.tsx` | keep | 147 | 80 | keep | String offsets and slicing (2); Async and promises (3) |
| 19 | `pi-src/apps/p2/plugins/artifacts/pages.ts` | caution | 102 | 79 | napi-rs | String offsets and slicing (1); Insertion-ordered maps and object keys (3) |
| 19 | `pi-src/apps/p2/plugins/artifacts/tool.ts` | caution | 307 | 66 | napi-rs | String offsets and slicing (1); Async and promises (33) |
| 19 | `pi-src/apps/p2/plugins/artifacts/store.ts` | ready | 225 | 61 | napi-rs | String offsets and slicing (6); Async and promises (23) |
| 20 | `pi-src/apps/p2/plugins/artifacts/activity.tsx` | keep | 177 | 88 | keep | String offsets and slicing (1); Callback-shaped APIs (1) |
| 20 | `pi-src/apps/p2/plugins/artifacts/worker.ts` | ready | 57 | 88 | napi-rs | Async and promises (33) |
| 20 | `pi-src/apps/p2/plugins/artifacts/server.ts` | ready | 242 | 79 | napi-rs | Async and promises (26); Insertion-ordered maps and object keys (1) |
| 20 | `pi-src/apps/p2/plugins/artifacts/panes.tsx` | keep | 288 | 69 | keep | String offsets and slicing (4); Callback-shaped APIs (3) |
| 21 | `pi-src/apps/p2/plugins/artifacts/web.tsx` | keep | 272 | 82 | keep | String offsets and slicing (1); Insertion-ordered maps and object keys (2) |

### 6. backup (`pi-src/apps/p2/plugins/backup`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/backup/contract.ts` | ready | 65 | 97 | napi-rs | Callback-shaped APIs (5) |
| 19 | `pi-src/apps/p2/plugins/backup/chunked-remote.ts` | caution | 70 | 80 | napi-rs | Async and promises (5); Generators (3) |
| 19 | `pi-src/apps/p2/plugins/backup/server.ts` | caution | 330 | 73 | napi-rs | Async and promises (34); Callback-shaped APIs (7) |
| 20 | `pi-src/apps/p2/plugins/backup/worker.ts` | ready | 64 | 91 | napi-rs | Async and promises (7); Exceptions (1) |

### 7. cache-warming (`pi-src/apps/p2/plugins/cache-warming`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/cache-warming/warmer.ts` | caution | 99 | 91 | napi-rs | Async and promises (6); Callback-shaped APIs (4) |
| 19 | `pi-src/apps/p2/plugins/cache-warming/worker.ts` | ready | 89 | 89 | napi-rs | Async and promises (11); JSON round-trips (2) |

### 8. chat (`pi-src/apps/p2/plugins/chat`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/apps/p2/plugins/chat/draft-history.ts` | ready | 49 | 100 | napi-rs | Clock and random numbers (1) |
| 17 | `pi-src/apps/p2/plugins/chat/contract.ts` | ready | 215 | 78 | napi-rs | String offsets and slicing (3); Callback-shaped APIs (2) |
| 18 | `pi-src/apps/p2/plugins/chat/server.ts` | ready | 59 | 91 | napi-rs | Async and promises (4); Insertion-ordered maps and object keys (1) |
| 18 | `pi-src/apps/p2/plugins/chat/outbox.ts` | ready | 116 | 90 | napi-rs | Callback-shaped APIs (9); Insertion-ordered maps and object keys (2) |
| 18 | `pi-src/apps/p2/plugins/chat/model-screens.ts` | keep | 45 | 87 | keep | Callback-shaped APIs (5); Insertion-ordered maps and object keys (2) |
| 18 | `pi-src/apps/p2/plugins/chat/shell.ts` | ready | 252 | 69 | napi-rs | String offsets and slicing (2); Async and promises (19) |
| 18 | `pi-src/apps/p2/plugins/chat/draft-editor.tsx` | keep | 607 | 53 | keep | String offsets and slicing (15); Callback-shaped APIs (9) |
| 19 | `pi-src/apps/p2/plugins/chat/model-choice.ts` | ready | 25 | 96 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/chat/session-start.ts` | caution | 78 | 94 | napi-rs | Callback-shaped APIs (9); Async and promises (2) |
| 19 | `pi-src/apps/p2/plugins/chat/tui-commands.ts` | ready | 158 | 75 | napi-rs | String offsets and slicing (3); Async and promises (4) |
| 19 | `pi-src/apps/p2/plugins/chat/worker.ts` | ready | 532 | 66 | napi-rs | String offsets and slicing (2); Async and promises (90) |
| 19 | `pi-src/apps/p2/plugins/chat/tui.ts` | caution | 1037 | 38 | napi-rs | String offsets and slicing (5); Async and promises (13) |
| 20 | `pi-src/apps/p2/plugins/chat/commands.tsx` | keep | 250 | 74 | keep | String offsets and slicing (1); Async and promises (19) |
| 27 | `pi-src/apps/p2/plugins/chat/web.tsx` | keep | 1807 | 18 | keep | String offsets and slicing (1); Async and promises (23) |

### 9. chat-mode (`pi-src/apps/p2/plugins/chat-mode`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/chat-mode/contract.ts` | ready | 80 | 91 | napi-rs | Regexes built at runtime (2) |
| 19 | `pi-src/apps/p2/plugins/chat-mode/display.ts` | ready | 13 | 95 | napi-rs | Callback-shaped APIs (1) |
| 19 | `pi-src/apps/p2/plugins/chat-mode/digest.ts` | ready | 128 | 94 | napi-rs | String offsets and slicing (4); Insertion-ordered maps and object keys (1) |
| 20 | `pi-src/apps/p2/plugins/chat-mode/tui.ts` | caution | 162 | 100 | napi-rs | Callback-shaped APIs (2) |
| 20 | `pi-src/apps/p2/plugins/chat-mode/web.tsx` | keep | 144 | 93 | keep | Callback-shaped APIs (2) |
| 20 | `pi-src/apps/p2/plugins/chat-mode/worker.ts` | caution | 198 | 87 | napi-rs | String offsets and slicing (1); Async and promises (36) |

### 10. codemode (`pi-src/apps/p2/plugins/codemode`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/codemode/nested.ts` | ready | 48 | 100 | napi-rs | Dynamic typing (1); Exceptions (1) |
| 18 | `pi-src/apps/p2/plugins/codemode/contract.ts` | ready | 3 | 98 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/codemode/card.ts` | ready | 128 | 87 | napi-rs | String offsets and slicing (2); Number semantics (2) |
| 18 | `pi-src/apps/p2/plugins/codemode/calls.ts` | caution | 75 | 80 | napi-rs | Async and promises (15); Insertion-ordered maps and object keys (2) |
| 19 | `pi-src/apps/p2/plugins/codemode/tui.ts` | ready | 10 | 100 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/codemode/worker.ts` | ready | 48 | 100 | napi-rs | Async and promises (4) |
| 19 | `pi-src/apps/p2/plugins/codemode/activity.tsx` | keep | 58 | 97 | keep | - |
| 20 | `pi-src/apps/p2/plugins/codemode/web.tsx` | keep | 8 | 100 | keep | - |

### 11. coding-tools (`pi-src/apps/p2/plugins/coding-tools`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/coding-tools/worker.ts` | ready | 31 | 95 | napi-rs | Async and promises (3) |
| 18 | `pi-src/apps/p2/plugins/coding-tools/activity.tsx` | keep | 162 | 85 | keep | String offsets and slicing (4) |
| 18 | `pi-src/apps/p2/plugins/coding-tools/cards.ts` | ready | 226 | 81 | napi-rs | String offsets and slicing (3); Callback-shaped APIs (2) |
| 19 | `pi-src/apps/p2/plugins/coding-tools/tui.ts` | ready | 17 | 100 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/coding-tools/web.tsx` | keep | 8 | 100 | keep | - |

### 12. conversations (`pi-src/apps/p2/plugins/conversations`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/conversations/activity.tsx` | keep | 56 | 95 | keep | Insertion-ordered maps and object keys (1) |
| 19 | `pi-src/apps/p2/plugins/conversations/contract.ts` | ready | 108 | 100 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/conversations/web.tsx` | keep | 45 | 99 | keep | - |
| 20 | `pi-src/apps/p2/plugins/conversations/worker.ts` | ready | 265 | 75 | napi-rs | String offsets and slicing (1); Async and promises (24) |
| 20 | `pi-src/apps/p2/plugins/conversations/server.ts` | ready | 207 | 66 | napi-rs | String offsets and slicing (9); Async and promises (28) |

### 13. dev-tools (`pi-src/apps/p2/plugins/dev-tools`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/dev-tools/contract.ts` | ready | 28 | 95 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/dev-tools/server.ts` | ready | 13 | 99 | napi-rs | Exceptions (1) |
| 19 | `pi-src/apps/p2/plugins/dev-tools/tool.ts` | caution | 60 | 98 | napi-rs | Async and promises (4); Callback-shaped APIs (2) |
| 20 | `pi-src/apps/p2/plugins/dev-tools/worker.ts` | ready | 33 | 96 | napi-rs | Async and promises (2) |

### 14. devices (`pi-src/apps/p2/plugins/devices`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/devices/owner-access.ts` | ready | 164 | 72 | napi-rs | String offsets and slicing (1); Async and promises (23) |
| 19 | `pi-src/apps/p2/plugins/devices/contract.ts` | ready | 19 | 100 | napi-rs | - |
| 20 | `pi-src/apps/p2/plugins/devices/tui.ts` | ready | 95 | 100 | napi-rs | Async and promises (2) |
| 20 | `pi-src/apps/p2/plugins/devices/web.tsx` | keep | 152 | 92 | keep | Async and promises (2); Exceptions (1) |
| 20 | `pi-src/apps/p2/plugins/devices/server.ts` | ready | 205 | 82 | napi-rs | Async and promises (12); Dynamic typing (2) |

### 15. discord (`pi-src/apps/p2/plugins/discord`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/discord/contract.ts` | ready | 159 | 93 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/discord/held.ts` | keep | 7 | 95 | keep | - |
| 19 | `pi-src/apps/p2/plugins/discord/relay.ts` | caution | 585 | 48 | napi-rs | String offsets and slicing (4); Async and promises (39) |
| 20 | `pi-src/apps/p2/plugins/discord/BotsSheet.tsx` | keep | 287 | 80 | keep | String offsets and slicing (2); Callback-shaped APIs (2) |
| 20 | `pi-src/apps/p2/plugins/discord/worker.ts` | ready | 381 | 68 | napi-rs | String offsets and slicing (1); Async and promises (66) |
| 20 | `pi-src/apps/p2/plugins/discord/bot.ts` | caution | 211 | 62 | napi-rs | Async and promises (31); Callback-shaped APIs (5) |
| 21 | `pi-src/apps/p2/plugins/discord/fleet.ts` | caution | 157 | 67 | napi-rs | Async and promises (27); Insertion-ordered maps and object keys (6) |
| 23 | `pi-src/apps/p2/plugins/discord/LinkSheet.tsx` | keep | 533 | 78 | keep | Callback-shaped APIs (6); JSON round-trips (2) |
| 23 | `pi-src/apps/p2/plugins/discord/server.ts` | ready | 284 | 65 | napi-rs | Async and promises (40); Insertion-ordered maps and object keys (14) |
| 24 | `pi-src/apps/p2/plugins/discord/web.tsx` | keep | 69 | 98 | keep | - |

### 16. drafts (`pi-src/apps/p2/plugins/drafts`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/drafts/contract.ts` | ready | 38 | 100 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/drafts/worker.ts` | ready | 56 | 100 | napi-rs | Async and promises (4); Insertion-ordered maps and object keys (1) |

### 17. envs (`pi-src/apps/p2/plugins/envs`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/apps/p2/plugins/envs/secret-presets.ts` | ready | 36 | 96 | napi-rs | - |
| 0 | `pi-src/apps/p2/plugins/envs/sandbox/` | caution | 466 | 43 | napi-rs | String offsets and slicing (12); Regexes built at runtime (1) |
| 18 | `pi-src/apps/p2/plugins/envs/sandbox-schema.ts` | ready | 18 | 100 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/envs/schema.ts` | ready | 38 | 99 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/envs/secrets.ts` | ready | 116 | 82 | napi-rs | Async and promises (16); Insertion-ordered maps and object keys (5) |
| 19 | `pi-src/apps/p2/plugins/envs/connection-revision.ts` | ready | 7 | 100 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/envs/config.ts` | ready | 130 | 69 | napi-rs | String offsets and slicing (3); Async and promises (6) |
| 20 | `pi-src/apps/p2/plugins/envs/store.ts` | caution | 35 | 96 | napi-rs | Callback-shaped APIs (3); Async and promises (2) |
| 20 | `pi-src/apps/p2/plugins/envs/sandbox-host.ts` | ready | 137 | 83 | napi-rs | String offsets and slicing (4); Async and promises (7) |
| 21 | `pi-src/apps/p2/plugins/envs/remote-env.ts` | caution | 102 | 90 | napi-rs | Async and promises (9); Callback-shaped APIs (3) |
| 22 | `pi-src/apps/p2/plugins/envs/worker.ts` | ready | 41 | 92 | napi-rs | Async and promises (6); Exceptions (1) |
| 22 | `pi-src/apps/p2/plugins/envs/contract.ts` | ready | 171 | 91 | napi-rs | - |
| 22 | `pi-src/apps/p2/plugins/envs/manager.ts` | caution | 439 | 52 | napi-rs | String offsets and slicing (1); Async and promises (112) |
| 23 | `pi-src/apps/p2/plugins/envs/tui.ts` | ready | 202 | 100 | napi-rs | Async and promises (2); Callback-shaped APIs (1) |
| 23 | `pi-src/apps/p2/plugins/envs/SecretsSection.tsx` | keep | 182 | 86 | keep | Async and promises (2); Callback-shaped APIs (2) |
| 23 | `pi-src/apps/p2/plugins/envs/server.ts` | ready | 262 | 75 | napi-rs | Async and promises (39); Insertion-ordered maps and object keys (6) |
| 23 | `pi-src/apps/p2/plugins/envs/DirectoryPicker.tsx` | keep | 438 | 66 | keep | String offsets and slicing (5); Callback-shaped APIs (7) |
| 24 | `pi-src/apps/p2/plugins/envs/contract.web.tsx` | keep | 1 | 94 | keep | - |
| 24 | `pi-src/apps/p2/plugins/envs/EnvsPanel.tsx` | keep | 1170 | 34 | keep | String offsets and slicing (6); Async and promises (27) |
| 25 | `pi-src/apps/p2/plugins/envs/web.tsx` | keep | 15 | 100 | keep | - |

### 18. familiar (`pi-src/apps/p2/plugins/familiar`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 17 | `pi-src/apps/p2/plugins/familiar/cold.ts` | ready | 23 | 100 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/familiar/contract.ts` | ready | 24 | 100 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/familiar/server.ts` | ready | 58 | 92 | napi-rs | Async and promises (5); Exceptions (1) |
| 19 | `pi-src/apps/p2/plugins/familiar/worker.ts` | caution | 160 | 83 | napi-rs | Async and promises (23); Callback-shaped APIs (1) |
| 27 | `pi-src/apps/p2/plugins/familiar/web.tsx` | keep | 103 | 97 | keep | - |

### 19. file-index (`pi-src/apps/p2/plugins/file-index`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/file-index/listing.ts` | caution | 23 | 100 | napi-rs | Async and promises (2); Exceptions (1) |
| 18 | `pi-src/apps/p2/plugins/file-index/rank.ts` | ready | 47 | 94 | napi-rs | String offsets and slicing (6); Insertion-ordered maps and object keys (1) |
| 19 | `pi-src/apps/p2/plugins/file-index/worker.ts` | ready | 105 | 96 | napi-rs | Async and promises (15); Exceptions (6) |

### 20. files (`pi-src/apps/p2/plugins/files`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/files/contract.ts` | ready | 50 | 94 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/files/uploads.ts` | ready | 56 | 85 | napi-rs | Async and promises (10); Class inheritance (1) |
| 19 | `pi-src/apps/p2/plugins/files/worker.ts` | ready | 106 | 81 | napi-rs | String offsets and slicing (1); Async and promises (26) |
| 19 | `pi-src/apps/p2/plugins/files/server.ts` | ready | 204 | 77 | napi-rs | String offsets and slicing (1); Async and promises (28) |

### 21. fit-images (`pi-src/apps/p2/plugins/fit-images`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/fit-images/worker.ts` | ready | 9 | 100 | napi-rs | - |

### 22. history (`pi-src/apps/p2/plugins/history`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/history/activity.tsx` | keep | 167 | 89 | keep | Async and promises (4); JSON round-trips (1) |
| 19 | `pi-src/apps/p2/plugins/history/web.tsx` | keep | 8 | 100 | keep | - |
| 19 | `pi-src/apps/p2/plugins/history/tools.ts` | ready | 366 | 71 | napi-rs | String offsets and slicing (2); Async and promises (21) |
| 20 | `pi-src/apps/p2/plugins/history/tui.ts` | ready | 87 | 98 | napi-rs | - |
| 20 | `pi-src/apps/p2/plugins/history/worker.ts` | ready | 18 | 96 | napi-rs | Async and promises (2) |

### 23. locations (`pi-src/apps/p2/plugins/locations`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/locations/contract.ts` | ready | 39 | 100 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/locations/contract.worker.ts` | ready | 12 | 95 | napi-rs | - |
| 20 | `pi-src/apps/p2/plugins/locations/tool.ts` | ready | 208 | 83 | napi-rs | String offsets and slicing (1); Async and promises (20) |
| 21 | `pi-src/apps/p2/plugins/locations/worker.ts` | ready | 91 | 87 | napi-rs | Async and promises (20); Exceptions (2) |
| 23 | `pi-src/apps/p2/plugins/locations/server.ts` | ready | 63 | 91 | napi-rs | Async and promises (3); Insertion-ordered maps and object keys (1) |

### 24. login (`pi-src/apps/p2/plugins/login`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/login/contract.ts` | ready | 84 | 100 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/login/tui.ts` | caution | 454 | 80 | napi-rs | Async and promises (15); Callback-shaped APIs (5) |
| 19 | `pi-src/apps/p2/plugins/login/web.tsx` | keep | 356 | 78 | keep | String offsets and slicing (2); Callback-shaped APIs (2) |
| 19 | `pi-src/apps/p2/plugins/login/logins.ts` | caution | 257 | 69 | napi-rs | Async and promises (21); Callback-shaped APIs (4) |
| 20 | `pi-src/apps/p2/plugins/login/server.ts` | ready | 89 | 98 | napi-rs | Async and promises (11); Insertion-ordered maps and object keys (1) |

### 25. logs (`pi-src/apps/p2/plugins/logs`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/logs/contract.ts` | ready | 19 | 100 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/logs/tui.ts` | caution | 192 | 91 | napi-rs | String offsets and slicing (2); Callback-shaped APIs (3) |
| 19 | `pi-src/apps/p2/plugins/logs/web.tsx` | keep | 119 | 87 | keep | String offsets and slicing (1); Async and promises (2) |
| 19 | `pi-src/apps/p2/plugins/logs/server.ts` | ready | 161 | 81 | napi-rs | Async and promises (10); Insertion-ordered maps and object keys (3) |

### 26. mcp (`pi-src/apps/p2/plugins/mcp`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/apps/p2/plugins/mcp/windows-command.ts` | ready | 18 | 100 | napi-rs | Regex features the `regex` crate lacks (3) |
| 0 | `pi-src/apps/p2/plugins/mcp/pages.ts` | ready | 57 | 95 | napi-rs | JSON round-trips (1) |
| 0 | `pi-src/apps/p2/plugins/mcp/probe.ts` | caution | 58 | 80 | napi-rs | String offsets and slicing (1); Async and promises (9) |
| 18 | `pi-src/apps/p2/plugins/mcp/leases.ts` | ready | 49 | 100 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/mcp/contract.ts` | ready | 169 | 94 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/mcp/config.ts` | ready | 139 | 84 | napi-rs | Insertion-ordered maps and object keys (4); Callback-shaped APIs (3) |
| 18 | `pi-src/apps/p2/plugins/mcp/credentials.ts` | caution | 63 | 81 | napi-rs | Async and promises (7); Dynamic typing (1) |
| 19 | `pi-src/apps/p2/plugins/mcp/project.ts` | caution | 69 | 100 | napi-rs | Insertion-ordered maps and object keys (5); Exceptions (1) |
| 19 | `pi-src/apps/p2/plugins/mcp/web.tsx` | keep | 251 | 86 | keep | Callback-shaped APIs (6); Insertion-ordered maps and object keys (1) |
| 19 | `pi-src/apps/p2/plugins/mcp/tui.ts` | ready | 194 | 85 | napi-rs | Async and promises (8); Callback-shaped APIs (1) |
| 19 | `pi-src/apps/p2/plugins/mcp/routes.ts` | ready | 98 | 84 | napi-rs | String offsets and slicing (1); Async and promises (7) |
| 19 | `pi-src/apps/p2/plugins/mcp/tools.ts` | ready | 318 | 68 | napi-rs | String offsets and slicing (4); Insertion-ordered maps and object keys (13) |
| 19 | `pi-src/apps/p2/plugins/mcp/sign-in.ts` | caution | 271 | 66 | napi-rs | Async and promises (17); Callback-shaped APIs (4) |
| 20 | `pi-src/apps/p2/plugins/mcp/environments.ts` | caution | 30 | 99 | napi-rs | Callback-shaped APIs (2); Insertion-ordered maps and object keys (1) |
| 20 | `pi-src/apps/p2/plugins/mcp/placement.ts` | ready | 223 | 85 | napi-rs | String offsets and slicing (3); Insertion-ordered maps and object keys (6) |
| 20 | `pi-src/apps/p2/plugins/mcp/worker.ts` | ready | 235 | 70 | napi-rs | String offsets and slicing (1); Async and promises (29) |
| 21 | `pi-src/apps/p2/plugins/mcp/transports.ts` | caution | 89 | 75 | napi-rs | String offsets and slicing (1); Async and promises (7) |
| 22 | `pi-src/apps/p2/plugins/mcp/instances.ts` | caution | 328 | 73 | napi-rs | String offsets and slicing (1); Async and promises (20) |
| 23 | `pi-src/apps/p2/plugins/mcp/server.ts` | caution | 393 | 72 | napi-rs | Async and promises (37); Callback-shaped APIs (7) |

### 27. models (`pi-src/apps/p2/plugins/models`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 17 | `pi-src/apps/p2/plugins/models/models.ts` | caution | 6 | 100 | napi-rs | - |
| 17 | `pi-src/apps/p2/plugins/models/contract.ts` | ready | 70 | 93 | napi-rs | - |
| 17 | `pi-src/apps/p2/plugins/models/config-value.ts` | ready | 153 | 85 | napi-rs | String offsets and slicing (8); Insertion-ordered maps and object keys (2) |
| 17 | `pi-src/apps/p2/plugins/models/model-store.ts` | caution | 88 | 80 | napi-rs | Async and promises (9); Dynamic typing (2) |
| 17 | `pi-src/apps/p2/plugins/models/credentials.ts` | caution | 163 | 76 | napi-rs | Async and promises (16); Insertion-ordered maps and object keys (3) |
| 17 | `pi-src/apps/p2/plugins/models/resources.ts` | caution | 272 | 71 | napi-rs | Async and promises (27); Insertion-ordered maps and object keys (5) |
| 17 | `pi-src/apps/p2/plugins/models/model-config.ts` | caution | 369 | 70 | napi-rs | String offsets and slicing (1); Insertion-ordered maps and object keys (7) |
| 17 | `pi-src/apps/p2/plugins/models/provider-config.ts` | caution | 348 | 70 | napi-rs | Async and promises (16); Insertion-ordered maps and object keys (7) |
| 17 | `pi-src/apps/p2/plugins/models/fake.ts` | caution | 240 | 65 | napi-rs | String offsets and slicing (16); Insertion-ordered maps and object keys (3) |
| 18 | `pi-src/apps/p2/plugins/models/config.ts` | ready | 40 | 100 | napi-rs | Async and promises (2); Insertion-ordered maps and object keys (1) |
| 18 | `pi-src/apps/p2/plugins/models/scope.ts` | ready | 54 | 91 | napi-rs | String offsets and slicing (4); Regexes built at runtime (1) |
| 18 | `pi-src/apps/p2/plugins/models/tui.ts` | caution | 386 | 86 | napi-rs | String offsets and slicing (5); Callback-shaped APIs (2) |
| 19 | `pi-src/apps/p2/plugins/models/worker.ts` | ready | 26 | 99 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/models/server.ts` | ready | 63 | 92 | napi-rs | Async and promises (4); Exceptions (2) |

### 28. ports (`pi-src/apps/p2/plugins/ports`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/ports/contract.ts` | ready | 69 | 98 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/ports/activity.tsx` | keep | 97 | 95 | keep | JSON round-trips (1) |
| 18 | `pi-src/apps/p2/plugins/ports/protocol.ts` | ready | 43 | 94 | napi-rs | String offsets and slicing (3); Exceptions (2) |
| 18 | `pi-src/apps/p2/plugins/ports/store.ts` | caution | 151 | 83 | napi-rs | Async and promises (18); Insertion-ordered maps and object keys (4) |
| 19 | `pi-src/apps/p2/plugins/ports/tui.ts` | ready | 174 | 92 | napi-rs | String offsets and slicing (1); Async and promises (7) |
| 19 | `pi-src/apps/p2/plugins/ports/PortsPanel.tsx` | keep | 186 | 90 | keep | Callback-shaped APIs (2); Clock and random numbers (1) |
| 19 | `pi-src/apps/p2/plugins/ports/server.ts` | ready | 115 | 88 | napi-rs | Async and promises (13); Exceptions (4) |
| 19 | `pi-src/apps/p2/plugins/ports/tool.ts` | caution | 113 | 85 | napi-rs | Async and promises (4); Callback-shaped APIs (2) |
| 20 | `pi-src/apps/p2/plugins/ports/web.tsx` | keep | 37 | 99 | keep | - |
| 20 | `pi-src/apps/p2/plugins/ports/worker.ts` | ready | 18 | 96 | napi-rs | Async and promises (2) |

### 29. powershell (`pi-src/apps/p2/plugins/powershell`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/powershell/contract.ts` | ready | 9 | 100 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/powershell/activity.tsx` | keep | 5 | 98 | keep | - |
| 19 | `pi-src/apps/p2/plugins/powershell/tui.ts` | ready | 9 | 100 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/powershell/web.tsx` | keep | 9 | 100 | keep | - |
| 20 | `pi-src/apps/p2/plugins/powershell/worker.ts` | caution | 122 | 91 | napi-rs | Async and promises (14); Insertion-ordered maps and object keys (3) |

### 30. projects (`pi-src/apps/p2/plugins/projects`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/projects/contract.ts` | ready | 89 | 83 | napi-rs | String offsets and slicing (4) |
| 19 | `pi-src/apps/p2/plugins/projects/place.ts` | ready | 20 | 100 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/projects/store.ts` | ready | 33 | 98 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/projects/config.ts` | ready | 44 | 91 | napi-rs | String offsets and slicing (2); Insertion-ordered maps and object keys (2) |
| 20 | `pi-src/apps/p2/plugins/projects/server.ts` | ready | 266 | 72 | napi-rs | Async and promises (45); Insertion-ordered maps and object keys (4) |

### 31. prompt (`pi-src/apps/p2/plugins/prompt`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/apps/p2/plugins/prompt/templates.ts` | ready | 112 | 84 | napi-rs | String offsets and slicing (10); Async and promises (2) |
| 18 | `pi-src/apps/p2/plugins/prompt/contract.ts` | ready | 27 | 100 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/prompt/context-files.ts` | ready | 172 | 82 | napi-rs | Async and promises (21); Insertion-ordered maps and object keys (3) |
| 19 | `pi-src/apps/p2/plugins/prompt/composer.ts` | ready | 14 | 100 | napi-rs | Async and promises (3); Exceptions (1) |
| 19 | `pi-src/apps/p2/plugins/prompt/tui.ts` | caution | 89 | 100 | napi-rs | Async and promises (3); Callback-shaped APIs (1) |
| 19 | `pi-src/apps/p2/plugins/prompt/environment.ts` | caution | 107 | 90 | napi-rs | Async and promises (17); Callback-shaped APIs (6) |
| 20 | `pi-src/apps/p2/plugins/prompt/web.tsx` | keep | 2 | 100 | keep | - |
| 20 | `pi-src/apps/p2/plugins/prompt/worker.ts` | ready | 367 | 63 | napi-rs | Async and promises (55); Callback-shaped APIs (12) |

### 32. proxy-marker (`pi-src/apps/p2/plugins/proxy-marker`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/proxy-marker/contract.ts` | ready | 3 | 100 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/proxy-marker/server.ts` | ready | 5 | 100 | napi-rs | - |

### 33. quiet-tools (`pi-src/apps/p2/plugins/quiet-tools`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/quiet-tools/tui.ts` | caution | 124 | 95 | napi-rs | String offsets and slicing (1); Insertion-ordered maps and object keys (2) |

### 34. radius (`pi-src/apps/p2/plugins/radius`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/radius/relay-access.ts` | ready | 37 | 100 | napi-rs | JSON round-trips (2) |
| 19 | `pi-src/apps/p2/plugins/radius/brain.ts` | caution | 934 | 34 | napi-rs | String offsets and slicing (12); Async and promises (36) |
| 20 | `pi-src/apps/p2/plugins/radius/contract.ts` | ready | 9 | 100 | napi-rs | - |
| 20 | `pi-src/apps/p2/plugins/radius/login.ts` | ready | 23 | 95 | napi-rs | Async and promises (4) |
| 21 | `pi-src/apps/p2/plugins/radius/server.ts` | ready | 96 | 100 | napi-rs | Async and promises (5) |

### 35. rollups (`pi-src/apps/p2/plugins/rollups`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/rollups/contract.ts` | ready | 103 | 98 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/rollups/runner.ts` | ready | 138 | 91 | napi-rs | String offsets and slicing (3); Async and promises (2) |
| 19 | `pi-src/apps/p2/plugins/rollups/worker.ts` | ready | 139 | 80 | napi-rs | String offsets and slicing (1); Async and promises (10) |
| 20 | `pi-src/apps/p2/plugins/rollups/server.ts` | ready | 236 | 74 | napi-rs | Async and promises (30); Insertion-ordered maps and object keys (7) |

### 36. scope-prompt (`pi-src/apps/p2/plugins/scope-prompt`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/apps/p2/plugins/scope-prompt/scopes.ts` | ready | 92 | 86 | napi-rs | String offsets and slicing (3) |
| 18 | `pi-src/apps/p2/plugins/scope-prompt/contract.ts` | ready | 15 | 97 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/scope-prompt/message.ts` | ready | 32 | 97 | napi-rs | JSON round-trips (1) |
| 18 | `pi-src/apps/p2/plugins/scope-prompt/mode.ts` | keep | 61 | 88 | keep | Callback-shaped APIs (3); Insertion-ordered maps and object keys (1) |
| 19 | `pi-src/apps/p2/plugins/scope-prompt/Toggle.tsx` | keep | 27 | 98 | keep | - |
| 19 | `pi-src/apps/p2/plugins/scope-prompt/commands.tsx` | keep | 31 | 98 | keep | - |
| 19 | `pi-src/apps/p2/plugins/scope-prompt/worker.ts` | ready | 39 | 92 | napi-rs | Async and promises (3); Insertion-ordered maps and object keys (1) |
| 19 | `pi-src/apps/p2/plugins/scope-prompt/Overlay.tsx` | keep | 378 | 69 | keep | String offsets and slicing (2); Async and promises (8) |
| 20 | `pi-src/apps/p2/plugins/scope-prompt/web.tsx` | keep | 16 | 100 | keep | - |

### 37. search (`pi-src/apps/p2/plugins/search`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/apps/p2/plugins/search/query.ts` | ready | 20 | 100 | napi-rs | String offsets and slicing (2) |
| 18 | `pi-src/apps/p2/plugins/search/contract.ts` | ready | 142 | 91 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/search/tui.ts` | ready | 61 | 100 | napi-rs | Async and promises (6); Exceptions (1) |
| 19 | `pi-src/apps/p2/plugins/search/db.ts` | ready | 163 | 83 | napi-rs | String offsets and slicing (9); Insertion-ordered maps and object keys (3) |
| 20 | `pi-src/apps/p2/plugins/search/worker.ts` | ready | 151 | 81 | napi-rs | Async and promises (15); Insertion-ordered maps and object keys (2) |
| 20 | `pi-src/apps/p2/plugins/search/server.ts` | ready | 205 | 80 | napi-rs | Async and promises (28); Insertion-ordered maps and object keys (2) |

### 38. send-file (`pi-src/apps/p2/plugins/send-file`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/send-file/contract.ts` | ready | 26 | 100 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/send-file/tool.ts` | caution | 62 | 86 | napi-rs | Async and promises (7); Callback-shaped APIs (2) |
| 19 | `pi-src/apps/p2/plugins/send-file/tui.ts` | ready | 159 | 100 | napi-rs | Async and promises (2); Insertion-ordered maps and object keys (1) |
| 19 | `pi-src/apps/p2/plugins/send-file/worker.ts` | ready | 55 | 100 | napi-rs | Async and promises (5); Exceptions (3) |
| 19 | `pi-src/apps/p2/plugins/send-file/activity.tsx` | keep | 88 | 96 | keep | JSON round-trips (1) |
| 20 | `pi-src/apps/p2/plugins/send-file/web.tsx` | keep | 8 | 100 | keep | - |

### 39. sessions (`pi-src/apps/p2/plugins/sessions`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/sessions/copy.ts` | ready | 35 | 86 | napi-rs | Async and promises (14); Dynamic typing (1) |
| 18 | `pi-src/apps/p2/plugins/sessions/contract.ts` | ready | 355 | 83 | napi-rs | Callback-shaped APIs (2) |
| 19 | `pi-src/apps/p2/plugins/sessions/catalog.ts` | ready | 111 | 96 | napi-rs | String offsets and slicing (1); Async and promises (2) |
| 19 | `pi-src/apps/p2/plugins/sessions/locks.ts` | ready | 46 | 93 | napi-rs | Async and promises (2); Exceptions (1) |
| 19 | `pi-src/apps/p2/plugins/sessions/BranchSheet.tsx` | keep | 68 | 92 | keep | Callback-shaped APIs (2) |
| 19 | `pi-src/apps/p2/plugins/sessions/SessionSheet.tsx` | keep | 77 | 90 | keep | Callback-shaped APIs (7) |
| 19 | `pi-src/apps/p2/plugins/sessions/sidebar-settings.ts` | ready | 53 | 89 | napi-rs | Async and promises (6); Insertion-ordered maps and object keys (1) |
| 19 | `pi-src/apps/p2/plugins/sessions/agent-actions.ts` | ready | 21 | 88 | napi-rs | Async and promises (5); Insertion-ordered maps and object keys (1) |
| 19 | `pi-src/apps/p2/plugins/sessions/command-session.tsx` | keep | 47 | 88 | keep | Async and promises (2); Callback-shaped APIs (2) |
| 19 | `pi-src/apps/p2/plugins/sessions/lock-worker.ts` | ready | 43 | 87 | napi-rs | Async and promises (10); JSON round-trips (2) |
| 20 | `pi-src/apps/p2/plugins/sessions/worker.ts` | ready | 118 | 83 | napi-rs | Async and promises (18); Callback-shaped APIs (1) |
| 23 | `pi-src/apps/p2/plugins/sessions/tui-location.ts` | ready | 215 | 71 | napi-rs | String offsets and slicing (8); Async and promises (7) |
| 23 | `pi-src/apps/p2/plugins/sessions/server.ts` | caution | 747 | 48 | napi-rs | String offsets and slicing (3); Async and promises (124) |
| 24 | `pi-src/apps/p2/plugins/sessions/tui-projects.ts` | ready | 75 | 95 | napi-rs | Async and promises (3); Callback-shaped APIs (2) |
| 25 | `pi-src/apps/p2/plugins/sessions/tui-move.ts` | ready | 139 | 81 | napi-rs | Async and promises (6); Callback-shaped APIs (3) |
| 25 | `pi-src/apps/p2/plugins/sessions/project-commands.tsx` | keep | 290 | 77 | keep | Async and promises (12); Callback-shaped APIs (2) |
| 25 | `pi-src/apps/p2/plugins/sessions/sidebar/` | keep | 2156 | 10 | keep | String offsets and slicing (6); Callback-shaped APIs (67) |
| 26 | `pi-src/apps/p2/plugins/sessions/contract.web.tsx` | keep | 21 | 88 | keep | Callback-shaped APIs (1) |
| 26 | `pi-src/apps/p2/plugins/sessions/commands.tsx` | keep | 402 | 71 | keep | String offsets and slicing (2); Async and promises (13) |
| 26 | `pi-src/apps/p2/plugins/sessions/tui.ts` | caution | 1049 | 43 | napi-rs | String offsets and slicing (4); Async and promises (21) |
| 27 | `pi-src/apps/p2/plugins/sessions/web.tsx` | keep | 796 | 50 | keep | String offsets and slicing (3); Async and promises (11) |

### 40. settings (`pi-src/apps/p2/plugins/settings`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/apps/p2/plugins/settings/sections.ts` | ready | 8 | 97 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/settings/contract.ts` | ready | 20 | 100 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/settings/commands.tsx` | keep | 89 | 89 | keep | String offsets and slicing (1); Async and promises (2) |
| 19 | `pi-src/apps/p2/plugins/settings/server.ts` | ready | 31 | 91 | napi-rs | Async and promises (3); Insertion-ordered maps and object keys (2) |
| 19 | `pi-src/apps/p2/plugins/settings/web.tsx` | keep | 566 | 68 | keep | String offsets and slicing (3); Callback-shaped APIs (5) |

### 41. sharing (`pi-src/apps/p2/plugins/sharing`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/sharing/contract.ts` | ready | 88 | 94 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/sharing/links.ts` | ready | 23 | 92 | napi-rs | Async and promises (4) |
| 19 | `pi-src/apps/p2/plugins/sharing/tui.ts` | ready | 244 | 92 | napi-rs | Async and promises (18); Insertion-ordered maps and object keys (1) |
| 19 | `pi-src/apps/p2/plugins/sharing/store.ts` | ready | 299 | 69 | napi-rs | String offsets and slicing (3); Async and promises (33) |
| 20 | `pi-src/apps/p2/plugins/sharing/commands.tsx` | keep | 140 | 88 | keep | Async and promises (6); Exceptions (2) |
| 20 | `pi-src/apps/p2/plugins/sharing/server.ts` | ready | 385 | 79 | napi-rs | Async and promises (56); Insertion-ordered maps and object keys (6) |
| 21 | `pi-src/apps/p2/plugins/sharing/web.tsx` | keep | 268 | 78 | keep | Dynamic typing (7); Async and promises (7) |

### 42. status (`pi-src/apps/p2/plugins/status`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/status/contract.ts` | ready | 99 | 93 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/status/server.ts` | ready | 162 | 93 | napi-rs | Insertion-ordered maps and object keys (9); Async and promises (4) |
| 19 | `pi-src/apps/p2/plugins/status/worker.ts` | caution | 115 | 92 | napi-rs | Async and promises (16); Insertion-ordered maps and object keys (7) |
| 19 | `pi-src/apps/p2/plugins/status/tui.ts` | caution | 305 | 75 | napi-rs | String offsets and slicing (3); Async and promises (7) |

### 43. subagents (`pi-src/apps/p2/plugins/subagents`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/subagents/contract.web.ts` | ready | 5 | 97 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/subagents/activity.tsx` | keep | 84 | 96 | keep | - |
| 18 | `pi-src/apps/p2/plugins/subagents/contract.ts` | ready | 103 | 80 | napi-rs | String offsets and slicing (5); Insertion-ordered maps and object keys (1) |
| 19 | `pi-src/apps/p2/plugins/subagents/children.ts` | ready | 25 | 100 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/subagents/tui.ts` | ready | 200 | 99 | napi-rs | String offsets and slicing (1); Insertion-ordered maps and object keys (1) |
| 19 | `pi-src/apps/p2/plugins/subagents/contract.worker.ts` | caution | 161 | 82 | napi-rs | Insertion-ordered maps and object keys (7); Async and promises (5) |
| 20 | `pi-src/apps/p2/plugins/subagents/server.ts` | ready | 91 | 87 | napi-rs | Insertion-ordered maps and object keys (6); Async and promises (3) |
| 20 | `pi-src/apps/p2/plugins/subagents/tools.ts` | ready | 633 | 56 | napi-rs | String offsets and slicing (1); Async and promises (83) |
| 21 | `pi-src/apps/p2/plugins/subagents/worker.ts` | ready | 307 | 75 | napi-rs | Async and promises (36); Insertion-ordered maps and object keys (13) |
| 27 | `pi-src/apps/p2/plugins/subagents/commands.tsx` | keep | 132 | 88 | keep | String offsets and slicing (1); Async and promises (2) |
| 28 | `pi-src/apps/p2/plugins/subagents/web.tsx` | keep | 12 | 100 | keep | - |

### 44. sync (`pi-src/apps/p2/plugins/sync`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/sync/radius-remote.ts` | caution | 156 | 86 | napi-rs | Async and promises (28); Callback-shaped APIs (2) |

### 45. telemetry (`pi-src/apps/p2/plugins/telemetry`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/telemetry/contract.ts` | ready | 82 | 100 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/telemetry/tool.ts` | caution | 117 | 82 | napi-rs | Async and promises (8); Callback-shaped APIs (4) |
| 19 | `pi-src/apps/p2/plugins/telemetry/server.ts` | ready | 127 | 74 | napi-rs | String offsets and slicing (2); Async and promises (11) |
| 19 | `pi-src/apps/p2/plugins/telemetry/traces.tsx` | keep | 351 | 61 | keep | String offsets and slicing (5); Callback-shaped APIs (7) |
| 20 | `pi-src/apps/p2/plugins/telemetry/worker.ts` | ready | 17 | 93 | napi-rs | Async and promises (8) |
| 20 | `pi-src/apps/p2/plugins/telemetry/web.tsx` | keep | 73 | 90 | keep | String offsets and slicing (1); Async and promises (3) |

### 46. terminals (`pi-src/apps/p2/plugins/terminals`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/apps/p2/plugins/terminals/terminal-input.ts` | ready | 7 | 100 | napi-rs | String offsets and slicing (1) |
| 0 | `pi-src/apps/p2/plugins/terminals/plain.ts` | ready | 116 | 95 | napi-rs | String offsets and slicing (3); Callback-shaped APIs (1) |
| 0 | `pi-src/apps/p2/plugins/terminals/protocol.ts` | caution | 130 | 95 | napi-rs | Callback-shaped APIs (1); JSON round-trips (2) |
| 0 | `pi-src/apps/p2/plugins/terminals/new-terminal.ts` | keep | 17 | 90 | keep | Callback-shaped APIs (3); Insertion-ordered maps and object keys (1) |
| 0 | `pi-src/apps/p2/plugins/terminals/commands.ts` | ready | 133 | 85 | napi-rs | String offsets and slicing (4); Regexes built at runtime (1) |
| 0 | `pi-src/apps/p2/plugins/terminals/ghostty.ts` | caution | 750 | 52 | napi-rs | String offsets and slicing (10); Callback-shaped APIs (6) |
| 1 | `pi-src/apps/p2/plugins/terminals/keys.ts` | ready | 88 | 92 | napi-rs | String offsets and slicing (3); Regex features the `regex` crate lacks (1) |
| 1 | `pi-src/apps/p2/plugins/terminals/detached.ts` | ready | 145 | 80 | napi-rs | String offsets and slicing (2); Async and promises (16) |
| 7 | `pi-src/apps/p2/plugins/terminals/daemon.ts` | ready | 74 | 99 | napi-rs | Async and promises (6); Exceptions (6) |
| 18 | `pi-src/apps/p2/plugins/terminals/log-preload.ts` | ready | 3 | 100 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/terminals/activity.tsx` | keep | 58 | 96 | keep | JSON round-trips (1) |
| 18 | `pi-src/apps/p2/plugins/terminals/tui.ts` | ready | 40 | 95 | napi-rs | String offsets and slicing (1) |
| 18 | `pi-src/apps/p2/plugins/terminals/service.ts` | caution | 366 | 70 | napi-rs | Async and promises (37); Insertion-ordered maps and object keys (4) |
| 19 | `pi-src/apps/p2/plugins/terminals/contract.ts` | ready | 120 | 100 | napi-rs | - |
| 20 | `pi-src/apps/p2/plugins/terminals/server.ts` | ready | 192 | 90 | napi-rs | Async and promises (21); Insertion-ordered maps and object keys (1) |
| 20 | `pi-src/apps/p2/plugins/terminals/tool.ts` | caution | 289 | 77 | napi-rs | String offsets and slicing (1); Async and promises (20) |
| 20 | `pi-src/apps/p2/plugins/terminals/Terminal.tsx` | keep | 1358 | 29 | keep | String offsets and slicing (4); Callback-shaped APIs (17) |
| 21 | `pi-src/apps/p2/plugins/terminals/worker.ts` | ready | 50 | 90 | napi-rs | Async and promises (9); Callback-shaped APIs (1) |
| 22 | `pi-src/apps/p2/plugins/terminals/envs.ts` | ready | 3 | 98 | napi-rs | - |
| 23 | `pi-src/apps/p2/plugins/terminals/main.ts` | caution | 690 | 40 | napi-rs | String offsets and slicing (3); Regexes built at runtime (1) |
| 27 | `pi-src/apps/p2/plugins/terminals/commands.tsx` | keep | 45 | 97 | keep | - |
| 28 | `pi-src/apps/p2/plugins/terminals/web.tsx` | keep | 119 | 96 | keep | Clock and random numbers (1) |

### 47. tree (`pi-src/apps/p2/plugins/tree`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/apps/p2/plugins/tree/entry-in-view.ts` | keep | 56 | 94 | keep | Callback-shaped APIs (2) |
| 18 | `pi-src/apps/p2/plugins/tree/contract.ts` | ready | 55 | 98 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/tree/branch-target.ts` | ready | 20 | 95 | napi-rs | JSON round-trips (2); Exceptions (1) |
| 19 | `pi-src/apps/p2/plugins/tree/toc.ts` | ready | 109 | 100 | napi-rs | Insertion-ordered maps and object keys (7) |
| 19 | `pi-src/apps/p2/plugins/tree/worker.ts` | ready | 327 | 66 | napi-rs | String offsets and slicing (1); Async and promises (74) |
| 20 | `pi-src/apps/p2/plugins/tree/TreePanel.tsx` | keep | 204 | 91 | keep | Callback-shaped APIs (1); JSON round-trips (1) |
| 21 | `pi-src/apps/p2/plugins/tree/web.tsx` | keep | 32 | 99 | keep | - |

### 48. views (`pi-src/apps/p2/plugins/views`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/apps/p2/plugins/views/layout.ts` | ready | 327 | 77 | napi-rs | String offsets and slicing (2); Insertion-ordered maps and object keys (6) |
| 1 | `pi-src/apps/p2/plugins/views/layout-store.ts` | keep | 73 | 81 | keep | Callback-shaped APIs (3); Insertion-ordered maps and object keys (2) |
| 17 | `pi-src/apps/p2/plugins/views/contract.ts` | ready | 225 | 80 | napi-rs | Async and promises (2); Callback-shaped APIs (2) |
| 18 | `pi-src/apps/p2/plugins/views/activity.tsx` | keep | 38 | 98 | keep | - |
| 18 | `pi-src/apps/p2/plugins/views/display.ts` | ready | 19 | 94 | napi-rs | Dynamic typing (1) |
| 18 | `pi-src/apps/p2/plugins/views/sheet.tsx` | keep | 200 | 87 | keep | Async and promises (2); Insertion-ordered maps and object keys (1) |
| 18 | `pi-src/apps/p2/plugins/views/contract.web.ts` | ready | 13 | 83 | napi-rs | Callback-shaped APIs (7) |
| 18 | `pi-src/apps/p2/plugins/views/doc.ts` | ready | 188 | 71 | napi-rs | String offsets and slicing (3); Async and promises (6) |
| 19 | `pi-src/apps/p2/plugins/views/tui.ts` | ready | 9 | 100 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/views/pane-kit.tsx` | keep | 41 | 87 | keep | String offsets and slicing (2); Callback-shaped APIs (1) |
| 19 | `pi-src/apps/p2/plugins/views/kinds.ts` | ready | 14 | 86 | napi-rs | String offsets and slicing (1); Insertion-ordered maps and object keys (1) |
| 19 | `pi-src/apps/p2/plugins/views/commands.tsx` | keep | 207 | 82 | keep | Async and promises (7); Insertion-ordered maps and object keys (2) |
| 19 | `pi-src/apps/p2/plugins/views/tool.ts` | ready | 238 | 72 | napi-rs | String offsets and slicing (1); Async and promises (20) |
| 20 | `pi-src/apps/p2/plugins/views/panes.tsx` | keep | 182 | 94 | keep | - |
| 20 | `pi-src/apps/p2/plugins/views/annotate.tsx` | keep | 414 | 69 | keep | String offsets and slicing (5); Async and promises (2) |
| 20 | `pi-src/apps/p2/plugins/views/worker.ts` | ready | 199 | 68 | napi-rs | String offsets and slicing (2); Async and promises (35) |
| 21 | `pi-src/apps/p2/plugins/views/Workbench.tsx` | keep | 358 | 83 | keep | Insertion-ordered maps and object keys (2); Class inheritance (1) |
| 22 | `pi-src/apps/p2/plugins/views/web.tsx` | keep | 38 | 99 | keep | - |

### 49. voice (`pi-src/apps/p2/plugins/voice`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/apps/p2/plugins/voice/barge-in.ts` | ready | 164 | 98 | napi-rs | String offsets and slicing (1); Number semantics (1) |
| 0 | `pi-src/apps/p2/plugins/voice/speech-text.ts` | ready | 106 | 78 | napi-rs | String offsets and slicing (10); Regex features the `regex` crate lacks (4) |
| 0 | `pi-src/apps/p2/plugins/voice/playback.ts` | caution | 309 | 72 | napi-rs | Callback-shaped APIs (10); Async and promises (3) |
| 1 | `pi-src/apps/p2/plugins/voice/policy.ts` | ready | 5 | 100 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/voice/protocol.ts` | ready | 116 | 95 | napi-rs | String offsets and slicing (1); Number semantics (1) |
| 19 | `pi-src/apps/p2/plugins/voice/contract.ts` | ready | 49 | 100 | napi-rs | - |
| 19 | `pi-src/apps/p2/plugins/voice/transcripts.ts` | ready | 178 | 84 | napi-rs | String offsets and slicing (2); Callback-shaped APIs (7) |
| 20 | `pi-src/apps/p2/plugins/voice/interruption.ts` | ready | 135 | 95 | napi-rs | Callback-shaped APIs (7); Insertion-ordered maps and object keys (2) |
| 20 | `pi-src/apps/p2/plugins/voice/worker.ts` | ready | 153 | 90 | napi-rs | Async and promises (19); Insertion-ordered maps and object keys (2) |
| 20 | `pi-src/apps/p2/plugins/voice/server.ts` | ready | 403 | 62 | napi-rs | String offsets and slicing (2); Async and promises (30) |
| 21 | `pi-src/apps/p2/plugins/voice/engine.ts` | caution | 903 | 39 | napi-rs | String offsets and slicing (8); Async and promises (13) |
| 22 | `pi-src/apps/p2/plugins/voice/choice.ts` | ready | 36 | 100 | napi-rs | String offsets and slicing (2) |
| 23 | `pi-src/apps/p2/plugins/voice/web.tsx` | keep | 502 | 72 | keep | Callback-shaped APIs (10); Insertion-ordered maps and object keys (4) |

### 50. web-shell (`pi-src/apps/p2/plugins/web-shell`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 18 | `pi-src/apps/p2/plugins/web-shell/commands.tsx` | keep | 102 | 93 | keep | Async and promises (2) |
| 19 | `pi-src/apps/p2/plugins/web-shell/web.tsx` | keep | 52 | 99 | keep | - |

### 51. workspace (`pi-src/apps/p2/plugins/workspace`, node, 0 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/apps/p2/plugins/workspace/fuzzy.ts` | ready | 308 | 73 | napi-rs | String offsets and slicing (6); Regex features the `regex` crate lacks (1) |
| 18 | `pi-src/apps/p2/plugins/workspace/contract.ts` | ready | 93 | 96 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/workspace/tabs.ts` | ready | 23 | 95 | napi-rs | - |
| 18 | `pi-src/apps/p2/plugins/workspace/protocol.ts` | ready | 111 | 81 | napi-rs | String offsets and slicing (6); Insertion-ordered maps and object keys (1) |
| 19 | `pi-src/apps/p2/plugins/workspace/tui.ts` | ready | 353 | 79 | napi-rs | String offsets and slicing (8); Async and promises (3) |
| 19 | `pi-src/apps/p2/plugins/workspace/server.ts` | ready | 174 | 75 | napi-rs | String offsets and slicing (1); Async and promises (13) |
| 19 | `pi-src/apps/p2/plugins/workspace/file.tsx` | keep | 392 | 66 | keep | String offsets and slicing (6); Callback-shaped APIs (7) |
| 19 | `pi-src/apps/p2/plugins/workspace/environment.ts` | caution | 803 | 41 | napi-rs | String offsets and slicing (29); Async and promises (90) |
| 20 | `pi-src/apps/p2/plugins/workspace/worker.ts` | ready | 259 | 83 | napi-rs | Async and promises (29); Callback-shaped APIs (3) |
| 20 | `pi-src/apps/p2/plugins/workspace/Workspace.tsx` | keep | 1201 | 21 | keep | String offsets and slicing (18); Regex features the `regex` crate lacks (2) |
| 27 | `pi-src/apps/p2/plugins/workspace/commands.tsx` | keep | 41 | 97 | keep | - |
| 28 | `pi-src/apps/p2/plugins/workspace/web.tsx` | keep | 131 | 89 | keep | Async and promises (2); Callback-shaped APIs (1) |

### 52. @earendil-works/p3-sessions (`pi-src/apps/p3/plugins/sessions`, portable, 1 test file)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/apps/p3/plugins/sessions/contract.ts` | ready | 72 | 100 | wasm-bindgen | - |
| 1 | `pi-src/apps/p3/plugins/sessions/contract.server.ts` | ready | 17 | 100 | wasm-bindgen | - |
| 1 | `pi-src/apps/p3/plugins/sessions/worker.ts` | ready | 8 | 100 | wasm-bindgen | Exceptions (1) |
| 2 | `pi-src/apps/p3/plugins/sessions/server.ts` | ready | 112 | 99 | wasm-bindgen | Async and promises (5); Exceptions (4) |

### 53. @earendil-works/pi-codemode (`pi-src/packages/codemode`, node, 6 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/packages/codemode/src/identifier.ts` | ready | 8 | 98 | napi-rs | - |
| 0 | `pi-src/packages/codemode/src/wasm.ts` | ready | 22 | 91 | napi-rs | Insertion-ordered maps and object keys (1); Callback-shaped APIs (1) |
| 0 | `pi-src/packages/codemode/src/source.ts` | ready | 93 | 88 | napi-rs | String offsets and slicing (3); Insertion-ordered maps and object keys (1) |
| 1 | `pi-src/packages/codemode/src/types.ts` | ready | 110 | 89 | napi-rs | Callback-shaped APIs (8) |
| 2 | `pi-src/packages/codemode/src/declarations.ts` | ready | 283 | 67 | napi-rs | String offsets and slicing (7); Insertion-ordered maps and object keys (8) |
| 2 | `pi-src/packages/codemode/src/runtime/` | caution | 1517 | 9 | napi-rs | String offsets and slicing (2); Async and promises (26) |
| 3 | `pi-src/packages/codemode/src/portable.ts` | ready | 47 | 100 | napi-rs | - |
| 3 | `pi-src/packages/codemode/src/index.ts` | ready | 66 | 98 | napi-rs | - |
| 4 | `pi-src/packages/codemode/src/cloudflare.ts` | caution | 27 | 97 | napi-rs | Class inheritance (1) |

### 54. @earendil-works/pi-mcp (`pi-src/packages/mcp`, node, 16 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/packages/mcp/src/input-schema.ts` | ready | 7 | 100 | napi-rs | - |
| 0 | `pi-src/packages/mcp/src/auth-provider.ts` | ready | 11 | 95 | napi-rs | Callback-shaped APIs (3) |
| 0 | `pi-src/packages/mcp/src/config.ts` | ready | 152 | 95 | napi-rs | Insertion-ordered maps and object keys (1); Callback-shaped APIs (1) |
| 0 | `pi-src/packages/mcp/src/naming.ts` | ready | 19 | 93 | napi-rs | String offsets and slicing (2); Callback-shaped APIs (1) |
| 0 | `pi-src/packages/mcp/src/protocol/` | ready | 294 | 78 | napi-rs | Dynamic typing (5); Class inheritance (4) |
| 1 | `pi-src/packages/mcp/src/output-limit.ts` | caution | 47 | 95 | napi-rs | Async and promises (2); Callback-shaped APIs (1) |
| 1 | `pi-src/packages/mcp/src/transports/` | caution | 905 | 17 | napi-rs | String offsets and slicing (10); Regex features the `regex` crate lacks (1) |
| 1 | `pi-src/packages/mcp/src/oauth/` | caution | 1696 | 2 | napi-rs | String offsets and slicing (2); Regexes built at runtime (1) |
| 2 | `pi-src/packages/mcp/src/testing/` | ready | 1 | 100 | napi-rs | - |
| 2 | `pi-src/packages/mcp/src/client.ts` | caution | 547 | 55 | napi-rs | Async and promises (38); Callback-shaped APIs (18) |
| 3 | `pi-src/packages/mcp/src/index.ts` | ready | 96 | 93 | napi-rs | - |

### 55. @earendil-works/pi-schema (`pi-src/packages/schema`, portable, 19 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/packages/schema/src/typebox-static.ts` | ready | 71 | 99 | wasm-bindgen | Dynamic typing (1) |
| 0 | `pi-src/packages/schema/src/builders.ts` | ready | 149 | 92 | wasm-bindgen | Insertion-ordered maps and object keys (1); Callback-shaped APIs (1) |
| 0 | `pi-src/packages/schema/src/shared.ts` | ready | 20 | 92 | wasm-bindgen | Regexes built at runtime (2); Reflection and prototypes (2) |
| 0 | `pi-src/packages/schema/src/types.ts` | ready | 19 | 87 | wasm-bindgen | Callback-shaped APIs (2) |
| 0 | `pi-src/packages/schema/src/formats/typebox/idna/pattern/` | ready | 68 | 86 | wasm-bindgen | Regex features the `regex` crate lacks (2); Regexes built at runtime (2) |
| 1 | `pi-src/packages/schema/src/formats/typebox/idna/format/` | ready | 203 | 77 | wasm-bindgen | String offsets and slicing (7); Regexes built at runtime (1) |
| 1 | `pi-src/packages/schema/src/resolver.ts` | ready | 160 | 77 | wasm-bindgen | String offsets and slicing (4); Insertion-ordered maps and object keys (8) |
| 1 | `pi-src/packages/schema/src/checks.ts` | ready | 470 | 66 | wasm-bindgen | String offsets and slicing (3); Insertion-ordered maps and object keys (10) |
| 2 | `pi-src/packages/schema/src/annotations.ts` | ready | 169 | 89 | wasm-bindgen | Insertion-ordered maps and object keys (5); Callback-shaped APIs (1) |
| 2 | `pi-src/packages/schema/src/formats/typebox/idna/label/` | ready | 131 | 89 | wasm-bindgen | String offsets and slicing (4); Exceptions (1) |
| 3 | `pi-src/packages/schema/src/formats/typebox/idna/` | ready | 98 | 93 | wasm-bindgen | String offsets and slicing (2) |
| 3 | `pi-src/packages/schema/src/compiler.ts` | caution | 551 | 68 | wasm-bindgen | String offsets and slicing (1); Insertion-ordered maps and object keys (15) |
| 4 | `pi-src/packages/schema/src/coerce.ts` | ready | 472 | 77 | wasm-bindgen | String offsets and slicing (2); Insertion-ordered maps and object keys (11) |
| 4 | `pi-src/packages/schema/src/explain.ts` | ready | 358 | 73 | wasm-bindgen | String offsets and slicing (3); Insertion-ordered maps and object keys (12) |
| 4 | `pi-src/packages/schema/src/formats/typebox/` | ready | 530 | 69 | wasm-bindgen | Regex features the `regex` crate lacks (3); String offsets and slicing (2) |
| 5 | `pi-src/packages/schema/src/formats/` | ready | 45 | 100 | wasm-bindgen | Reflection and prototypes (1) |
| 5 | `pi-src/packages/schema/src/validator.ts` | caution | 62 | 92 | wasm-bindgen | Callback-shaped APIs (2); Class inheritance (1) |
| 6 | `pi-src/packages/schema/src/index.ts` | ready | 33 | 94 | wasm-bindgen | - |

### 56. @earendil-works/pi-telemetry (`pi-src/packages/telemetry`, node, 8 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/packages/telemetry/src/noop.ts` | ready | 28 | 90 | napi-rs | Reflection and prototypes (6); Async and promises (1) |
| 0 | `pi-src/packages/telemetry/src/metrics.ts` | caution | 119 | 86 | napi-rs | Callback-shaped APIs (11); Insertion-ordered maps and object keys (2) |
| 0 | `pi-src/packages/telemetry/src/index.ts` | ready | 387 | 76 | napi-rs | Callback-shaped APIs (18) |
| 0 | `pi-src/packages/telemetry/src/memory.ts` | ready | 429 | 64 | napi-rs | String offsets and slicing (1); Callback-shaped APIs (7) |
| 1 | `pi-src/packages/telemetry/src/render.ts` | ready | 57 | 93 | napi-rs | String offsets and slicing (2); Insertion-ordered maps and object keys (3) |
| 1 | `pi-src/packages/telemetry/src/testing/` | caution | 381 | 75 | napi-rs | Async and promises (74); Callback-shaped APIs (6) |

### 57. @earendil-works/pi-ai (`pi-src/packages/ai`, node, 199 test files, uses @earendil-works/pi-schema, @earendil-works/pi-telemetry)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/packages/ai/src/session-resources.ts` | ready | 21 | 99 | napi-rs | Callback-shaped APIs (2); Insertion-ordered maps and object keys (1) |
| 0 | `pi-src/packages/ai/scripts/` | ready | 399 | 68 | napi-rs | String offsets and slicing (4); Insertion-ordered maps and object keys (20) |
| 7 | `pi-src/packages/ai/src/models.generated.ts` | ready | 310 | 99 | napi-rs | - |
| 7 | `pi-src/packages/ai/src/model-catalog.ts` | caution | 69 | 97 | napi-rs | Insertion-ordered maps and object keys (3) |
| 7 | `pi-src/packages/ai/src/env-api-keys.ts` | blocked | 145 | 96 | napi-rs | Dynamic code (eval, new Function, computed import) (1); Callback-shaped APIs (1) |
| 7 | `pi-src/packages/ai/src/models-store.ts` | caution | 31 | 96 | napi-rs | Async and promises (3); Callback-shaped APIs (3) |
| 7 | `pi-src/packages/ai/src/telemetry.ts` | caution | 202 | 96 | napi-rs | Insertion-ordered maps and object keys (1); Async and promises (1) |
| 7 | `pi-src/packages/ai/src/types.ts` | ready | 568 | 73 | napi-rs | Callback-shaped APIs (14) |
| 7 | `pi-src/packages/ai/src/auth/` | blocked | 415 | 57 | napi-rs | String offsets and slicing (1); Dynamic code (eval, new Function, computed import) (1) |
| 7 | `pi-src/packages/ai/src/models.ts` | caution | 1049 | 25 | napi-rs | String offsets and slicing (1); Async and promises (85) |
| 7 | `pi-src/packages/ai/src/api/` | blocked | 11748 | 0 | napi-rs | String offsets and slicing (48); Regex features the `regex` crate lacks (3) |
| 7 | `pi-src/packages/ai/src/auth/oauth/` | blocked | 2673 | 0 | napi-rs | Dynamic code (eval, new Function, computed import) (1); Async and promises (230) |
| 7 | `pi-src/packages/ai/src/providers/` | caution | 3364 | 0 | napi-rs | String offsets and slicing (3); Async and promises (91) |
| 7 | `pi-src/packages/ai/src/utils/` | caution | 2157 | 0 | napi-rs | String offsets and slicing (41); Regex features the `regex` crate lacks (24) |
| 8 | `pi-src/packages/ai/src/bedrock-provider.ts` | ready | 5 | 100 | napi-rs | - |
| 8 | `pi-src/packages/ai/src/bun-oauth.ts` | ready | 23 | 100 | napi-rs | - |
| 8 | `pi-src/packages/ai/src/cache-warming.ts` | ready | 77 | 100 | napi-rs | - |
| 8 | `pi-src/packages/ai/src/image-models.ts` | ready | 34 | 100 | napi-rs | Insertion-ordered maps and object keys (4) |
| 8 | `pi-src/packages/ai/src/images-api-registry.ts` | caution | 45 | 100 | napi-rs | Insertion-ordered maps and object keys (1); Callback-shaped APIs (1) |
| 8 | `pi-src/packages/ai/src/lazy-bedrock-provider.ts` | ready | 4 | 100 | napi-rs | - |
| 8 | `pi-src/packages/ai/src/legacy-api-aliases.ts` | ready | 83 | 100 | napi-rs | - |
| 8 | `pi-src/packages/ai/src/compat/` | ready | 36 | 94 | napi-rs | Callback-shaped APIs (6) |
| 8 | `pi-src/packages/ai/src/lazy-oauth.ts` | ready | 14 | 93 | napi-rs | Async and promises (18) |
| 8 | `pi-src/packages/ai/catalog/families/` | ready | 327 | 82 | napi-rs | String offsets and slicing (1); Insertion-ordered maps and object keys (9) |
| 8 | `pi-src/packages/ai/catalog/sources/` | ready | 507 | 70 | napi-rs | Async and promises (25); Insertion-ordered maps and object keys (10) |
| 8 | `pi-src/packages/ai/catalog/` | ready | 536 | 62 | napi-rs | String offsets and slicing (1); Insertion-ordered maps and object keys (21) |
| 8 | `pi-src/packages/ai/catalog/providers/` | caution | 1825 | 40 | napi-rs | String offsets and slicing (6); Insertion-ordered maps and object keys (30) |
| 9 | `pi-src/packages/ai/src/oauth.ts` | ready | 9 | 100 | napi-rs | - |
| 9 | `pi-src/packages/ai/src/index.ts` | ready | 43 | 94 | napi-rs | - |
| 9 | `pi-src/packages/ai/src/providers/images/` | ready | 50 | 94 | napi-rs | Async and promises (3); Exceptions (1) |
| 10 | `pi-src/packages/ai/src/images.ts` | caution | 18 | 100 | napi-rs | Async and promises (1); Exceptions (1) |
| 10 | `pi-src/packages/ai/src/cli.ts` | ready | 115 | 83 | napi-rs | String offsets and slicing (3); Async and promises (8) |
| 11 | `pi-src/packages/ai/src/compat.ts` | ready | 247 | 77 | napi-rs | String offsets and slicing (1); Insertion-ordered maps and object keys (2) |

### 58. @earendil-works/pi-agent-core (`pi-src/packages/agent`, node, 8 test files, uses @earendil-works/pi-ai, @earendil-works/pi-schema)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 10 | `pi-src/packages/agent/src/types.ts` | ready | 165 | 83 | napi-rs | Callback-shaped APIs (18); Dynamic typing (16) |
| 10 | `pi-src/packages/agent/src/proxy.ts` | caution | 320 | 82 | napi-rs | String offsets and slicing (1); Async and promises (4) |
| 11 | `pi-src/packages/agent/src/stream-fn.ts` | caution | 11 | 99 | napi-rs | Exceptions (1) |
| 12 | `pi-src/packages/agent/src/agent-loop.ts` | caution | 775 | 52 | napi-rs | String offsets and slicing (2); Async and promises (91) |
| 13 | `pi-src/packages/agent/src/agent.ts` | caution | 497 | 45 | napi-rs | String offsets and slicing (8); Async and promises (33) |
| 14 | `pi-src/packages/agent/src/index.ts` | ready | 5 | 95 | napi-rs | - |

### 59. @earendil-works/chord (`pi-src/packages/chord`, node, 299 test files, uses @earendil-works/pi-schema, @earendil-works/pi-telemetry)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/packages/chord/src/json.ts` | ready | 121 | 74 | napi-rs | Reflection and prototypes (9); Dynamic typing (4) |
| 0 | `pi-src/packages/chord/src/output.ts` | ready | 304 | 67 | napi-rs | String offsets and slicing (15); Callback-shaped APIs (10) |
| 0 | `pi-src/packages/chord/src/sqlite/` | caution | 381 | 37 | napi-rs | String offsets and slicing (18); Async and promises (32) |
| 1 | `pi-src/packages/chord/src/context/` | caution | 120 | 74 | napi-rs | Reflection and prototypes (6); Callback-shaped APIs (5) |
| 1 | `pi-src/packages/chord/src/delta/` | caution | 3426 | 0 | napi-rs | String offsets and slicing (33); Insertion-ordered maps and object keys (42) |
| 2 | `pi-src/packages/chord/src/watch.ts` | caution | 307 | 69 | napi-rs | Callback-shaped APIs (18); Async and promises (3) |
| 7 | `pi-src/packages/chord/src/contracts/` | caution | 1779 | 0 | napi-rs | String offsets and slicing (4); Insertion-ordered maps and object keys (59) |
| 8 | `pi-src/packages/chord/src/telemetry.ts` | ready | 511 | 69 | napi-rs | String offsets and slicing (2); Insertion-ordered maps and object keys (1) |
| 8 | `pi-src/packages/chord/src/data/` | ready | 409 | 54 | napi-rs | String offsets and slicing (3); Callback-shaped APIs (9) |
| 9 | `pi-src/packages/chord/src/state/` | caution | 706 | 45 | napi-rs | Callback-shaped APIs (32); Insertion-ordered maps and object keys (7) |
| 9 | `pi-src/packages/chord/src/link/` | caution | 1608 | 0 | napi-rs | String offsets and slicing (20); Callback-shaped APIs (44) |
| 10 | `pi-src/packages/chord/src/node/` | ready | 15 | 100 | napi-rs | Exceptions (1) |
| 10 | `pi-src/packages/chord/src/store/` | caution | 6896 | 0 | napi-rs | String offsets and slicing (21); Async and promises (303) |
| 11 | `pi-src/packages/chord/src/registries/` | caution | 592 | 60 | napi-rs | String offsets and slicing (1); Callback-shaped APIs (22) |
| 11 | `pi-src/packages/chord/src/sync/` | caution | 969 | 18 | napi-rs | String offsets and slicing (8); Async and promises (143) |
| 11 | `pi-src/packages/chord/src/jobs/` | caution | 1811 | 13 | napi-rs | String offsets and slicing (2); Async and promises (59) |
| 11 | `pi-src/packages/chord/src/documents/` | caution | 1676 | 11 | napi-rs | Callback-shaped APIs (70); Async and promises (63) |
| 11 | `pi-src/packages/chord/src/host/` | caution | 9491 | 0 | napi-rs | String offsets and slicing (14); Callback-shaped APIs (273) |
| 11 | `pi-src/packages/chord/src/remote/` | caution | 2600 | 0 | napi-rs | String offsets and slicing (5); Callback-shaped APIs (29) |
| 12 | `pi-src/packages/chord/src/telemetry-ingest/` | ready | 573 | 54 | napi-rs | String offsets and slicing (3); Callback-shaped APIs (17) |
| 12 | `pi-src/packages/chord/src/surfaces/` | caution | 1266 | 18 | napi-rs | String offsets and slicing (3); Callback-shaped APIs (69) |
| 12 | `pi-src/packages/chord/src/node/http/` | caution | 1987 | 0 | napi-rs | String offsets and slicing (29); Async and promises (90) |
| 13 | `pi-src/packages/chord/src/react/` | keep | 141 | 97 | keep | Callback-shaped APIs (8); Async and promises (1) |
| 13 | `pi-src/packages/chord/src/index.ts` | ready | 40 | 94 | napi-rs | - |
| 13 | `pi-src/packages/chord/src/telemetry-sqlite/` | caution | 387 | 73 | napi-rs | Async and promises (26); Callback-shaped APIs (4) |
| 13 | `pi-src/packages/chord/src/web/` | caution | 404 | 60 | napi-rs | String offsets and slicing (2); Async and promises (16) |
| 13 | `pi-src/packages/chord/src/tui/` | ready | 533 | 53 | napi-rs | String offsets and slicing (7); Callback-shaped APIs (56) |
| 13 | `pi-src/packages/chord/src/node/plugins/` | caution | 2267 | 0 | napi-rs | String offsets and slicing (16); Regexes built at runtime (1) |
| 14 | `pi-src/packages/chord/src/node/surfaces/` | caution | 596 | 45 | napi-rs | String offsets and slicing (5); Async and promises (38) |
| 15 | `pi-src/packages/chord/src/node/workers/` | caution | 1449 | 11 | napi-rs | String offsets and slicing (2); Async and promises (83) |
| 16 | `pi-src/packages/chord/src/node/app/` | caution | 3161 | 0 | napi-rs | String offsets and slicing (24); Async and promises (420) |
| 17 | `pi-src/packages/chord/src/node/tui/` | caution | 1042 | 35 | napi-rs | String offsets and slicing (13); Async and promises (62) |

### 60. @earendil-works/pi-durable (`pi-src/packages/durable`, node, 130 test files, uses @earendil-works/pi-ai, @earendil-works/chord, @earendil-works/pi-codemode, @earendil-works/pi-schema, @earendil-works/pi-telemetry)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/packages/durable/src/search.ts` | ready | 127 | 88 | napi-rs | String offsets and slicing (4); Insertion-ordered maps and object keys (7) |
| 1 | `pi-src/packages/durable/src/truncate.ts` | ready | 114 | 100 | napi-rs | Number semantics (2) |
| 2 | `pi-src/packages/durable/src/env/` | caution | 1927 | 0 | napi-rs | String offsets and slicing (9); Async and promises (132) |
| 14 | `pi-src/packages/durable/src/entries.ts` | caution | 16 | 100 | napi-rs | Exceptions (1) |
| 14 | `pi-src/packages/durable/src/tasks.ts` | caution | 6 | 100 | napi-rs | - |
| 14 | `pi-src/packages/durable/src/ids.ts` | ready | 7 | 98 | napi-rs | - |
| 14 | `pi-src/packages/durable/src/timers.ts` | ready | 56 | 98 | napi-rs | Callback-shaped APIs (8); Exceptions (2) |
| 14 | `pi-src/packages/durable/src/documents.ts` | ready | 175 | 92 | napi-rs | Callback-shaped APIs (4); Exceptions (5) |
| 14 | `pi-src/packages/durable/src/errors.ts` | caution | 49 | 89 | napi-rs | Class inheritance (6); Exceptions (1) |
| 14 | `pi-src/packages/durable/src/signals.ts` | caution | 238 | 78 | napi-rs | Dynamic typing (4); Callback-shaped APIs (3) |
| 14 | `pi-src/packages/durable/src/types.ts` | ready | 902 | 58 | napi-rs | Callback-shaped APIs (122) |
| 14 | `pi-src/packages/durable/src/harness/` | caution | 9241 | 0 | napi-rs | String offsets and slicing (18); Async and promises (717) |
| 14 | `pi-src/packages/durable/src/session/` | caution | 3601 | 0 | napi-rs | String offsets and slicing (2); Async and promises (166) |
| 15 | `pi-src/packages/durable/src/tool-search/` | ready | 88 | 96 | napi-rs | Insertion-ordered maps and object keys (2); Async and promises (2) |
| 15 | `pi-src/packages/durable/src/tools/` | caution | 1275 | 18 | napi-rs | String offsets and slicing (31); Async and promises (71) |
| 15 | `pi-src/packages/durable/src/storage/` | caution | 2032 | 0 | napi-rs | String offsets and slicing (2); Async and promises (130) |
| 15 | `pi-src/packages/durable/src/testing/` | caution | 3230 | 0 | napi-rs | String offsets and slicing (14); Async and promises (786) |
| 16 | `pi-src/packages/durable/src/index.ts` | ready | 250 | 89 | napi-rs | - |
| 16 | `pi-src/packages/durable/src/images/` | ready | 408 | 61 | napi-rs | String offsets and slicing (2); Async and promises (17) |
| 16 | `pi-src/packages/durable/src/storage/store/` | caution | 1051 | 46 | napi-rs | String offsets and slicing (1); Async and promises (64) |
| 16 | `pi-src/packages/durable/src/storage/jsonl/` | caution | 872 | 43 | napi-rs | String offsets and slicing (3); Async and promises (76) |
| 16 | `pi-src/packages/durable/src/storage/sqlite/` | caution | 1418 | 26 | napi-rs | String offsets and slicing (3); Async and promises (103) |
| 16 | `pi-src/packages/durable/src/codemode/` | caution | 1333 | 21 | napi-rs | String offsets and slicing (7); Async and promises (72) |
| 17 | `pi-src/packages/durable/test-workers/` | caution | 125 | 94 | napi-rs | Async and promises (23); Class inheritance (1) |

### 61. @earendil-works/p3 (`pi-src/apps/p3`, node, 41 test files, uses @earendil-works/pi-ai, @earendil-works/chord, @earendil-works/pi-durable, @earendil-works/pi-schema, @earendil-works/pi-telemetry)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/apps/p3/src/minimum-node.ts` | ready | 10 | 100 | napi-rs | - |
| 0 | `pi-src/apps/p3/scripts/` | ready | 241 | 73 | napi-rs | String offsets and slicing (1); Async and promises (31) |
| 14 | `pi-src/apps/p3/src/principals.ts` | ready | 8 | 100 | napi-rs | - |
| 15 | `pi-src/apps/p3/src/app.ts` | ready | 25 | 100 | napi-rs | Callback-shaped APIs (1) |
| 15 | `pi-src/apps/p3/src/session-route.ts` | ready | 11 | 100 | napi-rs | - |
| 16 | `pi-src/apps/p3/src/kit/` | caution | 1036 | 19 | napi-rs | String offsets and slicing (7); Async and promises (113) |
| 17 | `pi-src/apps/p3/src/api/v1/` | caution | 21 | 100 | napi-rs | Callback-shaped APIs (1) |
| 17 | `pi-src/apps/p3/src/install.ts` | ready | 30 | 100 | napi-rs | JSON round-trips (1) |
| 17 | `pi-src/apps/p3/src/pi.ts` | ready | 29 | 100 | napi-rs | Async and promises (2); Exceptions (1) |
| 17 | `pi-src/apps/p3/src/ask.ts` | ready | 113 | 89 | napi-rs | Async and promises (11); Exceptions (3) |
| 18 | `pi-src/apps/p3/src/commands.ts` | caution | 214 | 70 | napi-rs | String offsets and slicing (4); Async and promises (38) |
| 19 | `pi-src/apps/p3/src/bootstrap/` | caution | 193 | 84 | napi-rs | String offsets and slicing (1); Async and promises (23) |
| 20 | `pi-src/apps/p3/bench/` | caution | 337 | 50 | napi-rs | String offsets and slicing (5); Regexes built at runtime (2) |

### 62. @earendil-works/pi-env (`pi-src/packages/env`, node, 12 test files, uses @earendil-works/chord, @earendil-works/pi-durable)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/packages/env/src/connection.ts` | ready | 375 | 55 | napi-rs | String offsets and slicing (4); Async and promises (16) |
| 1 | `pi-src/packages/env/src/terminal.ts` | caution | 95 | 100 | napi-rs | Async and promises (3); Callback-shaped APIs (1) |
| 1 | `pi-src/packages/env/src/process.ts` | caution | 87 | 98 | napi-rs | Async and promises (3); Callback-shaped APIs (2) |
| 1 | `pi-src/packages/env/src/ssh.ts` | ready | 420 | 63 | napi-rs | String offsets and slicing (3); Async and promises (41) |
| 3 | `pi-src/packages/env/src/errors.ts` | caution | 31 | 99 | napi-rs | - |
| 4 | `pi-src/packages/env/src/list-files.ts` | caution | 83 | 93 | napi-rs | String offsets and slicing (1); Async and promises (5) |
| 4 | `pi-src/packages/env/src/watch.ts` | caution | 137 | 85 | napi-rs | Async and promises (9); Callback-shaped APIs (4) |
| 5 | `pi-src/packages/env/src/remote-env.ts` | caution | 868 | 44 | napi-rs | String offsets and slicing (7); Regex features the `regex` crate lacks (1) |
| 6 | `pi-src/packages/env/src/index.ts` | ready | 33 | 94 | napi-rs | - |

### 63. @earendil-works/pi-tui (`pi-src/packages/tui`, node, 57 test files)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/packages/tui/src/kill-ring.ts` | caution | 24 | 100 | napi-rs | - |
| 0 | `pi-src/packages/tui/src/native-module-path.ts` | caution | 25 | 100 | napi-rs | Insertion-ordered maps and object keys (1); Callback-shaped APIs (1) |
| 0 | `pi-src/packages/tui/src/undo-stack.ts` | caution | 15 | 100 | napi-rs | - |
| 0 | `pi-src/packages/tui/src/wheel-scroll.ts` | ready | 55 | 100 | napi-rs | - |
| 0 | `pi-src/packages/tui/native/win32/` | ready | 205 | 97 | napi-rs | Exceptions (5) |
| 0 | `pi-src/packages/tui/src/fuzzy.ts` | ready | 114 | 92 | napi-rs | String offsets and slicing (3); Callback-shaped APIs (1) |
| 0 | `pi-src/packages/tui/src/terminal-colors.ts` | ready | 72 | 87 | napi-rs | String offsets and slicing (8); Number semantics (5) |
| 0 | `pi-src/packages/tui/src/stdin-buffer.ts` | caution | 307 | 79 | napi-rs | String offsets and slicing (18); Class inheritance (1) |
| 0 | `pi-src/packages/tui/src/keys.ts` | ready | 1173 | 51 | napi-rs | String offsets and slicing (5); Insertion-ordered maps and object keys (4) |
| 0 | `pi-src/packages/tui/src/utils.ts` | caution | 1184 | 19 | napi-rs | String offsets and slicing (44); Regexes built at runtime (3) |
| 1 | `pi-src/packages/tui/src/program-status.ts` | ready | 33 | 100 | napi-rs | String offsets and slicing (1) |
| 1 | `pi-src/packages/tui/src/native-platform.ts` | caution | 49 | 96 | napi-rs | Callback-shaped APIs (6); Insertion-ordered maps and object keys (1) |
| 1 | `pi-src/packages/tui/src/oklab.ts` | ready | 166 | 96 | napi-rs | Number semantics (1) |
| 1 | `pi-src/packages/tui/src/keybindings.ts` | ready | 301 | 88 | napi-rs | Insertion-ordered maps and object keys (7); Dynamic typing (1) |
| 1 | `pi-src/packages/tui/src/word-navigation.ts` | caution | 76 | 87 | napi-rs | String offsets and slicing (4); Regexes built at runtime (1) |
| 1 | `pi-src/packages/tui/src/autocomplete.ts` | caution | 682 | 41 | napi-rs | String offsets and slicing (29); Regexes built at runtime (1) |
| 1 | `pi-src/packages/tui/src/latex.ts` | caution | 1423 | 31 | napi-rs | String offsets and slicing (17); Regex features the `regex` crate lacks (4) |
| 2 | `pi-src/packages/tui/src/native-modifiers.ts` | ready | 11 | 100 | napi-rs | Exceptions (1) |
| 2 | `pi-src/packages/tui/src/colors.ts` | ready | 305 | 66 | napi-rs | String offsets and slicing (7); Regexes built at runtime (2) |
| 3 | `pi-src/packages/tui/src/terminal.ts` | ready | 442 | 66 | napi-rs | String offsets and slicing (5); Callback-shaped APIs (24) |
| 3 | `pi-src/packages/tui/src/terminal-image.ts` | ready | 643 | 53 | napi-rs | String offsets and slicing (29); Insertion-ordered maps and object keys (2) |
| 4 | `pi-src/packages/tui/src/tui.ts` | caution | 1221 | 26 | napi-rs | String offsets and slicing (5); Callback-shaped APIs (50) |
| 5 | `pi-src/packages/tui/src/editor-component.ts` | ready | 16 | 94 | napi-rs | Callback-shaped APIs (12) |
| 5 | `pi-src/packages/tui/src/layout-node.ts` | caution | 42 | 91 | napi-rs | Callback-shaped APIs (5); Reflection and prototypes (1) |
| 5 | `pi-src/packages/tui/src/tui-main-screen.ts` | caution | 533 | 67 | napi-rs | String offsets and slicing (9); Insertion-ordered maps and object keys (4) |
| 6 | `pi-src/packages/tui/src/components/` | caution | 4467 | 0 | napi-rs | String offsets and slicing (137); Regexes built at runtime (4) |
| 7 | `pi-src/packages/tui/src/layout.ts` | caution | 417 | 90 | napi-rs | Insertion-ordered maps and object keys (2); Callback-shaped APIs (2) |
| 7 | `pi-src/packages/tui/src/alt-screen-search.ts` | ready | 289 | 81 | napi-rs | String offsets and slicing (6); Regexes built at runtime (1) |
| 8 | `pi-src/packages/tui/src/tui-alt-screen.ts` | caution | 1632 | 28 | napi-rs | String offsets and slicing (9); Callback-shaped APIs (14) |
| 9 | `pi-src/packages/tui/src/index.ts` | ready | 180 | 91 | napi-rs | - |

### 64. p2 (`pi-src/apps/p2`, node, 247 test files, uses @earendil-works/pi-ai, @earendil-works/chord, @earendil-works/pi-codemode, @earendil-works/pi-durable, @earendil-works/pi-env, @earendil-works/pi-mcp, @earendil-works/pi-schema, @earendil-works/pi-telemetry, @earendil-works/pi-tui)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/apps/p2/src/source-resolver.ts` | ready | 80 | 85 | napi-rs | String offsets and slicing (6); Insertion-ordered maps and object keys (1) |
| 0 | `pi-src/apps/p2/public/` | ready | 78 | 82 | napi-rs | String offsets and slicing (3); Async and promises (33) |
| 0 | `pi-src/apps/p2/docs/p3-reviews/session-sync-prototype/` | ready | 175 | 72 | napi-rs | String offsets and slicing (9); Insertion-ordered maps and object keys (8) |
| 0 | `pi-src/apps/p2/src/web/elanor-extra/` | keep | 1119 | 47 | keep | String offsets and slicing (1); Callback-shaped APIs (25) |
| 17 | `pi-src/apps/p2/src/api/v1/` | ready | 205 | 90 | napi-rs | - |
| 17 | `pi-src/apps/p2/src/tui/tools/` | ready | 265 | 77 | napi-rs | String offsets and slicing (3); Callback-shaped APIs (6) |
| 17 | `pi-src/apps/p2/src/hosts/web/` | keep | 265 | 71 | keep | String offsets and slicing (1); Async and promises (8) |
| 17 | `pi-src/apps/p2/src/web/command-menu/` | keep | 782 | 54 | keep | String offsets and slicing (7); Callback-shaped APIs (15) |
| 17 | `pi-src/apps/p2/src/tui/theme/` | caution | 1360 | 26 | napi-rs | String offsets and slicing (6); Regex features the `regex` crate lacks (1) |
| 17 | `pi-src/apps/p2/src/tui/` | caution | 1330 | 22 | napi-rs | String offsets and slicing (12); Regexes built at runtime (1) |
| 17 | `pi-src/apps/p2/src/web/activity/` | keep | 1442 | 14 | keep | String offsets and slicing (13); Regex features the `regex` crate lacks (1) |
| 17 | `pi-src/apps/p2/src/hosts/` | blocked | 6194 | 0 | napi-rs | String offsets and slicing (72); Regex features the `regex` crate lacks (4) |
| 17 | `pi-src/apps/p2/src/runtime/` | keep | 6024 | 0 | keep | String offsets and slicing (56); Regex features the `regex` crate lacks (7) |
| 17 | `pi-src/apps/p2/src/tui/components/` | caution | 2926 | 0 | napi-rs | String offsets and slicing (16); Callback-shaped APIs (74) |
| 17 | `pi-src/apps/p2/src/web/` | keep | 6827 | 0 | keep | String offsets and slicing (44); Regex features the `regex` crate lacks (2) |
| 18 | `pi-src/apps/p2/src/source.ts` | ready | 2 | 100 | napi-rs | - |
| 20 | `pi-src/apps/p2/scripts/` | caution | 1757 | 0 | napi-rs | String offsets and slicing (21); Async and promises (163) |

### 65. @earendil-works/pi-coding-agent (`pi-src/packages/coding-agent`, node, 384 test files, uses @earendil-works/pi-agent-core, @earendil-works/pi-ai, @earendil-works/pi-codemode, @earendil-works/pi-mcp, @earendil-works/pi-tui)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/packages/coding-agent/src/experimental/` | ready | 75 | 86 | napi-rs | String offsets and slicing (6); Insertion-ordered maps and object keys (1) |
| 0 | `pi-src/packages/coding-agent/src/core/export-html/vendor/` | caution | 1279 | 0 | napi-rs | String offsets and slicing (180); Regex features the `regex` crate lacks (87) |
| 10 | `pi-src/packages/coding-agent/src/config.ts` | ready | 504 | 71 | napi-rs | String offsets and slicing (2); Insertion-ordered maps and object keys (1) |
| 10 | `pi-src/packages/coding-agent/src/utils/` | caution | 3013 | 0 | napi-rs | String offsets and slicing (47); Regex features the `regex` crate lacks (3) |
| 15 | `pi-src/packages/coding-agent/src/extensions/` | ready | 11 | 100 | napi-rs | - |
| 15 | `pi-src/packages/coding-agent/src/index.ts` | ready | 483 | 93 | napi-rs | - |
| 15 | `pi-src/packages/coding-agent/src/migrations.ts` | ready | 215 | 90 | napi-rs | Insertion-ordered maps and object keys (3); Async and promises (3) |
| 15 | `pi-src/packages/coding-agent/src/modes/` | ready | 211 | 82 | napi-rs | Async and promises (22); Callback-shaped APIs (3) |
| 15 | `pi-src/packages/coding-agent/src/extensions/tool-search/` | ready | 254 | 76 | napi-rs | String offsets and slicing (4); Insertion-ordered maps and object keys (9) |
| 15 | `pi-src/packages/coding-agent/src/main.ts` | ready | 919 | 57 | napi-rs | String offsets and slicing (3); Async and promises (60) |
| 15 | `pi-src/packages/coding-agent/src/package-manager-cli.ts` | ready | 1121 | 53 | napi-rs | Async and promises (39); Insertion-ordered maps and object keys (2) |
| 15 | `pi-src/packages/coding-agent/src/core/tools/renderers/` | caution | 895 | 52 | napi-rs | String offsets and slicing (12); Dynamic typing (31) |
| 15 | `pi-src/packages/coding-agent/src/core/compaction/` | caution | 1170 | 39 | napi-rs | String offsets and slicing (3); Async and promises (15) |
| 15 | `pi-src/packages/coding-agent/src/modes/rpc/` | caution | 1244 | 26 | napi-rs | String offsets and slicing (6); Async and promises (122) |
| 15 | `pi-src/packages/coding-agent/src/extensions/codemode/` | caution | 1181 | 21 | napi-rs | String offsets and slicing (4); Async and promises (33) |
| 15 | `pi-src/packages/coding-agent/src/extensions/llama/` | caution | 1490 | 13 | napi-rs | String offsets and slicing (12); Async and promises (123) |
| 15 | `pi-src/packages/coding-agent/src/cli/` | caution | 1667 | 9 | napi-rs | String offsets and slicing (25); Async and promises (68) |
| 15 | `pi-src/packages/coding-agent/src/core/` | caution | 19336 | 0 | napi-rs | String offsets and slicing (97); Regex features the `regex` crate lacks (2) |
| 15 | `pi-src/packages/coding-agent/src/core/export-html/` | caution | 2172 | 0 | napi-rs | String offsets and slicing (19); Regex features the `regex` crate lacks (3) |
| 15 | `pi-src/packages/coding-agent/src/core/extensions/` | blocked | 3640 | 0 | napi-rs | String offsets and slicing (3); Callback-shaped APIs (307) |
| 15 | `pi-src/packages/coding-agent/src/core/tools/` | caution | 3226 | 0 | napi-rs | String offsets and slicing (55); Async and promises (90) |
| 15 | `pi-src/packages/coding-agent/src/extensions/mcp/` | caution | 3263 | 0 | napi-rs | String offsets and slicing (15); Async and promises (186) |
| 15 | `pi-src/packages/coding-agent/src/modes/interactive/` | caution | 7053 | 0 | napi-rs | String offsets and slicing (31); Async and promises (230) |
| 15 | `pi-src/packages/coding-agent/src/modes/interactive/components/` | caution | 9983 | 0 | napi-rs | String offsets and slicing (59); Regexes built at runtime (1) |
| 15 | `pi-src/packages/coding-agent/src/modes/interactive/theme/` | caution | 1794 | 0 | napi-rs | String offsets and slicing (5); Regex features the `regex` crate lacks (2) |
| 16 | `pi-src/packages/coding-agent/src/rpc-entry.ts` | ready | 10 | 100 | napi-rs | String offsets and slicing (1) |
| 16 | `pi-src/packages/coding-agent/src/cli.ts` | ready | 5 | 98 | napi-rs | String offsets and slicing (1) |
| 16 | `pi-src/packages/coding-agent/scripts/` | ready | 301 | 87 | napi-rs | String offsets and slicing (1); Insertion-ordered maps and object keys (14) |
| 17 | `pi-src/packages/coding-agent/src/bun/` | ready | 31 | 94 | napi-rs | String offsets and slicing (3); Insertion-ordered maps and object keys (1) |

### 66. @earendil-works/pi-evals (`pi-src/packages/evals`, node, 6 test files, uses @earendil-works/pi-ai, @earendil-works/pi-coding-agent)

| Wave | Unit | Status | Lines | Readiness | Bridge | Main risks |
| ---: | --- | --- | ---: | ---: | --- | --- |
| 0 | `pi-src/packages/evals/src/plan.ts` | ready | 54 | 98 | napi-rs | Insertion-ordered maps and object keys (1); Exceptions (6) |
| 1 | `pi-src/packages/evals/src/docker.ts` | ready | 162 | 91 | napi-rs | String offsets and slicing (1); Exceptions (8) |
| 1 | `pi-src/packages/evals/src/report.ts` | ready | 449 | 63 | napi-rs | String offsets and slicing (2); Insertion-ordered maps and object keys (8) |
| 2 | `pi-src/packages/evals/src/cli.ts` | ready | 174 | 73 | napi-rs | String offsets and slicing (5); Async and promises (8) |
| 16 | `pi-src/packages/evals/src/harness.ts` | ready | 504 | 50 | napi-rs | String offsets and slicing (6); Async and promises (41) |
| 17 | `pi-src/packages/evals/evals/` | caution | 876 | 46 | napi-rs | String offsets and slicing (3); Async and promises (42) |
| 18 | `pi-src/packages/evals/docker/` | ready | 203 | 87 | napi-rs | String offsets and slicing (1); Insertion-ordered maps and object keys (2) |

## npm dependencies

| Package | Units | Rust | Fit | Note |
| --- | ---: | --- | --- | --- |
| `jiti` | 1 | - | none | Loads TS/JS config or plugins at runtime. Keep loading in JS and pass data to Rust. |
| `esbuild` | 6 | ? | unknown |  |
| `proper-lockfile` | 3 | ? | unknown |  |
| `quickjs-wasi` | 3 | ? | unknown |  |
| `typebox` | 3 | ? | unknown |  |
| `discord.js` | 2 | ? | unknown |  |
| `vitest` | 2 | ? | unknown |  |
| `vitest-evals` | 2 | ? | unknown |  |
| `@anthropic-ai/sdk` | 1 | ? | unknown |  |
| `@aws-sdk/client-bedrock-runtime` | 1 | ? | unknown |  |
| `@biomejs/wasm-nodejs` | 1 | ? | unknown |  |
| `@earendil-works/gondolin` | 1 | ? | unknown |  |
| `@google/genai` | 1 | ? | unknown |  |
| `@silvia-odwyer/photon` | 1 | ? | unknown |  |
| `@silvia-odwyer/photon-node` | 1 | ? | unknown |  |
| `@smithy/node-http-handler` | 1 | ? | unknown |  |
| `@vitest-evals/core` | 1 | ? | unknown |  |
| `canvas` | 1 | ? | unknown |  |
| `get-east-asian-width` | 1 | ? | unknown |  |
| `grok-mermaid` | 1 | ? | unknown |  |
| `highlight.js` | 1 | ? | unknown |  |
| `hosted-git-info` | 1 | ? | unknown |  |
| `http-proxy-agent` | 1 | ? | unknown |  |
| `https-proxy-agent` | 1 | ? | unknown |  |
| `lovely-mermaid` | 1 | ? | unknown |  |
| `lowlight` | 1 | ? | unknown |  |
| `marked-cjk-friendly` | 1 | ? | unknown |  |
| `openai` | 1 | ? | unknown |  |
| `partial-json` | 1 | ? | unknown |  |
| `yaml` | 4 | saphyr, serde_yaml_ng | partial | Parsing is covered. No crate rewrites YAML while keeping comments and layout. |
| `typescript` | 1 | oxc | partial | Parsing is covered. Type checking has no Rust equivalent. |
| `chalk` | 10 | owo-colors, anstream | good |  |
| `ws` | 7 | tokio-tungstenite | good |  |
| `diff` | 4 | similar | good |  |
| `cross-spawn` | 2 | std::process, tokio::process | good |  |
| `marked` | 2 | pulldown-cmark, comrak | good |  |
| `minimatch` | 2 | globset | good | Check brace and extglob edge cases. |
| `semver` | 2 | node-semver | good | Use node-semver, not semver: npm range syntax differs from Cargo's. |
| `ignore` | 1 | ignore | good |  |
| `undici` | 1 | reqwest | good |  |

## Blocked

- `pi-src/apps/p2/src/hosts/`: Uses dynamic code (eval, new Function or computed imports).
- `pi-src/packages/ai/src/api/`: Uses dynamic code (eval, new Function or computed imports).
- `pi-src/packages/ai/src/auth/`: Uses dynamic code (eval, new Function or computed imports).
- `pi-src/packages/ai/src/auth/oauth/`: Uses dynamic code (eval, new Function or computed imports).
- `pi-src/packages/ai/src/env-api-keys.ts`: Uses dynamic code (eval, new Function or computed imports).
- `pi-src/packages/coding-agent/src/core/extensions/`: `jiti` has no Rust equivalent.

## Risk guide

- **String offsets and slicing** (High): JS indexes strings in UTF-16 code units, Rust in UTF-8 bytes. Offsets that cross the boundary must be converted, or non-ASCII text gets wrong ranges silently.
- **Regex features the `regex` crate lacks** (High): Lookaround, backreferences and the `y` flag are not in the `regex` crate. Use `fancy-regex` for those patterns, or rewrite them, and test them on non-ASCII text: `\w`, `\b` and case-insensitive matching differ between engines even where a pattern compiles. JS `/i` without `u` never matches a non-ASCII letter to an ASCII one, Rust `(?i)` does (`ſ` matches `s`, the Kelvin sign `K` matches `k`): write `(?i-u)` or compare with `eq_ignore_ascii_case`.
- **Regexes built at runtime** (High): Patterns come from data or config in JS syntax. Rust regex syntax differs, so user-visible behavior can change. Pin it with tests on real patterns.
- **Dynamic code (eval, new Function, computed import)** (High): eval, new Function and computed imports have no Rust equivalent. Keep this part in JS or replace it with an explicit registry.
- **Dynamic typing** (Medium): Values whose shape is checked at runtime need an explicit Rust enum or `serde_json::Value`.
- **Insertion-ordered maps and object keys** (Medium): JS Map, Set and object keys keep insertion order. `HashMap` does not: use `indexmap` wherever order reaches output. With serde_json's `preserve_order`, `Map::remove` swaps the last key into the hole: use `shift_remove` for JS `delete`. Sort with `js::sort` when a comparator can return NaN: the order then depends on V8's algorithm.
- **Exceptions** (Low): Thrown errors become `Result`. Errors crossing the bridge must map back to the same JS error shape.
- **Async and promises** (Medium): Promises become futures. Bridge with napi-rs `async fn` or `wasm-bindgen-futures`, or keep the async shell in JS and port the sync core.
- **Generators** (Medium): Lazy generators become iterators, or return a collected Vec across the bridge.
- **Class inheritance** (Medium): Class hierarchies become traits plus composition; `instanceof` checks become enums.
- **Reflection and prototypes** (Medium): Proxy, Reflect, Symbol and prototype tricks need a redesign, not a translation.
- **Callback-shaped APIs** (Medium): Function-typed parameters and methods cross the bridge on every call. Keep boundaries coarse: pass data in, get data out.
- **Number semantics** (Low): JS numbers are f64; bitwise ops truncate to i32; parseInt is lenient. Pick Rust integer types deliberately. Printing differs: `${n}` gives `1e+21`, `1e-7` and `0` for -0 where Rust's `{}` does not, so use `js::number_to_string`. `Math.round` rounds halves up (-2.5 gives -2), Rust's `round` away from zero: use `js::math_round`. `toFixed` rounds exact halves up ((2.5).toFixed(0) is "3"), Rust's `{:.0}` to even.
- **JSON round-trips** (Low): JSON.stringify key order and number formatting must match `serde_json` output if the result is user-visible.
- **Clock and random numbers** (Low): Results that depend on the time or on random numbers differ on every run, so a plain recording cannot pin them down. Record with `record --pin-clock` (TS/JS with the napi bridge, Python): `Date.now()`, `new Date()` and `Math.random` (`time.time`, `datetime.now` and `random`) then return fixed values during record, diff and gate, and the port reads the same ones from `src/port/pinned.rs` (`now_ms()`, `random()`, a documented mulberry32). Or pass the time and the seed in as parameters.

## Whole app

Shape: **server**.

| Entry point | Kind | Found from |
| --- | --- | --- |
| `pi-src/apps/p2/plugins/envs/sandbox/p2-sandbox.mjs` | server | `createServer(` at pi-src/apps/p2/plugins/envs/sandbox/p2-sandbox.mjs:426 |
| `pi-src/apps/p2/plugins/mcp/probe.ts` | server | `createServer(` at pi-src/apps/p2/plugins/mcp/probe.ts:69 |
| `pi-src/apps/p2/plugins/terminals/main.ts` | server | `createServer(` at pi-src/apps/p2/plugins/terminals/main.ts:770 |
| `pi-src/apps/p2/src/hosts/dev-channel.ts` | server | `createServer(` at pi-src/apps/p2/src/hosts/dev-channel.ts:30 |
| `pi-src/apps/p2/src/hosts/server.ts` | server | `createServer(` at pi-src/apps/p2/src/hosts/server.ts:241 |
| `pi-src/apps/p2/src/web/composer-state.tsx` | server | `.listen(` at pi-src/apps/p2/src/web/composer-state.tsx:99 |
| `pi-src/packages/ai/catalog/generate.ts` | cli | `#!/usr/bin/env node` at pi-src/packages/ai/catalog/generate.ts:1 |
| `pi-src/packages/ai/scripts/check-model-data.ts` | cli | `#!/usr/bin/env node` at pi-src/packages/ai/scripts/check-model-data.ts:1 |
| `pi-src/packages/ai/scripts/generate-test-image.ts` | cli | `#!/usr/bin/env node` at pi-src/packages/ai/scripts/generate-test-image.ts:1 |
| `pi-src/packages/ai/scripts/hydrate-model-catalog.ts` | cli | `#!/usr/bin/env node` at pi-src/packages/ai/scripts/hydrate-model-catalog.ts:1 |
| `pi-src/packages/ai/src/auth/oauth/callback-server.ts` | server | `createServer(` at pi-src/packages/ai/src/auth/oauth/callback-server.ts:78 |
| `pi-src/packages/ai/src/auth/oauth/openai-chatgpt.ts` | server | `createServer(` at pi-src/packages/ai/src/auth/oauth/openai-chatgpt.ts:94 |
| `pi-src/packages/ai/src/cli.ts` | cli | `#!/usr/bin/env node` at pi-src/packages/ai/src/cli.ts:1 |
| `pi-src/packages/chord/src/node/http/http.ts` | server | `listen(listen, opening)` at pi-src/packages/chord/src/node/http/http.ts:178 |
| `pi-src/packages/chord/src/node/http/node.ts` | server | `createServer(` at pi-src/packages/chord/src/node/http/node.ts:44 |
| `pi-src/packages/chord/src/node/workers/lock.ts` | server | `createServer(` at pi-src/packages/chord/src/node/workers/lock.ts:88 |
| `pi-src/packages/coding-agent/scripts/generate-references.ts` | cli | `#!/usr/bin/env node` at pi-src/packages/coding-agent/scripts/generate-references.ts:1 |
| `pi-src/packages/coding-agent/scripts/generate-schemas.ts` | cli | `#!/usr/bin/env node` at pi-src/packages/coding-agent/scripts/generate-schemas.ts:1 |
| `pi-src/packages/coding-agent/src/bun/cli.ts` | cli | `#!/usr/bin/env node` at pi-src/packages/coding-agent/src/bun/cli.ts:1 |
| `pi-src/packages/coding-agent/src/cli.ts` | cli | `#!/usr/bin/env node` at pi-src/packages/coding-agent/src/cli.ts:1 |
| `pi-src/packages/coding-agent/src/rpc-entry.ts` | cli | `#!/usr/bin/env node` at pi-src/packages/coding-agent/src/rpc-entry.ts:1 |
| `pi-src/packages/evals/evals/acme-server.ts` | server | `createServer(` at pi-src/packages/evals/evals/acme-server.ts:133 |
| `pi-src/packages/mcp/src/oauth/callback.ts` | server | `createServer(` at pi-src/packages/mcp/src/oauth/callback.ts:69 |
| `pi-src/packages/mcp/src/oauth/loopback.ts` | server | `.listen(` at pi-src/packages/mcp/src/oauth/loopback.ts:59 |
| `pi-src/scripts/build-coding-agent-bundle.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/build-coding-agent-bundle.mjs:1 |
| `pi-src/scripts/check-entry-graphs.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/check-entry-graphs.mjs:1 |
| `pi-src/scripts/check-lockfile-commit.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/check-lockfile-commit.mjs:1 |
| `pi-src/scripts/check-model-catalog-compat.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/check-model-catalog-compat.mjs:1 |
| `pi-src/scripts/check-package-install.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/check-package-install.mjs:1 |
| `pi-src/scripts/check-released-changelogs.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/check-released-changelogs.mjs:1 |
| `pi-src/scripts/check-runtime-deps.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/check-runtime-deps.mjs:1 |
| `pi-src/scripts/cost.ts` | cli | `#!/usr/bin/env node` at pi-src/scripts/cost.ts:1 |
| `pi-src/scripts/diff-model-catalog.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/diff-model-catalog.mjs:1 |
| `pi-src/scripts/edit-tool-stats.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/edit-tool-stats.mjs:1 |
| `pi-src/scripts/generate-coding-agent-install-lock.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/generate-coding-agent-install-lock.mjs:1 |
| `pi-src/scripts/generate-thinking-capabilities.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/generate-thinking-capabilities.mjs:1 |
| `pi-src/scripts/local-release.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/local-release.mjs:1 |
| `pi-src/scripts/pack-packages.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/pack-packages.mjs:1 |
| `pi-src/scripts/preview-packages.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/preview-packages.mjs:1 |
| `pi-src/scripts/publish-model-catalog.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/publish-model-catalog.mjs:1 |
| `pi-src/scripts/publish-release-announcement.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/publish-release-announcement.mjs:1 |
| `pi-src/scripts/publish.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/publish.mjs:1 |
| `pi-src/scripts/read-tool-stats.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/read-tool-stats.mjs:1 |
| `pi-src/scripts/release-notes.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/release-notes.mjs:1 |
| `pi-src/scripts/release.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/release.mjs:1 |
| `pi-src/scripts/repro-5893-wsl-bash.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/repro-5893-wsl-bash.mjs:1 |
| `pi-src/scripts/session-context-stats.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/session-context-stats.mjs:1 |
| `pi-src/scripts/session-transcripts.ts` | cli | `#!/usr/bin/env node` at pi-src/scripts/session-transcripts.ts:1 |
| `pi-src/scripts/smoke-test-codemode-binary.mjs` | server | `createServer(` at pi-src/scripts/smoke-test-codemode-binary.mjs:59 |
| `pi-src/scripts/stats.ts` | cli | `#!/usr/bin/env node` at pi-src/scripts/stats.ts:1 |
| `pi-src/scripts/sync-versions.js` | cli | `#!/usr/bin/env node` at pi-src/scripts/sync-versions.js:1 |
| `pi-src/scripts/tool-stats.ts` | cli | `#!/usr/bin/env node` at pi-src/scripts/tool-stats.ts:1 |
| `pi-src/scripts/update-model-catalog-pin.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/update-model-catalog-pin.mjs:1 |
| `pi-src/scripts/use-local-packages.mjs` | cli | `#!/usr/bin/env node` at pi-src/scripts/use-local-packages.mjs:1 |

### Every dependency

| Dependency | Declared | Used by | Rust | Fit | Note |
| --- | --- | --- | --- | --- | --- |
| `jiti` | yes | 1 | - | none | Loads TS/JS config or plugins at runtime. Keep loading in JS and pass data to Rust. |
| `esbuild` | yes | 6 | ? | unknown |  |
| `proper-lockfile` | yes | 4 | ? | unknown |  |
| `@earendil-works/elanor-icons-react` | yes | 3 | ? | unknown |  |
| `@earendil-works/elanor-react` | yes | 3 | ? | unknown |  |
| `quickjs-wasi` | yes | 3 | ? | unknown |  |
| `typebox` | yes | 3 | ? | unknown |  |
| `discord.js` | yes | 2 | ? | unknown |  |
| `lowlight` | yes | 2 | ? | unknown |  |
| `vitest` | no | 2 | ? | unknown |  |
| `vitest-evals` | no | 2 | ? | unknown |  |
| `@anthropic-ai/sdk` | yes | 1 | ? | unknown |  |
| `@aws-sdk/client-bedrock-runtime` | yes | 1 | ? | unknown |  |
| `@biomejs/wasm-nodejs` | no | 1 | ? | unknown |  |
| `@earendil-works/gondolin` | no | 1 | ? | unknown |  |
| `@google/genai` | yes | 1 | ? | unknown |  |
| `@silvia-odwyer/photon` | yes | 1 | ? | unknown |  |
| `@silvia-odwyer/photon-node` | yes | 1 | ? | unknown |  |
| `@smithy/node-http-handler` | yes | 1 | ? | unknown |  |
| `@vitest-evals/core` | no | 1 | ? | unknown |  |
| `canvas` | no | 1 | ? | unknown |  |
| `get-east-asian-width` | yes | 1 | ? | unknown |  |
| `grok-mermaid` | yes | 1 | ? | unknown |  |
| `highlight.js` | yes | 1 | ? | unknown |  |
| `hosted-git-info` | yes | 1 | ? | unknown |  |
| `http-proxy-agent` | yes | 1 | ? | unknown |  |
| `https-proxy-agent` | yes | 1 | ? | unknown |  |
| `lovely-mermaid` | yes | 1 | ? | unknown |  |
| `marked-cjk-friendly` | yes | 1 | ? | unknown |  |
| `openai` | yes | 1 | ? | unknown |  |
| `partial-json` | yes | 1 | ? | unknown |  |
| `react` | yes | 1 | ? | unknown |  |
| `react-dom` | yes | 1 | ? | unknown |  |
| `@earendil-works/design-tokens` | yes | 0 | ? | unknown |  |
| `@earendil-works/elanor-fonts` | yes | 0 | ? | unknown |  |
| `brace-expansion` | yes | 0 | ? | unknown |  |
| `yaml` | yes | 4 | saphyr, serde_yaml_ng | partial | Parsing is covered. No crate rewrites YAML while keeping comments and layout. |
| `typescript` | no | 1 | oxc | partial | Parsing is covered. Type checking has no Rust equivalent. |
| `chalk` | yes | 10 | owo-colors, anstream | good |  |
| `ws` | yes | 7 | tokio-tungstenite | good |  |
| `diff` | yes | 4 | similar | good |  |
| `marked` | yes | 3 | pulldown-cmark, comrak | good |  |
| `cross-spawn` | yes | 2 | std::process, tokio::process | good |  |
| `minimatch` | yes | 2 | globset | good | Check brace and extglob edge cases. |
| `semver` | yes | 2 | node-semver | good | Use node-semver, not semver: npm range syntax differs from Cargo's. |
| `undici` | yes | 2 | reqwest | good |  |
| `ignore` | yes | 1 | ignore | good |  |

### Pieces, in port order

- wave 0: `pi-src/apps/p2/docs/p3-reviews/session-sync-prototype/` (175 lines)
- wave 0: `pi-src/apps/p2/plugins/artifacts/bridge.ts` (111 lines)
- wave 0: `pi-src/apps/p2/plugins/chat/draft-history.ts` (49 lines)
- wave 0: `pi-src/apps/p2/plugins/envs/sandbox/` (466 lines)
- wave 0: `pi-src/apps/p2/plugins/envs/secret-presets.ts` (36 lines)
- wave 0: `pi-src/apps/p2/plugins/mcp/pages.ts` (57 lines)
- wave 0: `pi-src/apps/p2/plugins/mcp/probe.ts` (58 lines)
- wave 0: `pi-src/apps/p2/plugins/mcp/windows-command.ts` (18 lines)
- wave 0: `pi-src/apps/p2/plugins/prompt/templates.ts` (112 lines)
- wave 0: `pi-src/apps/p2/plugins/scope-prompt/scopes.ts` (92 lines)
- wave 0: `pi-src/apps/p2/plugins/search/query.ts` (20 lines)
- wave 0: `pi-src/apps/p2/plugins/settings/sections.ts` (8 lines)
- wave 0: `pi-src/apps/p2/plugins/terminals/commands.ts` (133 lines)
- wave 0: `pi-src/apps/p2/plugins/terminals/ghostty.ts` (750 lines)
- wave 0: `pi-src/apps/p2/plugins/terminals/plain.ts` (116 lines)
- wave 0: `pi-src/apps/p2/plugins/terminals/protocol.ts` (130 lines)
- wave 0: `pi-src/apps/p2/plugins/terminals/terminal-input.ts` (7 lines)
- wave 0: `pi-src/apps/p2/plugins/views/layout.ts` (327 lines)
- wave 0: `pi-src/apps/p2/plugins/voice/barge-in.ts` (164 lines)
- wave 0: `pi-src/apps/p2/plugins/voice/playback.ts` (309 lines)
- wave 0: `pi-src/apps/p2/plugins/voice/speech-text.ts` (106 lines)
- wave 0: `pi-src/apps/p2/plugins/workspace/fuzzy.ts` (308 lines)
- wave 0: `pi-src/apps/p2/public/` (78 lines)
- wave 0: `pi-src/apps/p2/src/source-resolver.ts` (80 lines)
- wave 0: `pi-src/apps/p3/plugins/sessions/contract.ts` (72 lines)
- wave 0: `pi-src/apps/p3/scripts/` (241 lines)
- wave 0: `pi-src/apps/p3/src/minimum-node.ts` (10 lines)
- wave 0: `pi-src/packages/ai/scripts/` (399 lines)
- wave 0: `pi-src/packages/ai/src/session-resources.ts` (21 lines)
- wave 0: `pi-src/packages/chord/src/json.ts` (121 lines)
- wave 0: `pi-src/packages/chord/src/output.ts` (304 lines)
- wave 0: `pi-src/packages/chord/src/sqlite/` (381 lines)
- wave 0: `pi-src/packages/codemode/src/identifier.ts` (8 lines)
- wave 0: `pi-src/packages/codemode/src/source.ts` (93 lines)
- wave 0: `pi-src/packages/codemode/src/wasm.ts` (22 lines)
- wave 0: `pi-src/packages/coding-agent/src/core/export-html/vendor/` (1279 lines)
- wave 0: `pi-src/packages/coding-agent/src/experimental/` (75 lines)
- wave 0: `pi-src/packages/durable/src/search.ts` (127 lines)
- wave 0: `pi-src/packages/env/src/connection.ts` (375 lines)
- wave 0: `pi-src/packages/evals/src/plan.ts` (54 lines)
- wave 0: `pi-src/packages/mcp/src/auth-provider.ts` (11 lines)
- wave 0: `pi-src/packages/mcp/src/config.ts` (152 lines)
- wave 0: `pi-src/packages/mcp/src/input-schema.ts` (7 lines)
- wave 0: `pi-src/packages/mcp/src/naming.ts` (19 lines)
- wave 0: `pi-src/packages/mcp/src/protocol/` (294 lines)
- wave 0: `pi-src/packages/schema/src/builders.ts`, `pi-src/packages/schema/src/typebox-static.ts`, tangled: one piece (220 lines)
- wave 0: `pi-src/packages/schema/src/formats/typebox/idna/pattern/` (68 lines)
- wave 0: `pi-src/packages/schema/src/shared.ts` (20 lines)
- wave 0: `pi-src/packages/schema/src/types.ts` (19 lines)
- wave 0: `pi-src/packages/telemetry/src/index.ts`, `pi-src/packages/telemetry/src/memory.ts`, `pi-src/packages/telemetry/src/metrics.ts`, `pi-src/packages/telemetry/src/noop.ts`, tangled: one piece (963 lines)
- wave 0: `pi-src/packages/tui/native/win32/` (205 lines)
- wave 0: `pi-src/packages/tui/src/fuzzy.ts` (114 lines)
- wave 0: `pi-src/packages/tui/src/keys.ts` (1173 lines)
- wave 0: `pi-src/packages/tui/src/kill-ring.ts` (24 lines)
- wave 0: `pi-src/packages/tui/src/native-module-path.ts` (25 lines)
- wave 0: `pi-src/packages/tui/src/stdin-buffer.ts` (307 lines)
- wave 0: `pi-src/packages/tui/src/terminal-colors.ts` (72 lines)
- wave 0: `pi-src/packages/tui/src/undo-stack.ts` (15 lines)
- wave 0: `pi-src/packages/tui/src/utils.ts` (1184 lines)
- wave 0: `pi-src/packages/tui/src/wheel-scroll.ts` (55 lines)
- wave 0: `pi-src/vitest.base.ts` (94 lines)
- wave 1: `pi-src/apps/p2/plugins/artifacts/frame.ts` (10 lines)
- wave 1: `pi-src/apps/p2/plugins/terminals/detached.ts` (145 lines)
- wave 1: `pi-src/apps/p2/plugins/terminals/keys.ts` (88 lines)
- wave 1: `pi-src/apps/p2/plugins/voice/policy.ts` (5 lines)
- wave 1: `pi-src/apps/p3/plugins/sessions/contract.server.ts` (17 lines)
- wave 1: `pi-src/apps/p3/plugins/sessions/worker.ts` (8 lines)
- wave 1: `pi-src/packages/chord/src/context/` (120 lines)
- wave 1: `pi-src/packages/chord/src/delta/` (3426 lines)
- wave 1: `pi-src/packages/codemode/src/types.ts` (110 lines)
- wave 1: `pi-src/packages/durable/src/truncate.ts` (114 lines)
- wave 1: `pi-src/packages/env/src/process.ts` (87 lines)
- wave 1: `pi-src/packages/env/src/ssh.ts` (420 lines)
- wave 1: `pi-src/packages/env/src/terminal.ts` (95 lines)
- wave 1: `pi-src/packages/evals/src/docker.ts` (162 lines)
- wave 1: `pi-src/packages/evals/src/report.ts` (449 lines)
- wave 1: `pi-src/packages/mcp/src/oauth/` (1696 lines)
- wave 1: `pi-src/packages/mcp/src/output-limit.ts` (47 lines)
- wave 1: `pi-src/packages/mcp/src/transports/` (905 lines)
- wave 1: `pi-src/packages/schema/src/checks.ts` (470 lines)
- wave 1: `pi-src/packages/schema/src/formats/typebox/idna/format/` (203 lines)
- wave 1: `pi-src/packages/schema/src/resolver.ts` (160 lines)
- wave 1: `pi-src/packages/telemetry/src/render.ts` (57 lines)
- wave 1: `pi-src/packages/telemetry/src/testing/` (381 lines)
- wave 1: `pi-src/packages/tui/src/autocomplete.ts` (682 lines)
- wave 1: `pi-src/packages/tui/src/keybindings.ts` (301 lines)
- wave 1: `pi-src/packages/tui/src/latex.ts` (1423 lines)
- wave 1: `pi-src/packages/tui/src/native-platform.ts` (49 lines)
- wave 1: `pi-src/packages/tui/src/oklab.ts` (166 lines)
- wave 1: `pi-src/packages/tui/src/program-status.ts` (33 lines)
- wave 1: `pi-src/packages/tui/src/word-navigation.ts` (76 lines)
- wave 2: `pi-src/apps/p3/plugins/sessions/server.ts` (112 lines)
- wave 2: `pi-src/packages/chord/src/watch.ts` (307 lines)
- wave 2: `pi-src/packages/codemode/src/declarations.ts` (283 lines)
- wave 2: `pi-src/packages/codemode/src/runtime/` (1517 lines)
- wave 2: `pi-src/packages/durable/src/env/` (1927 lines)
- wave 2: `pi-src/packages/evals/src/cli.ts` (174 lines)
- wave 2: `pi-src/packages/mcp/src/client.ts` (547 lines)
- wave 2: `pi-src/packages/mcp/src/testing/` (1 line)
- wave 2: `pi-src/packages/schema/src/annotations.ts` (169 lines)
- wave 2: `pi-src/packages/schema/src/formats/typebox/idna/label/` (131 lines)
- wave 2: `pi-src/packages/tui/src/colors.ts` (305 lines)
- wave 2: `pi-src/packages/tui/src/native-modifiers.ts` (11 lines)
- wave 3: `pi-src/packages/codemode/src/index.ts` (66 lines)
- wave 3: `pi-src/packages/codemode/src/portable.ts` (47 lines)
- wave 3: `pi-src/packages/env/src/errors.ts` (31 lines)
- wave 3: `pi-src/packages/mcp/src/index.ts` (96 lines)
- wave 3: `pi-src/packages/schema/src/compiler.ts` (551 lines)
- wave 3: `pi-src/packages/schema/src/formats/typebox/idna/` (98 lines)
- wave 3: `pi-src/packages/tui/src/terminal-image.ts` (643 lines)
- wave 3: `pi-src/packages/tui/src/terminal.ts` (442 lines)
- wave 4: `pi-src/packages/codemode/src/cloudflare.ts` (27 lines)
- wave 4: `pi-src/packages/env/src/list-files.ts` (83 lines)
- wave 4: `pi-src/packages/env/src/watch.ts` (137 lines)
- wave 4: `pi-src/packages/schema/src/coerce.ts` (472 lines)
- wave 4: `pi-src/packages/schema/src/explain.ts` (358 lines)
- wave 4: `pi-src/packages/schema/src/formats/typebox/` (530 lines)
- wave 4: `pi-src/packages/tui/src/tui.ts` (1221 lines)
- wave 5: `pi-src/packages/env/src/remote-env.ts` (868 lines)
- wave 5: `pi-src/packages/schema/src/formats/` (45 lines)
- wave 5: `pi-src/packages/schema/src/validator.ts` (62 lines)
- wave 5: `pi-src/packages/tui/src/editor-component.ts` (16 lines)
- wave 5: `pi-src/packages/tui/src/layout-node.ts` (42 lines)
- wave 5: `pi-src/packages/tui/src/tui-main-screen.ts` (533 lines)
- wave 6: `pi-src/packages/env/src/index.ts` (33 lines)
- wave 6: `pi-src/packages/schema/src/index.ts` (33 lines)
- wave 6: `pi-src/packages/tui/src/components/` (4467 lines)
- wave 7: `pi-src/apps/p2/plugins/terminals/daemon.ts` (74 lines)
- wave 7: `pi-src/packages/ai/src/api/`, `pi-src/packages/ai/src/auth/`, `pi-src/packages/ai/src/auth/oauth/`, `pi-src/packages/ai/src/env-api-keys.ts`, `pi-src/packages/ai/src/model-catalog.ts`, `pi-src/packages/ai/src/models-store.ts`, `pi-src/packages/ai/src/models.generated.ts`, `pi-src/packages/ai/src/models.ts`, `pi-src/packages/ai/src/providers/`, `pi-src/packages/ai/src/telemetry.ts`, `pi-src/packages/ai/src/types.ts`, `pi-src/packages/ai/src/utils/`, tangled: one piece (22731 lines)
- wave 7: `pi-src/packages/chord/src/contracts/` (1779 lines)
- wave 7: `pi-src/packages/tui/src/alt-screen-search.ts` (289 lines)
- wave 7: `pi-src/packages/tui/src/layout.ts` (417 lines)
- wave 8: `pi-src/packages/ai/catalog/`, `pi-src/packages/ai/catalog/families/`, `pi-src/packages/ai/catalog/providers/`, `pi-src/packages/ai/catalog/sources/`, tangled: one piece (3195 lines)
- wave 8: `pi-src/packages/ai/src/bedrock-provider.ts` (5 lines)
- wave 8: `pi-src/packages/ai/src/bun-oauth.ts` (23 lines)
- wave 8: `pi-src/packages/ai/src/cache-warming.ts` (77 lines)
- wave 8: `pi-src/packages/ai/src/compat/` (36 lines)
- wave 8: `pi-src/packages/ai/src/image-models.ts` (34 lines)
- wave 8: `pi-src/packages/ai/src/images-api-registry.ts` (45 lines)
- wave 8: `pi-src/packages/ai/src/lazy-bedrock-provider.ts` (4 lines)
- wave 8: `pi-src/packages/ai/src/lazy-oauth.ts` (14 lines)
- wave 8: `pi-src/packages/ai/src/legacy-api-aliases.ts` (83 lines)
- wave 8: `pi-src/packages/chord/src/data/`, `pi-src/packages/chord/src/telemetry.ts`, tangled: one piece (920 lines)
- wave 8: `pi-src/packages/tui/src/tui-alt-screen.ts` (1632 lines)
- wave 9: `pi-src/packages/ai/src/index.ts` (43 lines)
- wave 9: `pi-src/packages/ai/src/oauth.ts` (9 lines)
- wave 9: `pi-src/packages/ai/src/providers/images/` (50 lines)
- wave 9: `pi-src/packages/chord/src/link/` (1608 lines)
- wave 9: `pi-src/packages/chord/src/state/` (706 lines)
- wave 9: `pi-src/packages/tui/src/index.ts` (180 lines)
- wave 10: `pi-src/packages/agent/src/proxy.ts` (320 lines)
- wave 10: `pi-src/packages/agent/src/types.ts` (165 lines)
- wave 10: `pi-src/packages/ai/src/cli.ts` (115 lines)
- wave 10: `pi-src/packages/ai/src/images.ts` (18 lines)
- wave 10: `pi-src/packages/chord/src/node/` (15 lines)
- wave 10: `pi-src/packages/chord/src/store/` (6896 lines)
- wave 10: `pi-src/packages/coding-agent/src/config.ts`, `pi-src/packages/coding-agent/src/utils/`, tangled: one piece (3517 lines)
- wave 11: `pi-src/packages/agent/src/stream-fn.ts` (11 lines)
- wave 11: `pi-src/packages/ai/src/compat.ts` (247 lines)
- wave 11: `pi-src/packages/chord/src/documents/`, `pi-src/packages/chord/src/host/`, `pi-src/packages/chord/src/jobs/`, `pi-src/packages/chord/src/registries/`, `pi-src/packages/chord/src/remote/`, tangled: one piece (16170 lines)
- wave 11: `pi-src/packages/chord/src/sync/` (969 lines)
- wave 12: `pi-src/packages/agent/src/agent-loop.ts` (775 lines)
- wave 12: `pi-src/packages/chord/src/node/http/` (1987 lines)
- wave 12: `pi-src/packages/chord/src/surfaces/` (1266 lines)
- wave 12: `pi-src/packages/chord/src/telemetry-ingest/` (573 lines)
- wave 13: `pi-src/packages/agent/src/agent.ts` (497 lines)
- wave 13: `pi-src/packages/chord/src/index.ts` (40 lines)
- wave 13: `pi-src/packages/chord/src/node/plugins/` (2267 lines)
- wave 13: `pi-src/packages/chord/src/telemetry-sqlite/` (387 lines)
- wave 13: `pi-src/packages/chord/src/tui/` (533 lines)
- wave 13: `pi-src/packages/chord/src/web/` (404 lines)
- wave 14: `pi-src/apps/p3/src/principals.ts` (8 lines)
- wave 14: `pi-src/packages/agent/src/index.ts` (5 lines)
- wave 14: `pi-src/packages/chord/src/node/surfaces/` (596 lines)
- wave 14: `pi-src/packages/durable/src/documents.ts`, `pi-src/packages/durable/src/entries.ts`, `pi-src/packages/durable/src/errors.ts`, `pi-src/packages/durable/src/harness/`, `pi-src/packages/durable/src/ids.ts`, `pi-src/packages/durable/src/session/`, `pi-src/packages/durable/src/signals.ts`, `pi-src/packages/durable/src/tasks.ts`, `pi-src/packages/durable/src/timers.ts`, `pi-src/packages/durable/src/types.ts`, tangled: one piece (14291 lines)
- wave 15: `pi-src/apps/p3/src/app.ts` (25 lines)
- wave 15: `pi-src/apps/p3/src/session-route.ts` (11 lines)
- wave 15: `pi-src/packages/chord/src/node/workers/` (1449 lines)
- wave 15: `pi-src/packages/coding-agent/src/cli/`, `pi-src/packages/coding-agent/src/core/`, `pi-src/packages/coding-agent/src/core/compaction/`, `pi-src/packages/coding-agent/src/core/export-html/`, `pi-src/packages/coding-agent/src/core/extensions/`, `pi-src/packages/coding-agent/src/core/tools/`, `pi-src/packages/coding-agent/src/core/tools/renderers/`, `pi-src/packages/coding-agent/src/extensions/`, `pi-src/packages/coding-agent/src/extensions/codemode/`, `pi-src/packages/coding-agent/src/extensions/llama/`, `pi-src/packages/coding-agent/src/extensions/mcp/`, `pi-src/packages/coding-agent/src/extensions/tool-search/`, `pi-src/packages/coding-agent/src/index.ts`, `pi-src/packages/coding-agent/src/main.ts`, `pi-src/packages/coding-agent/src/migrations.ts`, `pi-src/packages/coding-agent/src/modes/`, `pi-src/packages/coding-agent/src/modes/interactive/`, `pi-src/packages/coding-agent/src/modes/interactive/components/`, `pi-src/packages/coding-agent/src/modes/interactive/theme/`, `pi-src/packages/coding-agent/src/modes/rpc/`, `pi-src/packages/coding-agent/src/package-manager-cli.ts`, tangled: one piece (61328 lines)
- wave 15: `pi-src/packages/durable/src/storage/` (2032 lines)
- wave 15: `pi-src/packages/durable/src/testing/` (3230 lines)
- wave 15: `pi-src/packages/durable/src/tool-search/` (88 lines)
- wave 15: `pi-src/packages/durable/src/tools/` (1275 lines)
- wave 16: `pi-src/apps/p3/src/kit/` (1036 lines)
- wave 16: `pi-src/packages/chord/src/node/app/` (3161 lines)
- wave 16: `pi-src/packages/coding-agent/scripts/` (301 lines)
- wave 16: `pi-src/packages/coding-agent/src/cli.ts` (5 lines)
- wave 16: `pi-src/packages/coding-agent/src/rpc-entry.ts` (10 lines)
- wave 16: `pi-src/packages/durable/src/codemode/` (1333 lines)
- wave 16: `pi-src/packages/durable/src/images/` (408 lines)
- wave 16: `pi-src/packages/durable/src/index.ts` (250 lines)
- wave 16: `pi-src/packages/durable/src/storage/jsonl/` (872 lines)
- wave 16: `pi-src/packages/durable/src/storage/sqlite/` (1418 lines)
- wave 16: `pi-src/packages/durable/src/storage/store/` (1051 lines)
- wave 16: `pi-src/packages/evals/src/harness.ts` (504 lines)
- wave 17: `pi-src/apps/p2/plugins/chat/contract.ts` (215 lines)
- wave 17: `pi-src/apps/p2/plugins/familiar/cold.ts` (23 lines)
- wave 17: `pi-src/apps/p2/plugins/models/config-value.ts` (153 lines)
- wave 17: `pi-src/apps/p2/plugins/models/contract.ts` (70 lines)
- wave 17: `pi-src/apps/p2/plugins/models/credentials.ts` (163 lines)
- wave 17: `pi-src/apps/p2/plugins/models/fake.ts` (240 lines)
- wave 17: `pi-src/apps/p2/plugins/models/model-config.ts` (369 lines)
- wave 17: `pi-src/apps/p2/plugins/models/model-store.ts` (88 lines)
- wave 17: `pi-src/apps/p2/plugins/models/models.ts` (6 lines)
- wave 17: `pi-src/apps/p2/plugins/models/provider-config.ts` (348 lines)
- wave 17: `pi-src/apps/p2/plugins/models/resources.ts` (272 lines)
- wave 17: `pi-src/apps/p2/plugins/views/contract.ts` (225 lines)
- wave 17: `pi-src/apps/p2/src/api/v1/`, `pi-src/apps/p2/src/hosts/`, tangled: one piece (6399 lines)
- wave 17: `pi-src/apps/p2/src/tui/`, `pi-src/apps/p2/src/tui/components/`, `pi-src/apps/p2/src/tui/tools/`, tangled: one piece (4521 lines)
- wave 17: `pi-src/apps/p2/src/tui/theme/` (1360 lines)
- wave 17: `pi-src/apps/p3/src/api/v1/` (21 lines)
- wave 17: `pi-src/apps/p3/src/ask.ts` (113 lines)
- wave 17: `pi-src/apps/p3/src/install.ts` (30 lines)
- wave 17: `pi-src/apps/p3/src/pi.ts` (29 lines)
- wave 17: `pi-src/packages/chord/src/node/tui/` (1042 lines)
- wave 17: `pi-src/packages/coding-agent/src/bun/` (31 lines)
- wave 17: `pi-src/packages/durable/test-workers/` (125 lines)
- wave 17: `pi-src/packages/evals/evals/` (876 lines)
- wave 17: `pi-src/scripts/` (7637 lines)
- wave 18: `pi-src/apps/p2/plugins/agent-plugins/skills/writing-p2-plugins/example/counter/contract.ts` (14 lines)
- wave 18: `pi-src/apps/p2/plugins/agent-plugins/worker.ts` (25 lines)
- wave 18: `pi-src/apps/p2/plugins/artifacts/contract.ts` (107 lines)
- wave 18: `pi-src/apps/p2/plugins/backup/contract.ts` (65 lines)
- wave 18: `pi-src/apps/p2/plugins/cache-warming/warmer.ts` (99 lines)
- wave 18: `pi-src/apps/p2/plugins/chat-mode/contract.ts` (80 lines)
- wave 18: `pi-src/apps/p2/plugins/chat/outbox.ts` (116 lines)
- wave 18: `pi-src/apps/p2/plugins/chat/server.ts` (59 lines)
- wave 18: `pi-src/apps/p2/plugins/chat/shell.ts` (252 lines)
- wave 18: `pi-src/apps/p2/plugins/codemode/calls.ts` (75 lines)
- wave 18: `pi-src/apps/p2/plugins/codemode/card.ts` (128 lines)
- wave 18: `pi-src/apps/p2/plugins/codemode/contract.ts` (3 lines)
- wave 18: `pi-src/apps/p2/plugins/codemode/nested.ts` (48 lines)
- wave 18: `pi-src/apps/p2/plugins/coding-tools/cards.ts` (226 lines)
- wave 18: `pi-src/apps/p2/plugins/coding-tools/worker.ts` (31 lines)
- wave 18: `pi-src/apps/p2/plugins/dev-tools/contract.ts` (28 lines)
- wave 18: `pi-src/apps/p2/plugins/devices/owner-access.ts` (164 lines)
- wave 18: `pi-src/apps/p2/plugins/discord/contract.ts` (159 lines)
- wave 18: `pi-src/apps/p2/plugins/drafts/contract.ts` (38 lines)
- wave 18: `pi-src/apps/p2/plugins/envs/sandbox-schema.ts` (18 lines)
- wave 18: `pi-src/apps/p2/plugins/envs/schema.ts` (38 lines)
- wave 18: `pi-src/apps/p2/plugins/envs/secrets.ts` (116 lines)
- wave 18: `pi-src/apps/p2/plugins/familiar/contract.ts` (24 lines)
- wave 18: `pi-src/apps/p2/plugins/file-index/listing.ts` (23 lines)
- wave 18: `pi-src/apps/p2/plugins/file-index/rank.ts` (47 lines)
- wave 18: `pi-src/apps/p2/plugins/files/contract.ts` (50 lines)
- wave 18: `pi-src/apps/p2/plugins/files/uploads.ts` (56 lines)
- wave 18: `pi-src/apps/p2/plugins/fit-images/worker.ts` (9 lines)
- wave 18: `pi-src/apps/p2/plugins/locations/contract.ts` (39 lines)
- wave 18: `pi-src/apps/p2/plugins/locations/contract.worker.ts` (12 lines)
- wave 18: `pi-src/apps/p2/plugins/login/contract.ts` (84 lines)
- wave 18: `pi-src/apps/p2/plugins/logs/contract.ts` (19 lines)
- wave 18: `pi-src/apps/p2/plugins/mcp/config.ts` (139 lines)
- wave 18: `pi-src/apps/p2/plugins/mcp/contract.ts` (169 lines)
- wave 18: `pi-src/apps/p2/plugins/mcp/credentials.ts` (63 lines)
- wave 18: `pi-src/apps/p2/plugins/mcp/leases.ts` (49 lines)
- wave 18: `pi-src/apps/p2/plugins/models/config.ts` (40 lines)
- wave 18: `pi-src/apps/p2/plugins/models/scope.ts` (54 lines)
- wave 18: `pi-src/apps/p2/plugins/models/tui.ts` (386 lines)
- wave 18: `pi-src/apps/p2/plugins/ports/contract.ts`, `pi-src/apps/p2/plugins/ports/protocol.ts`, `pi-src/apps/p2/plugins/ports/store.ts`, tangled: one piece (263 lines)
- wave 18: `pi-src/apps/p2/plugins/powershell/contract.ts` (9 lines)
- wave 18: `pi-src/apps/p2/plugins/projects/contract.ts` (89 lines)
- wave 18: `pi-src/apps/p2/plugins/prompt/context-files.ts` (172 lines)
- wave 18: `pi-src/apps/p2/plugins/prompt/contract.ts` (27 lines)
- wave 18: `pi-src/apps/p2/plugins/proxy-marker/contract.ts` (3 lines)
- wave 18: `pi-src/apps/p2/plugins/quiet-tools/tui.ts` (124 lines)
- wave 18: `pi-src/apps/p2/plugins/radius/relay-access.ts` (37 lines)
- wave 18: `pi-src/apps/p2/plugins/rollups/contract.ts` (103 lines)
- wave 18: `pi-src/apps/p2/plugins/scope-prompt/contract.ts` (15 lines)
- wave 18: `pi-src/apps/p2/plugins/scope-prompt/message.ts` (32 lines)
- wave 18: `pi-src/apps/p2/plugins/search/contract.ts` (142 lines)
- wave 18: `pi-src/apps/p2/plugins/send-file/contract.ts` (26 lines)
- wave 18: `pi-src/apps/p2/plugins/send-file/tool.ts` (62 lines)
- wave 18: `pi-src/apps/p2/plugins/sessions/contract.ts` (355 lines)
- wave 18: `pi-src/apps/p2/plugins/sessions/copy.ts` (35 lines)
- wave 18: `pi-src/apps/p2/plugins/settings/contract.ts` (20 lines)
- wave 18: `pi-src/apps/p2/plugins/sharing/contract.ts` (88 lines)
- wave 18: `pi-src/apps/p2/plugins/status/contract.ts` (99 lines)
- wave 18: `pi-src/apps/p2/plugins/subagents/contract.ts` (103 lines)
- wave 18: `pi-src/apps/p2/plugins/subagents/contract.web.ts` (5 lines)
- wave 18: `pi-src/apps/p2/plugins/sync/radius-remote.ts` (156 lines)
- wave 18: `pi-src/apps/p2/plugins/telemetry/contract.ts` (82 lines)
- wave 18: `pi-src/apps/p2/plugins/terminals/log-preload.ts` (3 lines)
- wave 18: `pi-src/apps/p2/plugins/terminals/service.ts` (366 lines)
- wave 18: `pi-src/apps/p2/plugins/terminals/tui.ts` (40 lines)
- wave 18: `pi-src/apps/p2/plugins/tree/branch-target.ts` (20 lines)
- wave 18: `pi-src/apps/p2/plugins/tree/contract.ts` (55 lines)
- wave 18: `pi-src/apps/p2/plugins/views/contract.web.ts` (13 lines)
- wave 18: `pi-src/apps/p2/plugins/views/display.ts` (19 lines)
- wave 18: `pi-src/apps/p2/plugins/views/doc.ts` (188 lines)
- wave 18: `pi-src/apps/p2/plugins/voice/protocol.ts` (116 lines)
- wave 18: `pi-src/apps/p2/plugins/workspace/contract.ts`, `pi-src/apps/p2/plugins/workspace/protocol.ts`, tangled: one piece (204 lines)
- wave 18: `pi-src/apps/p2/plugins/workspace/tabs.ts` (23 lines)
- wave 18: `pi-src/apps/p2/src/source.ts` (2 lines)
- wave 18: `pi-src/apps/p3/src/commands.ts` (214 lines)
- wave 18: `pi-src/packages/evals/docker/` (203 lines)
- wave 19: `pi-src/apps/p2/plugins/agent-plugins/skills/writing-p2-plugins/example/counter/server.ts` (104 lines)
- wave 19: `pi-src/apps/p2/plugins/agent-plugins/skills/writing-p2-plugins/example/counter/worker.ts` (37 lines)
- wave 19: `pi-src/apps/p2/plugins/artifacts/pages.ts` (102 lines)
- wave 19: `pi-src/apps/p2/plugins/artifacts/store.ts` (225 lines)
- wave 19: `pi-src/apps/p2/plugins/artifacts/tool.ts` (307 lines)
- wave 19: `pi-src/apps/p2/plugins/artifacts/tui.ts` (194 lines)
- wave 19: `pi-src/apps/p2/plugins/artifacts/use-artifacts.ts` (11 lines)
- wave 19: `pi-src/apps/p2/plugins/artifacts/viewer.ts` (23 lines)
- wave 19: `pi-src/apps/p2/plugins/backup/chunked-remote.ts` (70 lines)
- wave 19: `pi-src/apps/p2/plugins/backup/server.ts` (330 lines)
- wave 19: `pi-src/apps/p2/plugins/cache-warming/worker.ts` (89 lines)
- wave 19: `pi-src/apps/p2/plugins/chat-mode/digest.ts` (128 lines)
- wave 19: `pi-src/apps/p2/plugins/chat-mode/display.ts` (13 lines)
- wave 19: `pi-src/apps/p2/plugins/chat/model-choice.ts` (25 lines)
- wave 19: `pi-src/apps/p2/plugins/chat/session-start.ts` (78 lines)
- wave 19: `pi-src/apps/p2/plugins/chat/tui-commands.ts`, `pi-src/apps/p2/plugins/chat/tui.ts`, tangled: one piece (1195 lines)
- wave 19: `pi-src/apps/p2/plugins/chat/worker.ts` (532 lines)
- wave 19: `pi-src/apps/p2/plugins/codemode/tui.ts` (10 lines)
- wave 19: `pi-src/apps/p2/plugins/codemode/worker.ts` (48 lines)
- wave 19: `pi-src/apps/p2/plugins/coding-tools/tui.ts` (17 lines)
- wave 19: `pi-src/apps/p2/plugins/conversations/contract.ts` (108 lines)
- wave 19: `pi-src/apps/p2/plugins/dev-tools/server.ts` (13 lines)
- wave 19: `pi-src/apps/p2/plugins/dev-tools/tool.ts` (60 lines)
- wave 19: `pi-src/apps/p2/plugins/devices/contract.ts` (19 lines)
- wave 19: `pi-src/apps/p2/plugins/discord/relay.ts` (585 lines)
- wave 19: `pi-src/apps/p2/plugins/drafts/worker.ts` (56 lines)
- wave 19: `pi-src/apps/p2/plugins/envs/config.ts` (130 lines)
- wave 19: `pi-src/apps/p2/plugins/envs/connection-revision.ts` (7 lines)
- wave 19: `pi-src/apps/p2/plugins/familiar/server.ts` (58 lines)
- wave 19: `pi-src/apps/p2/plugins/familiar/worker.ts` (160 lines)
- wave 19: `pi-src/apps/p2/plugins/file-index/worker.ts` (105 lines)
- wave 19: `pi-src/apps/p2/plugins/files/server.ts` (204 lines)
- wave 19: `pi-src/apps/p2/plugins/files/worker.ts` (106 lines)
- wave 19: `pi-src/apps/p2/plugins/history/tools.ts` (366 lines)
- wave 19: `pi-src/apps/p2/plugins/login/logins.ts` (257 lines)
- wave 19: `pi-src/apps/p2/plugins/login/tui.ts` (454 lines)
- wave 19: `pi-src/apps/p2/plugins/logs/server.ts` (161 lines)
- wave 19: `pi-src/apps/p2/plugins/logs/tui.ts` (192 lines)
- wave 19: `pi-src/apps/p2/plugins/mcp/project.ts` (69 lines)
- wave 19: `pi-src/apps/p2/plugins/mcp/routes.ts` (98 lines)
- wave 19: `pi-src/apps/p2/plugins/mcp/sign-in.ts` (271 lines)
- wave 19: `pi-src/apps/p2/plugins/mcp/tools.ts` (318 lines)
- wave 19: `pi-src/apps/p2/plugins/mcp/tui.ts` (194 lines)
- wave 19: `pi-src/apps/p2/plugins/models/server.ts` (63 lines)
- wave 19: `pi-src/apps/p2/plugins/models/worker.ts` (26 lines)
- wave 19: `pi-src/apps/p2/plugins/ports/server.ts` (115 lines)
- wave 19: `pi-src/apps/p2/plugins/ports/tool.ts` (113 lines)
- wave 19: `pi-src/apps/p2/plugins/ports/tui.ts` (174 lines)
- wave 19: `pi-src/apps/p2/plugins/powershell/tui.ts` (9 lines)
- wave 19: `pi-src/apps/p2/plugins/projects/config.ts` (44 lines)
- wave 19: `pi-src/apps/p2/plugins/projects/place.ts` (20 lines)
- wave 19: `pi-src/apps/p2/plugins/projects/store.ts` (33 lines)
- wave 19: `pi-src/apps/p2/plugins/prompt/composer.ts` (14 lines)
- wave 19: `pi-src/apps/p2/plugins/prompt/environment.ts` (107 lines)
- wave 19: `pi-src/apps/p2/plugins/prompt/tui.ts` (89 lines)
- wave 19: `pi-src/apps/p2/plugins/proxy-marker/server.ts` (5 lines)
- wave 19: `pi-src/apps/p2/plugins/radius/brain.ts` (934 lines)
- wave 19: `pi-src/apps/p2/plugins/rollups/runner.ts` (138 lines)
- wave 19: `pi-src/apps/p2/plugins/rollups/worker.ts` (139 lines)
- wave 19: `pi-src/apps/p2/plugins/scope-prompt/worker.ts` (39 lines)
- wave 19: `pi-src/apps/p2/plugins/search/db.ts` (163 lines)
- wave 19: `pi-src/apps/p2/plugins/search/tui.ts` (61 lines)
- wave 19: `pi-src/apps/p2/plugins/send-file/tui.ts` (159 lines)
- wave 19: `pi-src/apps/p2/plugins/send-file/worker.ts` (55 lines)
- wave 19: `pi-src/apps/p2/plugins/sessions/agent-actions.ts` (21 lines)
- wave 19: `pi-src/apps/p2/plugins/sessions/catalog.ts` (111 lines)
- wave 19: `pi-src/apps/p2/plugins/sessions/lock-worker.ts` (43 lines)
- wave 19: `pi-src/apps/p2/plugins/sessions/locks.ts` (46 lines)
- wave 19: `pi-src/apps/p2/plugins/sessions/sidebar-settings.ts` (53 lines)
- wave 19: `pi-src/apps/p2/plugins/settings/server.ts` (31 lines)
- wave 19: `pi-src/apps/p2/plugins/sharing/links.ts` (23 lines)
- wave 19: `pi-src/apps/p2/plugins/sharing/store.ts` (299 lines)
- wave 19: `pi-src/apps/p2/plugins/sharing/tui.ts` (244 lines)
- wave 19: `pi-src/apps/p2/plugins/status/server.ts` (162 lines)
- wave 19: `pi-src/apps/p2/plugins/status/tui.ts` (305 lines)
- wave 19: `pi-src/apps/p2/plugins/status/worker.ts` (115 lines)
- wave 19: `pi-src/apps/p2/plugins/subagents/children.ts` (25 lines)
- wave 19: `pi-src/apps/p2/plugins/subagents/contract.worker.ts` (161 lines)
- wave 19: `pi-src/apps/p2/plugins/subagents/tui.ts` (200 lines)
- wave 19: `pi-src/apps/p2/plugins/telemetry/server.ts` (127 lines)
- wave 19: `pi-src/apps/p2/plugins/telemetry/tool.ts` (117 lines)
- wave 19: `pi-src/apps/p2/plugins/terminals/contract.ts` (120 lines)
- wave 19: `pi-src/apps/p2/plugins/tree/toc.ts` (109 lines)
- wave 19: `pi-src/apps/p2/plugins/tree/worker.ts` (327 lines)
- wave 19: `pi-src/apps/p2/plugins/views/kinds.ts` (14 lines)
- wave 19: `pi-src/apps/p2/plugins/views/tool.ts` (238 lines)
- wave 19: `pi-src/apps/p2/plugins/views/tui.ts` (9 lines)
- wave 19: `pi-src/apps/p2/plugins/voice/contract.ts` (49 lines)
- wave 19: `pi-src/apps/p2/plugins/voice/transcripts.ts` (178 lines)
- wave 19: `pi-src/apps/p2/plugins/workspace/environment.ts` (803 lines)
- wave 19: `pi-src/apps/p2/plugins/workspace/server.ts` (174 lines)
- wave 19: `pi-src/apps/p2/plugins/workspace/tui.ts` (353 lines)
- wave 19: `pi-src/apps/p3/src/bootstrap/` (193 lines)
- wave 20: `pi-src/apps/p2/plugins/artifacts/server.ts` (242 lines)
- wave 20: `pi-src/apps/p2/plugins/artifacts/worker.ts` (57 lines)
- wave 20: `pi-src/apps/p2/plugins/backup/worker.ts` (64 lines)
- wave 20: `pi-src/apps/p2/plugins/chat-mode/tui.ts` (162 lines)
- wave 20: `pi-src/apps/p2/plugins/chat-mode/worker.ts` (198 lines)
- wave 20: `pi-src/apps/p2/plugins/conversations/server.ts` (207 lines)
- wave 20: `pi-src/apps/p2/plugins/conversations/worker.ts` (265 lines)
- wave 20: `pi-src/apps/p2/plugins/dev-tools/worker.ts` (33 lines)
- wave 20: `pi-src/apps/p2/plugins/devices/server.ts` (205 lines)
- wave 20: `pi-src/apps/p2/plugins/devices/tui.ts` (95 lines)
- wave 20: `pi-src/apps/p2/plugins/discord/bot.ts` (211 lines)
- wave 20: `pi-src/apps/p2/plugins/discord/worker.ts` (381 lines)
- wave 20: `pi-src/apps/p2/plugins/envs/sandbox-host.ts` (137 lines)
- wave 20: `pi-src/apps/p2/plugins/envs/store.ts` (35 lines)
- wave 20: `pi-src/apps/p2/plugins/history/tui.ts` (87 lines)
- wave 20: `pi-src/apps/p2/plugins/history/worker.ts` (18 lines)
- wave 20: `pi-src/apps/p2/plugins/locations/tool.ts` (208 lines)
- wave 20: `pi-src/apps/p2/plugins/login/server.ts` (89 lines)
- wave 20: `pi-src/apps/p2/plugins/mcp/environments.ts` (30 lines)
- wave 20: `pi-src/apps/p2/plugins/mcp/placement.ts` (223 lines)
- wave 20: `pi-src/apps/p2/plugins/mcp/worker.ts` (235 lines)
- wave 20: `pi-src/apps/p2/plugins/ports/worker.ts` (18 lines)
- wave 20: `pi-src/apps/p2/plugins/powershell/worker.ts` (122 lines)
- wave 20: `pi-src/apps/p2/plugins/projects/server.ts` (266 lines)
- wave 20: `pi-src/apps/p2/plugins/prompt/worker.ts` (367 lines)
- wave 20: `pi-src/apps/p2/plugins/radius/contract.ts` (9 lines)
- wave 20: `pi-src/apps/p2/plugins/radius/login.ts` (23 lines)
- wave 20: `pi-src/apps/p2/plugins/rollups/server.ts` (236 lines)
- wave 20: `pi-src/apps/p2/plugins/search/server.ts` (205 lines)
- wave 20: `pi-src/apps/p2/plugins/search/worker.ts` (151 lines)
- wave 20: `pi-src/apps/p2/plugins/sessions/worker.ts` (118 lines)
- wave 20: `pi-src/apps/p2/plugins/sharing/server.ts` (385 lines)
- wave 20: `pi-src/apps/p2/plugins/subagents/server.ts` (91 lines)
- wave 20: `pi-src/apps/p2/plugins/subagents/tools.ts` (633 lines)
- wave 20: `pi-src/apps/p2/plugins/telemetry/worker.ts` (17 lines)
- wave 20: `pi-src/apps/p2/plugins/terminals/server.ts` (192 lines)
- wave 20: `pi-src/apps/p2/plugins/terminals/tool.ts` (289 lines)
- wave 20: `pi-src/apps/p2/plugins/views/worker.ts` (199 lines)
- wave 20: `pi-src/apps/p2/plugins/voice/interruption.ts` (135 lines)
- wave 20: `pi-src/apps/p2/plugins/voice/server.ts` (403 lines)
- wave 20: `pi-src/apps/p2/plugins/voice/worker.ts` (153 lines)
- wave 20: `pi-src/apps/p2/plugins/workspace/worker.ts` (259 lines)
- wave 20: `pi-src/apps/p2/scripts/` (1757 lines)
- wave 20: `pi-src/apps/p3/bench/` (337 lines)
- wave 21: `pi-src/apps/p2/plugins/agent/tool.ts` (155 lines)
- wave 21: `pi-src/apps/p2/plugins/discord/fleet.ts` (157 lines)
- wave 21: `pi-src/apps/p2/plugins/envs/remote-env.ts` (102 lines)
- wave 21: `pi-src/apps/p2/plugins/locations/worker.ts` (91 lines)
- wave 21: `pi-src/apps/p2/plugins/mcp/transports.ts` (89 lines)
- wave 21: `pi-src/apps/p2/plugins/radius/server.ts` (96 lines)
- wave 21: `pi-src/apps/p2/plugins/subagents/worker.ts` (307 lines)
- wave 21: `pi-src/apps/p2/plugins/terminals/worker.ts` (50 lines)
- wave 21: `pi-src/apps/p2/plugins/voice/engine.ts` (903 lines)
- wave 22: `pi-src/apps/p2/plugins/agent/worker.ts` (20 lines)
- wave 22: `pi-src/apps/p2/plugins/envs/contract.ts`, `pi-src/apps/p2/plugins/envs/manager.ts`, tangled: one piece (610 lines)
- wave 22: `pi-src/apps/p2/plugins/envs/worker.ts` (41 lines)
- wave 22: `pi-src/apps/p2/plugins/mcp/instances.ts` (328 lines)
- wave 22: `pi-src/apps/p2/plugins/terminals/envs.ts` (3 lines)
- wave 22: `pi-src/apps/p2/plugins/voice/choice.ts` (36 lines)
- wave 23: `pi-src/apps/p2/plugins/discord/server.ts` (284 lines)
- wave 23: `pi-src/apps/p2/plugins/envs/server.ts` (262 lines)
- wave 23: `pi-src/apps/p2/plugins/envs/tui.ts` (202 lines)
- wave 23: `pi-src/apps/p2/plugins/locations/server.ts` (63 lines)
- wave 23: `pi-src/apps/p2/plugins/mcp/server.ts` (393 lines)
- wave 23: `pi-src/apps/p2/plugins/sessions/server.ts` (747 lines)
- wave 23: `pi-src/apps/p2/plugins/sessions/tui-location.ts` (215 lines)
- wave 23: `pi-src/apps/p2/plugins/terminals/main.ts` (690 lines)
- wave 24: `pi-src/apps/p2/plugins/sessions/tui-projects.ts` (75 lines)
- wave 25: `pi-src/apps/p2/plugins/sessions/tui-move.ts` (139 lines)
- wave 26: `pi-src/apps/p2/plugins/sessions/tui.ts` (1049 lines)

### Decisions for the user

**D1** (shape): The app is a server, entered through pi-src/apps/p2/plugins/envs/sandbox/p2-sandbox.mjs, pi-src/apps/p2/plugins/mcp/probe.ts, pi-src/apps/p2/plugins/terminals/main.ts, pi-src/apps/p2/src/hosts/dev-channel.ts, pi-src/apps/p2/src/hosts/server.ts, pi-src/apps/p2/src/web/composer-state.tsx, pi-src/packages/ai/catalog/generate.ts, pi-src/packages/ai/scripts/check-model-data.ts, pi-src/packages/ai/scripts/generate-test-image.ts, pi-src/packages/ai/scripts/hydrate-model-catalog.ts, pi-src/packages/ai/src/auth/oauth/callback-server.ts, pi-src/packages/ai/src/auth/oauth/openai-chatgpt.ts, pi-src/packages/ai/src/cli.ts, pi-src/packages/chord/src/node/http/http.ts, pi-src/packages/chord/src/node/http/node.ts, pi-src/packages/chord/src/node/workers/lock.ts, pi-src/packages/coding-agent/scripts/generate-references.ts, pi-src/packages/coding-agent/scripts/generate-schemas.ts, pi-src/packages/coding-agent/src/bun/cli.ts, pi-src/packages/coding-agent/src/cli.ts, pi-src/packages/coding-agent/src/rpc-entry.ts, pi-src/packages/evals/evals/acme-server.ts, pi-src/packages/mcp/src/oauth/callback.ts, pi-src/packages/mcp/src/oauth/loopback.ts, pi-src/scripts/build-coding-agent-bundle.mjs, pi-src/scripts/check-entry-graphs.mjs, pi-src/scripts/check-lockfile-commit.mjs, pi-src/scripts/check-model-catalog-compat.mjs, pi-src/scripts/check-package-install.mjs, pi-src/scripts/check-released-changelogs.mjs, pi-src/scripts/check-runtime-deps.mjs, pi-src/scripts/cost.ts, pi-src/scripts/diff-model-catalog.mjs, pi-src/scripts/edit-tool-stats.mjs, pi-src/scripts/generate-coding-agent-install-lock.mjs, pi-src/scripts/generate-thinking-capabilities.mjs, pi-src/scripts/local-release.mjs, pi-src/scripts/pack-packages.mjs, pi-src/scripts/preview-packages.mjs, pi-src/scripts/publish-model-catalog.mjs, pi-src/scripts/publish-release-announcement.mjs, pi-src/scripts/publish.mjs, pi-src/scripts/read-tool-stats.mjs, pi-src/scripts/release-notes.mjs, pi-src/scripts/release.mjs, pi-src/scripts/repro-5893-wsl-bash.mjs, pi-src/scripts/session-context-stats.mjs, pi-src/scripts/session-transcripts.ts, pi-src/scripts/smoke-test-codemode-binary.mjs, pi-src/scripts/stats.ts, pi-src/scripts/sync-versions.js, pi-src/scripts/tool-stats.ts, pi-src/scripts/update-model-catalog-pin.mjs, pi-src/scripts/use-local-packages.mjs. Port every entry point, in that shape?

> Yes: an axum front door (`app front-door`) takes the server over first

**D2** (keep pi-src/apps/p2/plugins/agent-plugins/skills/writing-p2-plugins/example/counter/web.tsx): pi-src/apps/p2/plugins/agent-plugins/skills/writing-p2-plugins/example/counter/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D3** (keep pi-src/apps/p2/plugins/agent/activity.tsx): pi-src/apps/p2/plugins/agent/activity.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D4** (keep pi-src/apps/p2/plugins/agent/web.tsx): pi-src/apps/p2/plugins/agent/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D5** (keep pi-src/apps/p2/plugins/artifacts/activity.tsx): pi-src/apps/p2/plugins/artifacts/activity.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D6** (keep pi-src/apps/p2/plugins/artifacts/commands.tsx): pi-src/apps/p2/plugins/artifacts/commands.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D7** (keep pi-src/apps/p2/plugins/artifacts/inline-size.ts): pi-src/apps/p2/plugins/artifacts/inline-size.ts stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D8** (keep pi-src/apps/p2/plugins/artifacts/local-item.ts): pi-src/apps/p2/plugins/artifacts/local-item.ts stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D9** (keep pi-src/apps/p2/plugins/artifacts/panes.tsx): pi-src/apps/p2/plugins/artifacts/panes.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D10** (keep pi-src/apps/p2/plugins/artifacts/web.tsx): pi-src/apps/p2/plugins/artifacts/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D11** (keep pi-src/apps/p2/plugins/chat-mode/web.tsx): pi-src/apps/p2/plugins/chat-mode/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D12** (keep pi-src/apps/p2/plugins/chat/commands.tsx): pi-src/apps/p2/plugins/chat/commands.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D13** (keep pi-src/apps/p2/plugins/chat/draft-editor.tsx): pi-src/apps/p2/plugins/chat/draft-editor.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D14** (keep pi-src/apps/p2/plugins/chat/model-screens.ts): pi-src/apps/p2/plugins/chat/model-screens.ts stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D15** (keep pi-src/apps/p2/plugins/chat/web.tsx): pi-src/apps/p2/plugins/chat/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D16** (keep pi-src/apps/p2/plugins/codemode/activity.tsx): pi-src/apps/p2/plugins/codemode/activity.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D17** (keep pi-src/apps/p2/plugins/codemode/web.tsx): pi-src/apps/p2/plugins/codemode/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D18** (keep pi-src/apps/p2/plugins/coding-tools/activity.tsx): pi-src/apps/p2/plugins/coding-tools/activity.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D19** (keep pi-src/apps/p2/plugins/coding-tools/web.tsx): pi-src/apps/p2/plugins/coding-tools/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D20** (keep pi-src/apps/p2/plugins/conversations/activity.tsx): pi-src/apps/p2/plugins/conversations/activity.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D21** (keep pi-src/apps/p2/plugins/conversations/web.tsx): pi-src/apps/p2/plugins/conversations/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D22** (keep pi-src/apps/p2/plugins/devices/web.tsx): pi-src/apps/p2/plugins/devices/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D23** (keep pi-src/apps/p2/plugins/discord/BotsSheet.tsx): pi-src/apps/p2/plugins/discord/BotsSheet.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D24** (keep pi-src/apps/p2/plugins/discord/LinkSheet.tsx): pi-src/apps/p2/plugins/discord/LinkSheet.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D25** (keep pi-src/apps/p2/plugins/discord/held.ts): pi-src/apps/p2/plugins/discord/held.ts stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D26** (keep pi-src/apps/p2/plugins/discord/web.tsx): pi-src/apps/p2/plugins/discord/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D27** (keep pi-src/apps/p2/plugins/envs/DirectoryPicker.tsx): pi-src/apps/p2/plugins/envs/DirectoryPicker.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D28** (keep pi-src/apps/p2/plugins/envs/EnvsPanel.tsx): pi-src/apps/p2/plugins/envs/EnvsPanel.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D29** (keep pi-src/apps/p2/plugins/envs/SecretsSection.tsx): pi-src/apps/p2/plugins/envs/SecretsSection.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D30** (keep pi-src/apps/p2/plugins/envs/contract.web.tsx): pi-src/apps/p2/plugins/envs/contract.web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D31** (keep pi-src/apps/p2/plugins/envs/web.tsx): pi-src/apps/p2/plugins/envs/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D32** (keep pi-src/apps/p2/plugins/familiar/web.tsx): pi-src/apps/p2/plugins/familiar/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D33** (keep pi-src/apps/p2/plugins/history/activity.tsx): pi-src/apps/p2/plugins/history/activity.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D34** (keep pi-src/apps/p2/plugins/history/web.tsx): pi-src/apps/p2/plugins/history/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D35** (keep pi-src/apps/p2/plugins/login/web.tsx): pi-src/apps/p2/plugins/login/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D36** (keep pi-src/apps/p2/plugins/logs/web.tsx): pi-src/apps/p2/plugins/logs/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D37** (keep pi-src/apps/p2/plugins/mcp/web.tsx): pi-src/apps/p2/plugins/mcp/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D38** (keep pi-src/apps/p2/plugins/ports/PortsPanel.tsx): pi-src/apps/p2/plugins/ports/PortsPanel.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D39** (keep pi-src/apps/p2/plugins/ports/activity.tsx): pi-src/apps/p2/plugins/ports/activity.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D40** (keep pi-src/apps/p2/plugins/ports/web.tsx): pi-src/apps/p2/plugins/ports/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D41** (keep pi-src/apps/p2/plugins/powershell/activity.tsx): pi-src/apps/p2/plugins/powershell/activity.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D42** (keep pi-src/apps/p2/plugins/powershell/web.tsx): pi-src/apps/p2/plugins/powershell/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D43** (keep pi-src/apps/p2/plugins/prompt/web.tsx): pi-src/apps/p2/plugins/prompt/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D44** (keep pi-src/apps/p2/plugins/scope-prompt/Overlay.tsx): pi-src/apps/p2/plugins/scope-prompt/Overlay.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D45** (keep pi-src/apps/p2/plugins/scope-prompt/Toggle.tsx): pi-src/apps/p2/plugins/scope-prompt/Toggle.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D46** (keep pi-src/apps/p2/plugins/scope-prompt/commands.tsx): pi-src/apps/p2/plugins/scope-prompt/commands.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D47** (keep pi-src/apps/p2/plugins/scope-prompt/mode.ts): pi-src/apps/p2/plugins/scope-prompt/mode.ts stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D48** (keep pi-src/apps/p2/plugins/scope-prompt/web.tsx): pi-src/apps/p2/plugins/scope-prompt/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D49** (keep pi-src/apps/p2/plugins/send-file/activity.tsx): pi-src/apps/p2/plugins/send-file/activity.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D50** (keep pi-src/apps/p2/plugins/send-file/web.tsx): pi-src/apps/p2/plugins/send-file/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D51** (keep pi-src/apps/p2/plugins/sessions/BranchSheet.tsx): pi-src/apps/p2/plugins/sessions/BranchSheet.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D52** (keep pi-src/apps/p2/plugins/sessions/SessionSheet.tsx): pi-src/apps/p2/plugins/sessions/SessionSheet.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D53** (keep pi-src/apps/p2/plugins/sessions/command-session.tsx): pi-src/apps/p2/plugins/sessions/command-session.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D54** (keep pi-src/apps/p2/plugins/sessions/commands.tsx): pi-src/apps/p2/plugins/sessions/commands.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D55** (keep pi-src/apps/p2/plugins/sessions/contract.web.tsx): pi-src/apps/p2/plugins/sessions/contract.web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D56** (keep pi-src/apps/p2/plugins/sessions/project-commands.tsx): pi-src/apps/p2/plugins/sessions/project-commands.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D57** (keep pi-src/apps/p2/plugins/sessions/sidebar/): pi-src/apps/p2/plugins/sessions/sidebar/ stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D58** (keep pi-src/apps/p2/plugins/sessions/web.tsx): pi-src/apps/p2/plugins/sessions/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D59** (keep pi-src/apps/p2/plugins/settings/commands.tsx): pi-src/apps/p2/plugins/settings/commands.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D60** (keep pi-src/apps/p2/plugins/settings/web.tsx): pi-src/apps/p2/plugins/settings/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D61** (keep pi-src/apps/p2/plugins/sharing/commands.tsx): pi-src/apps/p2/plugins/sharing/commands.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D62** (keep pi-src/apps/p2/plugins/sharing/web.tsx): pi-src/apps/p2/plugins/sharing/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D63** (keep pi-src/apps/p2/plugins/subagents/activity.tsx): pi-src/apps/p2/plugins/subagents/activity.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D64** (keep pi-src/apps/p2/plugins/subagents/commands.tsx): pi-src/apps/p2/plugins/subagents/commands.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D65** (keep pi-src/apps/p2/plugins/subagents/web.tsx): pi-src/apps/p2/plugins/subagents/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D66** (keep pi-src/apps/p2/plugins/telemetry/traces.tsx): pi-src/apps/p2/plugins/telemetry/traces.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D67** (keep pi-src/apps/p2/plugins/telemetry/web.tsx): pi-src/apps/p2/plugins/telemetry/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D68** (keep pi-src/apps/p2/plugins/terminals/Terminal.tsx): pi-src/apps/p2/plugins/terminals/Terminal.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D69** (keep pi-src/apps/p2/plugins/terminals/activity.tsx): pi-src/apps/p2/plugins/terminals/activity.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D70** (keep pi-src/apps/p2/plugins/terminals/commands.tsx): pi-src/apps/p2/plugins/terminals/commands.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D71** (keep pi-src/apps/p2/plugins/terminals/new-terminal.ts): pi-src/apps/p2/plugins/terminals/new-terminal.ts stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D72** (keep pi-src/apps/p2/plugins/terminals/web.tsx): pi-src/apps/p2/plugins/terminals/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D73** (keep pi-src/apps/p2/plugins/tree/TreePanel.tsx): pi-src/apps/p2/plugins/tree/TreePanel.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D74** (keep pi-src/apps/p2/plugins/tree/entry-in-view.ts): pi-src/apps/p2/plugins/tree/entry-in-view.ts stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D75** (keep pi-src/apps/p2/plugins/tree/web.tsx): pi-src/apps/p2/plugins/tree/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D76** (keep pi-src/apps/p2/plugins/views/Workbench.tsx): pi-src/apps/p2/plugins/views/Workbench.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D77** (keep pi-src/apps/p2/plugins/views/activity.tsx): pi-src/apps/p2/plugins/views/activity.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D78** (keep pi-src/apps/p2/plugins/views/annotate.tsx): pi-src/apps/p2/plugins/views/annotate.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D79** (keep pi-src/apps/p2/plugins/views/commands.tsx): pi-src/apps/p2/plugins/views/commands.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D80** (keep pi-src/apps/p2/plugins/views/layout-store.ts): pi-src/apps/p2/plugins/views/layout-store.ts stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D81** (keep pi-src/apps/p2/plugins/views/pane-kit.tsx): pi-src/apps/p2/plugins/views/pane-kit.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D82** (keep pi-src/apps/p2/plugins/views/panes.tsx): pi-src/apps/p2/plugins/views/panes.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D83** (keep pi-src/apps/p2/plugins/views/sheet.tsx): pi-src/apps/p2/plugins/views/sheet.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D84** (keep pi-src/apps/p2/plugins/views/web.tsx): pi-src/apps/p2/plugins/views/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D85** (keep pi-src/apps/p2/plugins/voice/web.tsx): pi-src/apps/p2/plugins/voice/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D86** (keep pi-src/apps/p2/plugins/web-shell/commands.tsx): pi-src/apps/p2/plugins/web-shell/commands.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D87** (keep pi-src/apps/p2/plugins/web-shell/web.tsx): pi-src/apps/p2/plugins/web-shell/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D88** (keep pi-src/apps/p2/plugins/workspace/Workspace.tsx): pi-src/apps/p2/plugins/workspace/Workspace.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D89** (keep pi-src/apps/p2/plugins/workspace/commands.tsx): pi-src/apps/p2/plugins/workspace/commands.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D90** (keep pi-src/apps/p2/plugins/workspace/file.tsx): pi-src/apps/p2/plugins/workspace/file.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D91** (keep pi-src/apps/p2/plugins/workspace/web.tsx): pi-src/apps/p2/plugins/workspace/web.tsx stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D92** (keep pi-src/apps/p2/src/hosts/web/): pi-src/apps/p2/src/hosts/web/ stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D93** (keep pi-src/apps/p2/src/runtime/): pi-src/apps/p2/src/runtime/ stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D94** (keep pi-src/apps/p2/src/web/): pi-src/apps/p2/src/web/ stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D95** (keep pi-src/apps/p2/src/web/activity/): pi-src/apps/p2/src/web/activity/ stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D96** (keep pi-src/apps/p2/src/web/command-menu/): pi-src/apps/p2/src/web/command-menu/ stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D97** (keep pi-src/apps/p2/src/web/elanor-extra/): pi-src/apps/p2/src/web/elanor-extra/ stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D98** (keep pi-src/packages/chord/src/react/): pi-src/packages/chord/src/react/ stays in the source language (UI code: keep in the original language). What replaces it in the Rust app, or does it stay outside?

> Keep it outside the Rust app, behind the front door, and decide its replacement once the rest is Rust

**D99** (blocked pi-src/apps/p2/src/hosts/): pi-src/apps/p2/src/hosts/ is blocked: Uses dynamic code (eval, new Function or computed imports). Rework that part in Rust, or keep the unit outside?

> Research the blocker first; then port it with the piece that uses it

**D100** (blocked pi-src/packages/ai/src/api/): pi-src/packages/ai/src/api/ is blocked: Uses dynamic code (eval, new Function or computed imports). Rework that part in Rust, or keep the unit outside?

> Research the blocker first; then port it with the piece that uses it

**D101** (blocked pi-src/packages/ai/src/auth/): pi-src/packages/ai/src/auth/ is blocked: Uses dynamic code (eval, new Function or computed imports). Rework that part in Rust, or keep the unit outside?

> Research the blocker first; then port it with the piece that uses it

**D102** (blocked pi-src/packages/ai/src/auth/oauth/): pi-src/packages/ai/src/auth/oauth/ is blocked: Uses dynamic code (eval, new Function or computed imports). Rework that part in Rust, or keep the unit outside?

> Research the blocker first; then port it with the piece that uses it

**D103** (blocked pi-src/packages/ai/src/env-api-keys.ts): pi-src/packages/ai/src/env-api-keys.ts is blocked: Uses dynamic code (eval, new Function or computed imports). Rework that part in Rust, or keep the unit outside?

> Research the blocker first; then port it with the piece that uses it

**D104** (blocked pi-src/packages/coding-agent/src/core/extensions/): pi-src/packages/coding-agent/src/core/extensions/ is blocked: `jiti` has no Rust equivalent. Rework that part in Rust, or keep the unit outside?

> Research the blocker first; then port it with the piece that uses it

**D105** (dependency jiti): No crate does what jiti does (used by 1 unit; Loads TS/JS config or plugins at runtime. Keep loading in JS and pass data to Rust). Reimplement what the app uses, or drop the feature?

> Reimplement only the part the app uses, proved by its recorded cases

**D106** (dependency esbuild): Which crate replaces esbuild (used by 6 units)?

> Research it against primary sources into rust/research/esbuild.md before porting what uses it

**D107** (dependency proper-lockfile): Which crate replaces proper-lockfile (used by 4 units)?

> Research it against primary sources into rust/research/proper-lockfile.md before porting what uses it

**D108** (dependency @earendil-works/elanor-icons-react): Which crate replaces @earendil-works/elanor-icons-react (used by 3 units)?

> Research it against primary sources into rust/research/@earendil-works-elanor-icons-react.md before porting what uses it

**D109** (dependency @earendil-works/elanor-react): Which crate replaces @earendil-works/elanor-react (used by 3 units)?

> Research it against primary sources into rust/research/@earendil-works-elanor-react.md before porting what uses it

**D110** (dependency quickjs-wasi): Which crate replaces quickjs-wasi (used by 3 units)?

> Research it against primary sources into rust/research/quickjs-wasi.md before porting what uses it

**D111** (dependency typebox): Which crate replaces typebox (used by 3 units)?

> Research it against primary sources into rust/research/typebox.md before porting what uses it

**D112** (dependency discord.js): Which crate replaces discord.js (used by 2 units)?

> Research it against primary sources into rust/research/discord.js.md before porting what uses it

**D113** (dependency lowlight): Which crate replaces lowlight (used by 2 units)?

> Research it against primary sources into rust/research/lowlight.md before porting what uses it

**D114** (dependency vitest): Which crate replaces vitest (used by 2 units)?

> Research it against primary sources into rust/research/vitest.md before porting what uses it

**D115** (dependency vitest-evals): Which crate replaces vitest-evals (used by 2 units)?

> Research it against primary sources into rust/research/vitest-evals.md before porting what uses it

**D116** (dependency @anthropic-ai/sdk): Which crate replaces @anthropic-ai/sdk (used by 1 unit)?

> Research it against primary sources into rust/research/@anthropic-ai-sdk.md before porting what uses it

**D117** (dependency @aws-sdk/client-bedrock-runtime): Which crate replaces @aws-sdk/client-bedrock-runtime (used by 1 unit)?

> Research it against primary sources into rust/research/@aws-sdk-client-bedrock-runtime.md before porting what uses it

**D118** (dependency @biomejs/wasm-nodejs): Which crate replaces @biomejs/wasm-nodejs (used by 1 unit)?

> Research it against primary sources into rust/research/@biomejs-wasm-nodejs.md before porting what uses it

**D119** (dependency @earendil-works/gondolin): Which crate replaces @earendil-works/gondolin (used by 1 unit)?

> Research it against primary sources into rust/research/@earendil-works-gondolin.md before porting what uses it

**D120** (dependency @google/genai): Which crate replaces @google/genai (used by 1 unit)?

> Research it against primary sources into rust/research/@google-genai.md before porting what uses it

**D121** (dependency @silvia-odwyer/photon): Which crate replaces @silvia-odwyer/photon (used by 1 unit)?

> Research it against primary sources into rust/research/@silvia-odwyer-photon.md before porting what uses it

**D122** (dependency @silvia-odwyer/photon-node): Which crate replaces @silvia-odwyer/photon-node (used by 1 unit)?

> Research it against primary sources into rust/research/@silvia-odwyer-photon-node.md before porting what uses it

**D123** (dependency @smithy/node-http-handler): Which crate replaces @smithy/node-http-handler (used by 1 unit)?

> Research it against primary sources into rust/research/@smithy-node-http-handler.md before porting what uses it

**D124** (dependency @vitest-evals/core): Which crate replaces @vitest-evals/core (used by 1 unit)?

> Research it against primary sources into rust/research/@vitest-evals-core.md before porting what uses it

**D125** (dependency canvas): Which crate replaces canvas (used by 1 unit)?

> Research it against primary sources into rust/research/canvas.md before porting what uses it

**D126** (dependency get-east-asian-width): Which crate replaces get-east-asian-width (used by 1 unit)?

> Research it against primary sources into rust/research/get-east-asian-width.md before porting what uses it

**D127** (dependency grok-mermaid): Which crate replaces grok-mermaid (used by 1 unit)?

> Research it against primary sources into rust/research/grok-mermaid.md before porting what uses it

**D128** (dependency highlight.js): Which crate replaces highlight.js (used by 1 unit)?

> Research it against primary sources into rust/research/highlight.js.md before porting what uses it

**D129** (dependency hosted-git-info): Which crate replaces hosted-git-info (used by 1 unit)?

> Research it against primary sources into rust/research/hosted-git-info.md before porting what uses it

**D130** (dependency http-proxy-agent): Which crate replaces http-proxy-agent (used by 1 unit)?

> Research it against primary sources into rust/research/http-proxy-agent.md before porting what uses it

**D131** (dependency https-proxy-agent): Which crate replaces https-proxy-agent (used by 1 unit)?

> Research it against primary sources into rust/research/https-proxy-agent.md before porting what uses it

**D132** (dependency lovely-mermaid): Which crate replaces lovely-mermaid (used by 1 unit)?

> Research it against primary sources into rust/research/lovely-mermaid.md before porting what uses it

**D133** (dependency marked-cjk-friendly): Which crate replaces marked-cjk-friendly (used by 1 unit)?

> Research it against primary sources into rust/research/marked-cjk-friendly.md before porting what uses it

**D134** (dependency openai): Which crate replaces openai (used by 1 unit)?

> Research it against primary sources into rust/research/openai.md before porting what uses it

**D135** (dependency partial-json): Which crate replaces partial-json (used by 1 unit)?

> Research it against primary sources into rust/research/partial-json.md before porting what uses it

**D136** (dependency react): Which crate replaces react (used by 1 unit)?

> Research it against primary sources into rust/research/react.md before porting what uses it

**D137** (dependency react-dom): Which crate replaces react-dom (used by 1 unit)?

> Research it against primary sources into rust/research/react-dom.md before porting what uses it

**D138** (dependency @earendil-works/design-tokens): Which crate replaces @earendil-works/design-tokens (declared, no unit imports it)?

> Research it against primary sources into rust/research/@earendil-works-design-tokens.md before porting what uses it

**D139** (dependency @earendil-works/elanor-fonts): Which crate replaces @earendil-works/elanor-fonts (declared, no unit imports it)?

> Research it against primary sources into rust/research/@earendil-works-elanor-fonts.md before porting what uses it

**D140** (dependency brace-expansion): Which crate replaces brace-expansion (declared, no unit imports it)?

> Research it against primary sources into rust/research/brace-expansion.md before porting what uses it

**D141** (dependency yaml): yaml maps to saphyr, serde_yaml_ng only in part (used by 4 units; Parsing is covered. No crate rewrites YAML while keeping comments and layout). Use it?

> Use saphyr, serde_yaml_ng, and record cases for the behavior it does not cover

**D142** (dependency typescript): typescript maps to oxc only in part (used by 1 unit; Parsing is covered. Type checking has no Rust equivalent). Use it?

> Use oxc, and record cases for the behavior it does not cover

**D143** (tangled units): pi-src/packages/schema/src/builders.ts, pi-src/packages/schema/src/typebox-static.ts import each other (220 lines). Port them as one piece?

> Yes: one piece in wave 0, no glue between its parts

**D144** (tangled units): pi-src/packages/telemetry/src/index.ts, pi-src/packages/telemetry/src/memory.ts, pi-src/packages/telemetry/src/metrics.ts, pi-src/packages/telemetry/src/noop.ts import each other (963 lines). Port them as one piece?

> Yes: one piece in wave 0, no glue between its parts

**D145** (tangled units): pi-src/packages/ai/src/api/, pi-src/packages/ai/src/auth/, pi-src/packages/ai/src/auth/oauth/, pi-src/packages/ai/src/env-api-keys.ts, pi-src/packages/ai/src/model-catalog.ts, pi-src/packages/ai/src/models-store.ts, pi-src/packages/ai/src/models.generated.ts, pi-src/packages/ai/src/models.ts, pi-src/packages/ai/src/providers/, pi-src/packages/ai/src/telemetry.ts, pi-src/packages/ai/src/types.ts, pi-src/packages/ai/src/utils/ import each other (22731 lines). Port them as one piece?

> Yes: one piece in wave 7, no glue between its parts

**D146** (tangled units): pi-src/packages/ai/catalog/, pi-src/packages/ai/catalog/families/, pi-src/packages/ai/catalog/providers/, pi-src/packages/ai/catalog/sources/ import each other (3195 lines). Port them as one piece?

> Yes: one piece in wave 8, no glue between its parts

**D147** (tangled units): pi-src/packages/chord/src/data/, pi-src/packages/chord/src/telemetry.ts import each other (920 lines). Port them as one piece?

> Yes: one piece in wave 8, no glue between its parts

**D148** (tangled units): pi-src/packages/coding-agent/src/config.ts, pi-src/packages/coding-agent/src/utils/ import each other (3517 lines). Port them as one piece?

> Yes: one piece in wave 10, no glue between its parts

**D149** (tangled units): pi-src/packages/chord/src/documents/, pi-src/packages/chord/src/host/, pi-src/packages/chord/src/jobs/, pi-src/packages/chord/src/registries/, pi-src/packages/chord/src/remote/ import each other (16170 lines). Port them as one piece?

> Yes: one piece in wave 11, no glue between its parts

**D150** (tangled units): pi-src/packages/durable/src/documents.ts, pi-src/packages/durable/src/entries.ts, pi-src/packages/durable/src/errors.ts, pi-src/packages/durable/src/harness/, pi-src/packages/durable/src/ids.ts, pi-src/packages/durable/src/session/, pi-src/packages/durable/src/signals.ts, pi-src/packages/durable/src/tasks.ts, pi-src/packages/durable/src/timers.ts, pi-src/packages/durable/src/types.ts import each other (14291 lines). Port them as one piece?

> Yes: one piece in wave 14, no glue between its parts

**D151** (tangled units): pi-src/packages/coding-agent/src/cli/, pi-src/packages/coding-agent/src/core/, pi-src/packages/coding-agent/src/core/compaction/, pi-src/packages/coding-agent/src/core/export-html/, pi-src/packages/coding-agent/src/core/extensions/, pi-src/packages/coding-agent/src/core/tools/, pi-src/packages/coding-agent/src/core/tools/renderers/, pi-src/packages/coding-agent/src/extensions/, pi-src/packages/coding-agent/src/extensions/codemode/, pi-src/packages/coding-agent/src/extensions/llama/, pi-src/packages/coding-agent/src/extensions/mcp/, pi-src/packages/coding-agent/src/extensions/tool-search/, pi-src/packages/coding-agent/src/index.ts, pi-src/packages/coding-agent/src/main.ts, pi-src/packages/coding-agent/src/migrations.ts, pi-src/packages/coding-agent/src/modes/, pi-src/packages/coding-agent/src/modes/interactive/, pi-src/packages/coding-agent/src/modes/interactive/components/, pi-src/packages/coding-agent/src/modes/interactive/theme/, pi-src/packages/coding-agent/src/modes/rpc/, pi-src/packages/coding-agent/src/package-manager-cli.ts import each other (61328 lines). Port them as one piece?

> Yes: one piece in wave 15, no glue between its parts

**D152** (tangled units): pi-src/apps/p2/src/api/v1/, pi-src/apps/p2/src/hosts/ import each other (6399 lines). Port them as one piece?

> Yes: one piece in wave 17, no glue between its parts

**D153** (tangled units): pi-src/apps/p2/src/tui/, pi-src/apps/p2/src/tui/components/, pi-src/apps/p2/src/tui/tools/ import each other (4521 lines). Port them as one piece?

> Yes: one piece in wave 17, no glue between its parts

**D154** (tangled units): pi-src/apps/p2/plugins/ports/contract.ts, pi-src/apps/p2/plugins/ports/protocol.ts, pi-src/apps/p2/plugins/ports/store.ts import each other (263 lines). Port them as one piece?

> Yes: one piece in wave 18, no glue between its parts

**D155** (tangled units): pi-src/apps/p2/plugins/workspace/contract.ts, pi-src/apps/p2/plugins/workspace/protocol.ts import each other (204 lines). Port them as one piece?

> Yes: one piece in wave 18, no glue between its parts

**D156** (tangled units): pi-src/apps/p2/plugins/chat/tui-commands.ts, pi-src/apps/p2/plugins/chat/tui.ts import each other (1195 lines). Port them as one piece?

> Yes: one piece in wave 19, no glue between its parts

**D157** (tangled units): pi-src/apps/p2/plugins/envs/contract.ts, pi-src/apps/p2/plugins/envs/manager.ts import each other (610 lines). Port them as one piece?

> Yes: one piece in wave 22, no glue between its parts

