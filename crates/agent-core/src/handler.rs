use crate::event::Usage;
use crate::tool::ToolOutput;

#[derive(Debug, Clone)]
pub enum AgentEvent {
    TextDelta(String),
    ToolCallBegin {
        id: String,
        name: String,
        arguments: String,
    },
    ToolCallEnd {
        id: String,
        output: ToolOutput,
    },
    ToolCallDenied {
        id: String,
        name: String,
        reason: String,
    },
    TurnComplete {
        usage: Usage,
    },
}

#[async_trait::async_trait]
pub trait Handler: Send + Sync {
    async fn on_event(&self, event: AgentEvent);
    async fn confirm(&self, tool_name: &str, input: &serde_json::Value) -> bool;
}
