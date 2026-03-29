---
description: Synchronize issue relationships for blockers and hierarchy.
---

# update_dependencies.py

## Use

`update_dependencies.py` **MUST** be used when blocker or parent-child relationships need to be set
or corrected.

## Required Arguments

- `--issue`

## Optional Arguments

- `--blocked-by`
- `--blocking`
- `--parent`
- `--children`

Pass `none` to clear an explicitly selected relationship set.

## Effect

Updates only the canonical GitHub dependency and hierarchy relationships explicitly selected for
the target issue.
On success, including idempotent no-op updates, the command prints `Action succeeded`.
