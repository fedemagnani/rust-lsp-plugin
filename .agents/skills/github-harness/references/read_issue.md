---
description: Read selected issue sections, fields, and relationships without side effects.
---

# read_issue.py

## Use

`read_issue.py` **MUST** be used when the agent needs a low-noise read of issue content before
planning, resuming, or implementing work.

## Required Arguments

- `--issue`

## Optional Arguments

- `--all`
- section selectors: `--description`, `--contract`, `--acceptance`, `--design`
- comment selector: `--comments`
- field selectors: `--status`, `--priority`, `--size`, `--kind`
- relationship selectors: `--parent`, `--children`, `--blocked-by`, `--blocking`
