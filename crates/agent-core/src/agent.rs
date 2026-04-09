use std::collections::{BTreeMap, HashMap};

use futures::StreamExt;

use crate::error::Error;
use crate::event::Event;
use crate::guard::{Decision, Guard};
use crate::handler::{AgentEvent, Handler};
use crate::message::Message;
use crate::model::{Model, Request, ToolDefinition};
use crate::storage::Storage;
use crate::tool::{Tool, ToolOutput};

pub struct Agent<M, G, S>
where
    M: Model,
    G: Guard,
    S: Storage,
{
    model: M,
    guard: G,
    #[allow(dead_code)]
    storage: S,
    tools: BTreeMap<String, Box<dyn Tool>>,
    system_prompt: String,
    messages: Vec<Message>,
}

impl<M, G, S> Agent<M, G, S>
where
    M: Model,
    G: Guard,
    S: Storage,
{
    pub fn builder() -> AgentBuilder<M, G, S> {
        AgentBuilder::new()
    }

    pub async fn run(&mut self, user_input: &str, handler: &dyn Handler) -> Result<(), Error> {
        // Add user message to history
        self.messages.push(Message::User { content: user_input.to_string() });

        // Build tool definitions once — tools don't change during a run.
        let tool_definitions: Vec<ToolDefinition> = self
            .tools
            .values()
            .map(|t| ToolDefinition {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.schema(),
            })
            .collect();

        loop {
            // Build request
            let request = Request {
                system: if self.system_prompt.is_empty() {
                    None
                } else {
                    Some(self.system_prompt.clone())
                },
                messages: self.messages.clone(),
                tools: tool_definitions.clone(),
                temperature: None,
                max_tokens: None,
            };

            // Stream from model
            let mut stream = self.model.stream(request).await?;

            // Consume stream events
            let mut accumulated_text = String::new();
            // Map from id -> (name, accumulated_arguments)
            let mut pending_tool_calls: HashMap<String, (String, String)> = HashMap::new();
            let mut tool_call_order: Vec<String> = Vec::new();
            let mut usage = crate::event::Usage::default();

            while let Some(event) = stream.next().await {
                match event {
                    Event::TextDelta(ref delta) => {
                        handler.on_event(AgentEvent::TextDelta(delta.clone())).await;
                        accumulated_text.push_str(delta);
                    }
                    Event::ToolCallBegin { ref id, ref name } => {
                        if !pending_tool_calls.contains_key(id) {
                            tool_call_order.push(id.clone());
                        }
                        pending_tool_calls
                            .insert(id.clone(), (name.clone(), String::new()));
                    }
                    Event::ToolCallDelta { ref id, ref arguments_delta } => {
                        if let Some((_, args)) = pending_tool_calls.get_mut(id) {
                            args.push_str(arguments_delta);
                        }
                    }
                    Event::ToolCallEnd { id: _ } => {
                        // nothing extra needed
                    }
                    Event::Done { usage: u } => {
                        usage = u;
                    }
                }
            }

            // Build the tool_calls list in order
            let tool_calls: Vec<crate::message::ToolCall> = tool_call_order
                .iter()
                .filter_map(|id| {
                    pending_tool_calls.get(id).map(|(name, arguments)| {
                        crate::message::ToolCall {
                            id: id.clone(),
                            name: name.clone(),
                            arguments: arguments.clone(),
                        }
                    })
                })
                .collect();

            // Add assistant message to history
            let text = if accumulated_text.is_empty() {
                None
            } else {
                Some(accumulated_text)
            };
            self.messages.push(Message::Assistant {
                text,
                tool_calls: tool_calls.clone(),
            });

            // If no tool calls, we're done
            if tool_calls.is_empty() {
                handler.on_event(AgentEvent::TurnComplete { usage }).await;
                return Ok(());
            }

            // Execute each tool call
            for tc in &tool_calls {
                // Parse arguments
                let input: serde_json::Value = match serde_json::from_str(&tc.arguments) {
                    Ok(v) => v,
                    Err(e) => {
                        let output = ToolOutput::Error(format!("invalid arguments JSON: {e}"));
                        handler
                            .on_event(AgentEvent::ToolCallBegin {
                                id: tc.id.clone(),
                                name: tc.name.clone(),
                                arguments: tc.arguments.clone(),
                            })
                            .await;
                        handler
                            .on_event(AgentEvent::ToolCallEnd {
                                id: tc.id.clone(),
                                output: output.clone(),
                            })
                            .await;
                        self.messages.push(Message::Tool {
                            tool_call_id: tc.id.clone(),
                            content: output.to_string(),
                        });
                        continue;
                    }
                };

                // Check guard
                let decision = self.guard.check(&tc.name, &input).await;
                match decision {
                    Decision::Deny(reason) => {
                        handler
                            .on_event(AgentEvent::ToolCallDenied {
                                id: tc.id.clone(),
                                name: tc.name.clone(),
                                reason: reason.clone(),
                            })
                            .await;
                        let output = ToolOutput::Error(format!("denied: {reason}"));
                        self.messages.push(Message::Tool {
                            tool_call_id: tc.id.clone(),
                            content: output.to_string(),
                        });
                        continue;
                    }
                    Decision::NeedConfirm => {
                        let confirmed = handler.confirm(&tc.name, &input).await;
                        if !confirmed {
                            let reason = "user denied confirmation".to_string();
                            handler
                                .on_event(AgentEvent::ToolCallDenied {
                                    id: tc.id.clone(),
                                    name: tc.name.clone(),
                                    reason: reason.clone(),
                                })
                                .await;
                            let output = ToolOutput::Error(format!("denied: {reason}"));
                            self.messages.push(Message::Tool {
                                tool_call_id: tc.id.clone(),
                                content: output.to_string(),
                            });
                            continue;
                        }
                    }
                    Decision::Allow => {}
                }

                // Emit ToolCallBegin
                handler
                    .on_event(AgentEvent::ToolCallBegin {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    })
                    .await;

                // Execute tool
                let output = match self.tools.get(&tc.name) {
                    Some(tool) => match tool.call(input).await {
                        Ok(out) => out,
                        Err(e) => ToolOutput::Error(e.to_string()),
                    },
                    None => ToolOutput::Error(format!("unknown tool: {}", tc.name)),
                };

                // Emit ToolCallEnd
                handler
                    .on_event(AgentEvent::ToolCallEnd {
                        id: tc.id.clone(),
                        output: output.clone(),
                    })
                    .await;

                // Add tool result message to history
                self.messages.push(Message::Tool {
                    tool_call_id: tc.id.clone(),
                    content: output.to_string(),
                });
            }

            // Loop back to stream again with tool results
        }
    }
}

