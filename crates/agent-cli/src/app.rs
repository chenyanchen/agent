use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use async_openai::config::OpenAIConfig;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::{mpsc, oneshot};

use agent_core::{Agent, AgentEvent, FileStorage, Handler, Message, OpenAIModel, Storage};
use agent_tools::{EditFileTool, GlobTool, GrepTool, ReadFileTool, ShellTool, WriteFileTool};

use crate::config::Config;
use crate::input::InputBuffer;
use crate::ui::{self, ChatEntry};

// ── App state ─────────────────────────────────────────────────────────────────

pub struct App {
    pub input: InputBuffer,
    pub chat_history: Vec<ChatEntry>,
    pub streaming_text: String,
    /// How many lines from the bottom the chat view is scrolled up.
    pub scroll_offset: usize,
    pub is_running: bool,
    pub model_id: String,
    pub total_tokens: u32,
    pub should_quit: bool,
    pub confirmation: Option<PendingConfirmation>,
}

struct ConfirmationRequest {
    name: String,
    input: serde_json::Value,
    response: oneshot::Sender<bool>,
}

pub struct PendingConfirmation {
    pub name: String,
    pub arguments: String,
    pub allow_selected: bool,
    response: oneshot::Sender<bool>,
}

impl PendingConfirmation {
    fn new(name: String, input: serde_json::Value, response: oneshot::Sender<bool>) -> Self {
        Self {
            name,
            arguments: input.to_string(),
            allow_selected: false,
            response,
        }
    }

    fn respond(self, allowed: bool) {
        let _ = self.response.send(allowed);
    }
}

impl App {
    fn new(model_id: impl Into<String>) -> Self {
        Self {
            input: InputBuffer::new(),
            chat_history: Vec::new(),
            streaming_text: String::new(),
            scroll_offset: 0,
            is_running: false,
            model_id: model_id.into(),
            total_tokens: 0,
            should_quit: false,
            confirmation: None,
        }
    }

    // ── Entry point ───────────────────────────────────────────────────────────

    /// Initialise the terminal, spawn the agent background task, and run the
    /// main event loop.  Blocks until the user quits.
    pub fn run(config: Config, session: String) -> io::Result<()> {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async { Self::run_async(config, session).await })
    }

    async fn run_async(config: Config, session: String) -> io::Result<()> {
        // ── Channels ──────────────────────────────────────────────────────────
        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<String>();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AgentEvent>();
        let (confirmation_tx, mut confirmation_rx) =
            mpsc::unbounded_channel::<ConfirmationRequest>();

        // ── Spawn agent task ──────────────────────────────────────────────────
        let system_prompt = config
            .system_prompt
            .clone()
            .unwrap_or_else(|| "You are a general-purpose AI assistant with access to shell, file, and search tools. Be concise and helpful.".into());

        let model_id = config.model.model_id.clone();
        let api_key = config.model.api_key.clone();
        let api_base = config.model.api_base.clone();
        let guard_mode = config.guard.mode.clone();
        let event_tx_clone = event_tx.clone();
        let storage_dir = sessions_dir()?;
        let storage = FileStorage::new(storage_dir);
        let history = storage
            .load(&session)
            .await
            .map_err(|error| io::Error::other(error.to_string()))?;

        tokio::spawn(async move {
            // Build OpenAI config
            let mut oai_cfg = OpenAIConfig::new();
            if let Some(key) = api_key {
                oai_cfg = oai_cfg.with_api_key(key);
            }
            if let Some(base) = api_base {
                oai_cfg = oai_cfg.with_api_base(base);
            }

            let model = OpenAIModel::with_config(model_id, oai_cfg);

            let mut agent = Agent::builder()
                .model(model)
                .guard(guard_mode)
                .storage(storage)
                .conversation_id(session)
                .system_prompt(&system_prompt)
                .tool(ShellTool)
                .tool(ReadFileTool)
                .tool(WriteFileTool)
                .tool(EditFileTool)
                .tool(GlobTool)
                .tool(GrepTool)
                .build();

            let handler = TuiHandler {
                tx: event_tx_clone.clone(),
                confirmation_tx,
            };
            while let Some(user_input) = input_rx.recv().await {
                if let Err(e) = agent.run(&user_input, &handler).await {
                    let _ = event_tx_clone.send(AgentEvent::TurnComplete {
                        usage: agent_core::Usage::default(),
                    });
                    // Surface the error as a chat entry via a special path;
                    // we encode it into a ToolCallDenied so app can detect it.
                    let _ = event_tx_clone.send(AgentEvent::ToolCallDenied {
                        id: "__error__".into(),
                        name: "__error__".into(),
                        reason: e.to_string(),
                    });
                }
            }
        });

        // ── Terminal setup ────────────────────────────────────────────────────
        //
        // We intentionally do NOT enable mouse capture: we never consume mouse
        // events, and capturing them would prevent the terminal emulator from
        // handling native click-and-drag text selection.
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = Terminal::new(backend)?;

        let mut app = App::new(&config.model.model_id);
        app.chat_history = restore_chat_history(history);

        let result = run_loop(
            &mut terminal,
            &mut app,
            &input_tx,
            &mut event_rx,
            &mut confirmation_rx,
        )
        .await;

        // ── Terminal cleanup ──────────────────────────────────────────────────
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        result
    }
}

