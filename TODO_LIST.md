# TODO LIST

High-level project roadmap and feature backlog for Navi.

## Current Focus

### ✅ TUI Implementation (Complete - Session 8)
- [x] Implement Elm Architecture (State → Action → Update → View)
- [x] Create App struct in `core/state.rs`
- [x] Create Action enum in `core/action.rs`
- [x] Implement pure `update()` reducer function
- [x] Build three-panel layout (title, chat, input)
- [x] Handle keyboard events in `tui/event.rs`
- [x] Integrate async API calls with `block_in_place()`
- [x] Remove old REPL code
- [x] Add 22 unit tests (actions, UI smoke tests)

### ✅ Conversation History (Complete)
- [x] Create `Context` struct with `Vec<ModelSegment>` to store conversation
- [x] Push user and model segments to context each turn
- [x] Modify `model_completion()` to accept full context
- [x] Implement `Context::add()` with reference return
- [x] Add unit tests for Context operations

### 🚧 System Prompts (Next Up)
- [x] Add initial Directive segment to Context on startup
- [ ] Define Navi's personality/behavior more fully
- [ ] Consider config file vs hardcoded

### ✅ OpenRouter API Integration (Complete)
- [x] Set up async runtime (Tokio) and HTTP client (reqwest)
- [x] Implement basic API client for OpenRouter
- [x] Define request/response types with serde
- [x] Integrate API calls into TUI loop
- [x] Implement type-safe Role enum with Display trait
- [x] Add unit tests for core functionality
- [ ] Handle errors gracefully (network failures, API errors) — Deferred

## Upcoming Features

### Core Functionality
- [x] **TUI Interface** — Replace blocking REPL with ratatui-based TUI
- [x] **Message History** — Store conversation context across messages
- [x] **Scroll Support** — Navigate long conversations in chat panel (via tui-scrollview)
- [ ] **System Prompts** — Define Navi's personality/behavior locally
- [ ] **Multi-turn Conversations** — Send full message history to API
- [ ] **Model Selection** — Allow user to choose/switch models via commands

### Configuration & Persistence
- [ ] **Config System** — YAML/TOML config file for settings
- [ ] **API Key Management** — Secure storage via environment variables or config
- [ ] **Conversation Persistence** — Save/load chat sessions to disk
- [ ] **User Preferences** — Customize prompt style, default model, etc.
- [ ] **Text Normalization Rules** — User-defined character/pattern replacements
  - [ ] Config-based mappings (TOML/YAML)
  - [ ] Simple DSL for transformation rules
  - [ ] Explore parser combinators (`nom`) for advanced patterns

### User Experience
- [ ] **Better Command System** — Expand beyond /quit and /help
  - [ ] `/model <name>` — Switch active model
  - [ ] `/system <prompt>` — Set system message
  - [ ] `/clear` — Clear conversation history
  - [ ] `/save <filename>` — Save current session
  - [ ] `/load <filename>` — Load previous session
- [x] **Streaming Responses** — Display AI responses as they arrive (Session 10)
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
- [x] **Testing Infrastructure** — 23 unit tests covering:
  - [x] parse_command (3 tests)
  - [x] ModelSegment Display (3 tests)
  - [x] Context operations (3 tests)
  - [x] Serde serialization contract test (1 test)
  - [x] Text normalization (8 tests via macro)
  - [x] Action/update reducer (7 tests)
  - [x] UI smoke tests (3 tests)
  - [ ] Integration tests for API client
  - [ ] Mock API responses for testing
- [x] **Module Organization** — Established structure:
  - `api/` — Types, client, external communication
  - `core/` — Pure logic (state, actions, update)
  - `tui/` — Terminal adapter (events, UI rendering)
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
- [x] Ch 5: Structs (API types, Context, App - Session 4, 5, 8)
- [x] Ch 6: Enums and pattern matching (Command, Action - Session 3, 5, 8)
- [x] Ch 7: Modules and code organization (Session 4, 8)
- [x] Ch 8: Collections (Vec<ModelSegment> for conversation history - Session 5)
- [ ] Ch 9: Error handling (custom error types)
- [x] Ch 10: Traits (Display impl, serde customization - Session 5)
- [x] Ch 11: Testing (unit tests, contract tests, smoke tests - Session 5, 8)
- [ ] Ch 12: I/O project (building CLI)
- [x] Ch 13: Closures and iterators (`.iter().map().collect()` - Session 8)
- [ ] Ch 15: Smart pointers (managing message history)
- [x] Ch 16: Concurrency (async API calls, block_in_place - Session 4, 8)
- [x] Ch 19: Macros (test_normalize_rules! - Session 6)
- [x] Ch 20: Async/await (async runtime with Tokio - Session 4, 8)
- [x] **Bonus: StatefulWidget pattern** — External crate integration with tui-scrollview (Session 9/10)

---

**Note:** This is a living document. Update when features are completed, new ideas emerge, or priorities shift.
