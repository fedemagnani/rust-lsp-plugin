---
description: Create or return the pull request that closes a satisfied issue.
---

# create_pr.py

## Use

`create_pr.py` **MUST** be used when the issue contract and acceptance criteria are satisfied and
the implementation outcome is `implemented`.

## Required Arguments

- `--title`
- `--description`
- `--closes`

## Optional Arguments

- None.

## Effect

Creates a pull request for the current branch or reuses the existing open pull request for that
branch when one already exists.
For issue execution, that branch **MUST** already be named `issue-<issue_number>` for the issue
being closed.
The body **MUST** close the issue passed through `--closes`.
On success, including reuse of an existing matching pull request, the command prints
`Action succeeded`.
