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
