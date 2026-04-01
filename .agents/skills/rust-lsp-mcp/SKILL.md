---
name: rust-lsp-mcp
description: >-
  Rust Analyzer Agent Skill. Use when navigating or analyzing a Rust codebase semantically:
  finding symbol definitions across crate boundaries, locating all references to a function or
  type, inspecting inferred types or trait implementations, searching for symbols by name across
  a workspace, renaming symbols safely, or managing rust-analyzer workspace state. Prefer this
  skill over grep or file reads whenever the task requires Rust-aware semantic understanding.
---

# Rust Analyzer Agent Skill

This skill teaches an agent when and how to use the rust-analyzer MCP tools for Rust codebase
navigation, analysis, and refactoring. The tools are backed by live rust-analyzer sessions and
return structured semantic results.

## When to use MCP tools instead of grep or file reads

**Prefer MCP tools when the task is semantic Rust navigation or analysis.** These tools understand
Rust's type system, module structure, trait implementations, macro expansions, and cross-crate
boundaries. Grep and file reads only see text.

Use the MCP tools when you need to:

- Find where a symbol is defined, including across crate boundaries
- Find all references to a function, type, struct field, or trait method
- Inspect type signatures, documentation, or inferred types at a position
- Search for symbols by name across an entire workspace
- Rename a symbol correctly across all usage sites
- Inspect rust-analyzer internal state for debugging

**Fall back to grep or file reads when:**

- The task is not Rust-specific (config files, markdown, CI scripts)
- You need to search for string literals, comments, or non-symbol text patterns
- You need the raw file contents for editing or context beyond what hover provides
- The workspace root is not registered with the MCP server
- You need to search across files that are not part of the Rust compilation (e.g. build scripts
  that rust-analyzer does not index)

## Workspace and document model

Every tool call requires a `workspace_root` parameter: the absolute path to the registered
workspace root. This routes the call to the correct rust-analyzer session.

Position-based tools (hover, definitions, references, rename) also require a `document_path` and
a zero-based `position` with `line` and `character` fields.

Before using position-based tools on a document, you **must** open it with `open_document` to
synchronize its contents with rust-analyzer. Close it with `close_document` when done. If the file
contents change, use `change_document` with a higher version number.

## Tool reference

### Read-only analysis tools

These tools observe workspace state without mutating anything.

**`hover`** — Inspect type signature, documentation, or inferred type at a position.

```json
{
  "workspace_root": "/path/to/project",
  "document_path": "/path/to/project/src/lib.rs",
  "position": { "line": 10, "character": 5 }
}
```

Use when: you need the type of an expression, the signature of a function, or the documentation on
a symbol. More precise than reading the source because it resolves inferred types and trait
implementations.

**`definitions`** — Resolve where a symbol is defined.

```json
{
  "workspace_root": "/path/to/project",
  "document_path": "/path/to/project/src/main.rs",
  "position": { "line": 15, "character": 12 }
}
```

Use when: you need to navigate to a function, type, or trait definition. Works across crate
boundaries and through re-exports. Returns the file path and range of the definition site.

**`references`** — Find all references to a symbol.

```json
{
  "workspace_root": "/path/to/project",
  "document_path": "/path/to/project/src/lib.rs",
  "position": { "line": 5, "character": 7 }
}
```

Use when: you need to understand where a function is called, where a type is used, or the impact
of changing a symbol. More accurate than grep because it understands scoping, imports, and trait
method dispatch.

**`workspace_symbols`** — Search for symbols by name across the workspace.

```json
{
  "workspace_root": "/path/to/project",
  "query": "Config"
}
```

Use when: you know the name of a type, function, or module but not which file it lives in.
Returns the symbol name, kind (function, struct, enum, etc.), container, and source location.

**`analyzer_status`** — Read the rust-analyzer internal status.

```json
{
  "workspace_root": "/path/to/project",
  "document_path": "/path/to/project/src/lib.rs"
}
```

Use when: debugging rust-analyzer behavior or checking whether the workspace has finished loading.
The `document_path` is optional; omit it to get workspace-level status.

**`view_syntax_tree`** — Inspect the syntax tree of a document.

```json
{
  "workspace_root": "/path/to/project",
  "document_path": "/path/to/project/src/lib.rs"
}
```

Use when: debugging macro expansion issues, parser behavior, or understanding the syntactic
structure of unusual Rust code.

### Stateful workspace tools

These tools mutate session state or produce workspace edits.

**`open_document`** — Open a document and synchronize its contents with rust-analyzer.

```json
{
  "workspace_root": "/path/to/project",
  "document_path": "/path/to/project/src/lib.rs",
  "text": "pub fn answer() -> u32 { 42 }\n"
}
```

You **must** call this before using position-based tools on a document. The optional `language_id`
field defaults to `"rust"`.

**`change_document`** — Replace the full contents of an already-open document.

```json
{
  "workspace_root": "/path/to/project",
  "document_path": "/path/to/project/src/lib.rs",
  "version": 1,
  "text": "pub fn answer() -> u64 { 42 }\n"
}
```

The `version` must be strictly greater than the previous version.

**`close_document`** — Stop synchronizing a document and release its tracked state.

```json
{
  "workspace_root": "/path/to/project",
  "document_path": "/path/to/project/src/lib.rs"
}
```

**`rename_symbol`** — Rename a symbol across the workspace.

```json
{
  "workspace_root": "/path/to/project",
  "document_path": "/path/to/project/src/lib.rs",
  "position": { "line": 0, "character": 7 },
  "new_name": "better_answer"
}
```

Returns the new name and a `workspace_edit` with all affected files and text edits. The edits are
reported but not automatically applied to disk.

**`reload_workspace`** — Reload workspace configuration.

```json
{
  "workspace_root": "/path/to/project"
}
```

Use after modifying `Cargo.toml`, adding or removing crates, or changing project-level
configuration.

**`rebuild_proc_macros`** — Rebuild procedural macros.

```json
{
  "workspace_root": "/path/to/project"
}
```

Use after changing proc-macro crate source code so that rust-analyzer picks up the new expansions.

## Common workflows

### Navigate to a symbol's definition

1. `open_document` with the file contents where the symbol is referenced
2. `definitions` at the position of the symbol
3. Read the target file at the returned location
4. `close_document` when done

### Find all callers of a function

1. `open_document` with the file contents where the function is defined
2. `references` at the position of the function name
3. Review the returned locations
4. `close_document` when done

### Understand a type at a position

1. `open_document` with the file contents
2. `hover` at the position — this returns the resolved type, even for inferred types,
   trait objects, or generic instantiations
3. `close_document` when done

### Find a symbol by name without knowing the file

1. `workspace_symbols` with the symbol name as the query
2. Read the file at the returned location if needed

### Rename a symbol safely

1. `open_document` with the file contents
2. `rename_symbol` at the symbol position with the new name
3. Apply the returned workspace edits to disk
4. `close_document` when done

### React to project structure changes

1. Edit `Cargo.toml` or add/remove crate files
2. `reload_workspace` to pick up the changes
3. If proc-macro crates changed, also call `rebuild_proc_macros`
