---
description: Secondary pull request review intake workflows.
trigger: Use only after a pull request exists and review feedback needs to be processed.
---

# Review Workflows

These workflows run after a pull request exists and stay separate from both planning and the core
implementation loop.

When a same-purpose harness script exists, the agent **MUST** use it and **MUST NOT** substitute
raw `gh` or other GitHub CLI commands unless the human explicitly asks to use `gh` or the GitHub
CLI.

## Secondary Review Flow

1. Use `.scripts/read_pr_reviews.py` to read pull request reviews after a pull request exists.
2. PR review intake **MUST** remain a secondary workflow; it **MUST NOT** replace the issue-driven
   execution loop.
3. If review feedback reveals missing requirements, continue the conversation on the issue or pull
   request instead of guessing intent.
4. `reviews_new` **SHOULD** be used first when the agent only needs feedback attached to the current
   head commit.
