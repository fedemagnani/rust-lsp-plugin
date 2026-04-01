# rust-lsp-mcp

MCP server that exposes rust-analyzer functionality as tools for AI agents.

## Install

### 1. Build the server

```bash
git clone https://github.com/fedemagnani/rust-lsp-mcp.git
cd rust-lsp-mcp
cargo build --release
```

The binary is at `target/release/rust-lsp-mcp`.

### 2. Install the agent skill

```bash
npx skills add https://github.com/fedemagnani/rust-lsp-mcp --skill rust-lsp-mcp -y
```

### 3. Add the MCP server

#### Claude Code

```bash
claude mcp add --transport stdio \
  --env RUST_LSP_MCP_RUST_ANALYZER_BIN=rust-analyzer \
  rust-lsp-mcp \
  -- /absolute/path/to/rust-lsp-mcp
```

#### Codex

```bash
codex mcp add \
  --env RUST_LSP_MCP_RUST_ANALYZER_BIN=rust-analyzer \
  rust-lsp-mcp \
  -- /absolute/path/to/rust-lsp-mcp
```

The server manages a single rust-analyzer session at a time. The workspace root is set
automatically on the first tool call. If a tool call targets a different workspace root, the
previous session is shut down and replaced.
