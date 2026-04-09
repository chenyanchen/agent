use agent_core::{Error, RiskLevel, Tool, ToolOutput};

pub struct GlobTool;

#[async_trait::async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Find files matching a glob pattern. Returns newline-separated paths."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Glob pattern to match files against." }
            },
            "required": ["pattern"]
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

        let paths: Vec<String> = glob::glob(pattern)
            .map_err(|e| Error::Tool(format!("invalid glob pattern: {e}")))?
            .filter_map(|entry| entry.ok())
            .map(|p| p.to_string_lossy().into_owned())
            .collect();

        if paths.is_empty() {
            Ok(ToolOutput::Text(String::new()))
        } else {
            Ok(ToolOutput::Text(paths.join("\n")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[tokio::test]
    async fn match_txt_files() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        let c = dir.path().join("c.rs");

        std::fs::File::create(&a).unwrap().write_all(b"").unwrap();
        std::fs::File::create(&b).unwrap().write_all(b"").unwrap();
        std::fs::File::create(&c).unwrap().write_all(b"").unwrap();

        let pattern = format!("{}/*.txt", dir.path().display());
        let tool = GlobTool;
        let input = serde_json::json!({ "pattern": pattern });
        let output = tool.call(input).await.unwrap();
        let text = output.to_string();

        assert!(text.contains("a.txt"));
        assert!(text.contains("b.txt"));
        assert!(!text.contains("c.rs"));
    }

    #[tokio::test]
    async fn no_matches_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let pattern = format!("{}/*.nomatch", dir.path().display());
        let tool = GlobTool;
        let input = serde_json::json!({ "pattern": pattern });
        let output = tool.call(input).await.unwrap();
        assert_eq!(output.to_string(), "");
    }

    #[tokio::test]
    async fn missing_pattern_field() {
        let tool = GlobTool;
        let input = serde_json::json!({});
        let err = tool.call(input).await.unwrap_err();
        assert!(matches!(err, Error::Tool(_)));
    }
}
