use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

pub fn render(source: &str, width: usize) -> Vec<Line<'static>> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_GFM);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let mut writer = Writer::new(width.max(1));
    let mut image_start = None;
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        if let Some(start) = image_start {
            if matches!(event, Event::End(TagEnd::Image)) {
                writer.text(&source[start..range.end], Style::default());
                image_start = None;
            }
            continue;
        }
        if matches!(event, Event::Start(Tag::Image { .. })) {
            image_start = Some(range.start);
            continue;
        }
        writer.event(event);
    }
    if let Some(start) = image_start {
        writer.text(&source[start..], Style::default());
    }
    writer.finish()
}

pub fn wrap_lines(lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .flat_map(|line| wrap_line(line, width.max(1)))
        .collect()
}

pub fn literal(text: &str, width: usize) -> Vec<Line<'static>> {
    wrap_lines(
        text.split('\n')
            .map(|line| Line::raw(sanitize(line, 0)))
            .collect(),
        width,
    )
}

struct Writer {
    width: usize,
    lines: Vec<Line<'static>>,
    current: Line<'static>,
    styles: Vec<Style>,
    quote_depth: usize,
    lists: Vec<u64>,
    item_prefixes: Vec<(String, String, bool)>,
    after_block: bool,
    code_block: bool,
    links: Vec<(String, String)>,
    table: Option<Table>,
}

impl Writer {
    fn new(width: usize) -> Self {
        Self {
            width,
            lines: Vec::new(),
            current: Line::default(),
            styles: vec![Style::default()],
            quote_depth: 0,
            lists: Vec::new(),
            item_prefixes: Vec::new(),
            after_block: false,
            code_block: false,
            links: Vec::new(),
            table: None,
        }
    }

    fn event(&mut self, event: Event<'_>) {
        if self.table.is_some() && self.table_event(&event) {
            return;
        }

        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => self.text(&text, self.style()),
            Event::Code(code) => self.text(
                &code,
                self.style().add_modifier(Modifier::BOLD | Modifier::DIM),
            ),
            Event::Html(html) | Event::InlineHtml(html) => self.text(&html, self.style()),
            Event::SoftBreak | Event::HardBreak => self.line_break(),
            Event::Rule => {
                self.separate();
                self.lines.push(Line::styled(
                    "─".repeat(self.width),
                    Style::default().add_modifier(Modifier::DIM),
                ));
                self.after_block = true;
            }
            Event::TaskListMarker(checked) => self.text(
                if checked { "[x] " } else { "[ ] " },
                self.style().add_modifier(Modifier::DIM),
            ),
            Event::FootnoteReference(reference) => {
                self.text(&format!("[^{reference}]"), self.style())
            }
            Event::InlineMath(math) => self.text(&format!("${math}$"), self.style()),
            Event::DisplayMath(math) => self.text(&format!("$${math}$$"), self.style()),
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Paragraph => {
                if self.after_block && self.lists.is_empty() {
                    self.separate();
                }
                self.after_block = false;
            }
            Tag::Heading { level, .. } => {
                self.separate();
                self.push_style(heading_style(level));
            }
            Tag::BlockQuote(_) => {
                self.separate();
                self.quote_depth += 1;
            }
            Tag::CodeBlock(kind) => {
                self.separate();
                let language = match kind {
                    CodeBlockKind::Fenced(language) if !language.is_empty() => {
                        format!(" {language}")
                    }
                    _ => String::new(),
                };
                self.lines.push(Line::styled(
                    format!("┌─{language}"),
                    Style::default().add_modifier(Modifier::DIM),
                ));
                self.code_block = true;
            }
            Tag::List(start) => {
                if self.lists.is_empty() {
                    self.separate();
                }
                self.lists.push(start.unwrap_or(0));
            }
            Tag::Item => {
                self.flush();
                let depth = self.lists.len().saturating_sub(1);
                let marker = match self.lists.last_mut() {
                    Some(next) if *next > 0 => {
                        let marker = format!("{next}. ");
                        *next += 1;
                        marker
                    }
                    _ => "- ".to_string(),
                };
                let first = format!("{}{marker}", "  ".repeat(depth));
                let continuation = " ".repeat(first.chars().count());
                self.item_prefixes.push((first, continuation, true));
            }
            Tag::Emphasis => self.push_style(Style::default().add_modifier(Modifier::ITALIC)),
            Tag::Strong => self.push_style(Style::default().add_modifier(Modifier::BOLD)),
            Tag::Strikethrough => {
                self.push_style(Style::default().add_modifier(Modifier::CROSSED_OUT))
            }
            Tag::Link { dest_url, .. } => {
                self.links.push((dest_url.into_string(), String::new()));
                self.push_style(Style::default().add_modifier(Modifier::UNDERLINED));
            }
            Tag::Table(alignments) => {
                self.separate();
                self.table = Some(Table::new(alignments));
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.flush();
                self.after_block = true;
            }
            TagEnd::Heading(_) => {
                self.pop_style();
                self.flush();
                self.after_block = true;
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quote_depth = self.quote_depth.saturating_sub(1);
                self.after_block = true;
            }
            TagEnd::CodeBlock => {
                self.flush();
                self.lines.push(Line::styled(
                    "└─",
                    Style::default().add_modifier(Modifier::DIM),
                ));
                self.code_block = false;
                self.after_block = true;
            }
            TagEnd::Item => {
                self.flush();
                self.item_prefixes.pop();
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
                if self.lists.is_empty() {
                    self.after_block = true;
                }
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => self.pop_style(),
            TagEnd::Link => {
                self.pop_style();
                if let Some((destination, label)) = self.links.pop()
                    && label != destination
                {
                    self.text(&format!(" ({destination})"), self.style());
                }
            }
            _ => {}
        }
    }

