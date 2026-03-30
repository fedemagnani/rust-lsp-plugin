# lsp-types Integration Notes

## Repository

- Upstream repository: `gluon-lang/lsp-types`
- Role in this project: canonical Rust data model for Language Server Protocol request, response, notification, and capability payloads

## Verified Fit For This Project

`lsp-types` should be the default source of truth for LSP protocol structures in this repository.

That means:

- request parameter types should come from `lsp-types` where available
- response payloads should decode directly into `lsp-types` enums and structs
- URI-bearing fields should use `lsp_types::Uri`
- capability negotiation should use typed `ClientCapabilities`, `ServerCapabilities`, `InitializeParams`, and `InitializeResult`

The crate already models the union-heavy parts of the protocol that are easy to get wrong when reimplemented locally.

Examples that matter directly here:

- `CompletionResponse`
- `GotoDefinitionResponse`
- `PrepareRenameResponse`
- `DocumentSymbolResponse`
- `WorkspaceSymbolResponse`

These types are designed for direct `serde` decoding from raw LSP payloads.

## Architectural Guidance

This project should not maintain local mirrors of standard LSP protocol types unless there is a clear reason to normalize or filter them for the MCP layer.

Preferred order:

1. Reuse `lsp-types` directly.
2. Add a thin local wrapper only when the MCP-facing API genuinely benefits from a narrower shape.
3. Avoid duplicating standard protocol enums, IDs, ranges, locations, symbol payloads, or capability structures.

## Where Local Types Still Make Sense

Local types are still appropriate for project state that is not just protocol duplication.

Examples:

- workspace/session lifecycle state
- tracked open-document state
- normalized progress state
- MCP-specific output summaries

Those are client concerns, not replacements for LSP protocol definitions.

## Practical Implications

When implementing or refactoring:

- prefer `lsp-types` request/response pairs over `serde_json::Value`
- prefer `Uri` over raw `String` for document and workspace identifiers
- prefer typed initialize and capability structures over hand-built JSON
- keep custom wrappers small and justified

## Documentation Outcome

`lsp-types` is the canonical protocol model for this repository. Custom protocol structs should be treated as exceptions that need justification, not as the default implementation pattern.
