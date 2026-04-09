# agent

A general-purpose LLM agent framework written in Rust. Ships a streaming
tool-call runtime as a library, six built-in tools, and a production-grade
TUI CLI that works with any OpenAI-compatible API — OpenAI, DeepSeek,
Ollama, LM Studio, and others.

## Architecture

The workspace is split into three crates:

- **`agent-core`** — Core runtime. Defines the `Model`, `Tool`, `Guard`,
  `Storage`, and `Handler` traits, plus a generic `Agent<M, G, S>` that runs
  the streaming tool-call loop. Includes an `OpenAIModel` implementation
  built on [`async-openai`](https://crates.io/crates/async-openai).
- **`agent-tools`** — Six built-in tools: `shell`, `read_file`, `write_file`,
  `edit_file`, `glob`, `grep`. Each tool declares a `RiskLevel` so the
  `Guard` can enforce a confirmation policy.
- **`agent-cli`** — TUI application built on
  [`ratatui`](https://crates.io/crates/ratatui) +
  [`crossterm`](https://crates.io/crates/crossterm). Loads config from
  `~/.agent/config.toml`, renders streaming chat and tool calls, and
  supports native terminal text selection.

## Installation

```sh
git clone https://github.com/chenyanchen/agent
cd agent
cargo build --release --workspace
# Binary at: target/release/agent
```

## Configuration

Create `~/.agent/config.toml`:

```toml
[model]
model_id = "gpt-4o"
api_key  = "sk-..."
# Optional: point at any OpenAI-compatible endpoint
# api_base = "https://api.deepseek.com/v1"

[guard]
mode = "confirm"   # or "auto"

# Optional custom system prompt
# system_prompt = "You are a helpful assistant."
```

CLI flags override the config file:

```sh
agent --model deepseek-chat --api-base https://api.deepseek.com/v1
agent --auto     # skip all tool-call confirmations
```

If `api_key` is not set in the config, the `OPENAI_API_KEY` environment
variable is used as a fallback.

### Supported providers

Any OpenAI-compatible Chat Completions API with streaming. Tested against
OpenAI and DeepSeek. Typical `api_base` values:

| Provider  | `api_base`                         |
| --------- | ---------------------------------- |
| OpenAI    | `https://api.openai.com/v1` (default) |
| DeepSeek  | `https://api.deepseek.com/v1`      |
| Ollama    | `http://localhost:11434/v1`        |
| LM Studio | `http://localhost:1234/v1`         |

## Usage

Run `agent` to start the TUI. Type a message and press `Enter`.

### Keybindings

| Key                  | Action                       |
| -------------------- | ---------------------------- |
| `Enter`              | Send message                 |
| `Backspace`          | Delete character             |
| `←` / `→`            | Move input cursor            |
| `↑` / `PageUp`       | Scroll chat history up       |
| `↓` / `PageDown`     | Scroll chat history down     |
| `Ctrl+C`             | Quit                         |

Chat content can be selected and copied using your terminal's native
click-and-drag selection.

## Library usage

```rust
use agent_core::{Agent, AutoGuard, MemoryStorage, OpenAIModel};
use agent_tools::{ReadFileTool, ShellTool};

let model = OpenAIModel::new("gpt-4o");
let mut agent = Agent::builder()
    .model(model)
    .guard(AutoGuard)
    .storage(MemoryStorage::new())
    .tool(ShellTool)
    .tool(ReadFileTool)
    .build();

agent.run("List files in /tmp", &my_handler).await?;
```

Implement your own `Tool`, `Guard`, `Storage`, or `Model` to extend the
agent with custom capabilities.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

## License

See [LICENSE](LICENSE).