    fn table_event(&mut self, event: &Event<'_>) -> bool {
        match event {
            Event::Start(Tag::TableHead) | Event::Start(Tag::TableRow) => {
                self.table.as_mut().unwrap().start_row();
            }
            Event::Start(Tag::TableCell) => self.table.as_mut().unwrap().start_cell(),
            Event::End(TagEnd::TableCell) => self.table.as_mut().unwrap().end_cell(),
            Event::End(TagEnd::TableHead) | Event::End(TagEnd::TableRow) => {
                self.table.as_mut().unwrap().end_row();
            }
            Event::End(TagEnd::Table) => {
                let table = self.table.take().unwrap();
                self.lines.extend(table.render(self.width));
                self.after_block = true;
            }
            Event::Text(text) => {
                if let Some((_, label)) = self.links.last_mut() {
                    label.push_str(text);
                }
                let style = self.style();
                self.table.as_mut().unwrap().text(text, style);
            }
            Event::Code(code) => {
                if let Some((_, label)) = self.links.last_mut() {
                    label.push_str(code);
                }
                let style = self.style().add_modifier(Modifier::BOLD | Modifier::DIM);
                self.table.as_mut().unwrap().text(code, style);
            }
            Event::SoftBreak | Event::HardBreak => self.table.as_mut().unwrap().line_break(),
            Event::Start(Tag::Strong) => {
                self.push_style(Style::default().add_modifier(Modifier::BOLD))
            }
            Event::End(TagEnd::Strong) => self.pop_style(),
            Event::Start(Tag::Emphasis) => {
                self.push_style(Style::default().add_modifier(Modifier::ITALIC))
            }
            Event::End(TagEnd::Emphasis) => self.pop_style(),
            Event::Start(Tag::Strikethrough) => {
                self.push_style(Style::default().add_modifier(Modifier::CROSSED_OUT))
            }
            Event::End(TagEnd::Strikethrough) => self.pop_style(),
            _ => return false,
        }
        true
    }

    fn text(&mut self, text: &str, style: Style) {
        if let Some((_, label)) = self.links.last_mut() {
            label.push_str(text);
        }
        if let Some(table) = self.table.as_mut() {
            table.text(text, style);
            return;
        }
        for (index, part) in text.split('\n').enumerate() {
            if index > 0 {
                self.line_break();
            }
            if part.is_empty() {
                continue;
            }
            self.ensure_prefix();
            self.current
                .spans
                .push(Span::styled(sanitize(part, self.current.width()), style));
        }
    }

