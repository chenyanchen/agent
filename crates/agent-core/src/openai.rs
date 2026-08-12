use async_openai::Client;
use async_openai::config::OpenAIConfig;
use async_openai::types::responses::ResponseStreamEvent;
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

#[async_trait::async_trait]
impl Model for OpenAIModel {
    async fn stream(&self, request: Request) -> Result<StreamResponse, Error> {
        let body = request_body(&self.model_id, request);
        let stream: OpenAIStream<ResponseStreamEvent> = self
            .client
            .responses()
            .create_stream_byot(body)
            .await
            .map_err(|error| Error::Model(error.to_string()))?;

        let events = stream.filter_map(|result| async move {
            match result {
                Err(error) => Some(Err(Error::Model(error.to_string()))),
                Ok(ResponseStreamEvent::ResponseOutputTextDelta(event)) => {
                    Some(Ok(Event::TextDelta(event.delta)))
                }
                Ok(ResponseStreamEvent::ResponseOutputItemDone(event)) => Some(
                    serde_json::to_value(event.item)
                        .map(Event::OutputItem)
                        .map_err(Error::from),
                ),
                Ok(ResponseStreamEvent::ResponseCompleted(event)) => {
                    let usage = event.response.usage;
                    Some(Ok(Event::Done {
                        usage: Usage {
                            input_tokens: usage.as_ref().map_or(0, |usage| usage.input_tokens),
                            output_tokens: usage.as_ref().map_or(0, |usage| usage.output_tokens),
                            total_tokens: usage.map_or(0, |usage| usage.total_tokens),
                        },
                    }))
                }
                Ok(
                    event @ (ResponseStreamEvent::ResponseFailed(_)
                    | ResponseStreamEvent::ResponseIncomplete(_)
                    | ResponseStreamEvent::ResponseError(_)),
                ) => Some(Err(Error::Model(
                    serde_json::to_string(&event).unwrap_or_else(|_| "response failed".into()),
                ))),
                Ok(_) => None,
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
}
