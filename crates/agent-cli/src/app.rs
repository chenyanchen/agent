use std::io;
use std::time::Duration;

use async_openai::config::OpenAIConfig;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;

use agent_core::{Agent, AgentEvent, AutoGuard, Handler, MemoryStorage, OpenAIModel};
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
        }
    }

    // ── Entry point ───────────────────────────────────────────────────────────

    /// Initialise the terminal, spawn the agent background task, and run the
    /// main event loop.  Blocks until the user quits.
    pub fn run(config: Config) -> io::Result<()> {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async { Self::run_async(config).await })
    }

    async fn run_async(config: Config) -> io::Result<()> {
        // ── Channels ──────────────────────────────────────────────────────────
        let (input_tx, mut input_rx) = mpsc::unbounded_channel::<String>();
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AgentEvent>();

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

            // Both Auto and Confirm use AutoGuard in TUI mode; interactive
            // confirmation is not yet implemented (TuiHandler always confirms).
            let _ = guard_mode;
            let guard = AutoGuard;
            let mut agent = Agent::builder()
                .model(model)
                .guard(guard)
                .storage(MemoryStorage::new())
                .system_prompt(&system_prompt)
                .tool(ShellTool)
                .tool(ReadFileTool)
                .tool(WriteFileTool)
                .tool(EditFileTool)
                .tool(GlobTool)
                .tool(GrepTool)
                .build();

            let handler = TuiHandler { tx: event_tx_clone.clone() };
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
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = Terminal::new(backend)?;

        let mut app = App::new(&config.model.model_id);

        let result = run_loop(&mut terminal, &mut app, &input_tx, &mut event_rx).await;

        // ── Terminal cleanup ──────────────────────────────────────────────────
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        result
    }
}

// ── Main event loop ───────────────────────────────────────────────────────────

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    input_tx: &mpsc::UnboundedSender<String>,
    event_rx: &mut mpsc::UnboundedReceiver<AgentEvent>,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // Drain all pending agent events (non-blocking).
        while let Ok(agent_event) = event_rx.try_recv() {
            handle_agent_event(app, agent_event);
        }

        // Poll for terminal input with a short timeout so we stay responsive
        // to both keyboard events and agent streaming events.
        if event::poll(Duration::from_millis(50))?
            && let Event::Key(key) = event::read()? {
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

// ── Agent event handler ───────────────────────────────────────────────────────

fn handle_agent_event(app: &mut App, event: AgentEvent) {
    match event {
        AgentEvent::TextDelta(delta) => {
            app.streaming_text.push_str(&delta);
        }
        AgentEvent::ToolCallBegin { name, arguments, .. } => {
            app.chat_history.push(ChatEntry::ToolCall { name, arguments });
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
            app.chat_history.push(ChatEntry::ToolResult { name, output: output_str });
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
}

#[async_trait::async_trait]
impl Handler for TuiHandler {
    async fn on_event(&self, event: AgentEvent) {
        let _ = self.tx.send(event);
    }

    /// In TUI mode we auto-confirm all tool calls.  A real implementation
    /// would pause the loop and render a confirmation prompt.
    async fn confirm(&self, _tool_name: &str, _input: &serde_json::Value) -> bool {
        true
    }
}
