# Epic Assessment Example

## Scenario

This example runs when an epic is first created and implementation issues still need to be spawned,
after an implementation cycle when all child issues have been closed, or when the human explicitly
requests reassessment.

## Review Prompt

```text
Review epic issue #38. Given the current implementation, is the issue fully resolved and is the
epic feature complete? To be considered complete, the feature MUST be tested end-to-end and
validated on its edge cases. Produce quality scores for effectiveness/soundness and feature
completeness. If the epic is not feature complete, avoid duplicate open implementation issues, ask
for permission before each new issue creation and perform creation outside the sandbox.
```

## Flow

1. Read the epic issue, including comments and children.
2. Inspect the linked implementation issues already visible from the epic and search for adjacent
   open work before creating anything new.
3. Decide whether the epic is feature complete based on the current implementation, end-to-end
   coverage, and edge-case validation.
4. If gaps remain, walk backward from the final deliverable and ask "What does this outcome need?"
   until the missing prerequisite implementation issues are identified with the correct dependency
   direction.
5. Record explicit quality scores for effectiveness/soundness and feature completeness.
6. Ask the human for permission before each issue creation and perform the mutation outside the
   sandbox.
7. Create the missing implementation issues

## Example Commands

```bash
scripts/read_issue.py --issue 38 --description --contract --acceptance --design --comments --children --status --kind
scripts/search_issues.py --keywords "execution outcomes, status model, blocker flow" --description
scripts/create_implementation_issue.py --title "feat: harden blocked-new stash handling" --description "Document and implement the stash path for blocked-new outcomes." --contract "Blocked-new handling should preserve incomplete work and keep the epic on a path to feature completeness." --acceptance "1. The stash path is explicit. 2. The blocker is linked to the epic. 3. Duplicate open implementation issues are avoided." --parent 38
```

## Notes

- The epic assessment loop **MUST** run at epic birth and after implementation cycles when all the
  children issues have been closed, or when explicitly requested by the human.
- The epic **MUST NOT** be marked feature complete without end-to-end and edge-case validation.
- Missing implementation issues **MUST** be discovered by walking backward from the final
  deliverable so dependency links follow "X needs Y".
- The result **MUST** include explicit quality scores and a duplicate check before new issue
  creation.
- When a same-purpose harness script exists, the agent **MUST** use it and **MUST NOT** substitute
  raw `gh` or other GitHub CLI commands unless the human explicitly asks to use `gh` or the
  GitHub CLI.
- See `references/ASSESSMENT_WORKFLOWS.md` for the full execution rules.
