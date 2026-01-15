# Learning Journal

A running log of progress, discoveries, and notes.

---

## Session 1 — Getting Started
**Date:** 2025-11-25

### What we're building
Navi: a TUI AI assistant where personality and context stay local.

### Today's focus
Setting up learning scaffolding and preparing to build the first interactive REPL.

### Next steps
- Implement basic REPL loop
- Read Chapter 2 (Guessing Game) for `io::stdin`, variables, loops

### Notes
_Space for observations, "aha" moments, or questions that arise_

---

## Session 2 — First REPL Implementation
**Date:** 2025-11-26

### What we built
Implemented a basic REPL (Read-Eval-Print Loop) that:
- Displays a prompt (`navi> `)
- Reads user input from stdin
- Echoes the input back
- Loops continuously

### Key concepts encountered

#### Trait Scoping
Hit the classic `E0599` error: `no method named 'flush' found for struct 'Stdout'`

**The issue:** The `flush()` method comes from the `Write` trait, which must be explicitly imported even though `Stdout` implements it.

**The fix:** Added `use std::io::Write;`

**Why this matters:** Traits must be in scope to use their methods. This prevents naming conflicts and makes code more explicit. See `concepts/traits.md` for deeper exploration.

#### I/O Fundamentals
- `io::stdin().read_line(&mut buffer)` pattern
- Mutable references (`&mut`)
- The `loop` keyword for infinite loops
- Flushing stdout to ensure prompt appears before reading input

### Book chapters referenced
- **Chapter 2:** Programming a Guessing Game (I/O, loops, variables)
- **Chapter 7.4:** Bringing Paths into Scope with `use`
- **Chapter 10.2:** Traits: Defining Shared Behavior

### Aha moments
Understanding *why* Rust requires explicit trait imports — comparing to extension method conflicts in C#, monkey patching in Python, and prototype pollution in JavaScript really clarified the design decision.

### Next steps
Options to explore:
- Command parsing (handle `exit`, `quit`, `/help`, etc.)
- Multi-line input support
- Mock AI response structure
- Module organization (move REPL out of main.rs)

---

## Session 3 — Command Parsing with Enums
**Date:** 2025-11-26

### What we built
Implemented a command parser that:
- Uses an `enum` to represent different command types (`Quit`, `Help`, `Unknown`)
- Parses user input to identify commands (e.g., `/quit`, `/help`)
- Handles each command with exhaustive pattern matching
- Extracted prompt logic into a reusable `prompt()` function

### Key concepts encountered

#### Enums and Pattern Matching
Created a `Command` enum to model the different types of user input. This is the Rust way of representing "one of several options" — much more type-safe than magic strings.

**Pattern matching with `match`:** Rust's `match` expression ensures exhaustive handling. The compiler forces you to cover every enum variant, preventing the "forgot to handle a case" bugs common in other languages.

See `concepts/enums.md` and `concepts/pattern-matching.md` for deeper exploration.

#### Deriving Traits
Hit an error trying to compare `Command` enum values: `binary operation '!=' cannot be applied to type 'Command'`

**The issue:** Rust doesn't automatically know how to compare custom types.

**The fix:** Added `#[derive(PartialEq)]` above the enum definition. This tells the compiler to auto-generate equality comparison for us.

**Why this matters:** The `derive` attribute is compile-time code generation — safer than reflection because it's all checked at compile time. Chapter 5.2 introduces `derive`, and Chapter 10 goes deeper.

#### Constants vs Variables
Tried to use `String::from()` in a `const` declaration, which failed because `const` values must be computed at compile time, but `String::from()` allocates at runtime.

**The fix:** Changed `const MOTD: String` to `const MOTD: &str` (string slice instead of heap-allocated String).

#### Function Extraction and Ownership
Extracted I/O logic into a `prompt()` function. This required thinking about ownership:
- Created a `String` inside the function
- Returned it with `.to_string()`, transferring ownership to the caller
- This is Rust's move semantics in action

The function signature `fn prompt(model_message: &str) -> String` borrows the input message but returns an owned String.

#### Control Flow Refinement
Initially used `while parse_command(&input) != Command::Quit` with a `break` inside the loop, creating redundant exit logic.

**The improvement:** Switched to `loop` with only the internal `break`, making the control flow clearer and relying on pattern matching inside the loop.

### Book chapters referenced
- **Chapter 3.3:** Functions
- **Chapter 3.5:** Control Flow (especially `match` and `loop`)
- **Chapter 6:** Enums and Pattern Matching (especially 6.2 on `match`)
- **Chapter 8.2:** String methods (`.trim()`, working with `&str` vs `String`)
- **Chapter 5.2:** Example Program Using Structs (introduces `derive`)

### Aha moments
1. **Exhaustive pattern matching** prevents bugs — when you add a new command to the enum later, the compiler will force you to handle it everywhere you match on `Command`
2. **The `derive` attribute** is Rust's way of auto-generating boilerplate code at compile time — much safer than runtime reflection
3. **Move semantics** became clearer when returning `String` from the `prompt()` function — the ownership transfers to the caller without any manual memory management

### Next steps
Options to explore:
- Add more commands (`/clear`, `/exit` as an alias)
- Handle command arguments (e.g., `/help <topic>`)
- Extract command handling into separate functions
- Explore `Result` type for better error handling
- Module organization (split into multiple files)

---

## Session 4 — Async API Integration & JSON
**Date:** 2025-12-04

