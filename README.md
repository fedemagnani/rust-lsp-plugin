# rust-lsp-plugin

MCP server that exposes rust-analyzer functionality as tools for AI agents.

## Prerequisites

Install `rust-analyzer` so the server can find it on PATH:

```bash
rustup component add rust-analyzer
```

## Install

### 1. Install the server

```bash
cargo install --git https://github.com/fedemagnani/rust-lsp-plugin
```

### 2. Install the agent skill

```bash
npx skills add https://github.com/fedemagnani/rust-lsp-plugin
```

### 3. Add the MCP server

#### Claude Code

```bash
claude mcp add --transport stdio rust-lsp-plugin -- rust-lsp-plugin
```

#### Codex

```bash
codex mcp add rust-lsp-plugin -- rust-lsp-plugin
```

The server manages a single rust-analyzer session at a time. The workspace root is set
automatically on the first tool call. If a tool call targets a different workspace root, the
previous session is shut down and replaced.