    fn ensure_prefix(&mut self) {
        if !self.current.spans.is_empty() {
            return;
        }
        let mut prefix = "│ ".repeat(self.quote_depth);
        if self.code_block {
            prefix.push_str("│ ");
        } else if let Some((first, continuation, first_line)) = self.item_prefixes.last() {
            prefix.push_str(if *first_line { first } else { continuation });
        }
        if !prefix.is_empty() {
            self.current.spans.push(Span::styled(
                prefix,
                Style::default().add_modifier(Modifier::DIM),
            ));
        }
    }

    fn line_break(&mut self) {
        self.ensure_prefix();
        self.lines.push(std::mem::take(&mut self.current));
        if let Some((_, _, first_line)) = self.item_prefixes.last_mut() {
            *first_line = false;
        }
    }

    fn flush(&mut self) {
        if !self.current.spans.is_empty() {
            self.lines.push(std::mem::take(&mut self.current));
            if let Some((_, _, first_line)) = self.item_prefixes.last_mut() {
                *first_line = false;
            }
        }
    }

    fn separate(&mut self) {
        self.flush();
        if self.lines.last().is_some_and(|line| line.width() > 0) {
            self.lines.push(Line::default());
        }
    }

    fn style(&self) -> Style {
        *self.styles.last().unwrap()
    }

    fn push_style(&mut self, style: Style) {
        self.styles.push(self.style().patch(style));
    }

    fn pop_style(&mut self) {
        if self.styles.len() > 1 {
            self.styles.pop();
        }
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        self.flush();
        while self.lines.last().is_some_and(|line| line.width() == 0) {
            self.lines.pop();
        }
        wrap_lines(self.lines, self.width)
    }
}

fn heading_style(level: HeadingLevel) -> Style {
    let modifier = match level {
        HeadingLevel::H1 => Modifier::BOLD | Modifier::UNDERLINED,
        HeadingLevel::H2 => Modifier::BOLD,
        HeadingLevel::H3 => Modifier::BOLD | Modifier::ITALIC,
        _ => Modifier::ITALIC,
    };
    Style::default().add_modifier(modifier)
}

#[derive(Default)]
struct Cell {
    lines: Vec<Line<'static>>,
    current: Line<'static>,
}

impl Cell {
    fn text(&mut self, text: &str, style: Style) {
        for (index, part) in text.split('\n').enumerate() {
            if index > 0 {
                self.line_break();
            }
            if !part.is_empty() {
                self.current
                    .spans
                    .push(Span::styled(sanitize(part, self.current.width()), style));
            }
        }
    }

    fn line_break(&mut self) {
        self.lines.push(std::mem::take(&mut self.current));
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        if !self.current.spans.is_empty() || self.lines.is_empty() {
            self.lines.push(self.current);
        }
        self.lines
    }

    fn width(&self) -> usize {
        self.lines
            .iter()
            .map(Line::width)
            .chain(std::iter::once(self.current.width()))
            .max()
            .unwrap_or(1)
            .max(1)
    }
}

struct Table {
    alignments: Vec<pulldown_cmark::Alignment>,
    rows: Vec<Vec<Cell>>,
    row: Vec<Cell>,
    cell: Option<Cell>,
}

impl Table {
    fn new(alignments: Vec<pulldown_cmark::Alignment>) -> Self {
        Self {
            alignments,
            rows: Vec::new(),
            row: Vec::new(),
            cell: None,
        }
    }

    fn start_row(&mut self) {
        self.row.clear();
    }

    fn end_row(&mut self) {
        if !self.row.is_empty() {
            self.rows.push(std::mem::take(&mut self.row));
        }
    }

    fn start_cell(&mut self) {
        self.cell = Some(Cell::default());
    }

    fn end_cell(&mut self) {
        self.row.push(self.cell.take().unwrap_or_default());
    }

    fn text(&mut self, text: &str, style: Style) {
        self.cell
            .get_or_insert_with(Cell::default)
            .text(text, style);
    }

    fn line_break(&mut self) {
        self.cell.get_or_insert_with(Cell::default).line_break();
    }

