---
name: run-checks
description: Run the full build, test, and lint pipeline for the Navi project
user-invocable: true
disable-model-invocation: true
---

# Run Checks

Run the Navi quality gate: build, test, and clippy — in that order. Stop on the first failure and report the issue.

## Steps

1. `cargo build` — compile the project
2. `cargo test` — run all unit and integration tests
3. `cargo clippy -- -D warnings` — lint with zero warnings enforced

Report a summary when done: pass/fail status for each step and any errors.
