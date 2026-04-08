# Agent Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a general-purpose LLM Agent system in Rust — a core library with streaming agent loop, built-in tools, and a production-grade TUI CLI.

**Architecture:** Workspace with three crates: `agent-core` (traits + agent loop), `agent-tools` (built-in tools), `agent-cli` (ratatui TUI). The core defines `Model`, `Tool`, `Guard`, `Storage`, `Handler` traits. The Agent loop streams events through a `Handler` trait, enabling real-time TUI rendering. OpenAI API integration via `async-openai` is feature-gated in core.

**Tech Stack:** Rust (2024 edition), tokio, async-openai, serde, thiserror, futures, ratatui, crossterm, clap, toml

---

## File Structure

```
Cargo.toml                          # workspace root
crates/
├── agent-core/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                  # Re-exports all public types
│       ├── error.rs                # Error enum (thiserror)
│       ├── message.rs              # Message enum, ToolCall struct
│       ├── event.rs                # Event enum, Usage, StreamResponse
│       ├── model.rs                # Model trait, Request, Response, ToolDefinition
│       ├── tool.rs                 # Tool trait, RiskLevel, ToolOutput
│       ├── storage.rs              # Storage trait, MemoryStorage
│       ├── guard.rs                # Guard trait, Decision, AutoGuard
│       ├── handler.rs              # Handler trait, AgentEvent
│       ├── agent.rs                # Agent struct, AgentBuilder, run loop
│       └── openai.rs               # OpenAI Model impl (feature-gated)
│
├── agent-tools/
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs                  # Re-exports, tool registry helper
│       ├── shell.rs                # ShellTool
│       ├── read_file.rs            # ReadFileTool
│       ├── write_file.rs           # WriteFileTool
│       ├── edit_file.rs            # EditFileTool
│       ├── glob.rs                 # GlobTool
│       └── grep.rs                 # GrepTool
│
└── agent-cli/
    ├── Cargo.toml
    └── src/
        ├── main.rs                 # Entry point, clap args, tokio::main
        ├── app.rs                  # App state, main event loop, Handler impl
        ├── ui.rs                   # ratatui widget rendering
        ├── input.rs                # Key event handling, input buffer
        └── config.rs               # ~/.agent/config.toml loading
```

---

## Task 1: Workspace Scaffolding

**Files:**
- Create: `Cargo.toml`
- Create: `crates/agent-core/Cargo.toml`
- Create: `crates/agent-core/src/lib.rs`
- Create: `crates/agent-tools/Cargo.toml`
- Create: `crates/agent-tools/src/lib.rs`
- Create: `crates/agent-cli/Cargo.toml`
- Create: `crates/agent-cli/src/main.rs`

- [ ] **Step 1: Create workspace root Cargo.toml**

```toml
[workspace]
resolver = "2"
members = [
    "crates/agent-core",
    "crates/agent-tools",
    "crates/agent-cli",
]
```

- [ ] **Step 2: Create agent-core Cargo.toml**

```toml
[package]
name = "agent-core"
version = "0.1.0"
edition = "2024"

[dependencies]
async-trait = "0.1"
futures = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tokio = { version = "1", features = ["sync"] }
async-openai = { version = "0.27", optional = true }

[dev-dependencies]
tokio = { version = "1", features = ["full"] }

[features]
default = ["openai"]
openai = ["dep:async-openai"]
```

- [ ] **Step 3: Create agent-core src/lib.rs placeholder**

```rust
pub mod error;
pub mod message;
```

- [ ] **Step 4: Create agent-tools Cargo.toml**

```toml
[package]
name = "agent-tools"
version = "0.1.0"
edition = "2024"

[dependencies]
agent-core = { path = "../agent-core" }
async-trait = "0.1"
glob = "0.3"
grep-regex = "0.1"
grep-searcher = "0.1"
reqwest = { version = "0.12", features = ["json"], optional = true }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["process", "fs"] }

[dev-dependencies]
tokio = { version = "1", features = ["full"] }
tempfile = "3"

[features]
default = ["shell", "file", "search"]
shell = []
file = []
search = []
web = ["dep:reqwest"]
```

- [ ] **Step 5: Create agent-tools src/lib.rs placeholder**

```rust
#[cfg(feature = "shell")]
pub mod shell;

#[cfg(feature = "file")]
pub mod read_file;

#[cfg(feature = "file")]
pub mod write_file;

#[cfg(feature = "file")]
pub mod edit_file;

#[cfg(feature = "search")]
pub mod glob;

#[cfg(feature = "search")]
pub mod grep;
```

- [ ] **Step 6: Create agent-cli Cargo.toml**

```toml
[package]
name = "agent-cli"
version = "0.1.0"
edition = "2024"

[dependencies]
agent-core = { path = "../agent-core" }
agent-tools = { path = "../agent-tools" }
clap = { version = "4", features = ["derive"] }
crossterm = "0.28"
ratatui = "0.29"
serde = { version = "1", features = ["derive"] }
toml = "0.8"
tokio = { version = "1", features = ["full"] }
dirs = "6"
```

- [ ] **Step 7: Create agent-cli src/main.rs placeholder**

```rust
fn main() {
    println!("agent-cli");
}
```

- [ ] **Step 8: Verify workspace compiles**