pub(crate) fn sessions_dir() -> io::Result<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(".agent").join("sessions"))
        .ok_or_else(|| io::Error::other("home directory not found"))
}

fn restore_chat_history(messages: Vec<Message>) -> Vec<ChatEntry> {
    let mut entries = Vec::new();
    let mut tool_names = HashMap::new();

    for message in messages {
        match message {
            Message::System { .. } => {}
            Message::User { content } => entries.push(ChatEntry::User(content)),
            Message::Assistant { text, tool_calls } => {
                if let Some(text) = text
                    && !text.is_empty()
                {
                    entries.push(ChatEntry::Assistant(text));
                }
                for call in tool_calls {
                    tool_names.insert(call.id, call.name.clone());
                    entries.push(ChatEntry::ToolCall {
                        name: call.name,
                        arguments: call.arguments,
                    });
                }
            }
            Message::Tool {
                tool_call_id,
                content,
            } => entries.push(ChatEntry::ToolResult {
                name: tool_names.get(&tool_call_id).cloned().unwrap_or_default(),
                output: content,
            }),
        }
    }

    entries
}

// ── Main event loop ───────────────────────────────────────────────────────────

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    input_tx: &mpsc::UnboundedSender<String>,
    event_rx: &mut mpsc::UnboundedReceiver<AgentEvent>,
    confirmation_rx: &mut mpsc::UnboundedReceiver<ConfirmationRequest>,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // Drain all pending agent events (non-blocking).
        while let Ok(agent_event) = event_rx.try_recv() {
            handle_agent_event(app, agent_event);
        }
        while let Ok(request) = confirmation_rx.try_recv() {
            app.confirmation = Some(PendingConfirmation::new(
                request.name,
                request.input,
                request.response,
            ));
        }

        // Poll for terminal input with a short timeout so we stay responsive
        // to both keyboard events and agent streaming events.
        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()?
        {
            if handle_confirmation_key(app, key) {
                if app.should_quit {
                    break;
                }
                continue;
            }
            match (key.code, key.modifiers) {
                // Quit on Ctrl+C
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                    app.should_quit = true;
                }
                // Submit on Enter (only when not already running)
                (KeyCode::Enter, _) => {
                    if !app.is_running && !app.input.is_empty() {
                        let text = app.input.take();
                        app.chat_history.push(ChatEntry::User(text.clone()));
                        app.is_running = true;
                        app.scroll_offset = 0;
                        let _ = input_tx.send(text);
                    }
                }
                // Text editing
                (KeyCode::Char(ch), _) => {
                    app.input.insert(ch);
                }
                (KeyCode::Backspace, _) => {
                    app.input.backspace();
                }
                (KeyCode::Left, _) => {
                    app.input.move_left();
                }
                (KeyCode::Right, _) => {
                    app.input.move_right();
                }
                // Scroll chat history
                (KeyCode::Up, _) | (KeyCode::PageUp, _) => {
                    app.scroll_offset = app.scroll_offset.saturating_add(3);
                }
                (KeyCode::Down, _) | (KeyCode::PageDown, _) => {
                    app.scroll_offset = app.scroll_offset.saturating_sub(3);
                }
                _ => {}
            }
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn handle_confirmation_key(app: &mut App, key: KeyEvent) -> bool {
    let Some(confirmation) = app.confirmation.as_mut() else {
        return false;
    };

    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
            app.confirmation.take().unwrap().respond(false);
            app.should_quit = true;
        }
        (KeyCode::Char('y'), _) => app.confirmation.take().unwrap().respond(true),
        (KeyCode::Char('n'), _) | (KeyCode::Esc, _) => {
            app.confirmation.take().unwrap().respond(false);
        }
        (KeyCode::Enter, _) => {
            let allowed = confirmation.allow_selected;
            app.confirmation.take().unwrap().respond(allowed);
        }
        (KeyCode::Up, _) | (KeyCode::Down, _) => {
            confirmation.allow_selected = !confirmation.allow_selected;
        }
        (KeyCode::PageUp, _) => {
            app.scroll_offset = app.scroll_offset.saturating_add(3);
        }
        (KeyCode::PageDown, _) => {
            app.scroll_offset = app.scroll_offset.saturating_sub(3);
        }
        _ => {}
    }
    true
}

