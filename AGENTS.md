# Agent Instructions for Navi

This document provides technical guidelines for autonomous agents operating in the `navi` codebase.

## 1. Environment & Build System

The project is a Rust application (2024 edition) using Cargo.

### Key Commands
- **Build**: `cargo build`
- **Run**: `cargo run`
- **Test All**: `cargo test`
- **Test Single**: `cargo test <test_name>` (e.g., `cargo test tests::my_test`)
- **Lint**: `cargo clippy -- -D warnings` (Ensure code is lint-free)
- **Format**: `cargo fmt` (Always run before committing)
- **Check**: `cargo check` (Fast syntax/type check)

### Dependencies
- **Async Runtime**: `tokio` (use `#[tokio::main]` for entry points)
- **TUI**: `ratatui` with `crossterm` backend
- **HTTP**: `reqwest`
- **Serialization**: `serde` / `serde_json`
- **Config**: `dotenv` for environment variables

## 2. Code Style & Conventions

Follow standard Rust idioms and the existing codebase style.

### Formatting & Linting
- Strictly adhere to `rustfmt`.
- Fix all `clippy` warnings.
- Maximize type safety; avoid `unwrap()` in production code. Use `?` operator or explicit error handling (e.g., `Result`, `Option`).
- Prefer `expect("context")` over `unwrap()` if a crash is intentional/unavoidable.

### Naming
- **Variables/Functions/Modules**: `snake_case`
- **Types/Structs/Enums/Traits**: `PascalCase`
- **Constants/Statics**: `SCREAMING_SNAKE_CASE`
- **Files**: `snake_case.rs` matching the module name.

### Imports
- Group imports logically:
  1. `std`
  2. External crates (e.g., `tokio`, `ratatui`)
  3. Local modules (`crate::...`, `super::...`)
- Use explicit imports rather than glob imports (`*`), except for preludes or extensive test modules.

### Project Structure
- `src/main.rs`: Entry point. Initializes environment, sets up logging, and starts the TUI loop.
- `src/tui/`: UI rendering, event handling, and terminal management (Ratatui).
- `src/api/`: External API integrations (LLM providers, HTTP clients).
- `src/core/`: Core business logic, domain models, and state management.

## 3. Implementation Guidelines

### Error Handling
- Use the `Result` type for fallible operations.
- Propagate errors using the `?` operator.
- Avoid silencing errors; log them or display them in the TUI status area.

### Testing Strategy
- **Unit Tests**: Co-locate with source code in a `#[cfg(test)] mod tests { ... }` block.
- **Integration Tests**: Place in the `tests/` directory.
- **Mocking**: When testing API clients, mock network responses to ensure tests are deterministic and offline-capable.

### Asynchronous Programming
- Use `async/await` syntax powered by `tokio`.
- The TUI event loop must remain responsive. Offload heavy computation or network I/O to `tokio::spawn` or separate threads/channels.

## 4. Documentation
- Document public structs and functions using doc comments (`///`).
- Include examples in doc comments where complex logic is involved.
- Update `README.md` if adding new major features or configuration requirements.

## 5. Development Workflow for Agents
1. **Explore**: Use `ls`, `read`, or `grep` to understand the relevant code context.
2. **Plan**: Formulate a change strategy that respects the existing architecture.
3. **Edit**: Apply changes using the provided tools.
4. **Verify**:
   - Run `cargo check` to catch compilation errors early.
   - Run `cargo test` to ensure no regressions.
   - Run `cargo fmt` to maintain style consistency.
