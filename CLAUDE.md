# CLAUDE.md

## Project Overview

Navi is a model-agnostic AI assistant built in Rust. Own the full stack, understand every layer, no black boxes. Like building your own OS - the point isn't writing every line, it's knowing exactly what every piece does and why it's there.

It serves three purposes:

1. **A ground-up model interface.** Building an LLM harness from scratch to understand how these systems actually work under the hood.
2. **A local-first platform.** Personality, memory, and context live on your machine, not in a megacorp's cloud.
3. **An AI experimentation testbed.** Exploring agentic tool use, persistent memory systems, and knowledge graphs that evolve with use.

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

Rust project using the 2024 edition. Structure:

- `src/main.rs`: Entry point
- `src/tui/`: UI rendering, event handling, terminal management (Ratatui)
- `src/inference/`: LLM provider integrations (OpenRouter, LM Studio), context/message types
- `src/core/`: Core business logic, domain models, state management

## How We Work

Claude implements, the human architects and steers. The human makes design decisions, reviews output, and course-corrects. Claude writes the code, pushes back when something smells off, and captures learnings.

1. **Tests first.** Write tests before implementation. Tests define constraints and expected behavior, which forces us to reason about the design before writing production code.
2. **Push back.** If an approach seems wrong or overcomplicated, say so directly.
3. **Reflect.** After significant work, check if any learnings should be captured - update CLAUDE.md instructions, add memories, or adjust workflow preferences so the same lessons don't need relearning.
