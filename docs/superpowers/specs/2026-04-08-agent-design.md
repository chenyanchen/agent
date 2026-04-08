# Agent — 通用 LLM Agent 设计文档

## 概述

用 Rust 构建一个通用 LLM Agent 系统。核心是一个提供 Agent 运行时的库（LLM 交互、tool 调度、对话管理、流式抽象），同时提供完整的内置 tools 和一个面向用户的生产级 CLI 应用。

**定位：** 核心库 + 生产级 CLI，不局限于 coding 场景。

**LLM 协议：** 基于 OpenAI Chat Completions API（de facto 标准），通过 `async-openai` crate 构建。无行业标准协议存在（无 IETF/W3C 规范），OpenAI API 兼容性覆盖面最广（Ollama、vLLM、Together AI、Groq、DeepSeek 等）。

## 项目结构

```
agent/
├── Cargo.toml              # workspace
├── crates/
│   ├── agent-core/         # 核心库
│   ├── agent-tools/        # 内置 tools
│   └── agent-cli/          # 生产级 CLI
```

### Crate 职责

- **`agent-core`** — 零业务逻辑，纯抽象 + 运行时。定义 `Model`、`Tool`、`Agent`、`Message`、`Storage`、`Guard` 等核心 trait 和 agent loop。不依赖任何具体 LLM provider 或具体 tool。
- **`agent-tools`** — 依赖 `agent-core`，实现内置 tools。每个 tool 是独立 module，可通过 feature flag 选择性编译。
- **`agent-cli`** — 依赖 `agent-core` + `agent-tools`，面向用户的生产级终端应用。负责 TUI 渲染、用户输入、配置文件、权限确认等。

**依赖方向严格单向：** `agent-cli` → `agent-tools` → `agent-core`，无循环依赖。

## 核心 Trait 抽象

### `Model` — LLM 交互

```rust
#[async_trait]
pub trait Model: Send + Sync {
    /// 发送消息并获取流式响应（唯一方法）
    async fn stream(&self, request: Request) -> Result<StreamResponse, Error>;
}
```

只提供 `stream` 一个方法。`complete` 作为通用扩展方法，通过收集 stream 实现。

**设计理由：** `complete` 是 `stream` 的特化（`stream().collect()`），不是独立能力。实现者只需实现一个方法；不支持 streaming 的 provider 可在 `stream` 内部包装为单 event 的 stream。

`Request` 包含：消息列表、tools 定义、模型参数（temperature 等）。
`Response` 包含：assistant 消息（text + tool calls）。

### `Tool` — 工具

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    /// 工具名称（唯一标识）
    fn name(&self) -> &str;

    /// 工具描述（供 LLM 理解用途）
    fn description(&self) -> &str;

    /// 参数的 JSON Schema（供 LLM 生成参数）
    fn schema(&self) -> serde_json::Value;

    /// 风险等级
    fn risk_level(&self) -> RiskLevel;

    /// 执行工具
    async fn call(&self, input: serde_json::Value) -> Result<ToolOutput, Error>;
}

pub enum RiskLevel {
    Low,    // 只读操作，自动执行
    Medium, // 写操作，可配置
    High,   // 破坏性操作，默认需确认
}
```

### `Storage` — 对话持久化

```rust
#[async_trait]
pub trait Storage: Send + Sync {
    async fn save(&self, id: &str, messages: &[Message]) -> Result<(), Error>;
    async fn load(&self, id: &str) -> Result<Vec<Message>, Error>;
}
```

初期提供 `MemoryStorage` 实现。抽象预留持久化扩展点。

### `Guard` — 权限控制

```rust
#[async_trait]
pub trait Guard: Send + Sync {
    async fn check(&self, tool_name: &str, input: &serde_json::Value) -> Decision;
}

pub enum Decision {
    Allow,
    Deny(String),     // 附带拒绝理由
    NeedConfirm,
}
```

内置两种实现：
- `AutoGuard` — 全部自动执行
- `ConfirmGuard` — 基于 `RiskLevel` 决策，High 需确认，Medium 可配置，Low 自动

## Agent Loop

```
用户输入
  ↓
构建 Request（system prompt + 历史消息 + tools 定义）
  ↓
调用 model.stream(request)
  ↓
