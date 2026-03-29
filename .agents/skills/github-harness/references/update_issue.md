---
description: Rewrite selected issue sections in place without disturbing untouched content.
---

# update_issue.py

## Use

`update_issue.py` **MUST** be used when an existing issue needs refinement after asynchronous review
in GitHub or when only selected sections need correction.

## Required Arguments

- `--issue`

## Optional Arguments

- `--title`
- `--description`
- `--contract`
- `--acceptance`
- `--design`

## Effect

Updates only the requested issue sections while preserving untouched content.
On success, including idempotent no-op updates, the command prints `Action succeeded`.
