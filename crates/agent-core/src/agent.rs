use std::collections::BTreeMap;
use std::path::PathBuf;

use futures::StreamExt;
use tracing::{Instrument, field};

use crate::event::{Event, Usage};
use crate::guard::{Decision, Guard};
use crate::handler::{AgentEvent, Handler};
use crate::instructions::load_agents;
use crate::skills::{SkillCatalog, SkillInfo};
use crate::storage::{SessionState, Storage};
use crate::tool::{Tool, ToolOutput};
use crate::{Error, Message, Model, Request, ToolCall, ToolDefinition};

pub struct Agent<M, G, S>
where
    M: Model,
    G: Guard,
    S: Storage,
{
    model: M,
    guard: G,
    storage: S,
    conversation_id: String,
    tools: BTreeMap<String, Box<dyn Tool>>,
    base_prompt: String,
    project_instructions: String,
    workdir: PathBuf,
    user_skills_dir: PathBuf,
    skills: SkillCatalog,
    context_window: u32,
    state: SessionState,
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

    pub fn transcript(&self) -> &[Message] {
        &self.state.transcript
    }

    pub fn skills(&self) -> &[SkillInfo] {
        &self.skills.skills
    }

    pub fn has_pending_request(&self) -> bool {
        self.state.pending_request.is_some()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.state.last_error.as_deref()
    }

    pub fn last_input_tokens(&self) -> u32 {
        self.state.last_input_tokens
    }

    pub fn compaction_count(&self) -> u64 {
        self.state.compaction_count
    }

    pub async fn run(&mut self, user_input: &str, handler: &dyn Handler) -> Result<(), Error> {
        let input = json_string(&serde_json::json!({"user_input": user_input}));
        let span = tracing::info_span!(
            "agent.run",
            "langfuse.trace.name" = "agent.run",
            "langfuse.session.id" = %self.conversation_id,
            "langfuse.environment" = "development",
            "langfuse.observation.type" = "span",
            "langfuse.observation.input" = %input,
            "langfuse.observation.output" = field::Empty,
            "langfuse.observation.level" = field::Empty,
            "langfuse.observation.status_message" = field::Empty,
            "otel.status_code" = field::Empty,
            "otel.status_message" = field::Empty,
            "langfuse.observation.metadata.workdir" = %self.workdir.display(),
        );
        let result = self
            .run_inner(user_input, handler)
            .instrument(span.clone())
            .await;
        record_result(&span, &result);
        result.map(|_| ())
    }

    async fn run_inner(
        &mut self,
        user_input: &str,
        handler: &dyn Handler,
    ) -> Result<String, Error> {
        if self.state.pending_request.is_some() {
            let error = Error::Other(
                "a failed request is pending; retry it before submitting a new message".into(),
            );
            handler
                .on_event(AgentEvent::Failed {
                    error: error.to_string(),
                    retryable: true,
                })
                .await;
            return Err(error);
        }

        let skills_span = tracing::info_span!(
            "skills.discover",
            "langfuse.observation.type" = "span",
            "langfuse.observation.input" = field::Empty,
            "langfuse.observation.output" = field::Empty,
        );
        let skills_input = json_string(&serde_json::json!({
            "workdir": self.workdir,
            "user_skills_dir": self.user_skills_dir,
        }));
        skills_span.record("langfuse.observation.input", skills_input.as_str());
        self.skills = skills_span.in_scope(|| {
            SkillCatalog::scan(&self.workdir, &self.user_skills_dir, self.context_window)
        });
        let skills_output = json_string(&serde_json::json!({
            "available_skills": self.skills.skills.iter().map(|skill| serde_json::json!({
                "name": skill.name,
                "description": skill.description,
                "path": skill.path,
                "allow_implicit_invocation": skill.allow_implicit_invocation,
            })).collect::<Vec<_>>(),
            "injected_prompt": self.skills.prompt(),
            "warnings": self.skills.warnings,
        }));
        skills_span.record("langfuse.observation.output", skills_output.as_str());
        drop(skills_span);

        handler
            .on_event(AgentEvent::SkillsUpdated {
                skills: self.skills.skills.clone(),
                warnings: self.skills.warnings.clone(),
            })
            .await;
        self.skills.validate_explicit(user_input)?;

        self.state.transcript.push(Message::User {
            content: user_input.to_owned(),
        });
        let mut input = self.state.context.clone();
        input.push(serde_json::json!({
            "role": "user",
            "content": user_input
        }));
        self.set_pending(input).await?;
        self.execute_pending(handler).await
    }

    pub async fn retry(&mut self, handler: &dyn Handler) -> Result<(), Error> {
        let input = json_string(&self.state.pending_request);
        let span = tracing::info_span!(
            "agent.retry",
            "langfuse.trace.name" = "agent.retry",
            "langfuse.session.id" = %self.conversation_id,
            "langfuse.environment" = "development",
            "langfuse.observation.type" = "span",
            "langfuse.observation.input" = %input,
            "langfuse.observation.output" = field::Empty,
            "langfuse.observation.level" = field::Empty,
            "langfuse.observation.status_message" = field::Empty,
            "otel.status_code" = field::Empty,
            "otel.status_message" = field::Empty,
        );
        let result = async {
            if self.state.pending_request.is_none() {
                return Err(Error::Other("there is no pending request to retry".into()));
            }
            self.execute_pending(handler).await
        }
        .instrument(span.clone())
        .await;
        record_result(&span, &result);
        result.map(|_| ())
    }

    async fn set_pending(&mut self, input: Vec<serde_json::Value>) -> Result<(), Error> {
        self.state.pending_request = Some(Request {
            instructions: self.instructions(),
            input,
            tools: self.tool_definitions(),
            compact_threshold: ((self.context_window as u64 * 8) / 10) as u32,
        });
        self.state.last_error = None;
        self.save().await
    }

    async fn execute_pending(&mut self, handler: &dyn Handler) -> Result<String, Error> {
        loop {
            let request = self
                .state
                .pending_request
                .clone()
                .ok_or_else(|| Error::Other("pending request disappeared".into()))?;
            let result = self.execute_request(request.clone(), handler).await;
            let (text, output, usage) = match result {
                Ok(result) => result,
                Err(error) => {
                    self.state.last_error = Some(error.to_string());
                    self.save().await?;
                    handler
                        .on_event(AgentEvent::Failed {
                            error: error.to_string(),
                            retryable: true,
                        })
                        .await;
                    return Err(error);
                }
            };

            let compacted = output
                .iter()
                .filter(|item| item["type"] == "compaction")
                .count() as u64;
            let context_before = (compacted > 0).then(|| json_string(&request.input));
            let context_items_before = request.input.len();
            self.state.context = request.input;
            self.state.context.extend(output.clone());
            if let Some(index) = self
                .state
                .context
                .iter()
                .rposition(|item| item["type"] == "compaction")
            {
                self.state.context.drain(..index);
            }
            self.state.compaction_count += compacted;
            self.state.last_input_tokens = usage.input_tokens;
            self.state.pending_request = None;
            self.state.last_error = None;

            if compacted > 0 {
                let before = context_before.unwrap_or_default();
                let after = json_string(&self.state.context);
                let compaction_span = tracing::info_span!(
                    "context.compact",
                    "langfuse.observation.type" = "span",
                    "langfuse.observation.input" = %before,
                    "langfuse.observation.output" = %after,
                    "langfuse.observation.metadata.compactionItems" = compacted,
                    "langfuse.observation.metadata.contextItemsBefore" = context_items_before as u64,
                    "langfuse.observation.metadata.contextItemsAfter" = self.state.context.len() as u64,
                    "langfuse.observation.metadata.inputTokens" = usage.input_tokens as u64,
                );
                compaction_span.in_scope(|| {});
            }

            let tool_calls = function_calls(&output);
            self.state.transcript.push(Message::Assistant {
                text: (!text.is_empty()).then_some(text.clone()),
                tool_calls: tool_calls.clone(),
            });
            self.save().await?;

            if compacted > 0 {
                handler
                    .on_event(AgentEvent::Compacted {
                        total: self.state.compaction_count,
                    })
                    .await;
            }
            if tool_calls.is_empty() {
                handler
                    .on_event(AgentEvent::TurnComplete {
                        usage,
                        context_window: self.context_window,
                    })
                    .await;
                return Ok(text);
            }

            self.execute_tools(&tool_calls, handler).await;
            self.save().await?;
            self.set_pending(self.state.context.clone()).await?;
        }
    }

    async fn execute_request(
        &self,
        request: Request,
        handler: &dyn Handler,
    ) -> Result<(String, Vec<serde_json::Value>, Usage), Error> {
        let observed_input = json_string(&serde_json::json!({
            "instructions": request.instructions,
            "input": request.input,
            "tools": request.tools,
            "compact_threshold": request.compact_threshold,
        }));
        let model = self.model.model_name().unwrap_or("unknown");
        let span = tracing::info_span!(
            "llm.call",
            "langfuse.observation.type" = "generation",
            "langfuse.observation.input" = %observed_input,
            "langfuse.observation.output" = field::Empty,
            "langfuse.observation.model.name" = %model,
            "langfuse.observation.usage_details" = field::Empty,
            "langfuse.observation.level" = field::Empty,
            "langfuse.observation.status_message" = field::Empty,
            "otel.status_code" = field::Empty,
            "otel.status_message" = field::Empty,
            "gen_ai.operation.name" = "chat",
            "gen_ai.request.model" = %model,
            "gen_ai.usage.input_tokens" = field::Empty,
            "gen_ai.usage.output_tokens" = field::Empty,
        );
        let result = async {
            let mut stream = self.model.stream(request).await?;
            let mut text = String::new();
            let mut output = Vec::new();
            let mut usage = None;
            while let Some(event) = stream.next().await {
                match event? {
                    Event::TextDelta(delta) => {
                        text.push_str(&delta);
                        handler.on_event(AgentEvent::TextDelta(delta)).await;
                    }
                    Event::OutputItem(item) => output.push(item),
                    Event::Done { usage: value } => usage = Some(value),
                }
            }
            let usage = usage
                .ok_or_else(|| Error::Model("response stream ended before completion".into()))?;
            Ok((text, output, usage))
        }
        .instrument(span.clone())
        .await;

        match &result {
            Ok((text, output, usage)) => {
                let observed_output = json_string(&serde_json::json!({
                    "text": text,
                    "items": output,
                }));
                let uncached_input = usage.input_tokens.saturating_sub(usage.cached_input_tokens);
                let usage_details = json_string(&serde_json::json!({
                    "input": uncached_input,
                    "output": usage.output_tokens,
                    "total": usage.total_tokens,
                    "cached_tokens": usage.cached_input_tokens,
                }));
                span.record("langfuse.observation.output", observed_output.as_str());
                span.record("langfuse.observation.usage_details", usage_details.as_str());
                span.record("gen_ai.usage.input_tokens", usage.input_tokens as u64);
                span.record("gen_ai.usage.output_tokens", usage.output_tokens as u64);
            }
            Err(error) => record_error(&span, error),
        }
        result
    }

    async fn execute_tools(&mut self, calls: &[ToolCall], handler: &dyn Handler) {
        for call in calls {
            handler
                .on_event(AgentEvent::ToolCallBegin {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                })
                .await;

            let observed_input = json_string(&serde_json::json!({
                "id": call.id,
                "name": call.name,
                "arguments": call.arguments,
            }));
            let span = tracing::info_span!(
                "tool.execute",
                "langfuse.observation.type" = "span",
                "langfuse.observation.input" = %observed_input,
                "langfuse.observation.output" = field::Empty,
                "langfuse.observation.level" = field::Empty,
                "langfuse.observation.status_message" = field::Empty,
                "otel.status_code" = field::Empty,
                "otel.status_message" = field::Empty,
                "langfuse.observation.metadata.toolName" = %call.name,
                "gen_ai.tool.call.id" = %call.id,
                "gen_ai.tool.call.name" = %call.name,
                "gen_ai.tool.call.arguments" = %call.arguments,
                "gen_ai.tool.call.result" = field::Empty,
            );
            let output = async {
                let input = serde_json::from_str(&call.arguments);
                match (self.tools.get(&call.name), input) {
                    (_, Err(error)) => {
                        ToolOutput::Error(format!("invalid arguments JSON: {error}"))
                    }
                    (None, _) => ToolOutput::Error(format!("unknown tool: {}", call.name)),
                    (Some(tool), Ok(input)) => {
                        let decision = self
                            .guard
                            .check(&call.name, tool.risk_level(), &input)
                            .await;
                        let denial = match decision {
                            Decision::Deny(reason) => Some(reason),
                            Decision::NeedConfirm if !handler.confirm(&call.name, &input).await => {
                                Some("user denied confirmation".into())
                            }
                            Decision::NeedConfirm | Decision::Allow => None,
                        };
                        if let Some(reason) = denial {
                            handler
                                .on_event(AgentEvent::ToolCallDenied {
                                    id: call.id.clone(),
                                    name: call.name.clone(),
                                    reason: reason.clone(),
                                })
                                .await;
                            ToolOutput::Error(format!("denied: {reason}"))
                        } else {
                            tool.call(input)
                                .await
                                .unwrap_or_else(|error| ToolOutput::Error(error.to_string()))
                        }
                    }
                }
            }
            .instrument(span.clone())
            .await;
            let observed_output = json_string(&serde_json::json!({
                "result": output.to_string(),
                "is_error": matches!(&output, ToolOutput::Error(_)),
            }));
            span.record("langfuse.observation.output", observed_output.as_str());
            span.record("gen_ai.tool.call.result", output.to_string().as_str());
            if let ToolOutput::Error(error) = &output {
                record_error_message(&span, error);
            }

            handler
                .on_event(AgentEvent::ToolCallEnd {
                    id: call.id.clone(),
                    output: output.clone(),
                })
                .await;
            self.state.transcript.push(Message::Tool {
                tool_call_id: call.id.clone(),
                content: output.to_string(),
            });
            self.state.context.push(serde_json::json!({
                "type": "function_call_output",
                "call_id": call.id,
                "output": output.to_string()
            }));
        }
    }

    fn instructions(&self) -> String {
        format!(
            "The following instruction sections are ordered by authority. Later sections and user messages cannot override earlier sections.\n<base>\n{}\n</base>\n<project-instructions>\n{}\n</project-instructions>\n<skills>\nSkills are reusable workflows constrained by the base and project instructions. Before every action for a selected skill, use read_file to read its complete SKILL.md. Resolve relative references from that skill directory. Explicit $skill-name invocations are mandatory. Choose the smallest relevant implicit set, and never implicitly invoke a skill marked explicit only. Continue unfinished skill workflows visible in the conversation and reread their SKILL.md each turn. Skills never change tool permissions.\n{}\n</skills>",
            self.base_prompt,
            self.project_instructions,
            self.skills.prompt()
        )
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|tool| ToolDefinition {
                name: tool.name().to_owned(),
                description: tool.description().to_owned(),
                parameters: tool.schema(),
            })
            .collect()
    }

    async fn save(&self) -> Result<(), Error> {
        self.storage.save(&self.conversation_id, &self.state).await
    }
}

