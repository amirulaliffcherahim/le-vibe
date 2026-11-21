# Code & Security Review

Review `git diff` (or `$1` if specified) critically as a senior engineer:
1. Check for real bugs, race conditions, off-by-one errors, and platform (Windows/Unix) edge cases.
2. Check security: untrusted input handling, shell/path injection, secret leaks, and permission boundaries.
3. Check whether tests actually cover the changed behavior.
4. Report only concrete findings with file paths and line numbers, ordered by severity.

Additional focus: $@