// ── Builder ──────────────────────────────────────────────────────────────────

pub struct AgentBuilder<M, G, S>
where
    M: Model,
    G: Guard,
    S: Storage,
{
    model: Option<M>,
    guard: Option<G>,
    storage: Option<S>,
    tools: BTreeMap<String, Box<dyn Tool>>,
    system_prompt: String,
}

impl<M, G, S> AgentBuilder<M, G, S>
where
    M: Model,
    G: Guard,
    S: Storage,
{
    fn new() -> Self {
        Self {
            model: None,
            guard: None,
            storage: None,
            tools: BTreeMap::new(),
            system_prompt: String::new(),
        }
    }

    pub fn model(mut self, model: M) -> Self {
        self.model = Some(model);
        self
    }

    pub fn guard(mut self, guard: G) -> Self {
        self.guard = Some(guard);
        self
    }

    pub fn storage(mut self, storage: S) -> Self {
        self.storage = Some(storage);
        self
    }

    pub fn tool(mut self, tool: impl Tool + 'static) -> Self {
        self.tools.insert(tool.name().to_string(), Box::new(tool));
        self
    }

    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    pub fn build(self) -> Agent<M, G, S> {
        Agent {
            model: self.model.expect("model is required"),
            guard: self.guard.expect("guard is required"),
            storage: self.storage.expect("storage is required"),
            tools: self.tools,
            system_prompt: self.system_prompt,
            messages: Vec::new(),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::event::{Event, Usage};
    use crate::guard::AutoGuard;
    use crate::storage::MemoryStorage;
    use crate::tool::RiskLevel;

    // ── MockModel ────────────────────────────────────────────────────────────

    struct MockModel {
        // Each call to stream() returns the next Vec<Event> in sequence
        responses: Arc<Mutex<Vec<Vec<Event>>>>,
    }

    impl MockModel {
        fn new(responses: Vec<Vec<Event>>) -> Self {
            Self {
                responses: Arc::new(Mutex::new(responses)),
            }
        }
    }

    #[async_trait::async_trait]
    impl Model for MockModel {
        async fn stream(&self, _request: Request) -> Result<crate::event::StreamResponse, Error> {
            let mut lock = self.responses.lock().unwrap();
            if lock.is_empty() {
                return Err(Error::Model("no more responses".to_string()));
            }
            let events = lock.remove(0);
            Ok(crate::event::StreamResponse::from_events(events))
        }
    }

    // ── MockTool ─────────────────────────────────────────────────────────────

    struct MockTool;

    #[async_trait::async_trait]
    impl Tool for MockTool {
        fn name(&self) -> &str {
            "mock_tool"
        }

        fn description(&self) -> &str {
            "A mock tool for testing."
        }

        fn schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "input": { "type": "string" }
                },
                "required": ["input"]
            })
        }

        fn risk_level(&self) -> RiskLevel {
            RiskLevel::Low
        }

        async fn call(&self, input: serde_json::Value) -> Result<ToolOutput, Error> {
            let text = input
                .get("input")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            Ok(ToolOutput::Text(format!("output: {text}")))
        }
    }

    // ── CollectingHandler ────────────────────────────────────────────────────

    struct CollectingHandler {
        events: Arc<Mutex<Vec<AgentEvent>>>,
    }

    impl CollectingHandler {
        fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn collected(&self) -> Vec<AgentEvent> {
            self.events.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl Handler for CollectingHandler {
        async fn on_event(&self, event: AgentEvent) {
            self.events.lock().unwrap().push(event);
        }

        async fn confirm(&self, _tool_name: &str, _input: &serde_json::Value) -> bool {
            true
        }
    }

    // ── Tests ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_agent_simple_text_response() {
        let model = MockModel::new(vec![vec![
            Event::TextDelta("Hello, world!".to_string()),
            Event::Done {
                usage: Usage {
                    prompt_tokens: 5,
                    completion_tokens: 3,
                    total_tokens: 8,
                },
            },
        ]]);

        let handler = CollectingHandler::new();

        let mut agent = Agent::builder()
            .model(model)
            .guard(AutoGuard)
            .storage(MemoryStorage::new())
            .system_prompt("You are helpful.")
            .build();

        agent.run("Hi", &handler).await.unwrap();

        let events = handler.collected();

        // Should have TextDelta and TurnComplete
        assert!(
            events.iter().any(|e| matches!(e, AgentEvent::TextDelta(s) if s == "Hello, world!")),
            "expected TextDelta with 'Hello, world!'"
        );
        assert!(
            events.iter().any(|e| matches!(e, AgentEvent::TurnComplete { .. })),
            "expected TurnComplete"
        );
    }

    #[tokio::test]
    async fn test_agent_tool_call_loop() {
        // First model call returns a tool call, second returns text
        let model = MockModel::new(vec![
            // Turn 1: model wants to call mock_tool
            vec![
                Event::ToolCallBegin {
                    id: "call_1".to_string(),
                    name: "mock_tool".to_string(),
                },
                Event::ToolCallDelta {
                    id: "call_1".to_string(),
                    arguments_delta: r#"{"input":"hello"}"#.to_string(),
                },
                Event::ToolCallEnd { id: "call_1".to_string() },
                Event::Done { usage: Usage::default() },
            ],
            // Turn 2: model returns text after seeing tool result
            vec![
                Event::TextDelta("Done!".to_string()),
                Event::Done {
                    usage: Usage {
                        prompt_tokens: 10,
                        completion_tokens: 1,
                        total_tokens: 11,
                    },
                },
            ],
        ]);

        let handler = CollectingHandler::new();

        let mut agent = Agent::builder()
            .model(model)
            .guard(AutoGuard)
            .storage(MemoryStorage::new())
            .tool(MockTool)
            .build();

        agent.run("Use mock_tool", &handler).await.unwrap();

        let events = handler.collected();

        // Should have ToolCallBegin with correct name
        assert!(
            events.iter().any(|e| matches!(
                e,
                AgentEvent::ToolCallBegin { id, name, .. }
                if id == "call_1" && name == "mock_tool"
            )),
            "expected ToolCallBegin for call_1/mock_tool"
        );

        // Should have ToolCallEnd with correct output
        assert!(
            events.iter().any(|e| matches!(
                e,
                AgentEvent::ToolCallEnd { id, output: ToolOutput::Text(s) }
                if id == "call_1" && s == "output: hello"
            )),
            "expected ToolCallEnd with 'output: hello'"
        );

        // Should have TextDelta from second model turn
        assert!(
            events.iter().any(|e| matches!(e, AgentEvent::TextDelta(s) if s == "Done!")),
            "expected TextDelta 'Done!'"
        );

        // Should end with TurnComplete
        assert!(
            events.iter().any(|e| matches!(e, AgentEvent::TurnComplete { .. })),
            "expected TurnComplete"
        );
    }
}