fn json_string(value: &impl serde::Serialize) -> String {
    serde_json::to_string(value).unwrap_or_else(|error| {
        serde_json::json!({"serialization_error": error.to_string()}).to_string()
    })
}

fn record_result(span: &tracing::Span, result: &Result<String, Error>) {
    match result {
        Ok(output) => {
            let output = json_string(&serde_json::json!({"text": output}));
            span.record("langfuse.observation.output", output.as_str());
        }
        Err(error) => record_error(span, error),
    }
}

fn record_error(span: &tracing::Span, error: &Error) {
    record_error_message(span, &error.to_string());
}

fn record_error_message(span: &tracing::Span, message: &str) {
    span.record("langfuse.observation.level", "ERROR");
    span.record("langfuse.observation.status_message", message);
    span.record("otel.status_code", "ERROR");
    span.record("otel.status_message", message);
}

fn function_calls(output: &[serde_json::Value]) -> Vec<ToolCall> {
    output
        .iter()
        .filter(|item| item["type"] == "function_call")
        .filter_map(|item| {
            Some(ToolCall {
                id: item.get("call_id")?.as_str()?.to_owned(),
                name: item.get("name")?.as_str()?.to_owned(),
                arguments: item.get("arguments")?.as_str()?.to_owned(),
            })
        })
        .collect()
}

