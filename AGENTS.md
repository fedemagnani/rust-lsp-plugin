# AGENTS.md

## Project Goal

This project aims to build a Rust client for `rust-analyzer` and then expose its callable Language Server Protocol functionality through an MCP server built with `rmcp` and `rmcp-macros`.

The intended design is:

- one `rust-analyzer` process per workspace root
- JSON-RPC 2.0 over LSP using `stdio`
- one MCP tool per rust-analyzer callable feature where the mapping is clean
- structured MCP outputs that filter protocol noise and retain only the information useful to the caller
- an agent workflow that prefers the MCP tools over broader text-search approaches when navigating Rust codebases

## Integrations

- `rust-analyzer`: [docs/integration/rust-analyzer.md](/Users/0xdrun/fiber/rust-lsp-mcp/docs/integration/rust-analyzer.md)
- `rmcp`: [docs/integration/rmcp.md](/Users/0xdrun/fiber/rust-lsp-mcp/docs/integration/rmcp.md)
