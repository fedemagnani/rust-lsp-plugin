# rust-lsp-plugin

This repository is a Codex plugin that adds LSP support for Rust via `rust-analyzer`.

The plugin is made of:

- a stdio MCP server: exposing `rust-analyzer` features via MCP tools
- a skill: for installing the MCP server binaries, and for guiding the agent to use it.

## Prerequisites

Install `rust-analyzer` so the server can find it on PATH:

```bash
rustup component add rust-analyzer
```

## Manual Installation

### 1. Install the server

```bash
cargo install --git https://github.com/fedemagnani/rust-lsp-plugin
```

### 2. Install the agent skill

```bash
npx skills add https://github.com/fedemagnani/rust-lsp-plugin
```

### 3. Add the MCP server to codex

```bash
codex mcp add rust-lsp-plugin -- rust-lsp-plugin
```

The server manages a single rust-analyzer session at a time. The workspace root is set
automatically on the first tool call. If a tool call targets a different workspace root, the
previous session is shut down and replaced.
