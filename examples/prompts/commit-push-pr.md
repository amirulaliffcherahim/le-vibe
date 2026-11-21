# Commit, Push & PR

1. Run `git status` and `git diff --stat` to verify only intentional changes are present.
2. Run the project's lint and test suite to confirm everything passes.
3. Stage explicit changed files (`git add <paths>`, never `git add -A`), commit with a concise conventional commit message (`feat(...)`, `fix(...)`, `docs(...)`), and push the branch.
4. If `$1` is `pr` or requested, open a concise PR with `gh pr create`.

Notes: $@
