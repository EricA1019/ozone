# Bug Report Template

Use this format for all new bug reports. Copy and fill in.

```markdown
# BUG-XXX: Short Title

- **Severity:** 🔴 Bug | 🟡 Silent | 🟣 Structural | 🟢 Minor
- **File(s):** `path/to/file.rs:line`
- **Found:** YYYY-MM-DD
- **Status:** Open | Fixed | Won't Fix

## What's Wrong
<!-- One sentence. What actually happens? -->

## Expected Behavior
<!-- What should happen instead? -->

## Evidence
<!-- Code snippet, log output, or test showing the bug -->

## Impact
<!-- What breaks for the user? Performance? Correctness? Data loss? -->

## Reproduction
<!-- Steps to trigger the issue -->

## Suggested Fix
<!-- Optional. How would you fix it? -->
```

## Severity Guide

| Severity | Meaning |
|----------|---------|
| 🔴 Bug | Produces wrong results, crashes, or silently does the wrong thing |
| 🟡 Silent | Error is swallowed, action doesn't execute, false impression given |
| 🟣 Structural | Design flaw, dead code, brittle pattern, maintenance hazard |
| 🟢 Minor | UX gap, unclear error message, potential future issue |
