use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

const DEFAULT_TIMEOUT_MS: u64 = 120_000;

use agent_core::{Error, RiskLevel, Tool, ToolOutput};

pub struct ShellTool {
    workdir: PathBuf,
}

impl ShellTool {
    pub fn new(workdir: impl Into<PathBuf>) -> Self {
        Self {
            workdir: workdir.into(),
        }
    }
}

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
                "command": { "type": "string", "description": "The shell command to execute." },
                "timeout": {
                    "type": "integer",
                    "minimum": 1,
                    "default": DEFAULT_TIMEOUT_MS,
                    "description": "Timeout in milliseconds. Defaults to 120000 (120 seconds)."
                }
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

        let timeout_ms = match input.get("timeout") {
            Some(value) => value.as_u64().filter(|value| *value > 0).ok_or_else(|| {
                Error::Tool("'timeout' must be a positive integer in milliseconds".to_string())
            })?,
            None => DEFAULT_TIMEOUT_MS,
        };

        let mut process = tokio::process::Command::new("sh");
        process
            .arg("-c")
            .arg(command)
            .stdin(Stdio::null())
            .current_dir(&self.workdir)
            .env("GIT_EDITOR", "true")
            .kill_on_drop(true);

        let output = tokio::time::timeout(Duration::from_millis(timeout_ms), process.output())
            .await
            .map_err(|_| Error::Tool(format!("command timed out after {timeout_ms}ms")))?
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
        let tool = ShellTool::new(".");
        let input = serde_json::json!({ "command": "echo hello" });
        let output = tool.call(input).await.unwrap();
        assert!(output.to_string().contains("hello"));
    }

    #[tokio::test]
    async fn false_command_exit_code() {
        let tool = ShellTool::new(".");
        let input = serde_json::json!({ "command": "false" });
        let output = tool.call(input).await.unwrap();
        assert!(output.to_string().contains("exit code"));
    }

    #[tokio::test]
    async fn command_times_out() {
        let tool = ShellTool::new(".");
        let started = std::time::Instant::now();
        let error = tool
            .call(serde_json::json!({
                "command": "exec sleep 10",
                "timeout": 50
            }))
            .await
            .unwrap_err();

        assert!(matches!(error, Error::Tool(message) if message == "command timed out after 50ms"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test]
    async fn timeout_must_be_a_positive_integer() {
        let tool = ShellTool::new(".");

        for timeout in [
            serde_json::json!(0),
            serde_json::json!(-1),
            serde_json::json!(1.5),
        ] {
            let error = tool
                .call(serde_json::json!({ "command": "true", "timeout": timeout }))
                .await
                .unwrap_err();
            assert!(matches!(error, Error::Tool(message) if message.contains("positive integer")));
        }
    }

    #[test]
    fn timeout_defaults_to_120_seconds() {
        let schema = ShellTool::new(".").schema();
        assert_eq!(schema["properties"]["timeout"]["default"], 120_000);
        assert!(schema["properties"]["timeout"].get("maximum").is_none());
    }

    #[tokio::test]
    async fn child_is_non_interactive() {
        let dir = tempfile::tempdir().unwrap();
        let dir = dir.path().to_str().unwrap();
        let command = format!(
            "git -C {dir} init -q && git -C {dir} config user.name test && git -C {dir} config user.email test@example.com && touch {dir}/file && git -C {dir} add file; git -C {dir} commit; printf SHELL_RETURNED"
        );
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            ShellTool::new(".").call(serde_json::json!({ "command": command })),
        )
        .await
        .expect("git must not wait for an interactive editor")
        .unwrap();

        assert_eq!(output.to_string(), "SHELL_RETURNED");
    }

    #[tokio::test]
    async fn missing_command_field() {
        let tool = ShellTool::new(".");
        let input = serde_json::json!({});
        let err = tool.call(input).await.unwrap_err();
        assert!(matches!(err, Error::Tool(_)));
    }
}
