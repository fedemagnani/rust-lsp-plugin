---
description: Epic assessment workflows for feature-completeness review and follow-up issue creation.
trigger: Use when an epic is just created with no children yet, after an implementation cycle ends, or when the human explicitly requests epic reassessment.
---

# Assessment Workflows

These workflows define the cold path for epic reassessment and follow-up implementation planning.

When a same-purpose harness script exists, the agent **MUST** use it and **MUST NOT** substitute
raw `gh` or other GitHub CLI commands unless the human explicitly asks to use `gh` or the GitHub
CLI.

## Epic Assessment Flow

1. Trigger this flow when an epic has just been created and implementation issues still need to be
   identified, after an implementation cycle when all child issues have been closed, or when the
   human explicitly requests reassessment.
2. Read the epic with `.scripts/read_issue.py`, including comments, current relationships, and any
   existing linked implementation issues.
3. Inspect open implementation issues under the epic with `.scripts/list_open_issues.py` and use
   `.scripts/search_issues.py` when duplicate work is plausible.
4. The epic **MUST NOT** be declared feature complete until the feature is tested end-to-end and
   validated on its edge cases.
5. If the epic is not feature complete, the agent **MUST** walk backward from the final
   deliverable and ask "What does this outcome need?" at each step until the missing prerequisite
   implementation issues are exposed.
6. Dependency direction **MUST** be derived from "X needs Y", not from guessed execution order, so
   the implementation graph preserves correct blocker relationships.
7. The analysis **MUST** produce explicit quality scores for effectiveness/soundness and feature
   completeness.
8. If the epic is not feature complete, the agent **MUST NOT** create duplicate open
   implementation issues.
9. A prior `blocked-new` outcome with a linked blocker and local stash **MUST NOT** be treated as
   missing implementation coverage by itself; epic reassessment **MUST** inspect the open blocked
   issue and its blocker relationships before creating follow-up work.
10. Before each new implementation issue is created, the agent **MUST** ask the human for
   permission and **MUST** perform the creation outside the sandbox.
11. Use `.scripts/create_implementation_issue.py` to create missing implementation issues, marking them as children of the parent epic.

