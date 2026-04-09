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
            "Converts input text to uppercase."
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
            match input.get("text").and_then(|v| v.as_str()) {
                Some(text) => Ok(ToolOutput::Text(text.to_uppercase())),
                None => Err(Error::Tool("missing 'text' field".to_string())),
            }
        }
    }

    #[tokio::test]
    async fn uppercase_tool_success() {
        let tool = UppercaseTool;
        let input = serde_json::json!({ "text": "hello world" });
        let output = tool.call(input).await.unwrap();
        match output {
            ToolOutput::Text(s) => assert_eq!(s, "HELLO WORLD"),
            _ => panic!("expected Text output"),
        }
    }

    #[tokio::test]
    async fn uppercase_tool_missing_field() {
        let tool = UppercaseTool;
        let input = serde_json::json!({});
        let err = tool.call(input).await.unwrap_err();
        assert!(matches!(err, crate::error::Error::Tool(_)));
    }

    #[test]
    fn tool_output_display() {
        assert_eq!(ToolOutput::Text("ok".to_string()).to_string(), "ok");
        assert_eq!(
            ToolOutput::Error("bad".to_string()).to_string(),
            "Error: bad"
        );
    }
}