Run: `cargo check --workspace`
Expected: compiles with no errors (may have unused warnings)

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml crates/
git commit -m "scaffold workspace with agent-core, agent-tools, agent-cli crates"
```

---

## Task 2: Core Types — Error + Message

**Files:**
- Create: `crates/agent-core/src/error.rs`
- Create: `crates/agent-core/src/message.rs`
- Modify: `crates/agent-core/src/lib.rs`

- [ ] **Step 1: Write tests for Message types**

Add to bottom of `crates/agent-core/src/message.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_system() {
        let msg = Message::System {
            content: "You are helpful.".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, Message::System { content } if content == "You are helpful."));
    }

    #[test]
    fn test_message_assistant_with_tool_calls() {
        let msg = Message::Assistant {
            text: Some("Let me check.".into()),
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                name: "shell".into(),
                arguments: r#"{"command":"ls"}"#.into(),
            }],
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        match deserialized {
            Message::Assistant { text, tool_calls } => {
                assert_eq!(text.unwrap(), "Let me check.");
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].name, "shell");
            }
            _ => panic!("expected Assistant"),
        }
    }

    #[test]
    fn test_message_tool_result() {
        let msg = Message::Tool {
            tool_call_id: "call_1".into(),
            content: "file1.txt\nfile2.txt".into(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, Message::Tool { tool_call_id, .. } if tool_call_id == "call_1"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p agent-core`
Expected: FAIL — structs/enums not defined yet

- [ ] **Step 3: Implement error.rs and message.rs**

`crates/agent-core/src/error.rs`:

```rust
use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("model error: {0}")]
    Model(String),

    #[error("tool error: {0}")]
    Tool(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}
```

`crates/agent-core/src/message.rs` (above the `#[cfg(test)]` block):

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role")]
pub enum Message {
    #[serde(rename = "system")]
    System { content: String },

    #[serde(rename = "user")]
    User { content: String },

    #[serde(rename = "assistant")]
    Assistant {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        tool_calls: Vec<ToolCall>,
    },

    #[serde(rename = "tool")]
    Tool {
        tool_call_id: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}
```

- [ ] **Step 4: Update lib.rs**

```rust
pub mod error;
pub mod message;

pub use error::Error;
pub use message::{Message, ToolCall};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p agent-core`
Expected: 3 tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/agent-core/
git commit -m "add Error and Message core types with serde support"
```

---

## Task 3: Event + StreamResponse

**Files:**
- Create: `crates/agent-core/src/event.rs`
- Modify: `crates/agent-core/src/lib.rs`

- [ ] **Step 1: Write tests for Usage and Event**

Add to bottom of `crates/agent-core/src/event.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[test]
    fn test_usage_default() {
        let usage = Usage::default();
        assert_eq!(usage.prompt_tokens, 0);
        assert_eq!(usage.completion_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
    }

    #[tokio::test]
    async fn test_stream_response_collect_text_only() {
        let events = vec![
            Event::TextDelta("Hello".into()),
            Event::TextDelta(" world".into()),
            Event::Done {
                usage: Usage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                },
            },
        ];
        let stream = StreamResponse::from_events(events);
        let response = stream.collect().await.unwrap();
        match &response.message {
            Message::Assistant { text, tool_calls } => {
                assert_eq!(text.as_deref(), Some("Hello world"));
                assert!(tool_calls.is_empty());
            }
            _ => panic!("expected Assistant message"),
        }
        assert_eq!(response.usage.total_tokens, 15);
    }

    #[tokio::test]
    async fn test_stream_response_collect_with_tool_call() {
        let events = vec![
            Event::TextDelta("Let me check.".into()),
            Event::ToolCallBegin {
                id: "call_1".into(),
                name: "shell".into(),
            },
            Event::ToolCallDelta {
                id: "call_1".into(),
                arguments_delta: r#"{"command""#.into(),
            },
            Event::ToolCallDelta {
                id: "call_1".into(),
                arguments_delta: r#":"ls"}"#.into(),
            },
            Event::ToolCallEnd {
                id: "call_1".into(),
            },
            Event::Done {
                usage: Usage::default(),
            },
        ];
        let stream = StreamResponse::from_events(events);
        let response = stream.collect().await.unwrap();
        match &response.message {
            Message::Assistant { text, tool_calls } => {
                assert_eq!(text.as_deref(), Some("Let me check."));
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].name, "shell");
                assert_eq!(tool_calls[0].arguments, r#"{"command":"ls"}"#);
            }
            _ => panic!("expected Assistant message"),
        }
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p agent-core`
Expected: FAIL — Event, StreamResponse not defined

- [ ] **Step 3: Implement event.rs**

`crates/agent-core/src/event.rs` (above the `#[cfg(test)]` block):

```rust
use std::pin::Pin;

use futures::stream::{self, Stream, StreamExt};

use crate::error::Error;
use crate::message::{Message, ToolCall};

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone)]
pub enum Event {
    TextDelta(String),
    ToolCallBegin { id: String, name: String },
    ToolCallDelta { id: String, arguments_delta: String },
    ToolCallEnd { id: String },
    Done { usage: Usage },
}

pub struct Response {
    pub message: Message,
    pub usage: Usage,
}

pub struct StreamResponse {
    inner: Pin<Box<dyn Stream<Item = Event> + Send>>,
}

impl StreamResponse {
    pub fn new(stream: impl Stream<Item = Event> + Send + 'static) -> Self {
        Self {
            inner: Box::pin(stream),
        }
    }

    /// Create a StreamResponse from a Vec of events (useful for testing).
    pub fn from_events(events: Vec<Event>) -> Self {
        Self::new(stream::iter(events))
    }

    /// Collect the stream into a complete Response.
    pub async fn collect(mut self) -> Result<Response, Error> {
        let mut text = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut usage = Usage::default();

        // Track in-progress tool calls by id
        let mut pending_calls: Vec<(String, String, String)> = Vec::new(); // (id, name, arguments)

        while let Some(event) = self.inner.next().await {
            match event {
                Event::TextDelta(delta) => text.push_str(&delta),
                Event::ToolCallBegin { id, name } => {
                    pending_calls.push((id, name, String::new()));
                }
                Event::ToolCallDelta {
                    id,
                    arguments_delta,
                } => {
                    if let Some(call) = pending_calls.iter_mut().find(|(cid, _, _)| cid == &id) {
                        call.2.push_str(&arguments_delta);
                    }
                }
                Event::ToolCallEnd { id } => {
                    if let Some(pos) = pending_calls.iter().position(|(cid, _, _)| cid == &id) {
                        let (id, name, arguments) = pending_calls.remove(pos);
                        tool_calls.push(ToolCall {
                            id,
                            name,
                            arguments,
                        });
                    }
                }
                Event::Done { usage: u } => {
                    usage = u;
                }
            }
        }

        let message = Message::Assistant {
            text: if text.is_empty() { None } else { Some(text) },
            tool_calls,
        };

        Ok(Response { message, usage })
    }
}

impl Stream for StreamResponse {
    type Item = Event;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}
```

- [ ] **Step 4: Update lib.rs**

```rust
pub mod error;
pub mod event;
pub mod message;

pub use error::Error;
pub use event::{Event, Response, StreamResponse, Usage};
pub use message::{Message, ToolCall};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p agent-core`
Expected: all tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/agent-core/
git commit -m "add Event, StreamResponse with collect-to-Response support"
```

---

## Task 4: Model Trait + Request

**Files:**
- Create: `crates/agent-core/src/model.rs`
- Modify: `crates/agent-core/src/lib.rs`

- [ ] **Step 1: Write test for Model trait with a mock**

Add to bottom of `crates/agent-core/src/model.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, StreamResponse, Usage};
    use futures::stream;

    struct EchoModel;

    #[async_trait::async_trait]
    impl Model for EchoModel {
        async fn stream(&self, request: Request) -> Result<StreamResponse, Error> {
            // Echo back the last user message
            let echo = request
                .messages
                .iter()
                .rev()
                .find_map(|m| match m {
                    Message::User { content } => Some(content.clone()),
                    _ => None,
                })
                .unwrap_or_default();

            let events = vec![
                Event::TextDelta(echo),
                Event::Done {
                    usage: Usage::default(),
                },
            ];
            Ok(StreamResponse::new(stream::iter(events)))
        }
    }

    #[tokio::test]
    async fn test_model_stream_and_collect() {
        let model = EchoModel;
        let request = Request {
            system: None,
            messages: vec![Message::User {
                content: "hello".into(),
            }],
            tools: vec![],
            temperature: None,
            max_tokens: None,
        };
        let response = model.stream(request).await.unwrap().collect().await.unwrap();
        match &response.message {
            Message::Assistant { text, .. } => assert_eq!(text.as_deref(), Some("hello")),
            _ => panic!("expected Assistant"),
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p agent-core`
Expected: FAIL — Model, Request not defined

- [ ] **Step 3: Implement model.rs**

`crates/agent-core/src/model.rs` (above the `#[cfg(test)]` block):

```rust
use crate::error::Error;
use crate::event::StreamResponse;
use crate::message::Message;

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub system: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

#[async_trait::async_trait]
pub trait Model: Send + Sync {
    async fn stream(&self, request: Request) -> Result<StreamResponse, Error>;
}
```

- [ ] **Step 4: Update lib.rs**

```rust
pub mod error;
pub mod event;
pub mod message;
pub mod model;

pub use error::Error;
pub use event::{Event, Response, StreamResponse, Usage};
pub use message::{Message, ToolCall};
pub use model::{Model, Request, ToolDefinition};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p agent-core`
Expected: all tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/agent-core/
git commit -m "add Model trait with Request and ToolDefinition"
```

---

## Task 5: Tool Trait

**Files:**
- Create: `crates/agent-core/src/tool.rs`
- Modify: `crates/agent-core/src/lib.rs`

- [ ] **Step 1: Write test for Tool trait with a mock**

Add to bottom of `crates/agent-core/src/tool.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct UppercaseTool;

    #[async_trait::async_trait]
    impl Tool for UppercaseTool {
        fn name(&self) -> &str {
            "uppercase"
        }

        fn description(&self) -> &str {
            "Converts text to uppercase"
        }

        fn schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string" }
                },
                "required": ["text"]
            })
        }

        fn risk_level(&self) -> RiskLevel {
            RiskLevel::Low
        }

        async fn call(&self, input: serde_json::Value) -> Result<ToolOutput, Error> {
            let text = input["text"]
                .as_str()
                .ok_or_else(|| Error::Tool("missing 'text' field".into()))?;
            Ok(ToolOutput::Text(text.to_uppercase()))
        }
    }

    #[tokio::test]
    async fn test_tool_call() {
        let tool = UppercaseTool;
        assert_eq!(tool.name(), "uppercase");
        assert_eq!(tool.risk_level(), RiskLevel::Low);

        let input = serde_json::json!({"text": "hello"});
        let output = tool.call(input).await.unwrap();
        assert!(matches!(output, ToolOutput::Text(s) if s == "HELLO"));
    }

    #[tokio::test]
    async fn test_tool_call_error() {
        let tool = UppercaseTool;
        let input = serde_json::json!({"wrong": "field"});
        let result = tool.call(input).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p agent-core`
Expected: FAIL — Tool, RiskLevel, ToolOutput not defined

- [ ] **Step 3: Implement tool.rs**

`crates/agent-core/src/tool.rs` (above the `#[cfg(test)]` block):

```rust
use crate::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone)]
pub enum ToolOutput {
    Text(String),
    Error(String),
}

impl std::fmt::Display for ToolOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolOutput::Text(s) => write!(f, "{s}"),
            ToolOutput::Error(s) => write!(f, "Error: {s}"),
        }
    }
}

#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> serde_json::Value;
    fn risk_level(&self) -> RiskLevel;
    async fn call(&self, input: serde_json::Value) -> Result<ToolOutput, Error>;
}
```

- [ ] **Step 4: Update lib.rs**

```rust
pub mod error;
pub mod event;
pub mod message;
pub mod model;
pub mod tool;

pub use error::Error;
pub use event::{Event, Response, StreamResponse, Usage};
pub use message::{Message, ToolCall};
pub use model::{Model, Request, ToolDefinition};
pub use tool::{RiskLevel, Tool, ToolOutput};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p agent-core`
Expected: all tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/agent-core/
git commit -m "add Tool trait with RiskLevel and ToolOutput"
```

---

## Task 6: Storage Trait + MemoryStorage

**Files:**
- Create: `crates/agent-core/src/storage.rs`
- Modify: `crates/agent-core/src/lib.rs`

- [ ] **Step 1: Write tests for MemoryStorage**

Add to bottom of `crates/agent-core/src/storage.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_storage_save_and_load() {
        let storage = MemoryStorage::new();
        let messages = vec![
            Message::User {
                content: "hello".into(),
            },
            Message::Assistant {
                text: Some("hi".into()),
                tool_calls: vec![],
            },
        ];

        storage.save("conv1", &messages).await.unwrap();
        let loaded = storage.load("conv1").await.unwrap();
        assert_eq!(loaded.len(), 2);
    }

    #[tokio::test]
    async fn test_memory_storage_load_nonexistent() {
        let storage = MemoryStorage::new();
        let loaded = storage.load("nonexistent").await.unwrap();
        assert!(loaded.is_empty());
    }

    #[tokio::test]
    async fn test_memory_storage_overwrite() {
        let storage = MemoryStorage::new();
        let messages1 = vec![Message::User {
            content: "first".into(),
        }];
        let messages2 = vec![Message::User {
            content: "second".into(),
        }];

        storage.save("conv1", &messages1).await.unwrap();
        storage.save("conv1", &messages2).await.unwrap();
        let loaded = storage.load("conv1").await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(matches!(&loaded[0], Message::User { content } if content == "second"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p agent-core`
Expected: FAIL — Storage, MemoryStorage not defined

- [ ] **Step 3: Implement storage.rs**

`crates/agent-core/src/storage.rs` (above the `#[cfg(test)]` block):

```rust
use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::error::Error;
use crate::message::Message;

#[async_trait::async_trait]
pub trait Storage: Send + Sync {
    async fn save(&self, id: &str, messages: &[Message]) -> Result<(), Error>;
    async fn load(&self, id: &str) -> Result<Vec<Message>, Error>;
}

pub struct MemoryStorage {
    data: Arc<RwLock<HashMap<String, Vec<Message>>>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Storage for MemoryStorage {
    async fn save(&self, id: &str, messages: &[Message]) -> Result<(), Error> {
        let mut data = self.data.write().await;
        data.insert(id.to_string(), messages.to_vec());
        Ok(())
    }

    async fn load(&self, id: &str) -> Result<Vec<Message>, Error> {
        let data = self.data.read().await;
        Ok(data.get(id).cloned().unwrap_or_default())
    }
}
```

- [ ] **Step 4: Update lib.rs**

```rust
pub mod error;
pub mod event;
pub mod message;
pub mod model;
pub mod storage;
pub mod tool;

pub use error::Error;
pub use event::{Event, Response, StreamResponse, Usage};
pub use message::{Message, ToolCall};
pub use model::{Model, Request, ToolDefinition};
pub use storage::{MemoryStorage, Storage};
pub use tool::{RiskLevel, Tool, ToolOutput};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p agent-core`
Expected: all tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/agent-core/
git commit -m "add Storage trait and MemoryStorage implementation"
```

---

## Task 7: Guard Trait + AutoGuard

**Files:**
- Create: `crates/agent-core/src/guard.rs`
- Modify: `crates/agent-core/src/lib.rs`

- [ ] **Step 1: Write tests for AutoGuard and ConfirmGuard**

Add to bottom of `crates/agent-core/src/guard.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::RiskLevel;

    #[tokio::test]
    async fn test_auto_guard_always_allows() {
        let guard = AutoGuard;
        let input = serde_json::json!({"command": "rm -rf /"});
        let decision = guard.check("shell", &input).await;
        assert!(matches!(decision, Decision::Allow));
    }

    #[tokio::test]
    async fn test_confirm_guard_low_risk_allows() {
        let guard = ConfirmGuard::new(|name| match name {
            "read_file" => RiskLevel::Low,
            _ => RiskLevel::High,
        });
        let input = serde_json::json!({"path": "/tmp/test"});
        let decision = guard.check("read_file", &input).await;
        assert!(matches!(decision, Decision::Allow));
    }

    #[tokio::test]
    async fn test_confirm_guard_high_risk_needs_confirm() {
        let guard = ConfirmGuard::new(|name| match name {
            "shell" => RiskLevel::High,
            _ => RiskLevel::Low,
        });
        let input = serde_json::json!({"command": "rm -rf /"});
        let decision = guard.check("shell", &input).await;
        assert!(matches!(decision, Decision::NeedConfirm));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p agent-core`
Expected: FAIL — Guard, AutoGuard, ConfirmGuard, Decision not defined

- [ ] **Step 3: Implement guard.rs**

`crates/agent-core/src/guard.rs` (above the `#[cfg(test)]` block):

```rust
use crate::tool::RiskLevel;

#[derive(Debug, Clone)]
pub enum Decision {
    Allow,
    Deny(String),
    NeedConfirm,
}

#[async_trait::async_trait]
pub trait Guard: Send + Sync {
    async fn check(&self, tool_name: &str, input: &serde_json::Value) -> Decision;
}

/// A guard that always allows execution. Suitable for fully autonomous mode.
pub struct AutoGuard;

#[async_trait::async_trait]
impl Guard for AutoGuard {
    async fn check(&self, _tool_name: &str, _input: &serde_json::Value) -> Decision {
        Decision::Allow
    }
}

/// A guard that requires confirmation for high-risk tools.
/// Takes a closure that maps tool names to their risk levels.
pub struct ConfirmGuard<F: Fn(&str) -> RiskLevel + Send + Sync> {
    risk_fn: F,
}

impl<F: Fn(&str) -> RiskLevel + Send + Sync> ConfirmGuard<F> {
    pub fn new(risk_fn: F) -> Self {
        Self { risk_fn }
    }
}

#[async_trait::async_trait]
impl<F: Fn(&str) -> RiskLevel + Send + Sync> Guard for ConfirmGuard<F> {
    async fn check(&self, tool_name: &str, _input: &serde_json::Value) -> Decision {
        match (self.risk_fn)(tool_name) {
            RiskLevel::Low => Decision::Allow,
            RiskLevel::Medium | RiskLevel::High => Decision::NeedConfirm,
        }
    }
}
```

- [ ] **Step 4: Update lib.rs**

```rust
pub mod error;
pub mod event;
pub mod guard;
pub mod message;
pub mod model;
pub mod storage;
pub mod tool;

pub use error::Error;
pub use event::{Event, Response, StreamResponse, Usage};
pub use guard::{AutoGuard, ConfirmGuard, Decision, Guard};
pub use message::{Message, ToolCall};
pub use model::{Model, Request, ToolDefinition};
pub use storage::{MemoryStorage, Storage};
pub use tool::{RiskLevel, Tool, ToolOutput};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p agent-core`
Expected: all tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/agent-core/
git commit -m "add Guard trait, Decision enum, and AutoGuard"
```

---

## Task 8: Handler Trait + AgentEvent

**Files:**
- Create: `crates/agent-core/src/handler.rs`
- Modify: `crates/agent-core/src/lib.rs`

- [ ] **Step 1: Implement handler.rs**

The Handler trait is consumed by Agent (Task 9). We define it here and test it as part of the Agent tests.

`crates/agent-core/src/handler.rs`:

```rust
use crate::event::Usage;
use crate::tool::ToolOutput;

/// Events emitted by the Agent during execution.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// LLM text delta, for real-time rendering.
    TextDelta(String),

    /// A tool call has been identified and is about to execute.
    ToolCallBegin {
        id: String,
        name: String,
        arguments: String,
    },

    /// A tool has finished executing.
    ToolCallEnd { id: String, output: ToolOutput },

    /// Guard denied a tool call.
    ToolCallDenied {
        id: String,
        name: String,
        reason: String,
    },

    /// Agent turn complete.
    TurnComplete { usage: Usage },
}

/// Handler receives agent events and handles user confirmations.
///
/// Implement this trait in the CLI layer to render events and prompt for confirmation.
#[async_trait::async_trait]
pub trait Handler: Send + Sync {
    /// Called for each event during agent execution.
    async fn on_event(&self, event: AgentEvent);

    /// Called when guard returns NeedConfirm. Return true to allow, false to deny.
    async fn confirm(&self, tool_name: &str, input: &serde_json::Value) -> bool;
}
```

- [ ] **Step 2: Update lib.rs**

```rust
pub mod error;
pub mod event;
pub mod guard;
pub mod handler;
pub mod message;
pub mod model;
pub mod storage;
pub mod tool;

pub use error::Error;
pub use event::{Event, Response, StreamResponse, Usage};
pub use guard::{AutoGuard, Decision, Guard};
pub use handler::{AgentEvent, Handler};
pub use message::{Message, ToolCall};
pub use model::{Model, Request, ToolDefinition};
pub use storage::{MemoryStorage, Storage};
pub use tool::{RiskLevel, Tool, ToolOutput};
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check -p agent-core`
Expected: compiles with no errors

- [ ] **Step 4: Commit**

```bash
git add crates/agent-core/
git commit -m "add Handler trait and AgentEvent for agent-to-UI communication"
```

---

## Task 9: Agent Struct + Run Loop

**Files:**
- Create: `crates/agent-core/src/agent.rs`
- Modify: `crates/agent-core/src/lib.rs`

This is the most complex task. The Agent loop is the core of the system.

- [ ] **Step 1: Write tests for Agent with mocks**

Add to bottom of `crates/agent-core/src/agent.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, StreamResponse, Usage};
    use crate::guard::AutoGuard;
    use crate::handler::AgentEvent;
    use crate::storage::MemoryStorage;
    use crate::tool::{RiskLevel, ToolOutput};
    use futures::stream;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    /// A mock model that returns a fixed response.
    struct MockModel {
        responses: Arc<Mutex<Vec<Vec<Event>>>>,
    }

    impl MockModel {
        fn new(responses: Vec<Vec<Event>>) -> Self {
            Self {
                responses: Arc::new(Mutex::new(responses)),
            }
        }
    }

    #[async_trait::async_trait]
    impl Model for MockModel {
        async fn stream(&self, _request: Request) -> Result<StreamResponse, crate::Error> {
            let mut responses = self.responses.lock().await;
            if responses.is_empty() {
                return Err(crate::Error::Model("no more responses".into()));
            }
            let events = responses.remove(0);
            Ok(StreamResponse::new(stream::iter(events)))
        }
    }

    /// A mock tool that returns "output: {input}".
    struct MockTool;

    #[async_trait::async_trait]
    impl Tool for MockTool {
        fn name(&self) -> &str {
            "mock_tool"
        }
        fn description(&self) -> &str {
            "A mock tool"
        }
        fn schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object", "properties": {"input": {"type": "string"}}})
        }
        fn risk_level(&self) -> RiskLevel {
            RiskLevel::Low
        }
        async fn call(&self, input: serde_json::Value) -> Result<ToolOutput, crate::Error> {
            let text = input["input"].as_str().unwrap_or("none");
            Ok(ToolOutput::Text(format!("output: {text}")))
        }
    }

    /// A handler that collects events.
    struct CollectingHandler {
        events: Arc<Mutex<Vec<AgentEvent>>>,
    }

    impl CollectingHandler {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }

        async fn events(&self) -> Vec<AgentEvent> {
            self.events.lock().await.clone()
        }
    }

    #[async_trait::async_trait]
    impl Handler for CollectingHandler {
        async fn on_event(&self, event: AgentEvent) {
            self.events.lock().await.push(event);
        }
        async fn confirm(&self, _tool_name: &str, _input: &serde_json::Value) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn test_agent_simple_text_response() {
        // Model returns plain text, no tool calls → single turn
        let model = MockModel::new(vec![vec![
            Event::TextDelta("Hello!".into()),
            Event::Done {
                usage: Usage::default(),
            },
        ]]);
        let handler = CollectingHandler::new();

        let mut agent = Agent::builder()
            .model(model)
            .guard(AutoGuard)
            .storage(MemoryStorage::new())
            .system_prompt("You are helpful.")
            .build();

        agent.run("Hi", &handler).await.unwrap();

        let events = handler.events().await;
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::TextDelta(t) if t == "Hello!")));
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::TurnComplete { .. })));
    }

    #[tokio::test]
    async fn test_agent_tool_call_loop() {
        // Turn 1: model requests a tool call
        // Turn 2: model returns final text after receiving tool result
        let model = MockModel::new(vec![
            // First response: tool call
            vec![
                Event::ToolCallBegin {
                    id: "call_1".into(),
                    name: "mock_tool".into(),
                },
                Event::ToolCallDelta {
                    id: "call_1".into(),
                    arguments_delta: r#"{"input":"test"}"#.into(),
                },
                Event::ToolCallEnd {
                    id: "call_1".into(),
                },
                Event::Done {
                    usage: Usage::default(),
                },
            ],
            // Second response: final text
            vec![
                Event::TextDelta("Done!".into()),
                Event::Done {
                    usage: Usage::default(),
                },
            ],
        ]);
        let handler = CollectingHandler::new();

        let mut agent = Agent::builder()
            .model(model)
            .guard(AutoGuard)
            .storage(MemoryStorage::new())
            .system_prompt("You are helpful.")
            .tool(MockTool)
            .build();

        agent.run("Do something", &handler).await.unwrap();

        let events = handler.events().await;
        // Should have: ToolCallBegin, ToolCallEnd, TextDelta("Done!"), TurnComplete
        assert!(events.iter().any(|e| matches!(e,
            AgentEvent::ToolCallBegin { name, .. } if name == "mock_tool"
        )));
        assert!(events.iter().any(|e| matches!(e,
            AgentEvent::ToolCallEnd { output: ToolOutput::Text(s), .. } if s == "output: test"
        )));
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::TextDelta(t) if t == "Done!")));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p agent-core`
Expected: FAIL — Agent not defined

- [ ] **Step 3: Implement agent.rs — AgentBuilder**

`crates/agent-core/src/agent.rs` (above the `#[cfg(test)]` block):

