use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{Error, Message, Request};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SessionState {
    pub transcript: Vec<Message>,
    pub context: Vec<serde_json::Value>,
    pub compaction_count: u64,
    pub pending_request: Option<Request>,
    pub last_input_tokens: u32,
    pub last_error: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SessionFile {
    Current(SessionState),
    Legacy(Vec<Message>),
}

impl SessionFile {
    fn into_state(self) -> SessionState {
        match self {
            Self::Current(state) => state,
            Self::Legacy(transcript) => SessionState {
                context: legacy_context(&transcript),
                transcript,
                ..SessionState::default()
            },
        }
    }
}

fn legacy_context(transcript: &[Message]) -> Vec<serde_json::Value> {
    let mut context = Vec::new();
    for message in transcript {
        match message {
            Message::System { .. } => {}
            Message::User { content } => {
                context.push(serde_json::json!({"role":"user","content":content}));
            }
            Message::Assistant { text, tool_calls } => {
                if let Some(text) = text {
                    context.push(serde_json::json!({"role":"assistant","content":text}));
                }
                context.extend(tool_calls.iter().map(|call| {
                    serde_json::json!({
                        "type":"function_call",
                        "call_id":call.id,
                        "name":call.name,
                        "arguments":call.arguments
                    })
                }));
            }
            Message::Tool {
                tool_call_id,
                content,
            } => context.push(serde_json::json!({
                "type":"function_call_output",
                "call_id":tool_call_id,
                "output":content
            })),
        }
    }
    context
}

#[async_trait::async_trait]
pub trait Storage: Send + Sync {
    async fn save(&self, id: &str, state: &SessionState) -> Result<(), Error>;
    async fn load(&self, id: &str) -> Result<SessionState, Error>;
}

#[derive(Clone, Default)]
pub struct MemoryStorage {
    data: Arc<RwLock<HashMap<String, SessionState>>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl Storage for MemoryStorage {
    async fn save(&self, id: &str, state: &SessionState) -> Result<(), Error> {
        self.data.write().await.insert(id.to_owned(), state.clone());
        Ok(())
    }

    async fn load(&self, id: &str) -> Result<SessionState, Error> {
        Ok(self.data.read().await.get(id).cloned().unwrap_or_default())
    }
}

#[derive(Clone)]
pub struct FileStorage {
    dir: PathBuf,
}

impl FileStorage {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn path(&self, id: &str) -> Result<PathBuf, Error> {
        if id.is_empty()
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(Error::Storage(format!("invalid conversation id: {id}")));
        }
        Ok(self.dir.join(format!("{id}.json")))
    }

    fn storage_error(path: &Path, error: std::io::Error) -> Error {
        Error::Storage(format!("{}: {error}", path.display()))
    }
}

#[async_trait::async_trait]
impl Storage for FileStorage {
    async fn save(&self, id: &str, state: &SessionState) -> Result<(), Error> {
        let path = self.path(id)?;
        fs::create_dir_all(&self.dir).map_err(|error| Self::storage_error(&self.dir, error))?;
        let temporary = path.with_extension("json.tmp");
        let data = serde_json::to_vec(state)?;

        // ponytail: one writer per session; add file locking if concurrent CLIs share an id.
        fs::write(&temporary, data).map_err(|error| Self::storage_error(&temporary, error))?;
        fs::rename(&temporary, &path).map_err(|error| Self::storage_error(&path, error))
    }

    async fn load(&self, id: &str) -> Result<SessionState, Error> {
        let path = self.path(id)?;
        match fs::read(&path) {
            Ok(data) => Ok(serde_json::from_slice::<SessionFile>(&data)?.into_state()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(SessionState::default())
            }
            Err(error) => Err(Self::storage_error(&path, error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn file_storage_atomically_round_trips_session_state() {
        let dir = tempfile::tempdir().unwrap();
        let state = SessionState {
            transcript: vec![Message::User {
                content: "persist me".into(),
            }],
            context: vec![serde_json::json!({"role":"user","content":"persist me"})],
            pending_request: Some(Request {
                instructions: "rules".into(),
                input: vec![],
                tools: vec![],
                compact_threshold: 80,
            }),
            ..SessionState::default()
        };

        let storage = FileStorage::new(dir.path());
        storage.save("conv-1", &state).await.unwrap();
        assert_eq!(storage.load("conv-1").await.unwrap(), state);
        assert!(!dir.path().join("conv-1.json.tmp").exists());
    }

    #[tokio::test]
    async fn rejects_path_traversal() {
        let error = FileStorage::new(tempfile::tempdir().unwrap().path())
            .save("../outside", &SessionState::default())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("invalid conversation id"));
    }

    #[tokio::test]
    async fn loads_legacy_message_array_as_responses_context() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("old.json"),
            serde_json::to_vec(&vec![Message::User {
                content: "hello".into(),
            }])
            .unwrap(),
        )
        .unwrap();

        let state = FileStorage::new(dir.path()).load("old").await.unwrap();
        assert_eq!(state.transcript.len(), 1);
        assert_eq!(state.context[0]["role"], "user");
    }
}
