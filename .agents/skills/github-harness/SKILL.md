---
name: github-harness
description: >-
  **MUST** be used always when planning work in the current project;
  **MUST** be used when starting implementation from a PRD;
  **MUST** be used when asked to implement issues;
  **MUST** be used to post clarifying questions on issues;
  **MUST** be used to create newly discovered blocking issues;
  **MUST** be used to stash blocked work; and
  **MUST** be used to create pull requests that close satisfied issues.
---

# GitHub Harness

`github-harness` is the repository-scoped GitHub planning and delivery skill.

## Activation

This skill **MUST** be used for repository project bootstrap, PRD decomposition, issue execution,
clarification comments, blocker creation, blocked-work stash handling, and pull request creation.

## Command Contract

- When a same-purpose harness script exists, the agent **MUST** use the corresponding
  `.scripts/*.py` command for that operation and **MUST NOT** substitute raw `gh` or other GitHub CLI commands. The `scripts` folder is located at the same level of this `SKILL.md`file: they are **NOT** located in the project root folder.
- Raw `gh` or other GitHub CLI commands **MAY** be used only when the human explicitly asks to use
  `gh` or the GitHub CLI.
- Commands that return data **MUST** print structured JSON and **MUST NOT** serialize empty or null
  fields.
- Read commands **MUST** be side-effect free.
- State-changing commands that do not otherwise return structured data **MUST** print the exact line
  `Action succeeded` on success.
- When an agent receives `Action succeeded`, it **SHOULD** trust that the requested mutation
  completed and **SHOULD NOT** perform an extra validation read whose only purpose is to confirm the
  write happened.
- When an agent receives `Action succeeded`, it **SHOULD** trust that the requested mutation
  completed and **SHOULD NOT** perform an extra validation read whose only purpose is to confirm the
  write happened.
- Every script **MUST** have a same-name usage reference in `references/`.

## Issue Contract

- Managed issue titles **MUST** use the form `<type>: <short title>`.
- Managed issue bodies **MUST** include `Description`, `Contract`, `Acceptance`, and `Design notes`.
- `Design notes` **MAY** be empty, but the section **MUST** still be present.
- Canonical GitHub relationship and field names **MUST** be preserved exactly; `kind` is the only
  custom project field introduced by this skill.

## Git Branch Contract

- Implementation work for issue `#<issue_number>` **MUST** happen on the local branch
  `issue-<issue_number>`.
- If that branch does not exist yet, the agent **MUST** create it with exactly that name. If it
  already exists, the agent **MUST** switch to it.
- The agent **MUST NOT** improvise alternate issue branch names with slugs, prefixes, suffixes, or
  other variations.
- Pull requests that close a specific implementation issue **MUST** originate from that issue's
  `issue-<issue_number>` branch.

## PRD Decomposition

- Epic issues **MUST** be clear, self-contained, and understandable without reopening the original
  product requirement document.
- Epic issues **MUST** represent top-level responsibility boundaries. Epics **MUST NOT** be
  chained together through parent-child or dependency relationships during decomposition.
- Epic issues **MUST** extract from the product requirement document everything needed to understand
  the problem, scope, constraints, and what feature-complete means for that concept.
- Epic titles **MUST** stay extremely short and concept-level, naming only the core
  responsibility, such as `epic: authorization`, `epic: database`, or
  `epic: execution outcomes`. Epic titles **MUST NOT** mention implementation details.
- Epic bodies **MAY** be longer than ordinary implementation issues, but they **MUST** remain
  precise and self-contained and **MUST** use bold RFC 2119 / RFC 8174 keywords when stating
  requirements.
- Ordinary implementation issues **SHOULD** stay narrower and shorter than epics and **SHOULD**
  contain only the context needed for one actionable delivery unit inside the epic. When linked to
  a parent epic, the workflow **MUST** rely on GitHub's native parent-child relationship and
  **MUST NOT** mirror child issue links into the epic body.

## Command Families

- Bootstrap: `create_project.py`
- Planning: `create_epic_issue.py`, `create_implementation_issue.py`, `update_issue.py`,
  `search_issues.py`, `list_epics.py`, `list_open_issues.py`, `validate_graph.py`
- Execution: `read_issue.py`, `read_comments.py`, `add_comment.py`, `update_dependencies.py`,
  `get_issue_graph.py`, `create_pr.py`
- Secondary review: `read_pr_reviews.py`

## Workflow References

- `references/PLANNING_WORKFLOWS.md`: bootstrap and PRD decomposition
- `references/IMPLEMENTATION_WORKFLOWS.md`: implementation hot path for implementation issues
- `references/ASSESSMENT_WORKFLOWS.md`: epic assessment cold path
- `references/REVIEW_WORKFLOWS.md`: pull request review intake
- `references/examples/`: concise usage examples

When the implementation workflow ends with `blocked-new`, the agent **MUST** follow the stash
contract documented in `references/IMPLEMENTATION_WORKFLOWS.md` and stop immediately after
reporting the blocker and stash state.