```rust
use std::collections::HashMap;

use futures::StreamExt;

use crate::error::Error;
use crate::event::{Event, Usage};
use crate::guard::{Decision, Guard};
use crate::handler::{AgentEvent, Handler};
use crate::message::Message;
use crate::model::{Model, Request, ToolDefinition};
use crate::storage::Storage;
use crate::tool::Tool;

pub struct Agent<M, G, S> {
    model: M,
    guard: G,
    storage: S,
    tools: HashMap<String, Box<dyn Tool>>,
    system_prompt: String,
    messages: Vec<Message>,
}

pub struct AgentBuilder<M, G, S> {
    model: Option<M>,
    guard: Option<G>,
    storage: Option<S>,
    tools: HashMap<String, Box<dyn Tool>>,
    system_prompt: String,
}

impl<M: Model, G: Guard, S: Storage> AgentBuilder<M, G, S> {
    pub fn model(mut self, model: M) -> Self {
        self.model = Some(model);
        self
    }

    pub fn guard(mut self, guard: G) -> Self {
        self.guard = Some(guard);
        self
    }

    pub fn storage(mut self, storage: S) -> Self {
        self.storage = Some(storage);
        self
    }

    pub fn tool(mut self, tool: impl Tool + 'static) -> Self {
        self.tools.insert(tool.name().to_string(), Box::new(tool));
        self
    }

    pub fn system_prompt(mut self, prompt: &str) -> Self {
        self.system_prompt = prompt.to_string();
        self
    }

    pub fn build(self) -> Agent<M, G, S> {
        Agent {
            model: self.model.expect("model is required"),
            guard: self.guard.expect("guard is required"),
            storage: self.storage.expect("storage is required"),
            tools: self.tools,
            system_prompt: self.system_prompt,
            messages: Vec::new(),
        }
    }
}

impl<M: Model, G: Guard, S: Storage> Agent<M, G, S> {
    pub fn builder() -> AgentBuilder<M, G, S> {
        AgentBuilder {
            model: None,
            guard: None,
            storage: None,
            tools: HashMap::new(),
            system_prompt: String::new(),
        }
    }

    /// Run one turn of the agent loop: process user input until the model
    /// produces a final text response with no tool calls.
    pub async fn run(&mut self, input: &str, handler: &dyn Handler) -> Result<(), Error> {
        // Add user message
        self.messages.push(Message::User {
            content: input.to_string(),
        });

        // Build tool definitions from registered tools
        let tool_defs: Vec<ToolDefinition> = self
            .tools
            .values()
            .map(|t| ToolDefinition {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.schema(),
            })
            .collect();

        loop {
            let request = Request {
                system: if self.system_prompt.is_empty() {
                    None
                } else {
                    Some(self.system_prompt.clone())
                },
                messages: self.messages.clone(),
                tools: tool_defs.clone(),
                temperature: None,
                max_tokens: None,
            };

            // Stream from model
            let mut stream = self.model.stream(request).await?;
            let mut text = String::new();
            let mut tool_calls: Vec<crate::message::ToolCall> = Vec::new();
            let mut pending: HashMap<String, (String, String)> = HashMap::new(); // id -> (name, args)
            let mut usage = Usage::default();

            while let Some(event) = stream.next().await {
                match event {
                    Event::TextDelta(delta) => {
                        handler.on_event(AgentEvent::TextDelta(delta.clone())).await;
                        text.push_str(&delta);
                    }
                    Event::ToolCallBegin { id, name } => {
                        pending.insert(id.clone(), (name.clone(), String::new()));
                    }
                    Event::ToolCallDelta {
                        id,
                        arguments_delta,
                    } => {
                        if let Some((_, args)) = pending.get_mut(&id) {
                            args.push_str(&arguments_delta);
                        }
                    }
                    Event::ToolCallEnd { id } => {
                        if let Some((name, arguments)) = pending.remove(&id) {
                            tool_calls.push(crate::message::ToolCall {
                                id: id.clone(),
                                name,
                                arguments,
                            });
                        }
                    }
                    Event::Done { usage: u } => {
                        usage = u;
                    }
                }
            }

            // Add assistant message to history
            self.messages.push(Message::Assistant {
                text: if text.is_empty() { None } else { Some(text) },
                tool_calls: tool_calls.clone(),
            });

            // If no tool calls, the turn is complete
            if tool_calls.is_empty() {
                handler
                    .on_event(AgentEvent::TurnComplete { usage })
                    .await;
                return Ok(());
            }

            // Execute tool calls
            for call in &tool_calls {
                let input: serde_json::Value =
                    serde_json::from_str(&call.arguments).unwrap_or(serde_json::Value::Null);

                // Check guard
                let decision = self.guard.check(&call.name, &input).await;
                match decision {
                    Decision::Allow => {}
                    Decision::Deny(reason) => {
                        handler
                            .on_event(AgentEvent::ToolCallDenied {
                                id: call.id.clone(),
                                name: call.name.clone(),
                                reason: reason.clone(),
                            })
                            .await;
                        self.messages.push(Message::Tool {
                            tool_call_id: call.id.clone(),
                            content: format!("Tool call denied: {reason}"),
                        });
                        continue;
                    }
                    Decision::NeedConfirm => {
                        let allowed = handler.confirm(&call.name, &input).await;
                        if !allowed {
                            handler
                                .on_event(AgentEvent::ToolCallDenied {
                                    id: call.id.clone(),
                                    name: call.name.clone(),
                                    reason: "User denied".into(),
                                })
                                .await;
                            self.messages.push(Message::Tool {
                                tool_call_id: call.id.clone(),
                                content: "Tool call denied by user.".into(),
                            });
                            continue;
                        }
                    }
                }

                // Execute tool
                handler
                    .on_event(AgentEvent::ToolCallBegin {
                        id: call.id.clone(),
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                    })
                    .await;

                let tool_result = match self.tools.get(&call.name) {
                    Some(tool) => match tool.call(input).await {
                        Ok(output) => output,
                        Err(e) => crate::tool::ToolOutput::Error(e.to_string()),
                    },
                    None => {
                        crate::tool::ToolOutput::Error(format!("Unknown tool: {}", call.name))
                    }
                };

                handler
                    .on_event(AgentEvent::ToolCallEnd {
                        id: call.id.clone(),
                        output: tool_result.clone(),
                    })
                    .await;

                self.messages.push(Message::Tool {
                    tool_call_id: call.id.clone(),
                    content: tool_result.to_string(),
                });
            }

            // Loop: send tool results back to model
        }
    }
}
```