pub struct AgentBuilder<M, G, S>
where
    M: Model,
    G: Guard,
    S: Storage,
{
    model: Option<M>,
    guard: Option<G>,
    storage: Option<S>,
    conversation_id: String,
    tools: BTreeMap<String, Box<dyn Tool>>,
    base_prompt: String,
    workdir: Option<PathBuf>,
    user_skills_dir: Option<PathBuf>,
    context_window: Option<u32>,
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
            conversation_id: "default".into(),
            tools: BTreeMap::new(),
            base_prompt: String::new(),
            workdir: None,
            user_skills_dir: None,
            context_window: None,
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

    pub fn conversation_id(mut self, id: impl Into<String>) -> Self {
        self.conversation_id = id.into();
        self
    }

    pub fn tool(mut self, tool: impl Tool + 'static) -> Self {
        self.tools.insert(tool.name().to_owned(), Box::new(tool));
        self
    }

    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.base_prompt = prompt.into();
        self
    }

    pub fn workdir(mut self, path: impl Into<PathBuf>) -> Self {
        self.workdir = Some(path.into());
        self
    }

    pub fn user_skills_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.user_skills_dir = Some(path.into());
        self
    }

    pub fn context_window(mut self, tokens: u32) -> Self {
        self.context_window = Some(tokens);
        self
    }

    pub async fn build(self) -> Result<Agent<M, G, S>, Error> {
        let workdir = self
            .workdir
            .ok_or_else(|| Error::Other("workdir is required".into()))?;
        let context_window = self
            .context_window
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                Error::Other("model.context_window must be a positive integer".into())
            })?;
        let storage = self
            .storage
            .ok_or_else(|| Error::Other("storage is required".into()))?;
        let state = storage.load(&self.conversation_id).await?;
        let project_instructions = load_agents(&workdir)?;
        let user_skills_dir = self.user_skills_dir.unwrap_or_default();
        let skills = SkillCatalog::scan(&workdir, &user_skills_dir, context_window);
        Ok(Agent {
            model: self
                .model
                .ok_or_else(|| Error::Other("model is required".into()))?,
            guard: self
                .guard
                .ok_or_else(|| Error::Other("guard is required".into()))?,
            storage,
            conversation_id: self.conversation_id,
            tools: self.tools,
            base_prompt: self.base_prompt,
            project_instructions,
            workdir,
            user_skills_dir,
            skills,
            context_window,
            state,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::{AutoGuard, MemoryStorage, RiskLevel, StreamResponse};

    type MockResponses = Arc<Mutex<Vec<Result<Vec<Event>, String>>>>;

    #[derive(Clone)]
    struct MockModel {
        responses: MockResponses,
        requests: Arc<Mutex<Vec<Request>>>,
    }

    #[async_trait::async_trait]
    impl Model for MockModel {
        async fn stream(&self, request: Request) -> Result<StreamResponse, Error> {
            self.requests.lock().unwrap().push(request);
            match self.responses.lock().unwrap().remove(0) {
                Ok(events) => Ok(StreamResponse::from_events(events)),
                Err(error) => Err(Error::Model(error)),
            }
        }
    }

    struct NoopHandler;

    #[async_trait::async_trait]
    impl Handler for NoopHandler {
        async fn on_event(&self, _: AgentEvent) {}
        async fn confirm(&self, _: &str, _: &serde_json::Value) -> bool {
            false
        }
    }

    #[derive(Default)]
    struct RecordingHandler {
        events: Mutex<Vec<AgentEvent>>,
    }

    #[async_trait::async_trait]
    impl Handler for RecordingHandler {
        async fn on_event(&self, event: AgentEvent) {
            self.events.lock().unwrap().push(event);
        }

        async fn confirm(&self, _: &str, _: &serde_json::Value) -> bool {
            false
        }
    }

    struct EchoTool;

    #[async_trait::async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "echo input"
        }
        fn schema(&self) -> serde_json::Value {
            serde_json::json!({"type":"object"})
        }
        fn risk_level(&self) -> RiskLevel {
            RiskLevel::Low
        }
        async fn call(&self, input: serde_json::Value) -> Result<ToolOutput, Error> {
            Ok(ToolOutput::Text(input.to_string()))
        }
    }

    fn model(responses: Vec<Result<Vec<Event>, String>>) -> MockModel {
        MockModel {
            responses: Arc::new(Mutex::new(responses)),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    async fn agent(
        model: MockModel,
        storage: MemoryStorage,
        workdir: &Path,
    ) -> Agent<MockModel, AutoGuard, MemoryStorage> {
        Agent::builder()
            .model(model)
            .guard(AutoGuard)
            .storage(storage)
            .workdir(workdir)
            .user_skills_dir(workdir.join("user-skills"))
            .context_window(100)
            .build()
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn compaction_prunes_only_canonical_context_and_preserves_transcript() {
        let workdir = tempfile::tempdir().unwrap();
        let storage = MemoryStorage::new();
        let model = model(vec![Ok(vec![
            Event::TextDelta("done".into()),
            Event::OutputItem(
                serde_json::json!({"type":"compaction","encrypted_content":"opaque"}),
            ),
            Event::OutputItem(
                serde_json::json!({"type":"message","role":"assistant","content":[]}),
            ),
            Event::Done {
                usage: Usage {
                    input_tokens: 81,
                    output_tokens: 2,
                    total_tokens: 83,
                    cached_input_tokens: 0,
                },
            },
        ])]);
        let mut agent = agent(model, storage.clone(), workdir.path()).await;

        agent.run("remember me", &NoopHandler).await.unwrap();
        let saved = storage.load("default").await.unwrap();
        assert!(
            matches!(&saved.transcript[0], Message::User { content } if content == "remember me")
        );
        assert_eq!(saved.context[0]["type"], "compaction");
        assert_eq!(saved.compaction_count, 1);
        assert!(saved.pending_request.is_none());
    }

    #[tokio::test]
    async fn failed_request_is_persisted_and_retry_is_exact() {
        let workdir = tempfile::tempdir().unwrap();
        let storage = MemoryStorage::new();
        let model = model(vec![
            Err("network".into()),
            Ok(vec![Event::Done {
                usage: Usage::default(),
            }]),
        ]);
        let requests = model.requests.clone();
        let mut agent = agent(model, storage.clone(), workdir.path()).await;

        assert!(agent.run("once", &NoopHandler).await.is_err());
        assert!(
            storage
                .load("default")
                .await
                .unwrap()
                .pending_request
                .is_some()
        );
        agent.retry(&NoopHandler).await.unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests[0], requests[1]);
        assert_eq!(
            agent
                .transcript()
                .iter()
                .filter(|message| matches!(message, Message::User { .. }))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn submitting_while_retry_is_pending_emits_failure() {
        let workdir = tempfile::tempdir().unwrap();
        let storage = MemoryStorage::new();
        let mut agent = agent(model(vec![Err("network".into())]), storage, workdir.path()).await;
        let handler = RecordingHandler::default();

        assert!(agent.run("once", &handler).await.is_err());
        let before = handler.events.lock().unwrap().len();
        assert!(agent.run("twice", &handler).await.is_err());
        let events = handler.events.lock().unwrap();

        assert!(events[before..].iter().any(|event| matches!(
            event,
            AgentEvent::Failed {
                retryable: true,
                ..
            }
        )));
    }

    #[tokio::test]
    async fn completed_function_call_and_output_are_chained_to_next_response() {
        let workdir = tempfile::tempdir().unwrap();
        let storage = MemoryStorage::new();
        let model = model(vec![
            Ok(vec![
                Event::OutputItem(serde_json::json!({
                    "type":"function_call",
                    "call_id":"call-1",
                    "name":"echo",
                    "arguments":"{\"value\":1}"
                })),
                Event::Done {
                    usage: Usage::default(),
                },
            ]),
            Ok(vec![
                Event::TextDelta("done".into()),
                Event::OutputItem(
                    serde_json::json!({"type":"message","role":"assistant","content":[]}),
                ),
                Event::Done {
                    usage: Usage::default(),
                },
            ]),
        ]);
        let requests = model.requests.clone();
        let mut agent = Agent::builder()
            .model(model)
            .guard(AutoGuard)
            .storage(storage)
            .workdir(workdir.path())
            .user_skills_dir(workdir.path().join("skills"))
            .context_window(100)
            .tool(EchoTool)
            .build()
            .await
            .unwrap();

        agent.run("go", &NoopHandler).await.unwrap();
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests[1]
                .input
                .iter()
                .any(|item| item["type"] == "function_call_output" && item["call_id"] == "call-1")
        );
        assert_eq!(agent.transcript().len(), 4);
    }
}
