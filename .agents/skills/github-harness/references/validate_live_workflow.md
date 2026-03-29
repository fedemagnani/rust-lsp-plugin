---
description: Exercise the live github-harness workflow on this repository and emit a structured validation report.
---

# validate_live_workflow.py

## Use

`validate_live_workflow.py` **SHOULD** be used when the harness command surface needs a repeatable
live validation run against GitHub state instead of unit tests alone.

## Required Arguments

- None when run inside the target repository.

## Optional Arguments

- `--label`
- `--keep-branch`

## Effect

Runs the live bootstrap, issue authoring, read/query, relationship, blocked-new stash, and
pull-request flows against this repository.
The command creates clearly prefixed validation issues, uses a dedicated validation branch for the
pull-request check, and prints a JSON report with artifact references, step evidence, and explicit
scores.
