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