### What we built
Integrated the OpenRouter API to allow real AI conversations.
- Added dependencies: `tokio`, `reqwest`, `serde`, `dotenv`.
- Created an `api` module with `ChatRequest`, `ChatResponse`, and `ChatMessage` structs.
- Implemented an async client function `chat_completion`.
- Updated the REPL to make network calls for non-command input.

### Key concepts encountered

#### Async/Await & Tokio
Moved from a synchronous `main` to `#[tokio::main]`. Rust's async model is unique because futures do nothing unless polled. Tokio provides the runtime to poll them.
- **Challenge:** Mixing async code with blocking I/O (stdin).
- **Solution:** Used `.await?` for HTTP calls but kept stdin blocking for now (temporary hybrid approach).

#### Serialization with Serde
Used `serde` and `serde_json` to convert between Rust structs and JSON.
- `#[derive(Serialize, Deserialize)]` generates the code to marshal data.
- `#[serde(rename = "...")]` maps Rust's `snake_case` fields to API's `camelCase` or snake_case conventions.

#### Result Propagation
Used the `?` operator extensively to propagate errors up from the HTTP client to main.

### Book chapters referenced
- **Chapter 17:** Async/Await (conceptual)
- **External Crates:** Cargo & Crates.io usage

---

## Session 5 — Domain Modeling & Type Safety
**Date:** 2025-12-19

### What we built
Refactored the codebase to use strong types instead of raw strings.
- Introduced `Role` enum (User, Assistant, System).
- Created `Context` struct to manage conversation history.
- Implemented `Display` trait for cleaner printing.
- Added unit tests for parsing logic.

### Key concepts encountered

#### Type Safety vs "Stringly Typed"
Replaced `String` for roles with `enum Role { User, Assistant, System }`. This prevents invalid roles (e.g., "admin") at compile time.

#### The Display Trait
Implemented `std::fmt::Display` for `Role` to customize how it prints. This is like `ToString` in other languages but trait-based.

#### Testing
Added `#[cfg(test)] mod tests { ... }` blocks. Rust places unit tests in the same file as the code, which allows testing private functions.

### Book chapters referenced
- **Chapter 10:** Traits (Display)
- **Chapter 11:** Writing Automated Tests

---

## Session 6 — Personality & Normalization
**Date:** 2025-12-28

### What we built
- Defined a System Directive to give Navi her "fairy guide" personality.
- Implemented text normalization to clean up model output (converting fancy quotes to ASCII, etc.).

### Key concepts encountered

#### Macros
Used `format!` macro for string interpolation.
Used `vec!` macro for initializing vectors.

#### Character Manipulation
Iterated over characters to replace specific unicode points (em-dash, smart quotes) with ASCII equivalents.

---

## Session 7 — TUI Foundations
**Date:** 2025-12-29

### What we built
Prepared for the transition from a CLI REPL to a full Terminal User Interface (TUI).
- Added `ratatui` (rendering) and `crossterm` (events) dependencies.
- Structured the `src/tui` module.

### Key concepts encountered
- **Immediate Mode Rendering:** Ratatui redraws the entire screen every frame based on state.
- **Terminal Raw Mode:** Taking control of the terminal to capture key presses directly without waiting for "Enter".

---

## Session 8 — The TUI Rewrite (Elm Architecture)
**Date:** 2025-12-31

### What we built
Completely replaced the REPL with a TUI application.
- **State:** `App` struct holding history, input buffer, and mode.
- **Action:** `Action` enum representing all possible events (KeyPress, Submit, Tick).
- **Update:** Pure function `update(app, action) -> app`.
- **View:** `ui::draw` function rendering the state to widgets.

### Key concepts encountered

#### The Elm Architecture / MVI
Separating State, Update logic, and View. This makes the UI predictable and easier to debug.

#### Event Loops
Created a main loop that checks for terminal events (keypresses) and dispatches actions.

#### Cross-thread Communication
We have a TUI thread and need to make async network requests. Initially used a blocking bridge (`block_in_place`) as a temporary measure.

---

## Session 9 — Rendering & Viewports
**Date:** 2026-01-04

### What we built
Improved the chat rendering logic.
- Implemented "bottom-up" rendering to ensure the latest messages are always visible.
- Added visual separation between messages.
- Integrated `tui-scrollview` for better scrolling behavior.

### Key concepts encountered
- **Viewports:** calculating which part of the content is visible.
- **Iterators:** Using `.rev()` to process messages from newest to oldest for bottom-up rendering.

---

## Session 10 — Async Streaming & Channels
**Date:** 2026-01-12

### What we built
True async streaming response support.
- Replaced blocking network calls with `tokio::spawn`.
- Used `std::sync::mpsc` channels to send `Action::ResponseChunk` from the network task to the UI thread.
- Updated the TUI to render partial chunks as they arrive.
- "Thinking" indicator while waiting for the first chunk.

### Key concepts encountered

#### Concurrency & Channels
Communication between the async network task and the synchronous UI rendering loop using channels (`Sender`/`Receiver`).

#### Interior Mutability (RefCell/Mutex)
Managed shared state challenges (though mostly avoided by using message passing).

#### Pinning & Boxing
Async streams often require `Pin<Box<dyn Stream...>>` to be used dynamically.

---

## Session 11 — Thinking Mode & Visual Polish
**Date:** 2026-01-13

### What we built
- Support for "Reasoning Models" (like DeepSeek R1).
- Parsed "thinking" tags/streams from the API.
- Visual styling: Dark gray/italic for thinking, colors for roles (Navi=Green, User=Cyan).

### Key concepts encountered
- **Advanced UI Styling:** Using Ratatui's `Style` and `Color`.
- **Complex State:** Handling multiple types of content (thought vs final answer) in the same message stream.

---
