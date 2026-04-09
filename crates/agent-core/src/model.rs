use crate::error::Error;
use crate::event::StreamResponse;
use crate::message::Message;

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct Request {
    pub system: Option<String>,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}

#[async_trait::async_trait]
pub trait Model: Send + Sync {
    async fn stream(&self, request: Request) -> Result<StreamResponse, Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, Usage};

    struct EchoModel;

    #[async_trait::async_trait]
    impl Model for EchoModel {
        async fn stream(&self, request: Request) -> Result<StreamResponse, Error> {
            // Echo back the last user message content as a TextDelta
            let echo_text = request
                .messages
                .iter()
                .rev()
                .find_map(|m| match m {
                    Message::User { content } => Some(content.clone()),
                    _ => None,
                })
                .unwrap_or_default();

            let events = vec![
                Event::TextDelta(echo_text),
                Event::Done {
                    usage: Usage {
                        prompt_tokens: 1,
                        completion_tokens: 1,
                        total_tokens: 2,
                    },
                },
            ];

            Ok(StreamResponse::from_events(events))
        }
    }

    #[tokio::test]
    async fn echo_model_stream_and_collect() {
        let model = EchoModel;
        let request = Request {
            system: None,
            messages: vec![Message::User { content: "ping".to_string() }],
            tools: vec![],
            temperature: None,
            max_tokens: None,
        };

        let stream = model.stream(request).await.unwrap();
        let response = stream.collect().await.unwrap();

        match response.message {
            crate::message::Message::Assistant { text, tool_calls } => {
                assert_eq!(text, Some("ping".to_string()));
                assert!(tool_calls.is_empty());
            }
            _ => panic!("expected Assistant message"),
        }

        assert_eq!(response.usage.total_tokens, 2);
    }
}
