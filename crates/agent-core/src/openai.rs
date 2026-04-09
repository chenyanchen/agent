use std::collections::HashMap;

use async_openai::{
    Client,
    config::OpenAIConfig,
    types::{
        ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessage,
        ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestMessage,
        ChatCompletionRequestSystemMessage, ChatCompletionRequestSystemMessageContent,
        ChatCompletionRequestToolMessage, ChatCompletionRequestToolMessageContent,
        ChatCompletionRequestUserMessage, ChatCompletionRequestUserMessageContent,
        ChatCompletionStreamOptions, ChatCompletionTool, ChatCompletionToolType,
        CreateChatCompletionRequest, FinishReason, FunctionCall, FunctionObject,
    },
};
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::{
    error::Error,
    event::{Event, StreamResponse, Usage},
    message::Message,
    model::{Model, Request, ToolDefinition},
};

pub struct OpenAIModel {
    client: Client<OpenAIConfig>,
    model_id: String,
}

impl OpenAIModel {
    /// Create a new `OpenAIModel` that reads `OPENAI_API_KEY` from the environment.
    pub fn new(model_id: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            model_id: model_id.into(),
        }
    }

    /// Create a new `OpenAIModel` with a custom [`OpenAIConfig`] (e.g. custom base URL).
    pub fn with_config(model_id: impl Into<String>, config: OpenAIConfig) -> Self {
        Self {
            client: Client::with_config(config),
            model_id: model_id.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn convert_message(msg: Message) -> ChatCompletionRequestMessage {
    match msg {
        Message::System { content } => {
            ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
                content: ChatCompletionRequestSystemMessageContent::Text(content),
                name: None,
            })
        }
        Message::User { content } => {
            ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
                content: ChatCompletionRequestUserMessageContent::Text(content),
                name: None,
            })
        }
        Message::Assistant { text, tool_calls } => {
            let content = text.map(ChatCompletionRequestAssistantMessageContent::Text);

            let oai_tool_calls: Option<Vec<ChatCompletionMessageToolCall>> =
                if tool_calls.is_empty() {
                    None
                } else {
                    Some(
                        tool_calls
                            .into_iter()
                            .map(|tc| ChatCompletionMessageToolCall {
                                id: tc.id,
                                r#type: ChatCompletionToolType::Function,
                                function: FunctionCall {
                                    name: tc.name,
                                    arguments: tc.arguments,
                                },
                            })
                            .collect(),
                    )
                };

            #[allow(deprecated)]
            ChatCompletionRequestMessage::Assistant(ChatCompletionRequestAssistantMessage {
                content,
                refusal: None,
                name: None,
                audio: None,
                tool_calls: oai_tool_calls,
                function_call: None,
            })
        }
        Message::Tool { tool_call_id, content } => {
            ChatCompletionRequestMessage::Tool(ChatCompletionRequestToolMessage {
                content: ChatCompletionRequestToolMessageContent::Text(content),
                tool_call_id,
            })
        }
    }
}

fn convert_tool(tool: ToolDefinition) -> ChatCompletionTool {
    ChatCompletionTool {
        r#type: ChatCompletionToolType::Function,
        function: FunctionObject {
            name: tool.name,
            description: Some(tool.description),
            parameters: Some(tool.parameters),
            strict: None,
        },
    }
}

