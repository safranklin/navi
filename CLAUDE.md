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
