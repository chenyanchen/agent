use crate::event::Usage;
use crate::skills::SkillInfo;
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
    SkillsUpdated {
        skills: Vec<SkillInfo>,
        warnings: Vec<String>,
    },
    Compacted {
        total: u64,
    },
    Failed {
        error: String,
        retryable: bool,
    },
    TurnComplete {
        usage: Usage,
        context_window: u32,
    },
}

#[async_trait::async_trait]
pub trait Handler: Send + Sync {
    async fn on_event(&self, event: AgentEvent);
    async fn confirm(&self, tool_name: &str, input: &serde_json::Value) -> bool;
}
