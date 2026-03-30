# lsp-server Integration Notes

## Repository

- Upstream repository: `rust-lang/rust-analyzer`
- Reusable crate of interest: `lsp-server`
- Role in this project: reusable JSON-RPC/LSP transport framing and message primitives used by the rust-analyzer stack itself

## Verified Fit For This Project

`rust-analyzer` does not expose a reusable downstream client wrapper of its own. The reusable layer from that ecosystem is `lsp-server`.

For this project, `lsp-server` should be reused for:

- `Message`
- `Request`
- `Response`
- `Notification`
- `RequestId`
- `Message::read`
- `Message::write`
- response construction helpers such as `Response::new_ok` and `Response::new_err`

This avoids maintaining local JSON-RPC framing code and reduces protocol drift.

## Important Constraint

`lsp-server` is a transport and message crate, not a full client abstraction.

This project still owns:

- spawning and supervising the `rust-analyzer` subprocess
- request timeout policy
- pending request correlation
- answering server-originated requests such as `workspace/configuration`
- workspace/document synchronization policy
- higher-level session lifecycle rules

So the right design is:

- reuse `lsp-server` for transport/message primitives
- keep a local client/session layer for process and state management

## Architectural Guidance

Preferred order:

1. Reuse `lsp-server` transport/message primitives directly.
2. Add minimal local logic around them for child-process management and request tracking.
3. Avoid reimplementing Content-Length framing, raw message parsing, or request/response structs unless blocked by a real limitation.

## Relation To rust-analyzer

This is aligned with how rust-analyzer itself is built:

- rust-analyzer uses `lsp-server` for LSP transport/message handling
- rust-analyzer-specific server logic lives above that layer

This project should mirror that split on the client side:

- `lsp-server` for transport/message mechanics
- local client adapter for session management and MCP-oriented behavior

## Documentation Outcome

`lsp-server` is the canonical transport/message dependency for this repository. Custom transport code should be treated as fallback-only, not as the preferred implementation path.
