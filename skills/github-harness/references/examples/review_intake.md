# Review Intake Example

## Scenario

A pull request exists for a satisfied issue and review comments arrive afterward.

## Flow

1. Read the review set with `.scripts/read_pr_reviews.py`.
2. If the feedback is actionable and in-scope, apply the change on the branch.
3. If the feedback shows that the issue contract is incomplete or ambiguous, continue the
   discussion on the pull request instead of guessing.
4. PR review intake **MUST** stay secondary to the issue-driven workflow.

## Example Commands

```bash
scripts/read_pr_reviews.py --pr 84
scripts/read_issue.py --issue 36 --contract --acceptance --design
scripts/add_comment.py --issue 36 "The review requests a broader status model. Should that become a follow-up issue?"
```

## Notes

- Review intake **SHOULD** confirm whether the comment is a bug, a follow-up, or a requirement gap.
- `reviews_new` **SHOULD** be checked first when only the current head commit feedback matters.
- If the feedback requires a broader change than the current issue contract allows, the agent
  **SHOULD** open or request a separate issue instead of expanding scope silently, replying to the
  reviewer with the links to the issues just opened.
- When a same-purpose harness script exists, the agent **MUST** use it and **MUST NOT** substitute
  raw `gh` or other GitHub CLI commands unless the human explicitly asks to use `gh` or the
  GitHub CLI.
- See `references/REVIEW_WORKFLOWS.md` for the secondary review loop.
