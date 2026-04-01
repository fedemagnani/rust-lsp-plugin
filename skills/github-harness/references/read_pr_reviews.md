---
description: Read pull request reviews.
---

# read_pr_reviews.py

## Use

`read_pr_reviews.py` **SHOULD** be used after a pull request exists and review intake is needed for
follow-up work.

## Required Arguments

- `--pr`

## Optional Arguments

- None.

## Output Behavior

The command **MUST** return sparse JSON containing non-empty review bodies and inline review
comments.
Entries for the current head commit **SHOULD** also be exposed through `reviews_new`.
