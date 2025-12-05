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

## Session Resumption & Context Management

Claude Code conversations can lose context due to token limits or gaps between sessions. To ensure continuity across sessions, this project maintains two dedicated resumption documents alongside the learning documentation.

### Why These Files Exist

- **Context Loss**: Long conversations hit token limits, requiring session restarts
- **Time Gaps**: Days or weeks may pass between coding sessions
- **Quick Recovery**: Resumption docs provide instant state snapshot without reading entire history
- **Learning Continuity**: Ensure concepts and progress aren't lost between sessions

### When to Update

- **End of each session** — Capture current state before closing
- **After major milestones** — Document completion of features or phases
- **Before context limits** — Proactively update if conversation grows large
- **When switching focus** — Moving between features or architectural work

### Resumption Documents

#### TODO_LIST.md
**Purpose:** High-level project roadmap and feature backlog

**Contains:**
- Planned features and architectural decisions
- Major milestones (e.g., "OpenRouter integration", "Message history", "Config system")
- Technical debt and refactoring needs
- Feature prioritization

**Scope:** Features and architecture, NOT implementation details or step-by-step tasks

**Update when:** New features are planned, milestones completed, or architectural decisions made

---

#### RECAP.md
**Purpose:** Current session state snapshot for resuming work

**Contains:**
- **Current Session**: What session number, what's being built
- **Files Changed**: What's been modified recently
- **Build State**: What compiles, what's broken, any errors
- **Pending Decisions**: Choices that need to be made to proceed
- **Concepts Encountered**: Rust concepts hit in current session
- **Next Steps**: Immediate tasks to continue work

**Scope:** Operational state — "where we left off" for continuation

**Update when:** End of each session, before token limits, or when switching between feature work

---

### Relationship to Learning Documentation

These files serve different but complementary purposes:

- **Learning Docs** (`docs/learning/`)
  - Purpose: Educational record and concept reference
  - `journal.md`: Chronological learning narrative
  - `concepts/`: Deep dives on Rust concepts
  - Audience: Future self, other learners

- **Resumption Docs** (project root)
  - Purpose: Operational state for session continuity
  - `TODO_LIST.md`: What needs to be built
  - `RECAP.md`: Current state and next steps
  - Audience: Future Claude instances resuming work

**Both are committed** to show the full evolution of the project — the learning journey AND the development progression.
