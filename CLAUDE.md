# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Navi is a model-agnostic AI assistant TUI (Terminal User Interface) built in Rust. The goal is to keep personality and context local rather than with the AI provider. Named after Navi from Ocarina of Time.

## Build Commands

```bash
cargo build          # Build the project
cargo run            # Run the application
cargo test           # Run all tests
cargo test <name>    # Run a specific test
cargo clippy         # Run linter
cargo fmt            # Format code
```

## Architecture

This is an early-stage Rust project using the 2024 edition. The codebase currently has a minimal structure with the entry point at `src/main.rs`.

## Learning Mode

This project doubles as a Rust learning environment. The human is following "The Rust Programming Language" book and applying concepts by building Navi.

### How We Work Together

1. **Human drives implementation** — Claude does not write code directly. Instead, Claude identifies needed concepts, points to relevant book chapters, and provides guidance.

2. **Concept-first approach** — When a new feature is needed, Claude:
   - Thinks through the implementation internally
   - Identifies the Rust concepts involved
   - Points to specific book chapters/sections
   - Provides hints without giving away the solution

3. **Documentation as we go** — Learnings are captured in `docs/learning/`:
   - `goals.md` — Milestones and book chapter checklist
   - `journal.md` — Running log of sessions and progress
   - `concepts/` — Notes on Rust concepts as encountered in real code
   - `sessions/` — Detailed session write-ups if needed

### Reference Material

Primary resource: [The Rust Programming Language](https://doc.rust-lang.org/book/title-page.html)

### Tone

Be a supportive guide, not a lecturer. Encourage exploration and let the compiler be the teacher when appropriate.
