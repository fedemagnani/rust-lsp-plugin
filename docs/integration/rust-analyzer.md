# rust-analyzer Integration Notes

## Repository

- Upstream repository: `rust-lang/rust-analyzer`
- Role in this project: Language Server Protocol backend for Rust code intelligence

## Reuse Policy

This repository should reuse the crates already aligned with the rust-analyzer stack wherever practical:

- use [`lsp-server`](/Users/0xdrun/rust-lsp-plugin/docs/integration/lsp-server.md) for transport framing and message primitives
- use [`lsp-types`](/Users/0xdrun/rust-lsp-plugin/docs/integration/lsp-types.md) for standard LSP protocol data types

The local code should focus on process ownership, session lifecycle, synchronization, and MCP-oriented normalization rather than reimplementing standard protocol structures.

## Verified Protocol Model

`rust-analyzer` is a Language Server Protocol server speaking JSON-RPC 2.0. The normal deployment model is one client spawning and owning one `rust-analyzer` process. The primary transport used in practice is `stdio`. TCP is supported in principle, but `stdio` is the expected transport for this project and matches the chosen implementation direction.

The LSP lifecycle follows the normal handshake:

1. Client sends `initialize`.
2. Server replies with advertised capabilities.
3. Client sends `initialized`.
4. Client maintains document and workspace state through notifications.
5. Client shuts down with `shutdown` and `exit`.

Request cancellation and progress are part of the supported protocol surface. Long-running operations can emit progress notifications.

## Important Architectural Constraint

The wrapper cannot assume that it can generically attach to a pre-existing `rust-analyzer` instance. The supported and expected model is that the wrapper client starts and manages its own `rust-analyzer` child process and owns the corresponding JSON-RPC connection.

This means the MCP server should maintain one `rust-analyzer` process per workspace root.

## Request/Response Features That Map Cleanly to MCP Tools

The following categories map well to one MCP tool per callable feature because they follow a direct request/response pattern.

### Standard LSP Features

- `textDocument/hover`
- `textDocument/completion`
- `textDocument/signatureHelp`
- `textDocument/definition`
- `textDocument/declaration`
- `textDocument/typeDefinition`
- `textDocument/implementation`
- `textDocument/references`
- `textDocument/documentHighlight`
- `textDocument/documentSymbol`
- `textDocument/codeAction`
- `textDocument/codeLens`
- `textDocument/formatting`
- `textDocument/rangeFormatting`
- `textDocument/onTypeFormatting`
- `textDocument/selectionRange`
- `textDocument/foldingRange`
- `textDocument/rename`
- `textDocument/prepareRename`
- `textDocument/inlayHint`
- `textDocument/semanticTokens/full`
- `textDocument/semanticTokens/range`
- workspace diagnostics support
- call hierarchy requests
- `workspace/symbol`
- file operation support such as rename participation where applicable

### rust-analyzer Custom Requests and Extensions

- `rust-analyzer/analyzerStatus`
- `rust-analyzer/fetchDependencyList`
- `rust-analyzer/memoryUsage`
- `rust-analyzer/reloadWorkspace`
- `rust-analyzer/rebuildProcMacros`
- `rust-analyzer/viewSyntaxTree`
- `rust-analyzer/viewHir`
- `rust-analyzer/viewMir`
- `rust-analyzer/interpretFunction`
- `rust-analyzer/viewFileText`
- `rust-analyzer/viewCrateGraph`
- `rust-analyzer/viewItemTree`
- `rust-analyzer/getFailedObligations`
- `rust-analyzer/expandMacro`
- `rust-analyzer/relatedTests`
- `rust-analyzer/discoverTest`
- `rust-analyzer/runTest`
- `rust-analyzer/workspaceSymbol`
- `rust-analyzer/ssr`
- `rust-analyzer/viewRecursiveMemoryLayout`
- `rust-analyzer/runnables`
- `rust-analyzer/codeAction`
- `rust-analyzer/codeActionResolve`
- `rust-analyzer/hover`
- `rust-analyzer/externalDocs`
- `rust-analyzer/openCargoToml`
- `rust-analyzer/moveItem`
- `experimental/matchingBrace`
- `experimental/parentModule`
- `experimental/childModules`
- `experimental/joinLines`
- `experimental/onEnter`
- `experimental/onTypeFormatting`

## Commands and Editor-Facing UX Entry Points

The VS Code extension exposes a broader command list than the pure protocol request surface. Some commands correspond directly to custom requests, while others are editor UX wrappers or client-side orchestration.

Examples from the canonical command surface include:

