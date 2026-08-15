use serde::{Deserialize, Serialize};

use crate::{Error, StreamResponse};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Request {
    pub instructions: String,
    pub input: Vec<serde_json::Value>,
    pub tools: Vec<ToolDefinition>,
    pub compact_threshold: u32,
}

#[async_trait::async_trait]
pub trait Model: Send + Sync {
    fn model_name(&self) -> Option<&str> {
        None
    }

    async fn stream(&self, request: Request) -> Result<StreamResponse, Error>;
}
