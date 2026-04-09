use agent_core::{Error, RiskLevel, Tool, ToolOutput};
use regex::Regex;
use walkdir::WalkDir;

pub struct GrepTool;

#[async_trait::async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search for a regex pattern in files under the given path. Returns 'file:line:content' format."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Regex pattern to search for." },
                "path": { "type": "string", "description": "File or directory to search in." }
            },
            "required": ["pattern", "path"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Low
    }

    async fn call(&self, input: serde_json::Value) -> Result<ToolOutput, Error> {
        let pattern = input
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Tool("missing 'pattern' field".to_string()))?;

        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Tool("missing 'path' field".to_string()))?;

        let re =
            Regex::new(pattern).map_err(|e| Error::Tool(format!("invalid regex pattern: {e}")))?;

        let mut matches: Vec<String> = Vec::new();

        for entry in WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let file_path = entry.path();
            // Skip binary/unreadable files silently
            let content = match std::fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            for (line_num, line) in content.lines().enumerate() {
                if re.is_match(line) {
                    matches.push(format!("{}:{}:{}", file_path.display(), line_num + 1, line));
                }
            }
        }

        Ok(ToolOutput::Text(matches.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[tokio::test]
    async fn search_in_directory() {
        let dir = tempfile::tempdir().unwrap();
        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");

        std::fs::File::create(&file_a)
            .unwrap()
            .write_all(b"hello world\nfoo bar\n")
            .unwrap();
        std::fs::File::create(&file_b)
            .unwrap()
            .write_all(b"no match here\nhello again\n")
            .unwrap();

        let tool = GrepTool;
        let input = serde_json::json!({
            "pattern": "hello",
            "path": dir.path().to_string_lossy()
        });
        let output = tool.call(input).await.unwrap();
        let text = output.to_string();

        assert!(text.contains("hello world"));
        assert!(text.contains("hello again"));
        assert!(!text.contains("foo bar"));
    }

    #[tokio::test]
    async fn search_single_file() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "line one").unwrap();
        writeln!(tmp, "line two matching pattern").unwrap();
        writeln!(tmp, "line three").unwrap();
        let path = tmp.path().to_string_lossy().to_string();

        let tool = GrepTool;
        let input = serde_json::json!({
            "pattern": "matching",
            "path": path
        });
        let output = tool.call(input).await.unwrap();
        let text = output.to_string();

        assert!(text.contains("matching pattern"));
        assert!(text.contains(":2:"));
    }

    #[tokio::test]
    async fn invalid_regex() {
        let tool = GrepTool;
        let input = serde_json::json!({
            "pattern": "[invalid",
            "path": "/tmp"
        });
        let err = tool.call(input).await.unwrap_err();
        assert!(matches!(err, Error::Tool(_)));
    }

    #[tokio::test]
    async fn missing_pattern_field() {
        let tool = GrepTool;
        let input = serde_json::json!({ "path": "/tmp" });
        let err = tool.call(input).await.unwrap_err();
        assert!(matches!(err, Error::Tool(_)));
    }
}
