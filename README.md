# agent

A general-purpose learning agent written in Rust. It ships a streaming
Responses runtime, six workdir-bound tools, local project instructions and
skills, durable sessions, and a TUI CLI.

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
# Optional custom system prompt
# system_prompt = "You are a helpful assistant."

[model]
model_id = "gpt-4o"
context_window = 128000 # Required; use the selected model's documented value
api_key  = "sk-..."
# Optional OpenAI Responses-compatible endpoint
# api_base = "https://api.openai.com/v1"

[guard]
mode = "confirm"   # default; medium/high-risk tools require confirmation

# Optional local Langfuse tracing. Run dev/observability/setup.sh to generate
# the endpoint and project keys for these fields.
[observability]
enabled = false
endpoint = "http://localhost:3000/api/public/otel/v1/traces"
# public_key = "pk-lf-..."
# secret_key = "sk-lf-..."
```

CLI flags override the config file:

```sh
agent --model gpt-4o
agent --auto
```

If `api_key` is not set in the config, the `OPENAI_API_KEY` environment
variable is used as a fallback. A missing config file uses these defaults;
an unreadable or invalid config reports its path and parse location instead of
silently ignoring the problem.

Agent uses only the Responses API with `store=false` and official server-side
compaction at 80% of `context_window`. The configured endpoint must implement
that contract; there is no Chat Completions fallback.

## Usage

Run `agent` to start the TUI. Type a message and press `Enter`.
The default conversation is saved under `~/.agent/sessions/default.json` and
resumed on the next run. Use `agent --session <name>` to create or resume a
different conversation. Run `agent sessions` to list saved session names.

The launch directory is the workdir. Agent loads only its `AGENTS.md` at
startup, anchors all relative tool paths there, and discovers repository plus
`~/.agents/skills` skills before each user turn. Type `$` to search skills or
write `$skill-name` directly.

### Keybindings

| Key                    | Action                                      |
| ---------------------- | ------------------------------------------- |
| `Enter`                | Send a non-blank message                    |
| `Shift+Enter`          | Insert a newline in supported terminals     |
| `Ctrl+J`               | Insert a newline (compatibility fallback)   |
| `Backspace`            | Delete character                            |
| `←` / `→` / `↑` / `↓`  | Move the input cursor                       |
| `PageUp` / `PageDown`  | Scroll chat history                         |
| `$`                    | Open the searchable skill picker            |
| `Ctrl+R`               | Retry the exact failed Responses request    |
| `Ctrl+C`               | Quit                                        |

The input grows to one third of the terminal, then scrolls with the cursor.
Long lines wrap visually, and multiline pasted text is inserted without being
submitted. The input title shows `Shift+Enter` when the terminal can report it
reliably, otherwise it shows `Ctrl+J`.

When a tool needs confirmation, `↑`/`↓` selects Allow or Deny and `Enter`
confirms the selection. `y`, `n`, and `Esc` are shortcuts; Deny is selected by
default. Use `agent --auto` to run without confirmation.

Chat content can be selected and copied using your terminal's native
click-and-drag selection.

User and Assistant messages render CommonMark and GitHub Flavored Markdown,
including headings, emphasis, lists, quotes, links, code blocks, task lists,
and tables. User messages use the terminal's reversed colors to remain distinct
on both dark and light themes. Source line breaks stay visible in chat; narrow
tables fall back to vertical key/value records.

## Library usage

```rust
use agent_core::{Agent, AutoGuard, MemoryStorage, OpenAIModel};
use agent_tools::{ReadFileTool, ShellTool};

let model = OpenAIModel::new("gpt-4o");
let mut agent = Agent::builder()
    .model(model)
    .guard(AutoGuard)
    .storage(MemoryStorage::new())
    .workdir("/path/to/project")
    .user_skills_dir("/home/me/.agents/skills")
    .context_window(128_000)
    .tool(ShellTool::new("/path/to/project"))
    .tool(ReadFileTool::new("/path/to/project"))
    .build()
    .await?;

agent.run("List files in /tmp", &my_handler).await?;
```

Implement your own `Tool`, `Guard`, `Storage`, or `Model` to extend the
agent with custom capabilities.

## Local observability

Run `./dev/observability/setup.sh` to start a local Langfuse v4 stack and print the configuration to add to `~/.agent/config.toml`. Once enabled, every Agent turn records the complete model request, discovered skills, tool calls, model output, token/cache usage, errors, and compaction activity. See [`dev/observability/README.md`](dev/observability/README.md) for setup and data-handling details.

Verify the connection with:

```sh
cargo run -p agent-cli -- observability-check
```

## Development

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --locked
```

## License

See [LICENSE](LICENSE).
