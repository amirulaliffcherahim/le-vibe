# Deslop Current Diff

Inspect `git diff` (and any staged changes). Remove unnecessary bloat while keeping all behavior, trust-boundary validation, security checks, and tests intact:
- Inline single-use 1-line helper wrappers
- Remove redundant comments that restate the next line
- Remove dead code, unused variables, and speculative abstractions
- Keep edits minimal and verify with the project's lint/test commands before reporting.

Focus area (if specified): $@
