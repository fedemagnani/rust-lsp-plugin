# rmcp Integration Notes

## Repository

- Upstream repository: `modelcontextprotocol/rust-sdk`
- Primary crates of interest: `rmcp`, `rmcp-macros`
- Role in this project: MCP server framework used to expose rust-analyzer functionality as structured MCP tools

## Verified Fit For This Project

The Rust SDK supports the exact server shape needed here:

- MCP server implemented in Rust
- `stdio` transport for IDE-spawned operation
- async request handling with Tokio
- macro-based tool definition and registration
- structured JSON outputs with generated schemas
- support for cancellation, progress, and long-lived server state

This makes it a suitable wrapper layer around a child `rust-analyzer` process.

## Core Server Architecture

The recommended architecture is centered on a server type that implements `ServerHandler`.

That server type can hold:

- a tool router
- per-workspace runtime state
- child process handles
- pending request state
- caches or normalized analysis state

The MCP server is therefore not forced into a stateless model. Shared mutable state can be kept inside the server object using ordinary Rust synchronization primitives where needed.

## Macro-Based Tool System

The most important building blocks are:

### `#[tool]`

Applied to a function or async method to define an MCP tool. It can declare:

- tool name
- title
- description
- annotations
- input schema overrides
- output schema overrides
- metadata

It supports async functions and generates the boilerplate needed to expose them as tools.

### `#[tool_router]`

Applied to an `impl` block. It collects all `#[tool]` methods into a generated `ToolRouter`. This is the main mechanism for organizing many tools behind one server.

This matters because the rust-analyzer wrapper is expected to expose a large tool surface. The SDK design is practical for that use case.

### `#[tool_handler]`

Applied on the `impl ServerHandler` block to connect `call_tool` and `list_tools` to the generated router.

## Structured Input And Output Model

The tool system is schema-aware.

- Inputs are typically expressed as `Parameters<T>`.
- Output schemas can be generated automatically.
- Structured results should be returned with `Json<T>` when possible.

Returning `Json<T>` is important for this project because the wrapper should not simply proxy raw JSON-RPC text. Instead, it should filter, normalize, and structure rust-analyzer responses into cleaner MCP outputs.

This is a good match for goals such as:

- simplifying complex LSP payloads
- preserving only the fields relevant to an agent
- removing transport noise and protocol boilerplate
- making outputs easier to consume without burning context window

## Error Handling

Tool functions can return typed `Result` values. Parameter extraction and schema-aware validation are supported by the framework. Errors can be mapped into MCP-compatible error responses.

For this project, that implies:

- invalid caller input can become tool-level validation errors
- rust-analyzer protocol failures can be translated into meaningful MCP errors
- timeouts, cancellations, or workspace state errors can be surfaced explicitly

## Tool Annotations

The SDK supports semantic hints on tools, including:

- `read_only_hint`
- `destructive_hint`
- `idempotent_hint`
- `open_world_hint`

These are especially useful here because the majority of the rust-analyzer wrapper surface will be read-only analysis and navigation, while a smaller subset will be mutating or stateful operations such as rename, code actions that apply edits, or explicit workspace reloads.

## Transport And Lifecycle

The Rust SDK supports `stdio` server startup, which matches the intended IDE-spawned model.

The basic pattern is:

1. Construct the server handler.
2. Call `serve((stdin(), stdout()))`.
3. Wait on the returned running service.

This is the correct transport for the MCP server itself.

## Child Process Integration

The SDK also contains child-process transport patterns. That is relevant because the MCP server will itself need to spawn and communicate with `rust-analyzer` as a child process over its `stdin` and `stdout`.

The important consequence is that the SDK is compatible with a layered process model:

- IDE spawns MCP server over `stdio`
- MCP server spawns rust-analyzer over `stdio`
- MCP server translates MCP tool calls into LSP JSON-RPC calls

## Concurrency And Long-Lived State

The SDK is based on Tokio and supports asynchronous handlers. This is sufficient for:

- keeping long-lived workspace sessions
- serializing operations that must not overlap
- serving multiple tool calls concurrently where safe
- waiting on child process responses
- maintaining pending request correlation

This is important because the rust-analyzer side is stateful and request/response correlation must be tracked carefully.

## Cancellation And Progress

The SDK supports cancellation and progress capabilities. Those features are relevant because rust-analyzer itself supports cancellation and emits progress for long-running operations such as workspace loading.

The wrapper should be able to:

- propagate cancellation downward when practical
- capture rust-analyzer progress internally
- expose stable, structured progress information where useful

## Large Tool Surface Feasibility

The SDK’s router and macro system make a large tool catalog practical. That matters because the project goal is a near one-to-one MCP mapping for rust-analyzer’s callable features.

The future server can organize tools in several groups internally even if all of them are surfaced through one MCP server:

- workspace and session tools
- navigation tools
- semantic inspection tools
- editing and refactoring tools
- debugging and introspection tools
- test and runnable tools

Even if the implementation is split internally, the generated router model remains appropriate.

## Implications For The Planned Wrapper

`rmcp` and `rmcp-macros` should be used to build:

- a long-lived MCP server process
- macro-defined tools with explicit schemas
- structured outputs using `Json<T>`
- a stateful runtime containing one rust-analyzer child process per workspace root
- tool annotations that distinguish read-only analysis from mutating actions

The SDK supports the project shape directly. The remaining complexity lies mostly in the rust-analyzer client adapter, not in MCP exposure.
