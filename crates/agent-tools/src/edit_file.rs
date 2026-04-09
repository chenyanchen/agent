use agent_core::{Error, RiskLevel, Tool, ToolOutput};

pub struct EditFileTool;

#[async_trait::async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "Replace exactly one occurrence of old_string with new_string in the file at path."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file to edit." },
                "old_string": { "type": "string", "description": "The string to replace (must appear exactly once)." },
                "new_string": { "type": "string", "description": "The replacement string." }
            },
            "required": ["path", "old_string", "new_string"]
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

        let old_string = input
            .get("old_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Tool("missing 'old_string' field".to_string()))?;

        let new_string = input
            .get("new_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Tool("missing 'new_string' field".to_string()))?;

        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| Error::Tool(format!("failed to read file '{path}': {e}")))?;

        let count = content.matches(old_string).count();
        if count == 0 {
            return Err(Error::Tool(format!("old_string not found in '{path}'")));
        }
        if count > 1 {
            return Err(Error::Tool(format!(
                "old_string found {count} times in '{path}'; must appear exactly once"
            )));
        }

        let new_content = content.replacen(old_string, new_string, 1);
        tokio::fs::write(path, &new_content)
            .await
            .map_err(|e| Error::Tool(format!("failed to write file '{path}': {e}")))?;

        Ok(ToolOutput::Text(format!("Edited {path}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[tokio::test]
    async fn replace_in_file() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "hello world").unwrap();
        let path = tmp.path().to_string_lossy().to_string();

        let tool = EditFileTool;
        let input = serde_json::json!({
            "path": path,
            "old_string": "hello",
            "new_string": "goodbye"
        });
        tool.call(input).await.unwrap();

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert!(content.contains("goodbye"));
        assert!(!content.contains("hello"));
    }

    #[tokio::test]
    async fn old_string_not_found() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "hello world").unwrap();
        let path = tmp.path().to_string_lossy().to_string();

        let tool = EditFileTool;
        let input = serde_json::json!({
            "path": path,
            "old_string": "nonexistent",
            "new_string": "replacement"
        });
        let err = tool.call(input).await.unwrap_err();
        assert!(matches!(err, Error::Tool(_)));
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn old_string_multiple_times() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp, "foo foo foo").unwrap();
        let path = tmp.path().to_string_lossy().to_string();

        let tool = EditFileTool;
        let input = serde_json::json!({
            "path": path,
            "old_string": "foo",
            "new_string": "bar"
        });
        let err = tool.call(input).await.unwrap_err();
        assert!(matches!(err, Error::Tool(_)));
        assert!(err.to_string().contains("3 times"));
    }
}
