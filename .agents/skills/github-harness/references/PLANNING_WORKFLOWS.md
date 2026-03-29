---
description: Bootstrap and PRD decomposition workflows.
trigger: Use before epic and implementation issues exist, typically once per PRD decomposition cycle.
---

# Planning Workflows

These workflows run before implementation issues exist and usually execute once per product
requirement document decomposition cycle.

When a same-purpose harness script exists, the agent **MUST** use it and **MUST NOT** substitute
raw `gh` or other GitHub CLI commands unless the human explicitly asks to use `gh` or the GitHub
CLI.

For state-changing planning commands that do not otherwise return structured data, the exact line
`Action succeeded` **MUST** be treated as sufficient confirmation that the mutation completed.
The agent **SHOULD NOT** perform a follow-up read whose only purpose is to confirm the write.

## Bootstrap Flow

1. Run `.scripts/create_project.py` in the target repository.
2. The bootstrap step **MUST** validate the existing repository project or create it if it is
   missing.
3. The bootstrap step **MUST** ensure the `Status` field exists with the options `Backlog` and
   `In Progress`, the custom `kind` field exists with the options `epic` and `implementation`, the
   `Priority` field exists with the options `P0`, `P1`, and `P2`, and the `Size` field exists with
   the options `XS`, `S`, `M`, `L`, and `XL`.
4. Planning or execution **MUST NOT** proceed until bootstrap succeeds.

## PRD Decomposition Flow

1. Read the product requirement document until the product goals, scope, constraints, and
   completion conditions are clear.
2. Split the product requirement document into epics that each represent one concept or capability.
   Epics **MUST** behave as independent top-level responsibility boundaries and **MUST NOT** be
   linked to one another with parent or dependency relationships.
3. Each epic **MUST** be clear and self-contained. It **MUST** extract from the product
   requirement document everything needed to understand the epic without reopening the original
   document.
4. Epic titles **MUST** remain extremely short and concept-level, naming only the core
   responsibility, such as `epic: authorization`, `epic: database`, or `epic: issue reads`.
   Epic titles **MUST NOT** mention implementation details.
5. Epic bodies **MAY** be longer than ordinary implementation issues, but they **MUST** precisely
   restate the relevant product context and **MUST** describe feature completeness with bold RFC
   2119 / RFC 8174 keywords.
6. Use `.scripts/create_epic_issue.py` to create epic issues and
   `.scripts/create_implementation_issue.py` to create the narrower implementation issues that
   belong under each epic. Epic creation has no parent or dependency flags. When an implementation
   issue is created under an epic, GitHub's native parent-child relationship is the canonical link
   and the epic body **MUST NOT** be rewritten just to list child issues.
7. If the human later refines an issue in GitHub, use `.scripts/update_issue.py` to rewrite only the
   requested sections in place.