    fn render(self, width: usize) -> Vec<Line<'static>> {
        let columns = self
            .alignments
            .len()
            .max(self.rows.iter().map(Vec::len).max().unwrap_or(0));
        let overhead = columns.saturating_mul(3).saturating_add(1);
        if columns == 0 {
            return Vec::new();
        }
        if width < overhead + columns {
            return self.render_records(width);
        }

        let mut natural = vec![1; columns];
        for row in &self.rows {
            for (index, cell) in row.iter().enumerate() {
                natural[index] = natural[index].max(cell.width());
            }
        }
        let available = width - overhead;
        let mut widths = vec![1; columns];
        for _ in 0..available.saturating_sub(columns) {
            let Some((index, _)) = natural
                .iter()
                .zip(&widths)
                .enumerate()
                .filter(|(_, (natural, current))| natural > current)
                .max_by_key(|(_, (natural, current))| *natural - *current)
            else {
                break;
            };
            widths[index] += 1;
        }

        let border = Style::default().add_modifier(Modifier::DIM);
        let mut lines = vec![table_border('┌', '┬', '┐', &widths, border)];
        let row_count = self.rows.len();
        for (row_index, row) in self.rows.into_iter().enumerate() {
            let cells: Vec<Vec<Line>> = row
                .into_iter()
                .enumerate()
                .map(|(index, cell)| {
                    let style = if row_index == 0 {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    cell.finish()
                        .into_iter()
                        .flat_map(|line| wrap_line(line.style(style), widths[index]))
                        .collect()
                })
                .collect();
            let height = cells.iter().map(Vec::len).max().unwrap_or(1);
            for line_index in 0..height {
                let mut line = Line::from(Span::styled("│", border));
                for (column, column_width) in widths.iter().copied().enumerate() {
                    line.spans.push(Span::raw(" "));
                    if let Some(cell_line) = cells.get(column).and_then(|cell| cell.get(line_index))
                    {
                        let remaining = column_width.saturating_sub(cell_line.width());
                        let (left, right) = match self
                            .alignments
                            .get(column)
                            .copied()
                            .unwrap_or(pulldown_cmark::Alignment::None)
                        {
                            pulldown_cmark::Alignment::Center => {
                                (remaining / 2, remaining - remaining / 2)
                            }
                            pulldown_cmark::Alignment::Right => (remaining, 0),
                            _ => (0, remaining),
                        };
                        line.spans.push(Span::raw(" ".repeat(left)));
                        line.spans.extend(cell_line.spans.clone());
                        line.spans.push(Span::raw(" ".repeat(right)));
                    } else {
                        line.spans.push(Span::raw(" ".repeat(column_width)));
                    }
                    line.spans.push(Span::raw(" "));
                    line.spans.push(Span::styled("│", border));
                }
                lines.push(line);
            }
            if row_index == 0 && row_count > 1 {
                lines.push(table_border('├', '┼', '┤', &widths, border));
            }
        }
        lines.push(table_border('└', '┴', '┘', &widths, border));
        lines
    }

