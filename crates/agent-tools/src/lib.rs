use std::path::{Path, PathBuf};

fn resolve(workdir: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        workdir.join(path)
    }
}

#[cfg(feature = "file")]
pub mod edit_file;
#[cfg(feature = "search")]
pub mod glob;
#[cfg(feature = "search")]
pub mod grep;
#[cfg(feature = "file")]
pub mod read_file;
#[cfg(feature = "shell")]
pub mod shell;
#[cfg(feature = "file")]
pub mod write_file;

#[cfg(feature = "search")]
pub use self::glob::GlobTool;
#[cfg(feature = "file")]
pub use edit_file::EditFileTool;
#[cfg(feature = "search")]
pub use grep::GrepTool;
#[cfg(feature = "file")]
pub use read_file::ReadFileTool;
#[cfg(feature = "shell")]
pub use shell::ShellTool;
#[cfg(feature = "file")]
pub use write_file::WriteFileTool;

#[cfg(test)]
mod workdir_tests {
    use agent_core::Tool;

    use super::*;

    #[tokio::test]
    async fn every_relative_tool_path_is_anchored_to_workdir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello old").unwrap();

        let shell = ShellTool::new(dir.path());
        assert_eq!(
            shell
                .call(serde_json::json!({"command":"pwd"}))
                .await
                .unwrap()
                .to_string()
                .trim(),
            dir.path().canonicalize().unwrap().to_string_lossy()
        );
        assert!(
            ReadFileTool::new(dir.path())
                .call(serde_json::json!({"path":"a.txt"}))
                .await
                .unwrap()
                .to_string()
                .contains("hello")
        );
        WriteFileTool::new(dir.path())
            .call(serde_json::json!({"path":"b.txt","content":"written"}))
            .await
            .unwrap();
        EditFileTool::new(dir.path())
            .call(serde_json::json!({"path":"a.txt","old_string":"old","new_string":"new"}))
            .await
            .unwrap();
        assert!(
            GlobTool::new(dir.path())
                .call(serde_json::json!({"pattern":"*.txt"}))
                .await
                .unwrap()
                .to_string()
                .contains("b.txt")
        );
        assert!(
            GrepTool::new(dir.path())
                .call(serde_json::json!({"pattern":"written","path":"."}))
                .await
                .unwrap()
                .to_string()
                .contains("b.txt")
        );
    }
}
