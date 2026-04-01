# Issue Lifecycle Example

## Scenario

The agent is asked to implement an implementation issue under `epic: execution outcomes`.

## Lifecycle

1. Read the implementation issue with `.scripts/read_issue.py`, which already exposes the
   relationship state needed to detect blockers.
2. Before coding, the agent **MUST** work from the local branch `issue-<issue_number>` for that
   issue. If the branch does not exist, create it with exactly that name; otherwise switch to it.
3. If an open blocker already exists, the agent **MUST** stop with `blocked-existing`.
4. If the implementation issue contract is unclear, the agent **MUST** ask direct questions with
   `.scripts/add_comment.py`, end with `clarification-needed`, and stop.
5. If implementation reveals a new blocker, the agent **MUST** create a focused blocking
   implementation issue, link it with `.scripts/update_dependencies.py`, stash local work with the
   required `blocked-new` message if the working tree is dirty, end with `blocked-new`, and stop.
6. If the implementation issue satisfies its contract and acceptance criteria, the agent **MUST**
   create the closing pull request with `.scripts/create_pr.py`, end with `implemented`, and stop.

## Example Commands

```bash
scripts/read_issue.py --issue 36 --description --contract --acceptance --comments
git switch -c issue-36
scripts/add_comment.py --issue 36 "Which branch state should be preserved when blocked-new occurs?"
scripts/create_implementation_issue.py --title "feat: extract blocked-new stash contract" --description "Document the deterministic stash procedure required when a new blocker interrupts implementation." --contract "Blocked-new handling must preserve partial local work without improvising git commands." --acceptance "1. The stash command is explicit. 2. The blocker is linked before the attempt stops. 3. The agent reports whether a stash entry was created." --parent 38
scripts/update_dependencies.py --issue 36 --blocked-by 52
git stash push --include-untracked --message "blocked-new: issue #36 blocked by #52"
scripts/create_pr.py --title "feat: add github-harness workflow docs" --description "Add workflow references for execution outcomes." --closes 36
```

## Notes

- The issue body **MUST** remain the contract source of truth.
- Comments **SHOULD** capture clarification and follow-up reasoning.
- The implementation attempt **MUST** end in one outcome only.
- Issue execution branches **MUST** be named `issue-<issue_number>` exactly.
- The `blocked-new` stash step **MUST** happen only after the blocker issue exists and the
  dependency link has been recorded.
- If the working tree is already clean, the agent **MUST** report that no stash entry was created
  instead of forcing an empty stash.
- When a same-purpose harness script exists, the agent **MUST** use it and **MUST NOT** substitute
  raw `gh` or other GitHub CLI commands unless the human explicitly asks to use `gh` or the
  GitHub CLI.
- `.scripts/update_dependencies.py --issue 36 --blocked-by none` clears an explicit blocker set when
  the relationship no longer applies.
- `.scripts/get_issue_graph.py` **MAY** still be used when only relationship state needs to be
  isolated from the rest of the issue payload.
- See `references/IMPLEMENTATION_WORKFLOWS.md` for the repeated execution path.
