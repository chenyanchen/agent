use agent_core::{Error, RiskLevel, Tool, ToolOutput};

pub struct WriteFileTool;

#[async_trait::async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file at the given path, creating parent directories if needed."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file to write." },
                "content": { "type": "string", "description": "Content to write to the file." }
            },
            "required": ["path", "content"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Medium
    }

    async fn call(&self, input: serde_json::Value) -> Result<ToolOutput, Error> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Tool("missing 'path' field".to_string()))?;

        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Tool("missing 'content' field".to_string()))?;

        if let Some(parent) = std::path::Path::new(path).parent()
            && !parent.as_os_str().is_empty()
        {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::Tool(format!("failed to create directories: {e}")))?;
        }

        tokio::fs::write(path, content)
            .await
            .map_err(|e| Error::Tool(format!("failed to write file '{path}': {e}")))?;

        Ok(ToolOutput::Text(format!("Written to {path}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_and_verify() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        let path_str = path.to_string_lossy().to_string();

        let tool = WriteFileTool;
        let input = serde_json::json!({ "path": path_str, "content": "hello write" });
        let output = tool.call(input).await.unwrap();
        assert!(output.to_string().contains(&path_str));

        let written = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(written, "hello write");
    }

    #[tokio::test]
    async fn write_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("dir").join("file.txt");
        let path_str = path.to_string_lossy().to_string();

        let tool = WriteFileTool;
        let input = serde_json::json!({ "path": path_str, "content": "nested" });
        tool.call(input).await.unwrap();

        let written = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(written, "nested");
    }

    #[tokio::test]
    async fn missing_content_field() {
        let tool = WriteFileTool;
        let input = serde_json::json!({ "path": "/tmp/whatever.txt" });
        let err = tool.call(input).await.unwrap_err();
        assert!(matches!(err, Error::Tool(_)));
    }
}
