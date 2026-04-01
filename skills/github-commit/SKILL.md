---
name: github-commit
description: Creates git commits using the Conventional Commits format. Use when creating any git commit, staging changes before committing, or when another skill (e.g. github-kanban) delegates commit creation. Triggers on any commit operation during development workflows.
---

# GitHub Commit

All git commits must follow the Conventional Commits specification. Never use freeform commit messages.
The commit title line **MUST** be less than 72 characters, and every commit body line **MUST** be
less than 100 characters.

## Commit Message Format

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]
```

### Type (required)

Choose the type that best describes the change:

| Type | When to use | SemVer effect |
|------|-------------|---------------|
| `fix` | Patches a bug | PATCH |
| `feat` | Introduces a new feature | MINOR |
| `build` | Changes to build system or external dependencies | — |
| `chore` | Maintenance tasks that don't modify src or test files | — |
| `ci` | Changes to CI configuration files and scripts | — |
| `docs` | Documentation only changes | — |
| `style` | Changes that do not affect the meaning of the code (whitespace, formatting, semicolons) | — |
| `refactor` | Code change that neither fixes a bug nor adds a feature | — |
| `perf` | Code change that improves performance | — |
| `test` | Adding or correcting tests | — |

### Scope (optional)

A noun in parentheses describing the section of the codebase affected.

```
feat(parser): add ability to parse arrays
fix(auth): handle expired token refresh
```

### Description (required)

- Imperative, present tense: "add" not "added" nor "adds"
- Lowercase first letter
- No period at the end
- Complete the sentence: "If applied, this commit will _\<description\>_"
- The full commit title/header (`<type>[optional scope]: <description>`) **MUST** be less than 72
  characters 
  
### Body (optional)

Use the body to explain **what** and **why**, not how. Separate from the description with a blank line.
Every body line **MUST** be less than 100 characters (99 max).

### Footer (optional)

Footers follow the git trailer format: `token: value` or `token #value`.

**Breaking changes** must be indicated by either:

- A `BREAKING CHANGE:` footer:

  ```
  feat(api): change authentication flow

  BREAKING CHANGE: the /login endpoint now requires an API key header
  ```

- A `!` after the type/scope:

  ```
  feat(api)!: change authentication flow
  ```

A `BREAKING CHANGE` footer can appear in a commit of any type. It correlates with MAJOR in Semantic Versioning.

## Commit Procedure

Stage and commit with a conforming message:

```bash
git add -A
git commit -m "<type>[optional scope]: <description>"
```

## History Hygiene

Before pushing, inspect the branch history for non-conforming commits:

```bash
git log --oneline main..HEAD
```

If any commit message does not follow the Conventional Commits format:

- **Single non-conforming commit at HEAD**: amend it in place:

  ```bash
  git commit --amend -m "<type>[optional scope]: <description>"
  ```

- **Multiple non-conforming commits**: soft-reset to the branch point and recommit cleanly:

  ```bash
  git reset --soft main
  git commit -m "<type>[optional scope]: <description>"
  ```

  This collapses all branch commits into a single conforming commit without requiring an interactive editor. If the branch needs multiple conforming commits, reset soft to a specific SHA and recommit in stages:

  ```bash
  git reset --soft <sha>
  git commit -m "<type>[optional scope]: <description>"
  # repeat for each logical group of changes
  ```

Never push commits that violate the Conventional Commits format.

## Examples

Simple bug fix:

```
fix: prevent race condition in user session lookup
```

Feature with scope:

```
feat(lang): add Polish language support
```
