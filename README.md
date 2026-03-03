# Navi

A model-agnostic agent harness built in Rust. Context, memory, and personality live on your machine — not in a megacorp's cloud.

I use tools like Claude Code and OpenCode daily and wanted to understand how they actually work. Navi is the result: building my own agent harness from scratch to pull apart the ideas — context engineering, tool orchestration, persistent memory — and try my own out. No black boxes.

Named after Navi from Ocarina of Time.

## Demos

### Startup & Model Switching

Session manager on startup, model picker with live search, switch providers on the fly.

![Startup demo](docs/demo-start.gif)

### Markdown Conversation

Full markdown rendering: syntax-highlighted code blocks, tables, lists, blockquotes.

![Conversation demo](docs/demo-conversation.gif)

### Agentic Tool Use

Chained tool calls with collapsible result blocks. The model calls add → multiply → divide to solve `((3+3) * 5) / 2`.

![Tool use demo](docs/demo-tools.gif)

### Emacs-Style Editing

Kill/yank, word deletion, home/end, and input history recall.

![Editing demo](docs/demo-editing.gif)

### Modes & Reasoning Effort

Cycle reasoning effort (Auto → Low → Medium → High), then navigate messages in cursor mode.

![Modes demo](docs/demo-modes.gif)

## Why

Navi exists for three reasons:

1. **A ground-up agent harness.** Building the scaffolding around LLMs from scratch — context management, tool execution, memory systems — to understand how tools like Claude Code and OpenCode work under the hood.

2. **A local-first platform.** Data stays on your machine, not in a megacorp's cloud.

3. **An experimentation testbed.** A place to try ideas out: agentic tool use, persistent memory, knowledge graphs, and see what actually works.

## Quick Start

```bash
# Clone and build
git clone <repo-url> && cd navi
cargo run
```

On first run, Navi generates `~/.navi/config.toml` with commented defaults. Edit it to add your API key and preferred model.

## Features

- **Multi-provider support** — OpenRouter (cloud) and LM Studio (local), switchable at runtime
- **Agentic tool loop** — up to 20 rounds of chained tool calls with parallel dispatch
- **Streaming responses** — SSE streaming with animated spinner and pulsing text
- **Full markdown rendering** — syntax-highlighted code blocks, tables, lists, blockquotes, task lists
- **Emacs-style editing** — word navigation, kill/yank buffer, line kills, word deletion
- **Input history** — Up/Down recalls previous messages, preserves unsent draft
- **Session management** — persistent sessions with rename, delete, sequential numbering
- **Model picker** — live search across pinned and fetched models, switch without restarting
- **Reasoning effort** — cycle through Auto/Low/Medium/High/Off per message
- **Cursor mode** — keyboard navigation through the conversation, expand/collapse tool calls
- **Bracketed paste** — paste multi-line text with preserved newlines

## Configuration

Config lives at `~/.navi/config.toml`. Environment variables and CLI flags override it.

```toml
[general]
default_provider = "openrouter"
default_model = "anthropic/claude-sonnet-4"
max_agentic_rounds = 20
max_output_tokens = 16384
reasoning_effort = "auto"           # auto | low | medium | high | none
# system_prompt = "..."             # inline system prompt
# system_prompt_file = "prompt.md"  # or load from ~/.navi/prompt.md

[openrouter]
api_key = "your-key-here"
# base_url = "https://openrouter.ai/api/v1"

[lmstudio]
# base_url = "http://localhost:1234/v1"

# Pin models to the top of the model picker
[[models]]
name = "anthropic/claude-sonnet-4"
provider = "openrouter"
description = "Fast and capable"

[[models]]
name = "qwen3-8b"
provider = "lmstudio"
description = "Local 8B model"
```

### Environment Variables

| Variable | Overrides |
|----------|-----------|
| `OPENROUTER_API_KEY` | `openrouter.api_key` |
| `OPENROUTER_BASE_URL` | `openrouter.base_url` |
| `LM_STUDIO_BASE_URL` | `lmstudio.base_url` |
| `PRIMARY_MODEL_NAME` | `general.default_model` |
| `NAVI_PROVIDER` | `general.default_provider` |

### CLI Flags

```bash
cargo run                          # OpenRouter (default)
cargo run -- --provider lmstudio   # LM Studio (local)
cargo run -- -p lmstudio           # Short form
```

