# TODO LIST

High-level project roadmap and feature backlog for Navi.

## Current Focus

### 🚧 OpenRouter API Integration (In Progress)
- [ ] Set up async runtime (Tokio) and HTTP client (reqwest)
- [ ] Implement basic API client for OpenRouter
- [ ] Define request/response types with serde
- [ ] Integrate API calls into REPL loop
- [ ] Handle errors gracefully (network failures, API errors)

## Upcoming Features

### Core Functionality
- [ ] **Message History** — Store conversation context across messages
- [ ] **System Prompts** — Define Navi's personality/behavior locally
- [ ] **Multi-turn Conversations** — Send full message history to API
- [ ] **Model Selection** — Allow user to choose/switch models via commands

### Configuration & Persistence
- [ ] **Config System** — YAML/TOML config file for settings
- [ ] **API Key Management** — Secure storage via environment variables or config
- [ ] **Conversation Persistence** — Save/load chat sessions to disk
- [ ] **User Preferences** — Customize prompt style, default model, etc.

### User Experience
- [ ] **Better Command System** — Expand beyond /quit and /help
  - [ ] `/model <name>` — Switch active model
  - [ ] `/system <prompt>` — Set system message
  - [ ] `/clear` — Clear conversation history
  - [ ] `/save <filename>` — Save current session
  - [ ] `/load <filename>` — Load previous session
- [ ] **Streaming Responses** — Display AI responses as they arrive
- [ ] **Syntax Highlighting** — Color output for code blocks
- [ ] **Multi-line Input** — Support for longer prompts

### Advanced Features
- [ ] **Plugin System** — Extend Navi with custom functionality
- [ ] **Tool Calling** — Allow AI to use functions/tools
- [ ] **RAG Integration** — Local document search and context injection
- [ ] **Voice I/O** — Speech-to-text and text-to-speech support

## Technical Debt & Refactoring

- [ ] **Error Handling Strategy** — Define custom error types vs using anyhow
- [ ] **Logging System** — Add structured logging (tracing/log crate)
- [ ] **Testing Infrastructure** — Unit tests, integration tests, mocks for API
- [ ] **Module Organization** — Split into logical modules as codebase grows
- [ ] **CI/CD Pipeline** — Automated builds, tests, clippy checks

## Documentation

- [ ] **User Guide** — How to install, configure, and use Navi
- [ ] **Architecture Document** — High-level system design overview
- [ ] **API Wrapper Documentation** — How the OpenRouter client works
- [ ] **Contributing Guide** — For future contributors (if open-sourced)

## Learning Milestones

Track alongside "The Rust Programming Language" book chapters:

- [x] Ch 1-3: Variables, functions, control flow (Session 1-2)
- [x] Ch 6: Enums and pattern matching (Session 3)
- [ ] Ch 7: Modules and code organization (Session 4 - in progress)
- [ ] Ch 8: Collections (Vec, HashMap for message history)
- [ ] Ch 9: Error handling (custom error types)
- [ ] Ch 10: Traits (generic API client abstraction)
- [ ] Ch 11: Testing (unit tests for API wrapper)
- [ ] Ch 12: I/O project (building CLI)
- [ ] Ch 13: Closures and iterators (message processing)
- [ ] Ch 15: Smart pointers (managing message history)
- [ ] Ch 16: Concurrency (async API calls)
- [ ] Ch 20: Async/await (async runtime with Tokio)

---

**Note:** This is a living document. Update when features are completed, new ideas emerge, or priorities shift.