    fn render_records(self, width: usize) -> Vec<Line<'static>> {
        let mut rows = self.rows.into_iter();
        let headers: Vec<String> = rows
            .next()
            .unwrap_or_default()
            .into_iter()
            .map(|cell| {
                cell.finish()
                    .into_iter()
                    .map(|line| line.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        let mut lines = Vec::new();
        for (row_index, row) in rows.enumerate() {
            if row_index > 0 {
                lines.push(Line::default());
            }
            for (column, cell) in row.into_iter().enumerate() {
                let label = headers
                    .get(column)
                    .filter(|header| !header.is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("Column {}", column + 1));
                let mut line = Line::styled(
                    format!("{label}: "),
                    Style::default().add_modifier(Modifier::BOLD),
                );
                for cell_line in cell.finish() {
                    line.spans.extend(cell_line.spans);
                }
                lines.extend(wrap_line(line, width.max(1)));
            }
        }
        lines
    }
}

fn table_border(
    left: char,
    middle: char,
    right: char,
    widths: &[usize],
    style: Style,
) -> Line<'static> {
    let mut text = left.to_string();
    for (index, width) in widths.iter().enumerate() {
        text.push_str(&"─".repeat(width + 2));
        text.push(if index + 1 == widths.len() {
            right
        } else {
            middle
        });
    }
    Line::styled(text, style)
}

fn wrap_line(line: Line<'static>, width: usize) -> Vec<Line<'static>> {
    if line.width() <= width {
        return vec![line];
    }
    let line_style = line.style;
    let continuation = line.spans.first().and_then(|span| {
        span.style
            .add_modifier
            .contains(Modifier::DIM)
            .then(|| {
                Span::styled(
                    span.content
                        .chars()
                        .map(|character| {
                            if character == '│' || character.is_whitespace() {
                                character
                            } else {
                                ' '
                            }
                        })
                        .collect::<String>(),
                    span.style,
                )
            })
            .filter(|prefix| prefix.width() < width)
    });
    let mut result = Vec::new();
    let mut current = Line::default().style(line_style);
    let mut current_width = 0;
    for span in line.spans {
        for grapheme in span.styled_graphemes(line_style) {
            let grapheme_width = Line::from(grapheme.symbol).width();
            if current_width > 0 && current_width + grapheme_width > width {
                result.push(std::mem::take(&mut current).style(line_style));
                if let Some(prefix) = &continuation {
                    current_width = prefix.width();
                    current.spans.push(prefix.clone());
                } else {
                    current_width = 0;
                }
            }
            current
                .spans
                .push(Span::styled(grapheme.symbol.to_string(), grapheme.style));
            current_width += grapheme_width;
        }
    }
    result.push(current);
    result
}

fn sanitize(text: &str, mut column: usize) -> String {
    let mut safe = String::new();
    for character in text.chars() {
        let escaped = match character {
            '\t' => {
                let spaces = 4 - column % 4;
                column += spaces;
                safe.push_str(&" ".repeat(spaces));
                continue;
            }
            '\r' => "\\r".to_string(),
            character if character.is_control() && (character as u32) <= 0xff => {
                format!("\\x{:02x}", character as u32)
            }
            character if character.is_control() => format!("\\u{{{:x}}}", character as u32),
            character => character.to_string(),
        };
        column += Line::from(escaped.as_str()).width();
        safe.push_str(&escaped);
    }
    safe
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(lines: &[Line]) -> Vec<String> {
        lines.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn renders_gfm_without_losing_source_line_breaks() {
        let lines = render(
            "# Head\nline one\nline two with **bold** and [docs](https://example.com)\n![diagram](image.png)\n\x1b",
            80,
        );

        assert_eq!(
            strings(&lines),
            [
                "Head",
                "",
                "line one",
                "line two with bold and docs (https://example.com)",
                "![diagram](image.png)",
                "\\x1b",
            ]
        );
        assert!(
            lines[0].spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert!(lines[3].spans.iter().any(|span| {
            span.style.add_modifier.contains(Modifier::BOLD) && span.content == "bold"
        }));

        let blocks = strings(&render(
            "> quote\n\n- [x] done\n  - nested\n\n```rust\nfn main() {}\n```\n\n**streaming",
            20,
        ));
        assert!(blocks.contains(&"│ quote".to_string()));
        assert!(blocks.contains(&"- [x] done".to_string()));
        assert!(blocks.contains(&"  - nested".to_string()));
        assert!(blocks.contains(&"┌─ rust".to_string()));
        assert!(blocks.contains(&"│ fn main() {}".to_string()));
        assert!(blocks.contains(&"**streaming".to_string()));
    }

    #[test]
    fn tables_fit_or_fall_back_to_records() {
        let source = "| Name | Value |\n| --- | --- |\n| alpha | something long |";
        let grid = strings(&render(source, 30));
        assert!(grid.first().unwrap().starts_with('┌'));
        assert!(
            grid.iter()
                .all(|line| Line::from(line.as_str()).width() <= 30)
        );
        assert!(
            strings(&render(
                "| Link |\n| --- |\n| [docs](https://example.com) |",
                30,
            ))
            .iter()
            .any(|line| line.contains("docs (https://example.com)"))
        );

        assert_eq!(
            strings(&render(source, 7)),
            ["Name: a", "lpha", "Value: ", "somethi", "ng long"]
        );
    }
}
