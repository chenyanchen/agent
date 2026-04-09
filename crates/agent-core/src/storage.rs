use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::error::Error;
use crate::message::Message;

#[async_trait::async_trait]
pub trait Storage: Send + Sync {
    async fn save(&self, id: &str, messages: &[Message]) -> Result<(), Error>;
    async fn load(&self, id: &str) -> Result<Vec<Message>, Error>;
}

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
}
