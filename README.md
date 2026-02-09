# Navi

A model-agnostic agent harness built in Rust. Context, memory, and personality live on your machine — not in a megacorp's cloud.

I use tools like Claude Code and OpenCode daily and wanted to understand how they actually work. Navi is the result: building my own agent harness from scratch to pull apart the ideas — context engineering, tool orchestration, persistent memory — and try my own out. No black boxes.

Named after Navi from Ocarina of Time.

![Navi demo](docs/demo.gif)

## Why

Navi exists for three reasons:

1. **A ground-up agent harness.** Building the scaffolding around LLMs from scratch — context management, tool execution, memory systems — to understand how tools like Claude Code and OpenCode work under the hood.

2. **A local-first platform.** Data stays on your machine, not in a megacorp's cloud.

3. **An experimentation testbed.** A place to try ideas out: agentic tool use, persistent memory, knowledge graphs, and see what actually works.

## Quick Start

```bash
# 1. Clone and enter
git clone <repo-url> && cd navi

# 2. Set up environment
cp .env.example .env
# Edit .env with your API key and model

# 3. Run
cargo run
```

## Configuration

Create a `.env` file (or copy `.env.example`):

```
OPENROUTER_API_KEY=your-key-here
PRIMARY_MODEL_NAME=anthropic/claude-sonnet-4
```

### Providers

Navi supports multiple LLM backends, selected via CLI flag:

```bash
cargo run                          # OpenRouter (default)
cargo run -- --provider lmstudio   # LM Studio (local)
cargo run -- -p lmstudio           # Short form
```

| Provider | Description | Auth |
|----------|-------------|------|
| **OpenRouter** | Cloud gateway to many models ([openrouter.ai](https://openrouter.ai/)) | `OPENROUTER_API_KEY` env var |
| **LM Studio** | Local inference server (v0.3.29+) | None (local) |

LM Studio connects to `http://localhost:1234/v1` by default. Override with `LM_STUDIO_BASE_URL`.

Both providers use the Responses API with SSE streaming.

### Reasoning Effort

Controls how much the model reasons before responding. Cycle with `Ctrl+R`:

**Auto** → Low → Medium → High → Off → Auto

- **Auto** (default): Model decides whether and how much to reason.
- **Low/Medium/High**: Explicit reasoning effort levels.
- **Off**: Disables reasoning entirely.

## Controls

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Ctrl+J` | Insert newline |
| `Esc` | Quit |
| `←` `→` `↑` `↓` | Move cursor |
| `Home` / `End` | Jump to start/end of line |
| `Backspace` / `Delete` | Delete characters |
| `Ctrl+R` | Cycle reasoning effort |
| `Page Up` / `Page Down` | Scroll messages |
| `Mouse wheel` | Scroll messages |

Bracketed paste is supported — paste multi-line text and newlines are preserved.

## Project Structure

```
src/
├── main.rs                       # Entry point, CLI args, logger setup
├── lib.rs                        # Library root, Provider enum
├── core/                         # Pure business logic (no I/O)
│   ├── state.rs                  # App state
│   └── action.rs                 # Action enum + update() reducer
├── inference/                    # LLM provider integrations
│   ├── types.rs                  # Domain types (Context, Source, Effort, StreamChunk)
│   ├── provider.rs               # CompletionProvider trait
│   └── providers/
│       ├── openrouter.rs         # OpenRouter streaming client
│       └── lmstudio.rs           # LM Studio streaming client
└── tui/                          # Terminal UI (Ratatui)
    ├── mod.rs                    # Event loop, terminal setup
    ├── event.rs                  # Input event mapping
    ├── ui.rs                     # Top-level rendering, hit testing
    ├── component.rs              # Component + EventHandler traits
    └── components/
        ├── title_bar.rs          # Status bar (model name, effort level)
        ├── message.rs            # Single message widget
        ├── message_list.rs       # Scrollable conversation view
        ├── landing.rs            # Landing page
        ├── logo.rs               # Animated braille logo
        └── input_box/
            ├── mod.rs            # Text input field
            ├── cursor.rs         # Cursor and scroll state
            └── text_wrap.rs      # Text wrapping utilities
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

The `core/` module knows nothing about terminals, HTTP, or any specific interface. The TUI is a component-based adapter using Ratatui, with stateful and stateless components following a React-like pattern — components receive props each frame and manage their own rendering.

## Development

```bash
cargo build          # Build
cargo run            # Run
cargo test           # Test
cargo clippy         # Lint
cargo fmt            # Format
```

Rust 2024 edition. Logs are written to `navi.log` in the current directory.

## License

MIT