处理 StreamResponse：
  ├── Text delta → 实时输出给用户
  └── Tool call →
        ├── guard.check() → Deny → 告知 LLM 被拒绝，继续循环
        │                 → NeedConfirm → 等待用户确认
        │                 → Allow → 执行
        └── tool.call(input) → 将 tool result 追加到消息历史
                             → 回到 "调用 model.stream" 继续循环
  ↓
LLM 返回纯文本（无 tool call）→ 本轮结束，等待用户下一次输入
```

**关键设计点：**

- **循环终止条件：** LLM 响应中不包含 tool call 时，本轮结束。
- **多 tool call：** 一次响应可能包含多个 tool calls，按顺序执行（或可配置并发）。
- **错误处理：** tool 执行失败时，将错误信息作为 tool result 返回给 LLM，让它自行调整，而非中断循环。
- **上下文管理：** 消息历史超出 context window 时需要截断策略（初期简单截断早期消息）。

```rust
pub struct Agent<M: Model, G: Guard, S: Storage> {
    model: M,
    guard: G,
    storage: S,
    tools: HashMap<String, Box<dyn Tool>>,
    system_prompt: String,
}
```

Agent 通过泛型组合 Model、Guard、Storage，编译期确定类型，零成本抽象。

## 流式抽象

```rust
pub enum Event {
    /// 文本增量
    TextDelta(String),
    /// Tool call 开始
    ToolCallBegin { id: String, name: String },
    /// Tool call 参数增量（JSON 片段）
    ToolCallDelta { id: String, arguments_delta: String },
    /// Tool call 参数完整接收
    ToolCallEnd { id: String },
    /// 响应结束
    Done { usage: Usage },
    /// 错误
    Error(Error),
}

pub struct StreamResponse {
    inner: Pin<Box<dyn Stream<Item = Event> + Send>>,
}

impl StreamResponse {
    pub async fn collect(self) -> Result<Response, Error> { ... }
}
```

**设计要点：**

- Event 是最小消费单元，库消费者通过遍历 Event 流实现实时渲染。
- Tool call 三段式（Begin/Delta/End）与 OpenAI SSE 事件自然映射。实际 Event 定义需对齐 OpenAI SSE 的具体字段和语义，实现阶段再细化。
- Done 事件携带 token 用量，方便统计和上下文管理。
- Agent loop 遍历 Event 流，累积 text 和 tool calls，同时将 Event 转发给外部（CLI）做渲染。

## 内置 Tools

| Tool | 功能 | 风险等级 |
|------|------|----------|
| `shell` | 执行 shell 命令 | High |
| `read_file` | 读取文件内容 | Low |
| `write_file` | 写入/创建文件 | Medium |
| `edit_file` | 编辑文件（基于字符串替换） | Medium |
| `glob` | 按模式搜索文件路径 | Low |
| `grep` | 按正则搜索文件内容 | Low |

每个 tool 是独立 module，通过 feature flag 选择性编译：`agent-tools = { features = ["shell", "file"] }`。

## CLI 应用

### 交互模型

纯对话式全屏 TUI。

### 技术栈

- **`ratatui` + `crossterm`** — 全屏 TUI 框架
- **`tokio`** — 异步运行时
- **`toml`** — 配置文件解析
- **`clap`** — 命令行参数

### TUI 功能

- **分区布局** — 对话区域可滚动浏览历史，输入区域固定在底部，状态栏显示模型/token 用量等信息。
- **富文本渲染** — Markdown 语法高亮、代码块着色。
- **Tool 执行可视化** — 显示正在执行的 tool、进度、输出。
- **流畅的滚动** — 长对话中自由翻阅历史而不中断当前输入。
- **键盘快捷键** — 全局快捷键控制（取消执行、切换面板等）。

### 配置

配置文件 `~/.agent/config.toml`，包含：模型设置、API key、默认 Guard 策略等。

System prompt 内置合理默认，用户可通过配置或项目级文件覆盖。自动注入当前目录、OS、shell 等环境信息。

## 依赖总览

```
agent-core
├── async-trait
├── serde / serde_json
├── tokio
├── futures (Stream trait)
└── async-openai (feature-gated: "openai")

agent-tools
├── agent-core
├── tokio (process, fs)
├── glob
├── grep-regex
└── reqwest (feature-gated: web tool)

agent-cli
├── agent-core
├── agent-tools
├── ratatui + crossterm
├── toml
└── clap
```