- `rust-analyzer.viewHir`
- `rust-analyzer.viewMir`
- `rust-analyzer.interpretFunction`
- `rust-analyzer.viewFileText`
- `rust-analyzer.viewItemTree`
- `rust-analyzer.memoryUsage`
- `rust-analyzer.viewCrateGraph`
- `rust-analyzer.viewFullCrateGraph`
- `rust-analyzer.expandMacro`
- `rust-analyzer.matchingBrace`
- `rust-analyzer.parentModule`
- `rust-analyzer.childModules`
- `rust-analyzer.joinLines`
- `rust-analyzer.run`
- `rust-analyzer.copyRunCommandLine`
- `rust-analyzer.debug`
- `rust-analyzer.newDebugConfig`
- `rust-analyzer.analyzerStatus`
- `rust-analyzer.reloadWorkspace`
- `rust-analyzer.rebuildProcMacros`
- `rust-analyzer.restartServer`
- `rust-analyzer.startServer`
- `rust-analyzer.stopServer`
- `rust-analyzer.onEnter`
- `rust-analyzer.ssr`
- `rust-analyzer.serverVersion`
- `rust-analyzer.openDocs`
- `rust-analyzer.openExternalDocs`
- `rust-analyzer.openCargoToml`
- `rust-analyzer.peekTests`
- `rust-analyzer.moveItemUp`
- `rust-analyzer.moveItemDown`
- `rust-analyzer.cancelFlycheck`
- `rust-analyzer.runFlycheck`
- `rust-analyzer.clearFlycheck`
- `rust-analyzer.revealDependency`
- `rust-analyzer.syntaxTreeReveal`
- `rust-analyzer.syntaxTreeCopy`
- `rust-analyzer.syntaxTreeHideWhitespace`
- `rust-analyzer.syntaxTreeShowWhitespace`
- `rust-analyzer.viewMemoryLayout`
- `rust-analyzer.toggleCheckOnSave`
- `rust-analyzer.toggleLSPLogs`
- `rust-analyzer.openWalkthrough`
- `rust-analyzer.getFailedObligations`

For the MCP wrapper, this command surface should not be mirrored blindly. The MCP tools should map to the actual server-callable features, not to every editor command invented for VS Code UX.

## Features That Require Adapter Logic Instead of Simple One-To-One Wrapping

Some parts of the rust-analyzer surface are not meaningful as isolated RPC calls. They depend on continuous client state, event sequencing, or server-push notifications.

These include:

- `textDocument/didOpen`
- `textDocument/didChange`
- `textDocument/didSave`
- `textDocument/didClose`
- `workspace/didChangeConfiguration`
- `workspace/didChangeWatchedFiles`
- diagnostic publication and diagnostic refresh flows
- progress notifications such as `$/progress`
- semantic token refresh and delta state
- code lens refresh
- inlay hint refresh
- test explorer state notifications
- flycheck control and related notifications

This means the MCP server must contain an internal adapter layer that:

- keeps documents synchronized with rust-analyzer
- tracks workspace roots and server state
- translates push-style server output into structured tool results or internal state
- hides irrelevant JSON-RPC noise from callers

## Minimum Client State Required For Correct Results

For requests such as hover, completion, definition, references, or rename to be meaningful, the client must first synchronize the document with rust-analyzer.

At minimum:

1. The file must be known to the server through `textDocument/didOpen`.
2. Subsequent edits must be forwarded through `textDocument/didChange`.
3. Save events should be forwarded when relevant because they can trigger checks, rebuilds, or workspace reload effects.

Without this state management, rust-analyzer will answer against stale or missing source text.

## Initialization And Capability Negotiation

Several initialization details matter for a non-editor client:

- `initializationOptions` can carry rust-analyzer-specific configuration early.
- `workspace/configuration` is used after initialization for config refresh.
- position encoding must be negotiated explicitly.
- `rust-analyzer` supports `UTF-8`, `UTF-16`, and `UTF-32`, with a preference away from `UTF-16` when possible.
- dynamic registration matters for watched files and save notifications.
- workspace edit resource operations should be declared if supported.
- experimental client capabilities can unlock richer responses.

Important examples of client-side capability concerns:

- file watching support
- code action grouping
- snippet text edits
- server status notifications
- test explorer capability

## Workspace And Project Loading Model

Workspace loading is important and can be expensive. `rust-analyzer` uses cargo metadata, sysroot discovery, build scripts, and proc-macro handling to build its internal model.

Important consequences for the wrapper:

- workspace initialization may take noticeable time
- rebuild and reload operations need explicit tool mappings
- progress and status should be normalized into structured MCP output
- only one workspace-loading operation should run at a time per workspace

## Implications For The Planned MCP Wrapper

The future MCP layer should:

- spawn and manage one rust-analyzer child process per workspace root
- communicate over `stdio` using `lsp-server` transport/message primitives
- own LSP initialization and shutdown
- maintain synchronized document state internally
- expose one MCP tool per request/response capability where the mapping is clean
- normalize responses into structured output instead of leaking raw LSP payloads when avoidable
- retain enough detail for advanced use cases such as navigation, refactoring, macro expansion, HIR and MIR inspection, and runnable discovery

## Documentation Outcome

The integration work should treat rust-analyzer as a stateful LSP subsystem, not as a collection of stateless functions. The MCP API can still feel tool-oriented, but it must be backed by an internal session and synchronization model that preserves the assumptions rust-analyzer makes about clients.
