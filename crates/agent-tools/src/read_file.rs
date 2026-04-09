use agent_core::{Error, RiskLevel, Tool, ToolOutput};

pub struct ReadFileTool;

#[async_trait::async_trait]
impl Tool for ReadFileTool {
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
                "path": { "type": "string", "description": "Path to the file to read." }
            },
            "required": ["path"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Low
    }

    async fn call(&self, input: serde_json::Value) -> Result<ToolOutput, Error> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Tool("missing 'path' field".to_string()))?;

        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| Error::Tool(format!("failed to read file '{path}': {e}")))?;

        Ok(ToolOutput::Text(content))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[tokio::test]
    async fn read_existing_file() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "hello from file").unwrap();
        let path = tmp.path().to_string_lossy().to_string();

        let tool = ReadFileTool;
        let input = serde_json::json!({ "path": path });
        let output = tool.call(input).await.unwrap();
        assert!(output.to_string().contains("hello from file"));
    }

    #[tokio::test]
    async fn read_nonexistent_file() {
        let tool = ReadFileTool;
        let input = serde_json::json!({ "path": "/nonexistent/path/file.txt" });
        let err = tool.call(input).await.unwrap_err();
        assert!(matches!(err, Error::Tool(_)));
    }

    #[tokio::test]
    async fn missing_path_field() {
        let tool = ReadFileTool;
        let input = serde_json::json!({});
        let err = tool.call(input).await.unwrap_err();
        assert!(matches!(err, Error::Tool(_)));
    }
}