// ── Agent event handler ───────────────────────────────────────────────────────

fn handle_agent_event(app: &mut App, event: AgentEvent) {
    match event {
        AgentEvent::TextDelta(delta) => {
            app.streaming_text.push_str(&delta);
        }
        AgentEvent::ToolCallBegin {
            name, arguments, ..
        } => {
            app.chat_history
                .push(ChatEntry::ToolCall { name, arguments });
        }
        AgentEvent::ToolCallEnd { id, output } => {
            // Ignore internal error sentinels
            if id == "__error__" {
                return;
            }
            let output_str = match &output {
                agent_core::ToolOutput::Text(t) => t.clone(),
                agent_core::ToolOutput::Error(e) => format!("error: {e}"),
            };
            // Pair with the most recently added ToolCall entry to get the name.
            let name = app
                .chat_history
                .iter()
                .rev()
                .find_map(|e| {
                    if let ChatEntry::ToolCall { name, .. } = e {
                        Some(name.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            app.chat_history.push(ChatEntry::ToolResult {
                name,
                output: output_str,
            });
        }
        AgentEvent::ToolCallDenied { id, name, reason } => {
            if id == "__error__" {
                // Propagated agent error
                app.chat_history.push(ChatEntry::Error(reason));
            } else {
                app.chat_history
                    .push(ChatEntry::Error(format!("Tool `{name}` denied: {reason}")));
            }
        }
        AgentEvent::TurnComplete { usage } => {
            // Flush any accumulated streaming text to the chat history.
            if !app.streaming_text.is_empty() {
                let text = std::mem::take(&mut app.streaming_text);
                app.chat_history.push(ChatEntry::Assistant(text));
            }
            app.total_tokens = usage.total_tokens;
            app.is_running = false;
            app.scroll_offset = 0;
        }
    }
}

// ── TuiHandler ────────────────────────────────────────────────────────────────

pub struct TuiHandler {
    pub tx: mpsc::UnboundedSender<AgentEvent>,
    confirmation_tx: mpsc::UnboundedSender<ConfirmationRequest>,
}

#[async_trait::async_trait]
impl Handler for TuiHandler {
    async fn on_event(&self, event: AgentEvent) {
        let _ = self.tx.send(event);
    }

    async fn confirm(&self, tool_name: &str, input: &serde_json::Value) -> bool {
        let (response, receiver) = oneshot::channel();
        if self
            .confirmation_tx
            .send(ConfirmationRequest {
                name: tool_name.to_string(),
                input: input.clone(),
                response,
            })
            .is_err()
        {
            return false;
        }
        receiver.await.unwrap_or(false)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::ToolCall;

    #[test]
    fn confirmation_defaults_to_deny_and_can_be_allowed() {
        let mut app = App::new("test-model");
        let (response_tx, mut response_rx) = tokio::sync::oneshot::channel();
        app.confirmation = Some(PendingConfirmation::new(
            "shell".to_string(),
            serde_json::json!({"command": "cargo test"}),
            response_tx,
        ));

        handle_confirmation_key(
            &mut app,
            crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(response_rx.try_recv(), Ok(false));

        let (response_tx, mut response_rx) = tokio::sync::oneshot::channel();
        app.confirmation = Some(PendingConfirmation::new(
            "shell".to_string(),
            serde_json::json!({"command": "cargo test"}),
            response_tx,
        ));
        assert!(handle_confirmation_key(
            &mut app,
            crossterm::event::KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
        ));
        assert!(app.confirmation.as_ref().unwrap().allow_selected);
        assert!(handle_confirmation_key(
            &mut app,
            crossterm::event::KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        ));
        assert_eq!(response_rx.try_recv(), Ok(true));
    }

    #[test]
    fn restores_messages_and_tool_names() {
        let entries = restore_chat_history(vec![
            Message::System {
                content: "hidden".into(),
            },
            Message::User {
                content: "question".into(),
            },
            Message::Assistant {
                text: None,
                tool_calls: vec![ToolCall {
                    id: "call-1".into(),
                    name: "shell".into(),
                    arguments: r#"{"command":"pwd"}"#.into(),
                }],
            },
            Message::Tool {
                tool_call_id: "call-1".into(),
                content: "/tmp".into(),
            },
            Message::Assistant {
                text: Some("done".into()),
                tool_calls: vec![],
            },
        ]);

        assert!(matches!(&entries[0], ChatEntry::User(text) if text == "question"));
        assert!(matches!(
            &entries[1],
            ChatEntry::ToolCall { name, .. } if name == "shell"
        ));
        assert!(matches!(
            &entries[2],
            ChatEntry::ToolResult { name, output } if name == "shell" && output == "/tmp"
        ));
        assert!(matches!(&entries[3], ChatEntry::Assistant(text) if text == "done"));
    }
}
