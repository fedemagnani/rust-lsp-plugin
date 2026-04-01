---
description: Repeated implementation workflows for implementation issues.
trigger: Use after implementation issues exist and whenever the implementation hot path must run.
---

# Implementation Workflows

These workflows define the repeated execution hot path once implementation issues exist.

When a same-purpose harness script exists, the agent **MUST** use it and **MUST NOT** substitute
raw `gh` or other GitHub CLI commands unless the human explicitly asks to use `gh` or the GitHub
CLI.

For state-changing implementation commands that do not otherwise return structured data, the exact
line `Action succeeded` **MUST** be treated as sufficient confirmation that the mutation completed.
The agent **SHOULD NOT** perform a follow-up read whose only purpose is to confirm the write.

## Branch Flow

1. Before implementation starts, the agent **MUST** work from the local branch
   `issue-<issue_number>` for the current issue.
2. If the branch does not already exist, the agent **MUST** create it with exactly that name. If
   it already exists, the agent **MUST** switch to it.
3. The agent **MUST NOT** use alternate issue branch names with titles, slugs, or extra prefixes.

## Existing Blocker Flow

1. Before implementation, inspect issue relationships with `.scripts/read_issue.py` or
   `.scripts/get_issue_graph.py`.
2. If an open blocking dependency already exists, the agent **MUST NOT** start implementation.
3. The agent **MUST** surface the blocked state as `blocked-existing` and stop.

## Clarification Flow

1. Read the issue body, comments, and dependencies before coding.
2. If any required behavior, boundary, or acceptance condition is unclear, the agent **MUST NOT**
   guess.
3. Use `.scripts/add_comment.py` to post direct clarifying questions on the issue.
4. End the attempt with the outcome `clarification-needed` and stop.

## New Blocker Flow

1. If implementation reveals a new architectural, dependency, or refactoring blocker, the agent
   **MUST** create a focused blocking issue instead of stretching the current issue contract.
2. The blocking issue **SHOULD** be created with `.scripts/create_implementation_issue.py` unless
   the new blocker is broad enough to require a new epic.
3. Use `.scripts/update_dependencies.py` to link the current issue and the new blocker correctly.
4. If the working tree contains tracked, staged, or untracked changes, the agent **MUST** run
   `git stash push --include-untracked --message "blocked-new: issue #<current> blocked by #<blocker>"`
   from the `issue-<issue_number>` implementation branch so the partial work is preserved under a
   deterministic stash
   label.
5. If the working tree is already clean, the agent **MUST NOT** create an empty stash and
   **MUST** report that no stash entry was needed.
6. After the blocker is linked and the stash state is known, the agent **MUST** report the blocker
   issue number, report whether a stash entry was created, end the attempt with the outcome
   `blocked-new`, and stop.

## Completion Flow

1. Re-read the issue contract, acceptance criteria, comments, and dependencies before declaring the
   work done.
2. If needed, validate issue relationships with `.scripts/validate_graph.py`.
3. Use `.scripts/create_pr.py` to create or reuse the pull request that closes the satisfied issue.
4. End the attempt with the outcome `implemented`.

## Outcome Contract

Every implementation attempt **MUST** end in exactly one of these outcomes:

- `blocked-existing`
- `clarification-needed`
- `blocked-new`
- `implemented`

For `blocked-new`, the stash step is terminal. The agent **MUST NOT** pop the stash, switch
branches for follow-up work, or continue coding in the same attempt after the blocker has been
reported.
