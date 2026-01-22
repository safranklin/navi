# Navi

A model-agnostic AI assistant built in Rust. Personality, memory, and context live locally... not with the provider.

The ultimate dogfooding project. Building my own AI harness to understand it completely, experiment with what works, and have full control over the interface. No black boxes.

Named after Navi from Ocarina of Time.

## Why

Navi exists for three reasons:

1. **A ground-up model interface.** Building an LLM harness from scratch to deeply understand how these systems work.

2. **A local-first platform.** Data stays on your machine, not in a megacorp's cloud.

3. **An AI experimentation testbed.** Exploring agentic tool use, persistent memory systems, and knowledge graphs that evolve with use.

## Quick Start

```bash
# 1. Clone and enter
git clone <repo-url> && cd navi

# 2. Set up environment
cp .env.example .env
# Edit .env with your OpenRouter API key and model

# 3. Run
cargo run
```

## Configuration

Create a `.env` file with:

```
OPENROUTER_API_KEY=your-key-here
PRIMARY_MODEL_NAME=anthropic/claude-sonnet-4
```

Get an API key at [openrouter.ai](https://openrouter.ai/).

## Controls

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Esc` | Quit |
| `↑` / `↓` | Scroll |
| `Page Up` / `Page Down` | Scroll faster |
| `End` | Jump to bottom (re-enables auto-scroll) |
| `Ctrl+T` | Cycle reasoning effort (None → Low → Medium → High) |

## Project Structure

```
src/
├── main.rs          # Entry point, logger setup
├── api/             # LLM provider integration
│   ├── client.rs    # OpenRouter streaming client
│   └── types.rs     # Request/response types
├── core/            # Pure business logic (no I/O, no UI)
│   ├── state.rs     # App state
│   └── action.rs    # Action enum + update() reducer
└── tui/             # Terminal UI adapter
    ├── mod.rs       # Event loop, layout caching
    ├── event.rs     # Input event handling
    └── ui.rs        # Rendering
```

## Architecture

Elm-style architecture with strict separation:

```
┌─────────────────────────────────┐
│             CORE                │
│  State + Action + update()      │
│  Pure functions. No I/O.        │
└───────────────┬─────────────────┘
                │
    ┌───────────┼───────────┐
    ▼           ▼           ▼
┌───────┐  ┌────────┐  ┌─────────┐
│  TUI  │  │  CLI   │  │   Web   │
│adapter│  │adapter │  │ adapter │
└───────┘  └────────┘  └─────────┘
```

The `core/` module knows nothing about terminals, HTTP, or any specific interface. The same core can power a TUI, a CLI, a web app, or an IDE extension (like how Claude Code works across terminal and VSCode).

## Development

```bash
cargo build          # Build
cargo run            # Run
cargo test           # Test
cargo clippy         # Lint
cargo fmt            # Format
```

Logs are written to `navi.log` in the current directory.

## License

MIT
