# TODO LIST

High-level project roadmap and feature backlog for Navi.

## Current Focus

### ✅ Conversation History (Complete)
- [x] Create `Context` struct with `Vec<ModelSegment>` to store conversation
- [x] Push user and model segments to context each turn
- [x] Modify `model_completion()` to accept full context
- [x] Implement `Context::add()` with reference return
- [x] Add unit tests for Context operations

### 🚧 System Prompts (Next Up)
- [ ] Add initial Directive segment to Context on startup
- [ ] Define Navi's personality/behavior
- [ ] Consider config file vs hardcoded

### ✅ OpenRouter API Integration (Complete)
- [x] Set up async runtime (Tokio) and HTTP client (reqwest)
- [x] Implement basic API client for OpenRouter
- [x] Define request/response types with serde
- [x] Integrate API calls into REPL loop
- [x] Implement type-safe Role enum with Display trait
- [x] Add unit tests for core functionality
- [ ] Handle errors gracefully (network failures, API errors) — Deferred

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
- [x] **Testing Infrastructure** — 10 unit tests covering:
  - [x] parse_command (3 tests)
  - [x] ModelSegment Display (3 tests)
  - [x] Context operations (3 tests)
  - [x] Serde serialization contract test (1 test)
  - [ ] Integration tests for API client
  - [ ] Mock API responses for testing
- [x] **Module Organization** — Basic structure established (api module, types, client)
- [ ] **CI/CD Pipeline** — Automated builds, tests, clippy checks

## Documentation

- [ ] **User Guide** — How to install, configure, and use Navi
- [ ] **Architecture Document** — High-level system design overview
- [ ] **API Wrapper Documentation** — How the OpenRouter client works
- [ ] **Contributing Guide** — For future contributors (if open-sourced)

## Learning Milestones

Track alongside "The Rust Programming Language" book chapters:

- [x] Ch 1-3: Variables, functions, control flow (Session 1-2)
- [x] Ch 4: Ownership, slices (String vs &str, &[T] vs &Vec<T> - Session 4, 5)
- [x] Ch 5: Structs (API types, Context - Session 4, 5)
- [x] Ch 6: Enums and pattern matching (Session 3, 5)
- [x] Ch 7: Modules and code organization (Session 4)
- [x] Ch 8: Collections (Vec<ModelSegment> for conversation history - Session 5)
- [ ] Ch 9: Error handling (custom error types)
- [x] Ch 10: Traits (Display impl, serde customization - Session 5)
- [x] Ch 11: Testing (unit tests, contract tests - Session 5)
- [ ] Ch 12: I/O project (building CLI)
- [ ] Ch 13: Closures and iterators (message processing)
- [ ] Ch 15: Smart pointers (managing message history)
- [ ] Ch 16: Concurrency (async API calls)
- [x] Ch 20: Async/await (async runtime with Tokio - Session 4)

---

**Note:** This is a living document. Update when features are completed, new ideas emerge, or priorities shift.
