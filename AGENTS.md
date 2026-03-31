# AGENTS.md

## Project Goal

This project aims to build a Rust client for `rust-analyzer` and then expose its callable Language Server Protocol functionality through an MCP server built with `rmcp` and `rmcp-macros`.

The intended design is:

- one `rust-analyzer` process per workspace root
- JSON-RPC 2.0 over LSP using `stdio`
- `lsp-server` as the default transport/message layer
- `lsp-types` as the default protocol type layer
- one MCP tool per rust-analyzer callable feature where the mapping is clean
- structured MCP outputs that filter protocol noise and retain only the information useful to the caller
- an agent workflow that prefers the MCP tools over broader text-search approaches when navigating Rust codebases

Implementation preference:

- reuse `lsp-server` and `lsp-types` wherever practical
- avoid introducing local mirrors of standard LSP protocol types unless there is a clear MCP-facing normalization need
- keep custom types focused on client state, lifecycle, and MCP-specific output shaping

## Agent Skill

- [Rust Analyzer Agent Skill](.agents/skills/rust-lsp-mcp/SKILL.md): when and how to use the
  MCP tools for Rust codebase navigation, analysis, and refactoring

## Integrations

- `rust-analyzer`: [docs/integration/rust-analyzer.md](docs/integration/rust-analyzer.md)
- `lsp-server`: [docs/integration/lsp-server.md](docs/integration/lsp-server.md)
- `lsp-types`: [docs/integration/lsp-types.md](docs/integration/lsp-types.md)
- `rmcp`: [docs/integration/rmcp.md](docs/integration/rmcp.md)
