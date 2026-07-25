use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::error::Error;
use crate::message::Message;

#[async_trait::async_trait]
pub trait Storage: Send + Sync {
    async fn save(&self, id: &str, messages: &[Message]) -> Result<(), Error>;
    async fn load(&self, id: &str) -> Result<Vec<Message>, Error>;
}

#[derive(Clone)]
pub struct MemoryStorage {
    data: Arc<RwLock<HashMap<String, Vec<Message>>>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Storage for MemoryStorage {
    async fn save(&self, id: &str, messages: &[Message]) -> Result<(), Error> {
        let mut data = self.data.write().await;
        data.insert(id.to_string(), messages.to_vec());
        Ok(())
    }

    async fn load(&self, id: &str) -> Result<Vec<Message>, Error> {
        let data = self.data.read().await;
        Ok(data.get(id).cloned().unwrap_or_default())
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
    async fn save(&self, id: &str, messages: &[Message]) -> Result<(), Error> {
        let path = self.path(id)?;
        fs::create_dir_all(&self.dir).map_err(|error| Self::storage_error(&self.dir, error))?;
        let temporary = path.with_extension("json.tmp");
        let data = serde_json::to_vec(messages)?;

        // ponytail: one writer per session; add file locking if concurrent CLIs share an id.
        fs::write(&temporary, data).map_err(|error| Self::storage_error(&temporary, error))?;
        fs::rename(&temporary, &path).map_err(|error| Self::storage_error(&path, error))
    }

    async fn load(&self, id: &str) -> Result<Vec<Message>, Error> {
        let path = self.path(id)?;
        match fs::read(&path) {
            Ok(data) => Ok(serde_json::from_slice(&data)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(Self::storage_error(&path, error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::Message;

    #[tokio::test]
    async fn save_and_load() {
        let storage = MemoryStorage::new();
        let messages = vec![
            Message::User {
                content: "Hello".to_string(),
            },
            Message::Assistant {
                text: Some("Hi!".to_string()),
                tool_calls: vec![],
            },
        ];

        storage.save("conv-1", &messages).await.unwrap();
        let loaded = storage.load("conv-1").await.unwrap();

        assert_eq!(loaded.len(), 2);
        match &loaded[0] {
            Message::User { content } => assert_eq!(content, "Hello"),
            _ => panic!("expected User"),
        }
    }

    #[tokio::test]
    async fn load_nonexistent_returns_empty() {
        let storage = MemoryStorage::new();
        let loaded = storage.load("nonexistent").await.unwrap();
        assert!(loaded.is_empty());
    }

    #[tokio::test]
    async fn overwrite_existing() {
        let storage = MemoryStorage::new();
        let first = vec![Message::User {
            content: "first".to_string(),
        }];
        let second = vec![
            Message::User {
                content: "second_a".to_string(),
            },
            Message::User {
                content: "second_b".to_string(),
            },
        ];

        storage.save("conv-1", &first).await.unwrap();
        storage.save("conv-1", &second).await.unwrap();

        let loaded = storage.load("conv-1").await.unwrap();
        assert_eq!(loaded.len(), 2);
        match &loaded[0] {
            Message::User { content } => assert_eq!(content, "second_a"),
            _ => panic!("expected User"),
        }
    }

    #[tokio::test]
    async fn file_storage_survives_new_instance() {
        let dir = tempfile::tempdir().unwrap();
        let messages = vec![Message::User {
            content: "persist me".to_string(),
        }];

        FileStorage::new(dir.path())
            .save("conv-1", &messages)
            .await
            .unwrap();
        let loaded = FileStorage::new(dir.path()).load("conv-1").await.unwrap();

        assert_eq!(loaded.len(), 1);
        assert!(matches!(
            &loaded[0],
            Message::User { content } if content == "persist me"
        ));
    }

    #[tokio::test]
    async fn file_storage_rejects_path_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let error = FileStorage::new(dir.path())
            .save("../outside", &[])
            .await
            .unwrap_err();

        assert!(error.to_string().contains("invalid conversation id"));
    }
}
