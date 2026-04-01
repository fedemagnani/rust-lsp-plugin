---
description: Create an implementation issue that follows the shared issue schema.
---

# create_implementation_issue.py

## Use

`create_implementation_issue.py` **MUST** be used for non-epic work items.

Implementation issues **SHOULD** stay narrower and shorter than epics, carrying only the context
needed for one actionable delivery unit.

## Required Arguments

- `--title`
- `--description`
- `--contract`
- `--acceptance`

## Optional Arguments

- `--design`
- `--parent`
- `--blocked-by`
- `--blocking`

## Effect

Creates an implementation issue, preserves the shared issue sections, and sets the project `kind`
field to `implementation`.
On success, the command prints `Action succeeded`.
