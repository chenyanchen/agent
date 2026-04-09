use std::pin::Pin;
use std::task::{Context, Poll};

use futures::stream::{Stream, StreamExt};

use crate::error::Error;
use crate::message::{Message, ToolCall};

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Debug, Clone)]
pub enum Event {
    TextDelta(String),
    ToolCallBegin { id: String, name: String },
    ToolCallDelta { id: String, arguments_delta: String },
    ToolCallEnd { id: String },
    Done { usage: Usage },
}

pub struct Response {
    pub message: Message,
    pub usage: Usage,
}

pub struct StreamResponse {
    inner: Pin<Box<dyn Stream<Item = Event> + Send>>,
}

impl StreamResponse {
    pub fn new(stream: impl Stream<Item = Event> + Send + 'static) -> Self {
        Self {
            inner: Box::pin(stream),
        }
    }

    pub fn from_events(events: Vec<Event>) -> Self {
        Self::new(futures::stream::iter(events))
    }

    pub async fn collect(mut self) -> Result<Response, Error> {
        let mut text_parts: Vec<String> = Vec::new();
        // Map from tool call id -> (name, accumulated arguments)
        let mut tool_call_map: std::collections::HashMap<String, (String, String)> =
            std::collections::HashMap::new();
        // Preserve insertion order of tool call ids
        let mut tool_call_order: Vec<String> = Vec::new();
        let mut usage = Usage::default();

        while let Some(event) = self.inner.next().await {
            match event {
                Event::TextDelta(delta) => text_parts.push(delta),
                Event::ToolCallBegin { id, name } => {
                    if !tool_call_map.contains_key(&id) {
                        tool_call_order.push(id.clone());
                    }
                    tool_call_map.insert(id, (name, String::new()));
                }
                Event::ToolCallDelta { id, arguments_delta } => {
                    if let Some((_, args)) = tool_call_map.get_mut(&id) {
                        args.push_str(&arguments_delta);
                    }
                }
                Event::ToolCallEnd { id: _ } => {
                    // nothing extra needed; the call is already in the map
                }
                Event::Done { usage: u } => {
                    usage = u;
                }
            }
        }

        let text = if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.concat())
        };

        let tool_calls: Vec<ToolCall> = tool_call_order
            .into_iter()
            .filter_map(|id| {
                tool_call_map.remove(&id).map(|(name, arguments)| ToolCall {
                    id,
                    name,
                    arguments,
                })
            })
            .collect();

        let message = Message::Assistant { text, tool_calls };

        Ok(Response { message, usage })
    }
}

impl Stream for StreamResponse {
    type Item = Event;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn collect_text_only_stream() {
        let events = vec![
            Event::TextDelta("Hello, ".to_string()),
            Event::TextDelta("world!".to_string()),
            Event::Done {
                usage: Usage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                },
            },
        ];

        let stream = StreamResponse::from_events(events);
        let response = stream.collect().await.unwrap();

        match response.message {
            Message::Assistant { text, tool_calls } => {
                assert_eq!(text, Some("Hello, world!".to_string()));
                assert!(tool_calls.is_empty());
            }
            _ => panic!("expected Assistant message"),
        }

        assert_eq!(response.usage.prompt_tokens, 10);
        assert_eq!(response.usage.completion_tokens, 5);
        assert_eq!(response.usage.total_tokens, 15);
    }

    #[tokio::test]
    async fn collect_stream_with_tool_calls() {
        let events = vec![
            Event::ToolCallBegin {
                id: "call_1".to_string(),
                name: "get_weather".to_string(),
            },
            Event::ToolCallDelta {
                id: "call_1".to_string(),
                arguments_delta: r#"{"city":"#.to_string(),
            },
            Event::ToolCallDelta {
                id: "call_1".to_string(),
                arguments_delta: r#""London"}"#.to_string(),
            },
            Event::ToolCallEnd { id: "call_1".to_string() },
            Event::Done { usage: Usage::default() },
        ];

        let stream = StreamResponse::from_events(events);
        let response = stream.collect().await.unwrap();

        match response.message {
            Message::Assistant { text, tool_calls } => {
                assert!(text.is_none());
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].id, "call_1");
                assert_eq!(tool_calls[0].name, "get_weather");
                assert_eq!(tool_calls[0].arguments, r#"{"city":"London"}"#);
            }
            _ => panic!("expected Assistant message"),
        }
    }
}
