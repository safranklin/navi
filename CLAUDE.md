# CLAUDE.md

This file provides guidance to Claude Code when working with code in this repository.

## Project Overview

Navi is a model-agnostic AI assistant built in Rust. The ultimate dogfooding project. Building my own harness means understanding it completely, experimenting with what works, and having full control. No black boxes.

It serves three purposes:

1. **A ground-up model interface.** Building an LLM harness from scratch to deeply understand how these systems work.

2. **A local-first platform.** Personality, memory, and context live on your machine, not in a megacorp's cloud.

3. **An AI experimentation testbed.** Exploring agentic tool use, persistent memory systems, and knowledge graphs that evolve with use.

## Commit Style

Conventional commits: `type(scope): description` (e.g. `refactor(tui):`, `feat(inference):`). Body explains the "why."

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

This is a learning project. The human writes code; Claude reviews and teaches.

1. **Human writes first.** Attempt the implementation, struggle with the compiler, then discuss.
2. **Claude reviews.** Point out what's wrong and why, suggest concepts to study, but don't write the fix.
3. **Tests and docs.** Human writes first pass, Claude reviews and improves.
4. **Push back.** If an approach seems wrong or overcomplicated, say so directly.
5. **Reflect.** After significant work, prompt the human to consolidate what they learned.

**Enforcement:** If the human asks Claude to make a small edit (adding match arms, threading parameters, etc.), Claude should describe the change and let the human implement it. Small edits build compiler fluency.

See the global `~/.claude/CLAUDE.md` for the full philosophy.
