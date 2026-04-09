use agent_core::{Error, RiskLevel, Tool, ToolOutput};

pub struct ShellTool;

#[async_trait::async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Execute a shell command and return its output."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The shell command to execute." }
            },
            "required": ["command"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::High
    }

    async fn call(&self, input: serde_json::Value) -> Result<ToolOutput, Error> {
        let command = input
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::Tool("missing 'command' field".to_string()))?;

        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
            .await
            .map_err(|e| Error::Tool(format!("failed to execute command: {e}")))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            Ok(ToolOutput::Text(stdout))
        } else {
            let exit_code = output
                .status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let stderr = String::from_utf8_lossy(&output.stderr);
            Ok(ToolOutput::Error(format!(
                "exit code {exit_code}: {stderr}"
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn echo_hello() {
        let tool = ShellTool;
        let input = serde_json::json!({ "command": "echo hello" });
        let output = tool.call(input).await.unwrap();
        assert!(output.to_string().contains("hello"));
    }

    #[tokio::test]
    async fn false_command_exit_code() {
        let tool = ShellTool;
        let input = serde_json::json!({ "command": "false" });
        let output = tool.call(input).await.unwrap();
        assert!(output.to_string().contains("exit code"));
    }

    #[tokio::test]
    async fn missing_command_field() {
        let tool = ShellTool;
        let input = serde_json::json!({});
        let err = tool.call(input).await.unwrap_err();
        assert!(matches!(err, Error::Tool(_)));
    }
}