- [ ] **Step 4: Update lib.rs**

```rust
pub mod agent;
pub mod error;
pub mod event;
pub mod guard;
pub mod handler;
pub mod message;
pub mod model;
pub mod storage;
pub mod tool;

pub use agent::Agent;
pub use error::Error;
pub use event::{Event, Response, StreamResponse, Usage};
pub use guard::{AutoGuard, Decision, Guard};
pub use handler::{AgentEvent, Handler};
pub use message::{Message, ToolCall};
pub use model::{Model, Request, ToolDefinition};
pub use storage::{MemoryStorage, Storage};
pub use tool::{RiskLevel, Tool, ToolOutput};
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p agent-core`
Expected: all tests PASS, including `test_agent_simple_text_response` and `test_agent_tool_call_loop`

- [ ] **Step 6: Commit**

```bash
git add crates/agent-core/
git commit -m "add Agent struct with builder and streaming tool-call loop"
```

---

## Task 10: OpenAI Model Implementation

**Files:**
- Create: `crates/agent-core/src/openai.rs`
- Modify: `crates/agent-core/src/lib.rs`

This task implements the `Model` trait using `async-openai`. Feature-gated behind `openai`.

- [ ] **Step 1: Implement openai.rs**

`crates/agent-core/src/openai.rs`:

```rust
use async_openai::config::OpenAIConfig;
use async_openai::types::{
    ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs,
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestToolMessage, ChatCompletionRequestUserMessage,
    ChatCompletionRequestUserMessageContent, ChatCompletionToolArgs, ChatCompletionToolType,
    CreateChatCompletionRequestArgs, FunctionCall, FunctionObjectArgs,
};
use async_openai::Client;
use futures::StreamExt;

use crate::error::Error;
use crate::event::{Event, StreamResponse, Usage};
use crate::message::Message;
use crate::model::{Model, Request, ToolDefinition};

pub struct OpenAIModel {
    client: Client<OpenAIConfig>,
    model_id: String,
}

impl OpenAIModel {
    /// Create a new OpenAIModel. Reads OPENAI_API_KEY from environment by default.
    pub fn new(model_id: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            model_id: model_id.into(),
        }
    }

    /// Create with a custom config (e.g., custom base URL for compatible providers).
    pub fn with_config(model_id: impl Into<String>, config: OpenAIConfig) -> Self {
        Self {
            client: Client::with_config(config),
            model_id: model_id.into(),
        }
    }
}

fn convert_messages(messages: &[Message]) -> Vec<ChatCompletionRequestMessage> {
    messages
        .iter()
        .map(|m| match m {
            Message::System { content } => ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessage {
                    content: content.clone().into(),
                    name: None,
                },
            ),
            Message::User { content } => {
                ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                    content: ChatCompletionRequestUserMessageContent::Text(content.clone()),
                    name: None,
                })
            }
            Message::Assistant { text, tool_calls } => {
                let mut builder = ChatCompletionRequestAssistantMessageArgs::default();
                if let Some(t) = text {
                    builder.content(t.clone());
                }
                if !tool_calls.is_empty() {
                    let tc: Vec<ChatCompletionMessageToolCall> = tool_calls
                        .iter()
                        .map(|tc| ChatCompletionMessageToolCall {
                            id: tc.id.clone(),
                            r#type: ChatCompletionToolType::Function,
                            function: FunctionCall {
                                name: tc.name.clone(),
                                arguments: tc.arguments.clone(),
                            },
                        })
                        .collect();
                    builder.tool_calls(tc);
                }
                ChatCompletionRequestMessage::Assistant(builder.build().unwrap())
            }
            Message::Tool {
                tool_call_id,
                content,
            } => ChatCompletionRequestMessage::Tool(ChatCompletionRequestToolMessage {
                content: content.clone().into(),
                tool_call_id: tool_call_id.clone(),
            }),
        })
        .collect()
}

fn convert_tools(
    tools: &[ToolDefinition],
) -> Result<Vec<async_openai::types::ChatCompletionTool>, Error> {
    tools
        .iter()
        .map(|t| {
            ChatCompletionToolArgs::default()
                .function(
                    FunctionObjectArgs::default()
                        .name(&t.name)
                        .description(&t.description)
                        .parameters(t.parameters.clone())
                        .build()
                        .map_err(|e| Error::Model(e.to_string()))?,
                )
                .build()
                .map_err(|e| Error::Model(e.to_string()))
        })
        .collect()
}

#[async_trait::async_trait]
impl Model for OpenAIModel {
    async fn stream(&self, request: Request) -> Result<StreamResponse, Error> {
        let mut messages = Vec::new();

        // Add system message if present
        if let Some(system) = &request.system {
            messages.push(ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessage {
                    content: system.clone().into(),
                    name: None,
                },
            ));
        }

        messages.extend(convert_messages(&request.messages));

        let mut builder = CreateChatCompletionRequestArgs::default();
        builder.model(&self.model_id).messages(messages);

        if !request.tools.is_empty() {
            builder.tools(convert_tools(&request.tools)?);
        }
        if let Some(temp) = request.temperature {
            builder.temperature(temp);
        }
        if let Some(max) = request.max_tokens {
            builder.max_tokens(max as u32);
        }

        let openai_request = builder.build().map_err(|e| Error::Model(e.to_string()))?;

        let mut openai_stream = self
            .client
            .chat()
            .create_stream(openai_request)
            .await
            .map_err(|e| Error::Model(e.to_string()))?;

        // Convert async-openai stream to our Event stream
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Event>();

        tokio::spawn(async move {
            while let Some(result) = openai_stream.next().await {
                match result {
                    Ok(chunk) => {
                        for choice in &chunk.choices {
                            // Text delta
                            if let Some(content) = &choice.delta.content {
                                if !content.is_empty() {
                                    let _ = tx.send(Event::TextDelta(content.clone()));
                                }
                            }

                            // Tool call chunks
                            if let Some(tool_calls) = &choice.delta.tool_calls {
                                for tc_chunk in tool_calls {
                                    if let Some(ref func) = tc_chunk.function {
                                        // Begin: when id is present, it's a new tool call
                                        if let Some(ref id) = tc_chunk.id {
                                            let name =
                                                func.name.clone().unwrap_or_default();
                                            let _ = tx.send(Event::ToolCallBegin {
                                                id: id.clone(),
                                                name,
                                            });
                                        }

                                        // Delta: when arguments are present
                                        if let Some(ref args) = func.arguments {
                                            if !args.is_empty() {
                                                // We need the id; reconstruct from index
                                                if let Some(ref id) = tc_chunk.id {
                                                    let _ = tx.send(Event::ToolCallDelta {
                                                        id: id.clone(),
                                                        arguments_delta: args.clone(),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Finish reason: ToolCalls means all tool calls are complete
                            if let Some(ref reason) = choice.finish_reason {
                                match reason {
                                    async_openai::types::FinishReason::ToolCalls => {
                                        // Emit ToolCallEnd for all pending tool calls
                                        // The agent loop handles this based on accumulated state
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Event::TextDelta(format!("[Error: {e}]")));
                    }
                }
            }

            // Extract usage from the last chunk if available
            let _ = tx.send(Event::Done {
                usage: Usage::default(),
            });
        });

        let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
        Ok(StreamResponse::new(stream))
    }
}
```

- [ ] **Step 2: Add tokio-stream dependency**

Add to `crates/agent-core/Cargo.toml` under `[dependencies]`:

```toml
tokio-stream = { version = "0.1", optional = true }
```

Update the `[features]` section:

```toml
[features]
default = ["openai"]
openai = ["dep:async-openai", "dep:tokio-stream"]
```

- [ ] **Step 3: Update lib.rs to include openai module**

Add after existing module declarations:

```rust
#[cfg(feature = "openai")]
pub mod openai;

// In the pub use section:
#[cfg(feature = "openai")]
pub use openai::OpenAIModel;
```

- [ ] **Step 4: Verify it compiles**

Run: `cargo check -p agent-core`
Expected: compiles with no errors

Note: Full integration testing of OpenAIModel requires an API key and is done manually or in CI with secrets. The mock-based Agent tests in Task 9 cover the agent loop logic.

- [ ] **Step 5: Commit**

```bash
git add crates/agent-core/
git commit -m "add OpenAI Model implementation with streaming support"
```

---

## Task 11: Shell Tool

**Files:**
- Create: `crates/agent-tools/src/shell.rs`

- [ ] **Step 1: Write tests for ShellTool**

Add to bottom of `crates/agent-tools/src/shell.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::Tool;

    #[tokio::test]
    async fn test_shell_echo() {
        let tool = ShellTool;
        let input = serde_json::json!({"command": "echo hello"});
        let output = tool.call(input).await.unwrap();
        match output {
            ToolOutput::Text(s) => assert!(s.contains("hello")),
            _ => panic!("expected text output"),
        }
    }

    #[tokio::test]
    async fn test_shell_failing_command() {
        let tool = ShellTool;
        let input = serde_json::json!({"command": "false"});
        let output = tool.call(input).await.unwrap();
        // `false` exits with code 1, tool should still return output (with exit code info)
        match output {
            ToolOutput::Text(s) => assert!(s.contains("exit code")),
            ToolOutput::Error(s) => assert!(s.contains("exit code")),
        }
    }

    #[test]
    fn test_shell_metadata() {
        let tool = ShellTool;
        assert_eq!(tool.name(), "shell");
        assert_eq!(tool.risk_level(), RiskLevel::High);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p agent-tools`
Expected: FAIL — ShellTool not defined

- [ ] **Step 3: Implement ShellTool**

`crates/agent-tools/src/shell.rs` (above the `#[cfg(test)]` block):