// ---------------------------------------------------------------------------
// Model trait implementation
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl Model for OpenAIModel {
    async fn stream(&self, request: Request) -> Result<StreamResponse, Error> {
        // Build the list of messages, prepending system if present.
        let mut messages: Vec<ChatCompletionRequestMessage> = Vec::new();

        if let Some(system) = request.system {
            messages.push(ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessage {
                    content: ChatCompletionRequestSystemMessageContent::Text(system),
                    name: None,
                },
            ));
        }

        for msg in request.messages {
            messages.push(convert_message(msg));
        }

        let tools: Option<Vec<ChatCompletionTool>> = if request.tools.is_empty() {
            None
        } else {
            Some(request.tools.into_iter().map(convert_tool).collect())
        };

        #[allow(deprecated)]
        let oai_request = CreateChatCompletionRequest {
            model: self.model_id.clone(),
            messages,
            temperature: request.temperature,
            max_completion_tokens: request.max_tokens,
            tools,
            stream_options: Some(ChatCompletionStreamOptions { include_usage: true }),
            // All other fields — use their defaults (None / false).
            store: None,
            reasoning_effort: None,
            metadata: None,
            frequency_penalty: None,
            logit_bias: None,
            logprobs: None,
            top_logprobs: None,
            max_tokens: None,
            n: None,
            modalities: None,
            prediction: None,
            audio: None,
            presence_penalty: None,
            response_format: None,
            seed: None,
            service_tier: None,
            stop: None,
            stream: None,
            top_p: None,
            tool_choice: None,
            parallel_tool_calls: None,
            user: None,
            function_call: None,
            functions: None,
        };

        let mut oai_stream = self
            .client
            .chat()
            .create_stream(oai_request)
            .await
            .map_err(|e| Error::Model(e.to_string()))?;

        let (tx, rx) = mpsc::unbounded_channel::<Event>();

        tokio::spawn(async move {
            // index → (id, name, accumulated_arguments)
            let mut pending: HashMap<u32, (String, String, String)> = HashMap::new();

            while let Some(result) = oai_stream.next().await {
                match result {
                    Err(e) => {
                        // We can't propagate errors through the channel elegantly;
                        // log and break so the stream ends.
                        eprintln!("OpenAI stream error: {e}");
                        break;
                    }
                    Ok(chunk) => {
                        // The last chunk (when include_usage=true) may have an empty
                        // choices array and carry usage information.
                        if let Some(usage) = chunk.usage {
                            let _ = tx.send(Event::Done {
                                usage: Usage {
                                    prompt_tokens: usage.prompt_tokens,
                                    completion_tokens: usage.completion_tokens,
                                    total_tokens: usage.total_tokens,
                                },
                            });
                        }

                        for choice in chunk.choices {
                            let delta = choice.delta;

                            // Text delta
                            if let Some(content) = delta.content {
                                if !content.is_empty() {
                                    let _ = tx.send(Event::TextDelta(content));
                                }
                            }

                            // Tool call chunks
                            if let Some(tc_chunks) = delta.tool_calls {
                                for tc_chunk in tc_chunks {
                                    let index = tc_chunk.index;

                                    if let Some(id) = tc_chunk.id {
                                        // First chunk for this index — extract name too.
                                        let name = tc_chunk
                                            .function
                                            .as_ref()
                                            .and_then(|f| f.name.clone())
                                            .unwrap_or_default();

                                        pending.insert(
                                            index,
                                            (id.clone(), name.clone(), String::new()),
                                        );
                                        let _ = tx.send(Event::ToolCallBegin { id, name });
                                    } else if let Some(func) = tc_chunk.function {
                                        // Subsequent chunk — arguments delta.
                                        if let Some(args_delta) = func.arguments {
                                            if let Some((id, _, accumulated)) =
                                                pending.get_mut(&index)
                                            {
                                                accumulated.push_str(&args_delta);
                                                let _ = tx.send(Event::ToolCallDelta {
                                                    id: id.clone(),
                                                    arguments_delta: args_delta,
                                                });
                                            }
                                        }
                                    }
                                }
                            }

                            // When finish_reason is ToolCalls, emit ToolCallEnd for all pending.
                            if let Some(FinishReason::ToolCalls) = choice.finish_reason {
                                let mut indices: Vec<u32> = pending.keys().copied().collect();
                                indices.sort_unstable();
                                for idx in indices {
                                    if let Some((id, _, _)) = pending.remove(&idx) {
                                        let _ = tx.send(Event::ToolCallEnd { id });
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // If there was no usage chunk (e.g. stream ended without include_usage),
            // send a Done with zeroed usage so the stream always terminates cleanly.
            // We detect this by checking whether Done was already sent via usage above;
            // since we can't easily track that, we send it unconditionally only when
            // include_usage is true and the last chunk carries it — which is handled above.
            // Drop tx so the receiver sees EOF.
            drop(tx);
        });

        let stream = UnboundedReceiverStream::new(rx);
        Ok(StreamResponse::new(stream))
    }
}
