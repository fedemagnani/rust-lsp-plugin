---
name: ask-deepwiki
description: Verify external repository behavior with DeepWiki before making integration or architecture claims. Use when working with an external codebase and the agent is unsure about APIs, internal data flow, component roles, boilerplate, example implementations, repeated build or compilation failures, unclear API errors, or how a library/crate/module works under the hood.
---

# Ask DeepWiki

## Overview

Use DeepWiki as the default verification layer for external repositories when recall is uncertain. Treat DeepWiki answers as expert guidance about that repository and use them to reduce hallucinations before coding, explaining internals, or proposing an integration.

## Prerequisites

Expect the DeepWiki MCP tools to be installed and available.

If the DeepWiki MCP tools are missing, unavailable, or return a tool-not-found/server-not-configured error, stop and warn the user immediately. Use plain language such as:

```text
DeepWiki MCP is not installed or not configured in this environment. I need it to verify the external repository before I can answer reliably.
```

Do not silently fall back to guessing about the external repository.

## Repository Identification

The minimum input for DeepWiki work is the repository identity.

Accept either:
- A GitHub repository URL such as `https://github.com/owner/repo`
- An explicit repository name in `owner/repo` format

If the user provides a URL, extract `owner/repo` from it and use that as the DeepWiki `repoName`.

If the repository name is not known, do not guess it. Stop and ask the user for the repository name or URL.

Use a direct question such as:

```text
I need the repository name to query DeepWiki reliably. Which GitHub repository should I inspect? Please send the URL or the owner/repo name.
```

## Use This Skill When

- Need boilerplate or starter code from an external repository
- Need example implementations of a component, module, or integration
- Need to understand how an external repository works internally
- Need to understand the data flow of an external repository
- Need to integrate library X with library Y and external APIs are not fully clear
- Need best practices for using a library, crate, or module from its own repository context
- Build or compilation keeps failing with the same unclear error
- API calls keep returning unexpected results and the external repository may explain why
- Need to answer questions such as:
  - "How can I integrate this component?"
  - "What are the core components of this crate or module?"
  - "Why does the build keep failing?"
  - "What is an example or boilerplate of this component?"
  - "What does this crate do?"
  - "How do I use this API?"

## Workflow

### 1. Decide whether verification is required

Use DeepWiki whenever the task depends on knowledge of an external repository and uncertainty is high enough that guessing could produce bad code or false explanations.

Critical rule: never present confidence about an external repository without verification when DeepWiki is available.

### 2. Establish the repository

Resolve the target repository before asking technical questions.

Rules:
- If the user gave a repository URL, extract `owner/repo`
- If the user gave `owner/repo`, use it directly
- If the user did not give enough information, ask for it
- Never infer the repository from a product name alone
- Never guess a likely GitHub org or repo slug

### 3. Query DeepWiki in natural language

Ask short, concrete questions in natural language.

Prefer a narrow sequence over one oversized question:
- Start with orientation: architecture, main modules, component map, or data flow
- Continue with the exact integration or failure mode
- Ask for examples or boilerplate after the conceptual answer is clear
- Ask follow-up questions until the uncertainty that blocks implementation is removed

Use `mcp__deepwiki__ask_question` for targeted questions. If repository-level orientation is useful and the tools are available, `mcp__deepwiki__read_wiki_structure` or `mcp__deepwiki__read_wiki_contents` can provide extra context before asking more specific questions.

### 4. Ask effective question types

Prefer questions like:
- "What are the core components of this repository and how do they interact?"
- "Explain the request or data flow for feature X from entry point to output."
- "How is API Y expected to be used in this repository?"
- "Show an example implementation of component Z."
- "What is the recommended integration pattern between A and B in this repository?"
- "Why would this build fail with error E in this repository?"
- "Which files or modules implement behavior B?"

When debugging repeated failures, include the exact error text, the command being run, and the relevant component or module.

### 5. Apply the verified information

Use DeepWiki output to guide implementation, explanation, or debugging.

Then:
- Ask additional DeepWiki questions if key uncertainty remains
- Ask the user only when the repository, scope, or failing scenario is still ambiguous
- Make decisions based on verified repository-specific information
- Reference the source in comments only when that is genuinely helpful

### 6. State uncertainty honestly

If DeepWiki does not answer the key question clearly, say so explicitly.

Do not:
- Invent missing APIs
- Claim a module behaves a certain way without verification
- Pretend a guessed integration pattern is confirmed

## Response Pattern

Use this operating pattern:

1. Confirm the repository identity.
2. Query DeepWiki before answering if the external repository knowledge matters.
3. Summarize the verified finding in plain language.
4. Explain how that finding changes the implementation or recommendation.
5. Continue with coding or research only after the uncertainty is reduced enough.

## Example Triggers

- "Integrate this SDK with our service, but I am not sure how its client lifecycle works."
- "Study this repo and tell me how the ingestion pipeline works."
- "Find boilerplate for this component in the upstream repo."
- "The build fails every time with this compiler error. Figure out what the repo expects."
- "I need to know how this crate exposes its API before I wire it into our project."
