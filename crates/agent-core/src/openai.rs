use async_openai::Client;
use async_openai::config::OpenAIConfig;
use async_openai::types::stream::StreamResponse as OpenAIStream;
use futures::StreamExt;

use crate::{Error, Event, Model, Request, StreamResponse, Usage};

pub struct OpenAIModel {
    client: Client<OpenAIConfig>,
    model_id: String,
}

impl OpenAIModel {
    pub fn new(model_id: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            model_id: model_id.into(),
        }
    }

    pub fn with_settings(
        model_id: impl Into<String>,
        api_key: Option<String>,
        api_base: Option<String>,
    ) -> Self {
        let mut config = OpenAIConfig::new();
        if let Some(api_key) = api_key {
            config = config.with_api_key(api_key);
        }
        if let Some(api_base) = api_base {
            config = config.with_api_base(api_base);
        }
        Self {
            client: Client::with_config(config),
            model_id: model_id.into(),
        }
    }
}

fn request_body(model_id: &str, request: Request) -> serde_json::Value {
    let tools = request
        .tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "type": "function",
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.parameters,
                "strict": false
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "model": model_id,
        "input": request.input,
        "instructions": request.instructions,
        "tools": tools,
        "store": false,
        "stream": true,
        "include": ["reasoning.encrypted_content"],
        "context_management": [{
            "type": "compaction",
            "compact_threshold": request.compact_threshold
        }]
    })
}

fn parse_stream_event(event: serde_json::Value) -> Option<Result<Event, Error>> {
    let event_type = event["type"].as_str()?;
    match event_type {
        "response.output_text.delta" => Some(
            event["delta"]
                .as_str()
                .map(|delta| Event::TextDelta(delta.to_owned()))
                .ok_or_else(|| Error::Model("response.output_text.delta missing delta".into())),
        ),
        "response.output_item.done" => Some(
            event
                .get("item")
                .cloned()
                .map(Event::OutputItem)
                .ok_or_else(|| Error::Model("response.output_item.done missing item".into())),
        ),
        "response.completed" => {
            let usage = &event["response"]["usage"];
            let token = |name: &str| {
                usage[name]
                    .as_u64()
                    .unwrap_or_default()
                    .min(u32::MAX.into()) as u32
            };
            Some(Ok(Event::Done {
                usage: Usage {
                    input_tokens: token("input_tokens"),
                    output_tokens: token("output_tokens"),
                    total_tokens: token("total_tokens"),
                    cached_input_tokens: usage["input_tokens_details"]["cached_tokens"]
                        .as_u64()
                        .unwrap_or_default()
                        .min(u32::MAX.into()) as u32,
                },
            }))
        }
        "response.failed" | "response.incomplete" | "error" => Some(Err(Error::Model(
            event
                .pointer("/response/error/message")
                .or_else(|| event.pointer("/error/message"))
                .or_else(|| event.pointer("/response/incomplete_details/reason"))
                .or_else(|| event.pointer("/message"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| event_type.to_owned()),
        ))),
        _ => None,
    }
}

#[async_trait::async_trait]
impl Model for OpenAIModel {
    fn model_name(&self) -> Option<&str> {
        Some(&self.model_id)
    }

    async fn stream(&self, request: Request) -> Result<StreamResponse, Error> {
        let body = request_body(&self.model_id, request);
        let stream: OpenAIStream<serde_json::Value> = self
            .client
            .responses()
            .create_stream_byot(body)
            .await
            .map_err(|error| Error::Model(error.to_string()))?;

        let events = stream.filter_map(|result| async move {
            match result {
                Err(error) => Some(Err(Error::Model(error.to_string()))),
                Ok(event) => parse_stream_event(event),
            }
        });
        Ok(StreamResponse::new(events))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_body_uses_responses_compaction_contract() {
        let request = Request {
            instructions: "rules".into(),
            input: vec![serde_json::json!({"role":"user","content":"hi"})],
            tools: vec![],
            compact_threshold: 80,
        };
        let body = request_body("test", request);
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert!(body.get("truncation").is_none());
        assert_eq!(body["include"][0], "reasoning.encrypted_content");
        assert_eq!(body["context_management"][0]["compact_threshold"], 80);
        assert!(body.get("previous_response_id").is_none());
    }

    #[test]
    fn completed_event_only_requires_fields_the_agent_uses() {
        let event = serde_json::json!({
            "type": "response.completed",
            "response": {
                "output": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "done"}]
                }],
                "usage": {
                    "input_tokens": 11,
                    "output_tokens": 2,
                    "total_tokens": 13,
                    "input_tokens_details": {"cached_tokens": 7}
                }
            }
        });

        let parsed = parse_stream_event(event).unwrap().unwrap();
        assert!(matches!(
            parsed,
            Event::Done {
                usage: Usage {
                    input_tokens: 11,
                    output_tokens: 2,
                    total_tokens: 13,
                    cached_input_tokens: 7
                }
            }
        ));
    }
}
