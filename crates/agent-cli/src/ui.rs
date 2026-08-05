use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::App;

// ── ChatEntry ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ChatEntry {
    User(String),
    Assistant(String),
    ToolCall { name: String, arguments: String },
    ToolResult { name: String, output: String },
    Error(String),
}

// ── draw ──────────────────────────────────────────────────────────────────────

/// Render the full TUI layout onto `frame`.
pub fn draw(frame: &mut Frame, app: &App) {
    // ── Layout: chat | status | input ────────────────────────────────────────
    let frame_area = frame.area();
    let input_height = if app.confirmation.is_some() {
        5
    } else {
        let rows = app
            .input
            .visual_rows(frame_area.width.saturating_sub(2) as usize)
            .len() as u16;
        (rows + 2).max(3).min((frame_area.height / 3).max(3))
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // chat area — fills remaining space
            Constraint::Length(1), // status bar — exactly 1 line
            Constraint::Length(input_height),
        ])
        .split(frame_area);

    draw_chat(frame, app, chunks[0]);
    draw_status(frame, app, chunks[1]);
    draw_input(frame, app, chunks[2]);
}

// ── Chat area ─────────────────────────────────────────────────────────────────

fn draw_chat(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let mut lines: Vec<Line> = Vec::new();

    for entry in &app.chat_history {
        match entry {
            ChatEntry::User(text) => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "You: ",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(text.clone()),
                ]));
            }
            ChatEntry::Assistant(text) => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "Assistant: ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(text.clone()),
                ]));
            }
            ChatEntry::ToolCall { name, arguments } => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "  [tool] ",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(name.clone(), Style::default().fg(Color::Yellow)),
                    Span::raw(format!("({arguments})")),
                ]));
            }
            ChatEntry::ToolResult { name, output } => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "  [result] ",
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(name.clone(), Style::default().fg(Color::Magenta)),
                    Span::raw(format!(": {output}")),
                ]));
            }
            ChatEntry::Error(msg) => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "  [error] ",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(msg.clone(), Style::default().fg(Color::Red)),
                ]));
            }
        }
    }

    // Append any in-progress streamed text.
    if !app.streaming_text.is_empty() {
        lines.push(Line::from(vec![
            Span::styled(
                "Assistant: ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(app.streaming_text.clone()),
            // Blinking cursor indicator while streaming
            Span::styled("▌", Style::default().fg(Color::Cyan)),
        ]));
    }

    let paragraph = Paragraph::new(lines)
        .block(Block::default().title(" Chat ").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    // line_count includes wrapping and the border, so long tool arguments stay scrollable.
    let max_scroll =
        (paragraph.line_count(area.width.saturating_sub(2)) as u16).saturating_sub(area.height);
    let scroll = max_scroll.saturating_sub(app.scroll_offset as u16);
    let paragraph = paragraph.scroll((scroll, 0));

    frame.render_widget(paragraph, area);
}

// ── Status bar ────────────────────────────────────────────────────────────────

fn draw_status(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let right_side = if app.confirmation.is_some() {
        Span::styled(
            " awaiting confirmation... ",
            Style::default().fg(Color::Yellow),
        )
    } else if app.is_running {
        Span::styled(" thinking... ", Style::default().fg(Color::Yellow))
    } else {
        Span::styled(
            format!(" tokens: {} ", app.total_tokens),
            Style::default().fg(Color::DarkGray),
        )
    };

    let status_line = Line::from(vec![
        Span::styled(
            format!(" model: {} ", app.model_id),
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" | ", Style::default().fg(Color::DarkGray)),
        right_side,
        Span::styled(" | ", Style::default().fg(Color::DarkGray)),
        Span::styled(" Ctrl+C to quit ", Style::default().fg(Color::DarkGray)),
    ]);

    let status_bar = Paragraph::new(status_line).style(Style::default().bg(Color::Reset));

    frame.render_widget(status_bar, area);
}

// ── Input area ────────────────────────────────────────────────────────────────

fn draw_input(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    if let Some(confirmation) = &app.confirmation {
        let choice = |selected, text| {
            Line::from(vec![
                Span::styled(
                    if selected { "> " } else { "  " },
                    Style::default().fg(Color::Yellow),
                ),
                Span::raw(text),
            ])
        };
        let lines = vec![
            choice(confirmation.allow_selected, "Allow this tool call"),
            choice(!confirmation.allow_selected, "Deny this tool call"),
        ];
        let widget = Paragraph::new(lines).block(
            Block::default()
                .title(format!(
                    " {}({}) ",
                    confirmation.name, confirmation.arguments
                ))
                .borders(Borders::ALL),
        );
        frame.render_widget(widget, area);
        return;
    }

    let width = area.width.saturating_sub(2) as usize;
    let rows = app.input.visual_rows(width);
    let (cursor_row, _) = app.input.cursor_position(width);
    let cursor = app.input.cursor();
    let chars: Vec<char> = app.input.content().chars().collect();
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            if index != cursor_row {
                return Line::raw(chars[row.clone()].iter().collect::<String>());
            }

            let cursor = cursor.clamp(row.start, row.end);
            let before = chars[row.start..cursor].iter().collect::<String>();
            let cursor_char = if cursor < row.end { chars[cursor] } else { ' ' }.to_string();
            let after_start = (cursor + usize::from(cursor < row.end)).min(row.end);
            let after = chars[after_start..row.end].iter().collect::<String>();
            Line::from(vec![
                Span::raw(before),
                Span::styled(
                    cursor_char,
                    Style::default().bg(Color::White).fg(Color::Black),
                ),
                Span::raw(after),
            ])
        })
        .collect();

    let newline_key = if app.shift_enter_supported {
        "Shift+Enter"
    } else {
        "Ctrl+J"
    };
    let title = if app.is_running {
        format!(" Input ({newline_key} for newline, waiting...) ")
    } else {
        format!(" Input ({newline_key} for newline) ")
    };

    let viewport_height = area.height.saturating_sub(2) as usize;
    let scroll = cursor_row.saturating_sub(viewport_height.saturating_sub(1)) as u16;
    let input_widget = Paragraph::new(lines)
        .block(Block::default().title(title).borders(Borders::ALL))
        .scroll((scroll, 0));

    frame.render_widget(input_widget, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapped_chat_lines_contribute_to_scroll_height() {
        let paragraph = Paragraph::new("12345678901234567890")
            .block(Block::default().borders(Borders::ALL))
            .wrap(Wrap { trim: false });

        assert_eq!(paragraph.line_count(6), 6);
    }
}
