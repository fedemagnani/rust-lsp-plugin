---
name: rust-lsp
description: >-
  It MUST be used for semantic Rust navigation and analysis: resolving
  definitions across crate boundaries, finding all references to a symbol, inspecting inferred
  types or trait implementations, searching symbols by name, and renaming symbols safely. SHOULD
  be preferred over grep or file reads whenever the task requires Rust-aware understanding.
---

# Rust Analyzer Agent Skill

## When to use

**MUST** use the MCP tools when the task involves:

- Resolving where a symbol is defined, including across crate boundaries
- Finding all references to a function, type, field, or trait method
- Inspecting type signatures, inferred types, or trait implementations
- Searching for symbols by name across a workspace
- Renaming a symbol across all usage sites

**SHOULD** prefer the MCP tools over grep or file reads when the task requires Rust-aware
semantic understanding (scoping, imports, trait dispatch, macro expansion).

**Fall back to grep or file reads** when the task is not Rust-specific, targets string literals
or comments, or covers files outside the Rust compilation.

## Installation

If the Rust LSP tools are not available yet, run `scripts/install.sh` from this skill before using
the MCP tools. That local script installs `rust-analyzer` and the `rust-lsp-plugin` server binary.

## Common workflows

### Navigate to a definition

1. `definitions` at the symbol position
2. Read the target file at the returned location

### Find all callers

1. `references` at the function name position
2. Review the returned locations

### Inspect a type

1. `hover` at the position — returns the resolved type even for inferred types,
   trait objects, and generic instantiations

### Find a symbol by name

1. `workspace_symbols` with the name as query
2. Read the returned location if needed

### Rename a symbol safely

1. `open_document` with the file contents
2. `rename_symbol` at the position with the new name
3. Apply the returned workspace edits to disk
4. `close_document` when done

### Update in-memory content for analysis

Position-based tools work on saved files without `open_document`. Use the document
lifecycle only when analyzing unsaved or in-memory content:

1. `open_document` to synchronize contents with rust-analyzer
2. `replace_document` to update contents (version auto-increments)
3. `close_document` when done (idempotent)

### React to project structure changes

1. `reload_workspace` after editing `Cargo.toml` or adding/removing crates
2. `rebuild_proc_macros` if proc-macro source changed
