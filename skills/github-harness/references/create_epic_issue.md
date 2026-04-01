---
description: Create a self-contained epic issue from product requirement document context.
---

# create_epic_issue.py

## Use

`create_epic_issue.py` **MUST** be used when decomposing a product requirement document into epic
issues.

Epic issues **MUST** stay short in the title, using `epic: <concept>`, and **MUST** carry a
self-contained body that extracts the product context needed to understand scope and
feature-complete behavior without reopening the original document. When the epic states
requirements, it **MUST** use bold RFC 2119 / RFC 8174 keywords such as **MUST**, **MUST NOT**,
and **SHOULD**. Epic issues **MUST** act as independent top-level responsibility boundaries, so
they **MUST NOT** declare parent, `blocked by`, or `blocking` relationships when created.

## Required Arguments

- `--title`
- `--description`
- `--contract`
- `--acceptance`

## Optional Arguments

- `--design`

## Effect

Creates an epic issue, preserves the shared issue sections, and sets the project `kind` field to
`epic`.
On success, the command prints `Action succeeded`.