```rust
use agent_core::tool::{RiskLevel, ToolOutput};
use agent_core::Error;

pub struct ShellTool;

#[async_trait::async_trait]
impl agent_core::Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Execute a shell command and return its output (stdout and stderr)."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                }
            },
            "required": ["command"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::High
    }

    async fn call(&self, input: serde_json::Value) -> Result<ToolOutput, Error> {
        let command = input["command"]
            .as_str()
            .ok_or_else(|| Error::Tool("missing 'command' field".into()))?;

        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .await
            .map_err(|e| Error::Tool(format!("failed to execute command: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let result = if output.status.success() {
            if stderr.is_empty() {
                stdout.to_string()
            } else {
                format!("{stdout}\n[stderr]\n{stderr}")
            }
        } else {
            let code = output.status.code().unwrap_or(-1);
            format!("[exit code: {code}]\n{stdout}\n[stderr]\n{stderr}")
        };

        Ok(ToolOutput::Text(result.trim().to_string()))
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p agent-tools`
Expected: all shell tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/agent-tools/
git commit -m "add ShellTool for executing shell commands"
```

---

## Task 12: ReadFile + WriteFile Tools

**Files:**
- Create: `crates/agent-tools/src/read_file.rs`
- Create: `crates/agent-tools/src/write_file.rs`

- [ ] **Step 1: Write tests for ReadFileTool and WriteFileTool**

Add to bottom of `crates/agent-tools/src/read_file.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::Tool;
    use std::io::Write;

    #[tokio::test]
    async fn test_read_file() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp, "hello world").unwrap();
        let path = tmp.path().to_str().unwrap();

        let tool = ReadFileTool;
        let input = serde_json::json!({"path": path});
        let output = tool.call(input).await.unwrap();
        match output {
            ToolOutput::Text(s) => assert_eq!(s, "hello world"),
            _ => panic!("expected text output"),
        }
    }

    #[tokio::test]
    async fn test_read_file_not_found() {
        let tool = ReadFileTool;
        let input = serde_json::json!({"path": "/nonexistent/file.txt"});
        let result = tool.call(input).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_read_file_metadata() {
        let tool = ReadFileTool;
        assert_eq!(tool.name(), "read_file");
        assert_eq!(tool.risk_level(), RiskLevel::Low);
    }
}
```

Add to bottom of `crates/agent-tools/src/write_file.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::Tool;

    #[tokio::test]
    async fn test_write_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        let path_str = path.to_str().unwrap();

        let tool = WriteFileTool;
        let input = serde_json::json!({"path": path_str, "content": "hello"});
        let output = tool.call(input).await.unwrap();
        assert!(matches!(output, ToolOutput::Text(_)));

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "hello");
    }

    #[test]
    fn test_write_file_metadata() {
        let tool = WriteFileTool;
        assert_eq!(tool.name(), "write_file");
        assert_eq!(tool.risk_level(), RiskLevel::Medium);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p agent-tools`
Expected: FAIL — ReadFileTool, WriteFileTool not defined

- [ ] **Step 3: Implement ReadFileTool**

`crates/agent-tools/src/read_file.rs` (above the `#[cfg(test)]` block):

```rust
use agent_core::tool::{RiskLevel, ToolOutput};
use agent_core::Error;

pub struct ReadFileTool;

#[async_trait::async_trait]
impl agent_core::Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file at the given path."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to read"
                }
            },
            "required": ["path"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Low
    }

    async fn call(&self, input: serde_json::Value) -> Result<ToolOutput, Error> {
        let path = input["path"]
            .as_str()
            .ok_or_else(|| Error::Tool("missing 'path' field".into()))?;

        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| Error::Tool(format!("failed to read file '{path}': {e}")))?;

        Ok(ToolOutput::Text(content))
    }
}
```

- [ ] **Step 4: Implement WriteFileTool**

`crates/agent-tools/src/write_file.rs` (above the `#[cfg(test)]` block):

```rust
use agent_core::tool::{RiskLevel, ToolOutput};
use agent_core::Error;

pub struct WriteFileTool;

#[async_trait::async_trait]
impl agent_core::Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file at the given path. Creates the file if it doesn't exist, overwrites if it does."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Medium
    }

    async fn call(&self, input: serde_json::Value) -> Result<ToolOutput, Error> {
        let path = input["path"]
            .as_str()
            .ok_or_else(|| Error::Tool("missing 'path' field".into()))?;
        let content = input["content"]
            .as_str()
            .ok_or_else(|| Error::Tool("missing 'content' field".into()))?;

        // Create parent directories if needed
        if let Some(parent) = std::path::Path::new(path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::Tool(format!("failed to create directories: {e}")))?;
        }

        tokio::fs::write(path, content)
            .await
            .map_err(|e| Error::Tool(format!("failed to write file '{path}': {e}")))?;

        Ok(ToolOutput::Text(format!("Successfully wrote to {path}")))
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p agent-tools`
Expected: all file tool tests PASS

- [ ] **Step 6: Commit**

```bash
git add crates/agent-tools/
git commit -m "add ReadFileTool and WriteFileTool"
```

---

## Task 13: EditFile Tool

**Files:**
- Create: `crates/agent-tools/src/edit_file.rs`

- [ ] **Step 1: Write tests for EditFileTool**

Add to bottom of `crates/agent-tools/src/edit_file.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::Tool;

    #[tokio::test]
    async fn test_edit_file_replace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "hello world\nfoo bar\n").unwrap();
        let path_str = path.to_str().unwrap();

        let tool = EditFileTool;
        let input = serde_json::json!({
            "path": path_str,
            "old_string": "foo bar",
            "new_string": "baz qux"
        });
        let output = tool.call(input).await.unwrap();
        assert!(matches!(output, ToolOutput::Text(_)));

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "hello world\nbaz qux\n");
    }

    #[tokio::test]
    async fn test_edit_file_old_string_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, "hello world").unwrap();
        let path_str = path.to_str().unwrap();

        let tool = EditFileTool;
        let input = serde_json::json!({
            "path": path_str,
            "old_string": "nonexistent",
            "new_string": "replacement"
        });
        let result = tool.call(input).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_edit_file_metadata() {
        let tool = EditFileTool;
        assert_eq!(tool.name(), "edit_file");
        assert_eq!(tool.risk_level(), RiskLevel::Medium);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p agent-tools`
Expected: FAIL — EditFileTool not defined

- [ ] **Step 3: Implement EditFileTool**

`crates/agent-tools/src/edit_file.rs` (above the `#[cfg(test)]` block):

```rust
use agent_core::tool::{RiskLevel, ToolOutput};
use agent_core::Error;

pub struct EditFileTool;

#[async_trait::async_trait]
impl agent_core::Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "Edit a file by replacing an exact string match with new content. The old_string must appear exactly once in the file."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute path to the file to edit"
                },
                "old_string": {
                    "type": "string",
                    "description": "The exact string to find and replace (must be unique in the file)"
                },
                "new_string": {
                    "type": "string",
                    "description": "The replacement string"
                }
            },
            "required": ["path", "old_string", "new_string"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Medium
    }

    async fn call(&self, input: serde_json::Value) -> Result<ToolOutput, Error> {
        let path = input["path"]
            .as_str()
            .ok_or_else(|| Error::Tool("missing 'path' field".into()))?;
        let old_string = input["old_string"]
            .as_str()
            .ok_or_else(|| Error::Tool("missing 'old_string' field".into()))?;
        let new_string = input["new_string"]
            .as_str()
            .ok_or_else(|| Error::Tool("missing 'new_string' field".into()))?;

        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| Error::Tool(format!("failed to read file '{path}': {e}")))?;

        let count = content.matches(old_string).count();
        if count == 0 {
            return Err(Error::Tool(format!(
                "old_string not found in '{path}'"
            )));
        }
        if count > 1 {
            return Err(Error::Tool(format!(
                "old_string found {count} times in '{path}', must be unique"
            )));
        }

        let new_content = content.replacen(old_string, new_string, 1);
        tokio::fs::write(path, &new_content)
            .await
            .map_err(|e| Error::Tool(format!("failed to write file '{path}': {e}")))?;

        Ok(ToolOutput::Text(format!("Successfully edited {path}")))
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p agent-tools`
Expected: all tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/agent-tools/
git commit -m "add EditFileTool with exact-match string replacement"
```

---

## Task 14: Glob + Grep Tools

**Files:**
- Create: `crates/agent-tools/src/glob.rs`
- Create: `crates/agent-tools/src/grep.rs`

- [ ] **Step 1: Write tests for GlobTool**

Add to bottom of `crates/agent-tools/src/glob.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::Tool;

    #[tokio::test]
    async fn test_glob_find_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("foo.txt"), "").unwrap();
        std::fs::write(dir.path().join("bar.txt"), "").unwrap();
        std::fs::write(dir.path().join("baz.rs"), "").unwrap();
        let pattern = format!("{}/*.txt", dir.path().display());

        let tool = GlobTool;
        let input = serde_json::json!({"pattern": pattern});
        let output = tool.call(input).await.unwrap();
        match output {
            ToolOutput::Text(s) => {
                assert!(s.contains("foo.txt"));
                assert!(s.contains("bar.txt"));
                assert!(!s.contains("baz.rs"));
            }
            _ => panic!("expected text output"),
        }
    }

    #[test]
    fn test_glob_metadata() {
        let tool = GlobTool;
        assert_eq!(tool.name(), "glob");
        assert_eq!(tool.risk_level(), RiskLevel::Low);
    }
}
```

- [ ] **Step 2: Write tests for GrepTool**

Add to bottom of `crates/agent-tools/src/grep.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::Tool;

    #[tokio::test]
    async fn test_grep_find_pattern() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello world\nfoo bar\n").unwrap();
        std::fs::write(dir.path().join("b.txt"), "no match here\n").unwrap();
        let path = dir.path().to_str().unwrap();

        let tool = GrepTool;
        let input = serde_json::json!({"pattern": "hello", "path": path});
        let output = tool.call(input).await.unwrap();
        match output {
            ToolOutput::Text(s) => {
                assert!(s.contains("a.txt"));
                assert!(s.contains("hello world"));
                assert!(!s.contains("b.txt"));
            }
            _ => panic!("expected text output"),
        }
    }

    #[test]
    fn test_grep_metadata() {
        let tool = GrepTool;
        assert_eq!(tool.name(), "grep");
        assert_eq!(tool.risk_level(), RiskLevel::Low);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p agent-tools`
Expected: FAIL — GlobTool, GrepTool not defined

- [ ] **Step 4: Implement GlobTool**

`crates/agent-tools/src/glob.rs` (above the `#[cfg(test)]` block):

```rust
use agent_core::tool::{RiskLevel, ToolOutput};
use agent_core::Error;

pub struct GlobTool;

#[async_trait::async_trait]
impl agent_core::Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files matching a glob pattern. Returns a newline-separated list of matching file paths."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern to match files (e.g., 'src/**/*.rs')"
                }
            },
            "required": ["pattern"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Low
    }

    async fn call(&self, input: serde_json::Value) -> Result<ToolOutput, Error> {
        let pattern = input["pattern"]
            .as_str()
            .ok_or_else(|| Error::Tool("missing 'pattern' field".into()))?;

        let paths: Vec<String> = glob::glob(pattern)
            .map_err(|e| Error::Tool(format!("invalid glob pattern: {e}")))?
            .filter_map(|entry| entry.ok())
            .map(|path| path.display().to_string())
            .collect();

        if paths.is_empty() {
            Ok(ToolOutput::Text("No files matched.".into()))
        } else {
            Ok(ToolOutput::Text(paths.join("\n")))
        }
    }
}
```

- [ ] **Step 5: Implement GrepTool**

Replace `grep-regex` and `grep-searcher` dependencies in `crates/agent-tools/Cargo.toml` with a simpler approach using `regex` and `tokio::fs`:

Update `crates/agent-tools/Cargo.toml` dependencies:

```toml
[dependencies]
agent-core = { path = "../agent-core" }
async-trait = "0.1"
glob = "0.3"
regex = "1"
reqwest = { version = "0.12", features = ["json"], optional = true }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["process", "fs"] }
walkdir = "2"
```

`crates/agent-tools/src/grep.rs` (above the `#[cfg(test)]` block):

```rust
use agent_core::tool::{RiskLevel, ToolOutput};
use agent_core::Error;
use regex::Regex;

pub struct GrepTool;

#[async_trait::async_trait]
impl agent_core::Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search for a regex pattern in files under a directory. Returns matching lines with file paths and line numbers."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file path to search in"
                }
            },
            "required": ["pattern", "path"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Low
    }

    async fn call(&self, input: serde_json::Value) -> Result<ToolOutput, Error> {
        let pattern = input["pattern"]
            .as_str()
            .ok_or_else(|| Error::Tool("missing 'pattern' field".into()))?;
        let path = input["path"]
            .as_str()
            .ok_or_else(|| Error::Tool("missing 'path' field".into()))?;

        let regex =
            Regex::new(pattern).map_err(|e| Error::Tool(format!("invalid regex: {e}")))?;

        let mut results = Vec::new();
        let path = std::path::Path::new(path);

        if path.is_file() {
            search_file(path, &regex, &mut results).await?;
        } else if path.is_dir() {
            for entry in walkdir::WalkDir::new(path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                search_file(entry.path(), &regex, &mut results).await?;
            }
        } else {
            return Err(Error::Tool(format!("path does not exist: {}", path.display())));
        }

        if results.is_empty() {
            Ok(ToolOutput::Text("No matches found.".into()))
        } else {
            Ok(ToolOutput::Text(results.join("\n")))
        }
    }
}

async fn search_file(
    path: &std::path::Path,
    regex: &Regex,
    results: &mut Vec<String>,
) -> Result<(), Error> {
    let content = match tokio::fs::read_to_string(path).await {
        Ok(c) => c,
        Err(_) => return Ok(()), // Skip files that can't be read (binary, permissions, etc.)
    };

    for (line_num, line) in content.lines().enumerate() {
        if regex.is_match(line) {
            results.push(format!("{}:{}:{}", path.display(), line_num + 1, line));
        }
    }

    Ok(())
}
```

- [ ] **Step 6: Update agent-tools lib.rs to export tools**

```rust
#[cfg(feature = "shell")]
pub mod shell;

#[cfg(feature = "file")]
pub mod read_file;

#[cfg(feature = "file")]
pub mod write_file;

#[cfg(feature = "file")]
pub mod edit_file;

#[cfg(feature = "search")]
pub mod glob;

#[cfg(feature = "search")]
pub mod grep;

#[cfg(feature = "shell")]
pub use shell::ShellTool;

#[cfg(feature = "file")]
pub use read_file::ReadFileTool;

#[cfg(feature = "file")]
pub use write_file::WriteFileTool;

#[cfg(feature = "file")]
pub use edit_file::EditFileTool;

#[cfg(feature = "search")]
pub use self::glob::GlobTool;

#[cfg(feature = "search")]
pub use grep::GrepTool;
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p agent-tools`
Expected: all tests PASS

- [ ] **Step 8: Commit**

```bash
git add crates/agent-tools/
git commit -m "add GlobTool and GrepTool for file search"
```

---

## Task 15: CLI Scaffolding + Config

**Files:**
- Create: `crates/agent-cli/src/config.rs`
- Modify: `crates/agent-cli/src/main.rs`

- [ ] **Step 1: Write tests for Config**

Add to bottom of `crates/agent-cli/src/config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = Config::default();
        assert_eq!(config.model.model_id, "gpt-4o");
        assert!(config.model.api_key.is_none());
        assert_eq!(config.guard.mode, GuardMode::Confirm);
    }

    #[test]
    fn test_config_from_toml() {
        let toml_str = r#"
[model]
model_id = "gpt-4o-mini"
api_base = "http://localhost:11434/v1"

[guard]
mode = "auto"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.model.model_id, "gpt-4o-mini");
        assert_eq!(
            config.model.api_base.as_deref(),
            Some("http://localhost:11434/v1")
        );
        assert_eq!(config.guard.mode, GuardMode::Auto);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p agent-cli`
Expected: FAIL — Config not defined

- [ ] **Step 3: Implement config.rs**

`crates/agent-cli/src/config.rs` (above the `#[cfg(test)]` block):

```rust
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub model: ModelConfig,
    #[serde(default)]
    pub guard: GuardConfig,
    #[serde(default)]
    pub system_prompt: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: ModelConfig::default(),
            guard: GuardConfig::default(),
            system_prompt: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    #[serde(default = "default_model_id")]
    pub model_id: String,
    pub api_key: Option<String>,
    pub api_base: Option<String>,
}

fn default_model_id() -> String {
    "gpt-4o".into()
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            model_id: default_model_id(),
            api_key: None,
            api_base: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardConfig {
    #[serde(default = "default_guard_mode")]
    pub mode: GuardMode,
}

fn default_guard_mode() -> GuardMode {
    GuardMode::Confirm
}

impl Default for GuardConfig {
    fn default() -> Self {
        Self {
            mode: default_guard_mode(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuardMode {
    Auto,
    Confirm,
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        if path.exists() {
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            toml::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }
}

fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".agent")
        .join("config.toml")
}
```

- [ ] **Step 4: Implement main.rs with clap**

`crates/agent-cli/src/main.rs`:

```rust
mod config;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "agent", about = "A general-purpose LLM agent")]
struct Cli {
    /// Model to use (overrides config)
    #[arg(short, long)]
    model: Option<String>,

    /// API base URL (overrides config)
    #[arg(long)]
    api_base: Option<String>,

    /// Run in auto mode (no confirmations)
    #[arg(long)]
    auto: bool,
}

fn main() {
    let cli = Cli::parse();
    let mut config = config::Config::load();

    // CLI args override config
    if let Some(model) = cli.model {
        config.model.model_id = model;
    }
    if let Some(api_base) = cli.api_base {
        config.model.api_base = Some(api_base);
    }
    if cli.auto {
        config.guard.mode = config::GuardMode::Auto;
    }

    println!("agent v{}", env!("CARGO_PKG_VERSION"));
    println!("model: {}", config.model.model_id);

    // TUI app will be wired in Task 16
}
```

- [ ] **Step 5: Run tests and verify compilation**

Run: `cargo test -p agent-cli && cargo check --workspace`
Expected: all tests PASS, workspace compiles

- [ ] **Step 6: Commit**

```bash
git add crates/agent-cli/
git commit -m "add CLI scaffolding with config loading and clap argument parsing"
```

---

## Task 16: TUI Layout + Rendering

**Files:**
- Create: `crates/agent-cli/src/ui.rs`
- Create: `crates/agent-cli/src/input.rs`
- Create: `crates/agent-cli/src/app.rs`

- [ ] **Step 1: Implement input.rs — input buffer handling**

`crates/agent-cli/src/input.rs`:

```rust
/// A simple text input buffer with cursor support.
pub struct InputBuffer {
    content: String,
    cursor: usize,
}

impl InputBuffer {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            cursor: 0,
        }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn insert(&mut self, ch: char) {
        self.content.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let prev = self.content[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.content.remove(prev);
            self.cursor = prev;
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.content[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.content.len() {
            self.cursor += self.content[self.cursor..].chars().next().map_or(0, |c| c.len_utf8());
        }
    }

    /// Take the content and reset the buffer.
    pub fn take(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.content)
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_buffer_basic() {
        let mut buf = InputBuffer::new();
        buf.insert('h');
        buf.insert('i');
        assert_eq!(buf.content(), "hi");
        assert_eq!(buf.cursor(), 2);
    }

    #[test]
    fn test_input_buffer_backspace() {
        let mut buf = InputBuffer::new();
        buf.insert('a');
        buf.insert('b');
        buf.backspace();
        assert_eq!(buf.content(), "a");
        assert_eq!(buf.cursor(), 1);
    }

    #[test]
    fn test_input_buffer_take() {
        let mut buf = InputBuffer::new();
        buf.insert('x');
        let taken = buf.take();
        assert_eq!(taken, "x");
        assert!(buf.is_empty());
        assert_eq!(buf.cursor(), 0);
    }

    #[test]
    fn test_input_buffer_cursor_movement() {
        let mut buf = InputBuffer::new();
        buf.insert('a');
        buf.insert('b');
        buf.insert('c');
        buf.move_left();
        buf.move_left();
        assert_eq!(buf.cursor(), 1);
        buf.insert('X');
        assert_eq!(buf.content(), "aXbc");
    }
}
```

- [ ] **Step 2: Implement ui.rs — ratatui rendering**

`crates/agent-cli/src/ui.rs`:

```rust
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),        // Chat area
            Constraint::Length(1),      // Status bar
            Constraint::Length(3),      // Input area
        ])
        .split(frame.area());

    draw_chat(frame, app, chunks[0]);
    draw_status_bar(frame, app, chunks[1]);
    draw_input(frame, app, chunks[2]);
}

fn draw_chat(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    for entry in &app.chat_history {
        match entry {
            ChatEntry::User(text) => {
                lines.push(Line::from(vec![
                    Span::styled("> ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                    Span::raw(text),
                ]));
            }
            ChatEntry::Assistant(text) => {
                lines.push(Line::from(Span::raw(text)));
            }
            ChatEntry::ToolCall { name, .. } => {
                lines.push(Line::from(Span::styled(
                    format!("⚙ {name}"),
                    Style::default().fg(Color::Yellow),
                )));
            }
            ChatEntry::ToolResult { output, .. } => {
                lines.push(Line::from(Span::styled(
                    output,
                    Style::default().fg(Color::DarkGray),
                )));
            }
            ChatEntry::Error(text) => {
                lines.push(Line::from(Span::styled(
                    text,
                    Style::default().fg(Color::Red),
                )));
            }
        }
        lines.push(Line::from(""));
    }

    // Append streaming text if any
    if !app.streaming_text.is_empty() {
        lines.push(Line::from(Span::raw(&app.streaming_text)));
    }

    let chat = Paragraph::new(lines)
        .block(Block::default().borders(Borders::NONE))
        .wrap(Wrap { trim: false })
        .scroll((app.scroll_offset, 0));

    frame.render_widget(chat, area);
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let status = if app.is_running {
        Span::styled(" ● thinking... ", Style::default().fg(Color::Yellow))
    } else {
        Span::styled(
            format!(" {} | tokens: {} ", app.model_id, app.total_tokens),
            Style::default().fg(Color::DarkGray),
        )
    };

    let bar = Paragraph::new(Line::from(status))
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(bar, area);
}

fn draw_input(frame: &mut Frame, app: &App, area: Rect) {
    let input_text = app.input.content();
    let input = Paragraph::new(input_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Input "),
        );
    frame.render_widget(input, area);

    // Position cursor
    let cursor_x = area.x + 1 + app.input.cursor() as u16;
    let cursor_y = area.y + 1;
    frame.set_cursor_position((cursor_x, cursor_y));
}

/// Entries in the chat display.
#[derive(Debug, Clone)]
pub enum ChatEntry {
    User(String),
    Assistant(String),
    ToolCall { name: String, arguments: String },
    ToolResult { name: String, output: String },
    Error(String),
}
```

- [ ] **Step 3: Implement app.rs — application state and event loop**

`crates/agent-cli/src/app.rs`:

```rust
use std::collections::HashMap;
use std::sync::Arc;

use agent_core::guard::{AutoGuard, Decision, Guard};
use agent_core::handler::{AgentEvent, Handler};
use agent_core::tool::RiskLevel;
use agent_core::{Agent, MemoryStorage};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::DefaultTerminal;
use tokio::sync::{mpsc, Mutex};

use crate::config::{Config, GuardMode};
use crate::input::InputBuffer;
use crate::ui::{self, ChatEntry};

pub struct App {
    pub input: InputBuffer,
    pub chat_history: Vec<ChatEntry>,
    pub streaming_text: String,
    pub scroll_offset: u16,
    pub is_running: bool,
    pub model_id: String,
    pub total_tokens: u32,
    pub should_quit: bool,
}

impl App {
    pub fn new(model_id: &str) -> Self {
        Self {
            input: InputBuffer::new(),
            chat_history: Vec::new(),
            streaming_text: String::new(),
            scroll_offset: 0,
            is_running: false,
            model_id: model_id.to_string(),
            total_tokens: 0,
            should_quit: false,
        }
    }

    pub fn run(config: Config) -> std::io::Result<()> {
        let mut terminal = ratatui::init();
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(Self::main_loop(&mut terminal, config));
        ratatui::restore();
        result
    }

    async fn main_loop(terminal: &mut DefaultTerminal, config: Config) -> std::io::Result<()> {
        let mut app = App::new(&config.model.model_id);

        // Build OpenAI model
        #[cfg(feature = "openai")]
        let model = {
            let mut openai_config = async_openai::config::OpenAIConfig::default();
            if let Some(ref base) = config.model.api_base {
                openai_config = openai_config.with_api_base(base);
            }
            if let Some(ref key) = config.model.api_key {
                openai_config = openai_config.with_api_key(key);
            }
            agent_core::OpenAIModel::with_config(&config.model.model_id, openai_config)
        };

        let guard = AutoGuard; // ConfirmGuard wired in Task 17

        let system_prompt = config.system_prompt.unwrap_or_else(|| {
            format!(
                "You are a helpful assistant. Current directory: {}. OS: {}.",
                std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                std::env::consts::OS,
            )
        });

        let mut agent = Agent::builder()
            .model(model)
            .guard(guard)
            .storage(MemoryStorage::new())
            .system_prompt(&system_prompt)
            .tool(agent_tools::ShellTool)
            .tool(agent_tools::ReadFileTool)
            .tool(agent_tools::WriteFileTool)
            .tool(agent_tools::EditFileTool)
            .tool(agent_tools::GlobTool)
            .tool(agent_tools::GrepTool)
            .build();

        // Channel for agent events → UI
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AgentEvent>();

        loop {
            terminal.draw(|frame| ui::draw(frame, &app))?;

            // Check for agent events (non-blocking)
            while let Ok(agent_event) = event_rx.try_recv() {
                handle_agent_event(&mut app, agent_event);
            }

            // Poll for terminal events with a short timeout
            if event::poll(std::time::Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                            app.should_quit = true;
                        }
                        (KeyCode::Enter, _) if !app.is_running && !app.input.is_empty() => {
                            let user_input = app.input.take();
                            app.chat_history
                                .push(ChatEntry::User(user_input.clone()));
                            app.is_running = true;
                            app.streaming_text.clear();

                            // Spawn agent task
                            let handler = TuiHandler {
                                tx: event_tx.clone(),
                            };
                            // Note: Agent run needs &mut self, so we use a channel-based
                            // approach. For initial implementation, run synchronously
                            // in a spawned task. This requires Agent to be Send.
                            // Full async integration will be refined during implementation.
                        }
                        (KeyCode::Char(c), _) if !app.is_running => {
                            app.input.insert(c);
                        }
                        (KeyCode::Backspace, _) if !app.is_running => {
                            app.input.backspace();
                        }
                        (KeyCode::Left, _) if !app.is_running => {
                            app.input.move_left();
                        }
                        (KeyCode::Right, _) if !app.is_running => {
                            app.input.move_right();
                        }
                        _ => {}
                    }
                }
            }

            if app.should_quit {
                break;
            }
        }

        Ok(())
    }
}

fn handle_agent_event(app: &mut App, event: AgentEvent) {
    match event {
        AgentEvent::TextDelta(text) => {
            app.streaming_text.push_str(&text);
        }
        AgentEvent::ToolCallBegin {
            name, arguments, ..
        } => {
            // Flush streaming text to history
            if !app.streaming_text.is_empty() {
                let text = std::mem::take(&mut app.streaming_text);
                app.chat_history.push(ChatEntry::Assistant(text));
            }
            app.chat_history.push(ChatEntry::ToolCall {
                name,
                arguments,
            });
        }
        AgentEvent::ToolCallEnd { output, .. } => {
            app.chat_history.push(ChatEntry::ToolResult {
                name: String::new(),
                output: output.to_string(),
            });
        }
        AgentEvent::ToolCallDenied { name, reason, .. } => {
            app.chat_history
                .push(ChatEntry::Error(format!("Denied {name}: {reason}")));
        }
        AgentEvent::TurnComplete { usage } => {
            // Flush streaming text
            if !app.streaming_text.is_empty() {
                let text = std::mem::take(&mut app.streaming_text);
                app.chat_history.push(ChatEntry::Assistant(text));
            }
            app.total_tokens += usage.total_tokens;
            app.is_running = false;
        }
    }
}

/// Handler that sends AgentEvents over a channel to the TUI.
struct TuiHandler {
    tx: mpsc::UnboundedSender<AgentEvent>,
}

#[async_trait::async_trait]
impl Handler for TuiHandler {
    async fn on_event(&self, event: AgentEvent) {
        let _ = self.tx.send(event);
    }

    async fn confirm(&self, _tool_name: &str, _input: &serde_json::Value) -> bool {
        // Initial version auto-confirms; TUI confirmation dialog wired in Task 17
        true
    }
}
```

- [ ] **Step 4: Update main.rs to wire everything together**

`crates/agent-cli/src/main.rs`:

```rust
mod app;
mod config;
mod input;
mod ui;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "agent", about = "A general-purpose LLM agent")]
struct Cli {
    /// Model to use (overrides config)
    #[arg(short, long)]
    model: Option<String>,

    /// API base URL (overrides config)
    #[arg(long)]
    api_base: Option<String>,

    /// Run in auto mode (no confirmations)
    #[arg(long)]
    auto: bool,
}

fn main() -> std::io::Result<()> {
    let cli = Cli::parse();
    let mut config = config::Config::load();

    if let Some(model) = cli.model {
        config.model.model_id = model;
    }
    if let Some(api_base) = cli.api_base {
        config.model.api_base = Some(api_base);
    }
    if cli.auto {
        config.guard.mode = config::GuardMode::Auto;
    }

    app::App::run(config)
}
```

- [ ] **Step 5: Add async-openai dependency to agent-cli**

Update `crates/agent-cli/Cargo.toml` dependencies to include:

```toml
async-openai = "0.27"
async-trait = "0.1"
serde_json = "1"
```

- [ ] **Step 6: Verify compilation**

Run: `cargo check --workspace`
Expected: compiles (may have warnings about unused code — that's OK for scaffolding)

- [ ] **Step 7: Run all tests**

Run: `cargo test --workspace`
Expected: all tests PASS

- [ ] **Step 8: Commit**

```bash
git add crates/agent-cli/
git commit -m "add TUI application with ratatui layout, input handling, and agent integration"
```

---

## Task 17: Integration — Wire Agent Run in TUI

**Files:**
- Modify: `crates/agent-core/src/agent.rs`
- Modify: `crates/agent-cli/src/app.rs`

The Agent's `run` method takes `&mut self`, which makes it hard to use across async tasks. We solve this by wrapping the Agent in `Arc<Mutex<>>` and spawning the run on a tokio task.

- [ ] **Step 1: Update app.rs to spawn agent run**

Replace the `(KeyCode::Enter, _)` match arm in `main_loop` with:

```rust
(KeyCode::Enter, _) if !app.is_running && !app.input.is_empty() => {
    let user_input = app.input.take();
    app.chat_history.push(ChatEntry::User(user_input.clone()));
    app.is_running = true;
    app.streaming_text.clear();

    let handler = TuiHandler {
        tx: event_tx.clone(),
    };

    // We need to move agent into the spawned task.
    // Use a channel to send the user input to a long-lived agent task.
    let _ = input_tx.send((user_input, handler));
}
```

Refactor `main_loop` to spawn a dedicated agent task that receives user input via a channel:

Replace the agent creation and loop in `main_loop` with:

```rust
async fn main_loop(terminal: &mut DefaultTerminal, config: Config) -> std::io::Result<()> {
    let mut app = App::new(&config.model.model_id);
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AgentEvent>();
    let (input_tx, mut input_rx) =
        mpsc::unbounded_channel::<(String, TuiHandler)>();

    // Spawn long-lived agent task
    let agent_config = config.clone();
    tokio::spawn(async move {
        #[cfg(feature = "openai")]
        let model = {
            let mut openai_config = async_openai::config::OpenAIConfig::default();
            if let Some(ref base) = agent_config.model.api_base {
                openai_config = openai_config.with_api_base(base);
            }
            if let Some(ref key) = agent_config.model.api_key {
                openai_config = openai_config.with_api_key(key);
            }
            agent_core::OpenAIModel::with_config(
                &agent_config.model.model_id,
                openai_config,
            )
        };

        let guard = AutoGuard;

        let system_prompt = agent_config.system_prompt.unwrap_or_else(|| {
            format!(
                "You are a helpful assistant. Current directory: {}. OS: {}.",
                std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                std::env::consts::OS,
            )
        });

        let mut agent = Agent::builder()
            .model(model)
            .guard(guard)
            .storage(MemoryStorage::new())
            .system_prompt(&system_prompt)
            .tool(agent_tools::ShellTool)
            .tool(agent_tools::ReadFileTool)
            .tool(agent_tools::WriteFileTool)
            .tool(agent_tools::EditFileTool)
            .tool(agent_tools::GlobTool)
            .tool(agent_tools::GrepTool)
            .build();

        while let Some((user_input, handler)) = input_rx.recv().await {
            if let Err(e) = agent.run(&user_input, &handler).await {
                handler
                    .on_event(AgentEvent::TurnComplete {
                        usage: agent_core::Usage::default(),
                    })
                    .await;
            }
        }
    });

    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        while let Ok(agent_event) = event_rx.try_recv() {
            handle_agent_event(&mut app, agent_event);
        }

        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        app.should_quit = true;
                    }
                    (KeyCode::Enter, _) if !app.is_running && !app.input.is_empty() => {
                        let user_input = app.input.take();
                        app.chat_history
                            .push(ChatEntry::User(user_input.clone()));
                        app.is_running = true;
                        app.streaming_text.clear();

                        let handler = TuiHandler {
                            tx: event_tx.clone(),
                        };
                        let _ = input_tx.send((user_input, handler));
                    }
                    (KeyCode::Char(c), _) if !app.is_running => {
                        app.input.insert(c);
                    }
                    (KeyCode::Backspace, _) if !app.is_running => {
                        app.input.backspace();
                    }
                    (KeyCode::Left, _) if !app.is_running => {
                        app.input.move_left();
                    }
                    (KeyCode::Right, _) if !app.is_running => {
                        app.input.move_right();
                    }
                    _ => {}
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check --workspace`
Expected: compiles with no errors

- [ ] **Step 3: Run all tests**

Run: `cargo test --workspace`
Expected: all tests PASS

- [ ] **Step 4: Test the CLI manually**

Run: `cargo run -p agent-cli`
Expected: TUI launches with input area, status bar. Ctrl+C quits cleanly. (Full agent interaction requires an API key set via `OPENAI_API_KEY` env var.)

- [ ] **Step 5: Commit**

```bash
git add crates/
git commit -m "wire agent run loop into TUI with async channel-based communication"
```

---

## Task 18: Update .gitignore and Final Cleanup

**Files:**
- Modify: `.gitignore`

- [ ] **Step 1: Replace Rust .gitignore with one that includes Cargo.lock policy**

Replace `.gitignore` content with:

```gitignore
# Rust / Cargo
/target
debug/
**/*.rs.bk
*.pdb
**/mutants.out*/

# IDE
.idea/
.vscode/
*.swp
*.swo

# OS
.DS_Store
Thumbs.db
```

Note: `Cargo.lock` is NOT ignored — for binaries (agent-cli), it should be committed.

- [ ] **Step 2: Run final workspace check**

Run: `cargo test --workspace && cargo clippy --workspace`
Expected: all tests PASS, no clippy errors (warnings OK for initial scaffolding)

- [ ] **Step 3: Commit**

```bash
git add .gitignore Cargo.lock
git commit -m "update .gitignore for Rust workspace, commit Cargo.lock"
```
