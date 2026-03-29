---
description: Bootstrap or validate the repository project and ensure the required project fields.
---

# create_project.py

## Use

`create_project.py` **MUST** be used before PRD decomposition or issue execution when the
repository project might be missing or drifted.

## Required Arguments

- None when run inside the target repository.

## Optional Arguments

- None.

## Effect

Creates the repository project if it is missing, validates it if it already exists, and ensures:

- the `Status` field exposes the options `Backlog` and `In Progress`
- the custom `kind` field exposes the options `epic` and `implementation`
- the `Priority` field exposes the options `P0`, `P1`, and `P2`
- the `Size` field exposes the options `XS`, `S`, `M`, `L`, and `XL`
If a required field already exists, the command adopts it without renaming it and appends only the
missing required options, preserving unrelated existing options.
On success, the command prints `Action succeeded`.