### Providers

| Provider | Description | Auth |
|----------|-------------|------|
| **OpenRouter** | Cloud gateway to many models ([openrouter.ai](https://openrouter.ai/)) | `OPENROUTER_API_KEY` |
| **LM Studio** | Local inference server (v0.3.29+) | None (local) |

Both providers use the Responses API with SSE streaming.

## Controls

Navi uses a modal input system: **Input mode** (default) for typing, and **Cursor mode** for navigating messages. Overlays (session manager, model picker) float above both.

### Input Mode

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Shift+Enter` / `Ctrl+J` | Insert newline |
| `Esc` | Cancel generation (if loading), otherwise enter Cursor mode |
| `Ctrl+C` | Quit |
| `←` `→` | Move cursor |
| `↑` `↓` | Move cursor; at input boundary, navigate input history |
| `Home` / `End` | Jump to start/end of line |
| `Ctrl+A` / `Ctrl+E` | Start/end of line (Emacs) |
| `Alt+←` / `Alt+→` | Move by word |
| `Backspace` / `Delete` | Delete character |
| `Ctrl+W` / `Alt+Backspace` | Delete word backward |
| `Alt+D` | Delete word forward |
| `Ctrl+U` | Kill to line start |
| `Ctrl+K` | Kill to line end |
| `Ctrl+Y` | Yank (paste from kill buffer) |
| `Ctrl+R` | Cycle reasoning effort |
| `Ctrl+P` | Open model picker |
| `Ctrl+O` | Open session manager |

### Cursor Mode

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate messages |
| `Space` | Expand/collapse tool call block |
| `Enter` or any character | Switch back to Input mode |
| `Esc` | Cancel generation (if loading) |
| `Ctrl+C` | Quit |

### Session Manager (`Ctrl+O`)

| Key | Action |
|-----|--------|
| `↑` / `↓` | Move selection |
| `Enter` | Load session |
| `n` | New session |
| `r` | Rename selected session (inline edit) |
| `d` `d` | Delete session (press twice to confirm) |
| `Esc` | Dismiss |

### Model Picker (`Ctrl+P`)

| Key | Action |
|-----|--------|
| Type to search | Live filter by name, provider, or description |
| `↑` / `↓` | Move selection |
| `Enter` | Switch to selected model |
| `Backspace` | Clear search character |
| `Esc` | Clear search (first), dismiss (second) |

### Always Active

| Key | Action |
|-----|--------|
| `Page Up` / `Page Down` | Scroll messages |
| `Mouse wheel` | Scroll messages |
| Mouse click | Select message; toggle tool call expand/collapse |

Bracketed paste is supported — paste multi-line text and newlines are preserved.

## Project Structure

```
src/
├── main.rs                       # Entry point, CLI args, logger setup
├── lib.rs                        # Library root, Provider enum
├── core/                         # Pure business logic (no I/O)
│   ├── state.rs                  # App state
│   ├── action.rs                 # Action enum + update() reducer
│   ├── config.rs                 # Config loading (TOML + env + CLI)
│   ├── session.rs                # Session persistence
│   └── tools/                    # Tool system
│       ├── mod.rs                # Tool trait, registry, type erasure
│       └── arithmetic.rs         # Add, subtract, multiply, divide
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
    ├── markdown.rs               # Markdown → styled spans (pulldown_cmark + syntect)
    ├── component.rs              # Component + EventHandler traits
    └── components/
        ├── title_bar.rs          # Status bar with spinner, model, tokens
        ├── message.rs            # Single message widget
        ├── message_list.rs       # Scrollable conversation view
        ├── tool_message.rs       # Collapsible tool call/result blocks
        ├── landing.rs            # Landing page
        ├── logo.rs               # Animated braille logo
        ├── session_manager.rs    # Session list overlay
        ├── model_picker.rs       # Model search/select overlay
        └── input_box/
            ├── mod.rs            # Text input with emacs bindings
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

### Recording Demos

Demo tapes use [VHS](https://github.com/charmbracelet/vhs) for terminal recording:

```bash
vhs demos/start.tape          # Startup & model switching
vhs demos/conversation.tape   # Markdown conversation
vhs demos/tools.tape          # Agentic tool use
vhs demos/editing.tape        # Emacs-style editing
vhs demos/modes.tape          # Modes & reasoning effort
```

## License

MIT
