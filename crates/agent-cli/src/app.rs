use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyModifiers,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
        supports_keyboard_enhancement,
    },
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::{mpsc, oneshot};

use agent_core::{Agent, AgentEvent, FileStorage, Handler, Message, OpenAIModel, SkillInfo};
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
    pub input_tokens: u32,
    pub context_window: u32,
    pub compaction_count: u64,
    pub should_quit: bool,
    pub confirmation: Option<PendingConfirmation>,
    pub shift_enter_supported: bool,
    pub pending_retry: bool,
    pub skills: Vec<SkillInfo>,
    pub skill_selection: Option<usize>,
}

enum AgentCommand {
    Submit(String),
    Retry,
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
    fn new(model_id: impl Into<String>, context_window: u32) -> Self {
        Self {
            input: InputBuffer::new(),
            chat_history: Vec::new(),
            streaming_text: String::new(),
            scroll_offset: 0,
            is_running: false,
            model_id: model_id.into(),
            input_tokens: 0,
            context_window,
            compaction_count: 0,
            should_quit: false,
            confirmation: None,
            shift_enter_supported: false,
            pending_retry: false,
            skills: Vec::new(),
            skill_selection: None,
        }
    }

    pub(crate) fn filtered_skills(&self) -> Vec<&SkillInfo> {
        let Some(query) = self.input.dollar_query() else {
            return Vec::new();
        };
        self.skills
            .iter()
            .filter(|skill| skill.name.contains(&query))
            .collect()
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
        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<AgentCommand>();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AgentEvent>();
        let (confirmation_tx, mut confirmation_rx) =
            mpsc::unbounded_channel::<ConfirmationRequest>();

        // ── Spawn agent task ──────────────────────────────────────────────────
        let system_prompt = config
            .system_prompt
            .clone()
            .unwrap_or_else(|| "You are a general-purpose AI assistant with access to shell, file, and search tools. Be concise and helpful.".into());

        let model_id = config.model.model_id.clone();
        let context_window = config
            .model
            .context_window
            .ok_or_else(|| io::Error::other("model.context_window must be configured"))?;
        let workdir = std::env::current_dir()?;
        let user_skills_dir = dirs::home_dir()
            .ok_or_else(|| io::Error::other("home directory not found"))?
            .join(".agents/skills");
        let event_tx_clone = event_tx.clone();
        let storage_dir = sessions_dir()?;
        let storage = FileStorage::new(storage_dir);
        let model = OpenAIModel::with_settings(
            model_id,
            config.model.api_key.clone(),
            config.model.api_base.clone(),
        );
        let mut agent = Agent::builder()
            .model(model)
            .guard(config.guard.mode.clone())
            .storage(storage)
            .conversation_id(session)
            .system_prompt(&system_prompt)
            .workdir(&workdir)
            .user_skills_dir(user_skills_dir)
            .context_window(context_window)
            .tool(ShellTool::new(&workdir))
            .tool(ReadFileTool::new(&workdir))
            .tool(WriteFileTool::new(&workdir))
            .tool(EditFileTool::new(&workdir))
            .tool(GlobTool::new(&workdir))
            .tool(GrepTool::new(&workdir))
            .build()
            .await
            .map_err(|error| io::Error::other(error.to_string()))?;
        let history = agent.transcript().to_vec();
        let skills = agent.skills().to_vec();
        let pending_retry = agent.has_pending_request();
        let last_error = agent.last_error().map(str::to_owned);
        let input_tokens = agent.last_input_tokens();
        let compaction_count = agent.compaction_count();

        tokio::spawn(async move {
            let handler = TuiHandler {
                tx: event_tx_clone.clone(),
                confirmation_tx,
            };
            while let Some(command) = input_rx.recv().await {
                let result = match command {
                    AgentCommand::Submit(input) => agent.run(&input, &handler).await,
                    AgentCommand::Retry => agent.retry(&handler).await,
                };
                if let Err(error) = result
                    && agent.last_error().is_none()
                {
                    let _ = event_tx_clone.send(AgentEvent::Failed {
                        error: error.to_string(),
                        retryable: agent.has_pending_request(),
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
        let enhanced_keys_supported = supports_keyboard_enhancement().unwrap_or(false);
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableBracketedPaste)?;
        if enhanced_keys_supported {
            execute!(
                stdout,
                PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
            )?;
        }
        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = Terminal::new(backend)?;

        let mut app = App::new(&config.model.model_id, context_window);
        app.shift_enter_supported = cfg!(windows) || enhanced_keys_supported;
        app.chat_history = restore_chat_history(history);
        app.skills = skills;
        app.pending_retry = pending_retry;
        app.input_tokens = input_tokens;
        app.compaction_count = compaction_count;
        if let Some(error) = last_error {
            app.chat_history.push(ChatEntry::Error(error));
        }

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
        if enhanced_keys_supported {
            execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags)?;
        }
        execute!(
            terminal.backend_mut(),
            DisableBracketedPaste,
            LeaveAlternateScreen
        )?;
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
    input_tx: &mpsc::UnboundedSender<AgentCommand>,
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
        if event::poll(Duration::from_millis(50))? {
            let terminal_event = event::read()?;
            if let Event::Key(key) = terminal_event
                && handle_confirmation_key(app, key)
            {
                if app.should_quit {
                    break;
                }
                continue;
            }

            let input_width = terminal.size()?.width.saturating_sub(2) as usize;
            if let Some(command) = handle_input_event(app, terminal_event, input_width) {
                let _ = input_tx.send(command);
            }
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn handle_input_event(app: &mut App, event: Event, width: usize) -> Option<AgentCommand> {
    if let Event::Paste(text) = event {
        app.input
            .insert_str(&text.replace("\r\n", "\n").replace('\r', "\n"));
        return None;
    }

    let Event::Key(key) = event else {
        return None;
    };
    if app.skill_selection.is_some() {
        match key.code {
            KeyCode::Esc => app.skill_selection = None,
            KeyCode::Up => {
                let len = app.filtered_skills().len();
                if len > 0 {
                    app.skill_selection = Some(app.skill_selection.unwrap().saturating_sub(1));
                }
            }
            KeyCode::Down => {
                let len = app.filtered_skills().len();
                if len > 0 {
                    app.skill_selection = Some((app.skill_selection.unwrap() + 1).min(len - 1));
                }
            }
            KeyCode::Enter => {
                let name = app
                    .filtered_skills()
                    .get(app.skill_selection.unwrap())
                    .map(|skill| skill.name.clone());
                if let Some(name) = name {
                    app.input.complete_dollar(&name);
                }
                app.skill_selection = None;
            }
            KeyCode::Char(ch) => {
                app.input.insert(ch);
                app.skill_selection = Some(0);
            }
            KeyCode::Backspace => {
                app.input.backspace();
                app.skill_selection = app.input.dollar_query().map(|_| 0);
            }
            _ => app.skill_selection = None,
        }
        return None;
    }

    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => app.should_quit = true,
        (KeyCode::Enter, KeyModifiers::SHIFT) | (KeyCode::Char('j'), KeyModifiers::CONTROL) => {
            app.input.insert('\n')
        }
        (KeyCode::Enter, _) => {
            if !app.is_running && !app.input.is_blank() {
                let text = app.input.take();
                app.chat_history.push(ChatEntry::User(text.clone()));
                app.is_running = true;
                app.scroll_offset = 0;
                app.pending_retry = false;
                return Some(AgentCommand::Submit(text));
            }
        }
        (KeyCode::Char('r'), KeyModifiers::CONTROL) if app.pending_retry && !app.is_running => {
            app.is_running = true;
            return Some(AgentCommand::Retry);
        }
        (KeyCode::Char(ch), _) => {
            app.input.insert(ch);
            if ch == '$' && !app.skills.is_empty() {
                app.skill_selection = Some(0);
            }
        }
        (KeyCode::Backspace, _) => app.input.backspace(),
        (KeyCode::Left, _) => app.input.move_left(),
        (KeyCode::Right, _) => app.input.move_right(),
        (KeyCode::Up, _) => app.input.move_up(width),
        (KeyCode::Down, _) => app.input.move_down(width),
        (KeyCode::PageUp, _) => {
            app.scroll_offset = app.scroll_offset.saturating_add(3);
        }
        (KeyCode::PageDown, _) => {
            app.scroll_offset = app.scroll_offset.saturating_sub(3);
        }
        _ => {}
    }
    None
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
            if !app.streaming_text.is_empty() {
                let text = std::mem::take(&mut app.streaming_text);
                app.chat_history.push(ChatEntry::Assistant(text));
            }
            app.chat_history
                .push(ChatEntry::ToolCall { name, arguments });
        }
        AgentEvent::ToolCallEnd { id, output } => {
            let _ = id;
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
            let _ = id;
            app.chat_history
                .push(ChatEntry::Error(format!("Tool `{name}` denied: {reason}")));
        }
        AgentEvent::SkillsUpdated { skills, warnings } => {
            app.skills = skills;
            app.chat_history
                .extend(warnings.into_iter().map(ChatEntry::Activity));
        }
        AgentEvent::Compacted { total } => {
            app.compaction_count = total;
            app.chat_history
                .push(ChatEntry::Activity(format!("context compacted ({total})")));
        }
        AgentEvent::Failed { error, retryable } => {
            app.streaming_text.clear();
            app.chat_history.push(ChatEntry::Error(error));
            app.is_running = false;
            app.pending_retry = retryable;
        }
        AgentEvent::TurnComplete {
            usage,
            context_window,
        } => {
            // Flush any accumulated streaming text to the chat history.
            if !app.streaming_text.is_empty() {
                let text = std::mem::take(&mut app.streaming_text);
                app.chat_history.push(ChatEntry::Assistant(text));
            }
            app.input_tokens = usage.input_tokens;
            app.context_window = context_window;
            app.is_running = false;
            app.pending_retry = false;
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
        let mut app = App::new("test-model", 100);
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

    #[test]
    fn multiline_keys_and_paste_edit_without_submitting() {
        let mut app = App::new("test-model", 100);

        assert!(
            handle_input_event(
                &mut app,
                Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT)),
                20,
            )
            .is_none()
        );
        handle_input_event(&mut app, Event::Paste("one\r\ntwo".into()), 20);
        handle_input_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::CONTROL)),
            20,
        );

        assert_eq!(app.input.content(), "\none\ntwo\n");
        assert!(!app.is_running);
    }

    #[test]
    fn enter_ignores_blank_draft_and_submits_non_blank_draft() {
        let mut app = App::new("test-model", 100);
        app.input.insert_str(" \n");
        assert!(
            handle_input_event(
                &mut app,
                Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                20,
            )
            .is_none()
        );
        assert_eq!(app.input.content(), " \n");

        app.input.insert('x');
        let submitted = handle_input_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            20,
        );
        assert!(matches!(submitted, Some(AgentCommand::Submit(text)) if text == " \nx"));
        assert!(app.is_running);
        assert!(app.input.content().is_empty());
    }

    #[test]
    fn dollar_picker_filters_and_inserts_selected_skill() {
        let mut app = App::new("test-model", 100);
        app.skills = vec![SkillInfo {
            name: "wayfinder".into(),
            description: "plan work".into(),
            path: PathBuf::from("/skills/wayfinder/SKILL.md"),
            allow_implicit_invocation: true,
        }];

        handle_input_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Char('$'), KeyModifiers::NONE)),
            20,
        );
        handle_input_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE)),
            20,
        );
        handle_input_event(
            &mut app,
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            20,
        );

        assert_eq!(app.input.content(), "$wayfinder ");
        assert!(app.skill_selection.is_none());
    }

    #[test]
    fn only_retryable_failures_enable_retry() {
        let mut app = App::new("test-model", 100);
        handle_agent_event(
            &mut app,
            AgentEvent::Failed {
                error: "bad skill".into(),
                retryable: false,
            },
        );
        assert!(!app.pending_retry);
        handle_agent_event(
            &mut app,
            AgentEvent::Failed {
                error: "network".into(),
                retryable: true,
            },
        );
        assert!(app.pending_retry);
    }
}
