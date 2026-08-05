use std::ops::Range;

use ratatui::text::Line;

/// A simple UTF-8 aware line-editor buffer with a cursor position (in chars).
pub struct InputBuffer {
    /// Owned text in the buffer.
    buf: String,
    /// Cursor position expressed as a **char** index (not a byte offset).
    cursor: usize,
}

impl InputBuffer {
    pub fn new() -> Self {
        Self {
            buf: String::new(),
            cursor: 0,
        }
    }

    /// Insert a character at the current cursor position, then advance the cursor.
    pub fn insert(&mut self, ch: char) {
        let byte_pos = self.char_to_byte(self.cursor);
        self.buf.insert(byte_pos, ch);
        self.cursor += 1;
    }

    /// Insert pasted text at the current cursor position.
    pub fn insert_str(&mut self, text: &str) {
        let byte_pos = self.char_to_byte(self.cursor);
        self.buf.insert_str(byte_pos, text);
        self.cursor += text.chars().count();
    }

    /// Delete the character immediately before the cursor (backspace semantics).
    /// Does nothing when the cursor is at position 0.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let byte_pos = self.char_to_byte(self.cursor - 1);
        self.buf.remove(byte_pos);
        self.cursor -= 1;
    }

    /// Move the cursor one character to the left (clamped to 0).
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move the cursor one character to the right (clamped to buffer length).
    pub fn move_right(&mut self) {
        let len = self.buf.chars().count();
        if self.cursor < len {
            self.cursor += 1;
        }
    }

    /// Move the cursor by one visually wrapped row.
    pub fn move_up(&mut self, width: usize) {
        self.move_vertical(width, -1);
    }

    /// Move the cursor by one visually wrapped row.
    pub fn move_down(&mut self, width: usize) {
        self.move_vertical(width, 1);
    }

    /// Return the current content as a `&str`.
    pub fn content(&self) -> &str {
        &self.buf
    }

    /// Return the current cursor position (in chars).
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Return `true` when the buffer contains only whitespace.
    pub fn is_blank(&self) -> bool {
        self.buf.trim().is_empty()
    }

    /// Character ranges for each visible row at the given terminal width.
    pub fn visual_rows(&self, width: usize) -> Vec<Range<usize>> {
        let width = width.max(1);
        let chars: Vec<char> = self.buf.chars().collect();
        let mut rows = Vec::new();
        let mut start = 0;
        let mut column = 0;

        for (index, ch) in chars.iter().copied().enumerate() {
            if ch == '\n' {
                rows.push(start..index);
                start = index + 1;
                column = 0;
                continue;
            }

            let char_width = Line::from(ch.to_string()).width();
            if column > 0 && column + char_width > width {
                rows.push(start..index);
                start = index;
                column = 0;
            }
            column += char_width;
        }

        rows.push(start..chars.len());
        if column >= width && self.cursor == chars.len() {
            rows.push(chars.len()..chars.len());
        }
        rows
    }

    /// Return the cursor's visual row and display-cell column.
    pub fn cursor_position(&self, width: usize) -> (usize, usize) {
        let rows = self.visual_rows(width);
        let row = rows
            .iter()
            .rposition(|range| range.start <= self.cursor && self.cursor <= range.end)
            .unwrap_or(0);
        let column = Line::from(
            self.buf
                .chars()
                .skip(rows[row].start)
                .take(self.cursor.saturating_sub(rows[row].start))
                .collect::<String>(),
        )
        .width();
        (row, column)
    }

    /// Consume the buffer, returning its contents and resetting to empty.
    pub fn take(&mut self) -> String {
        self.cursor = 0;
        std::mem::take(&mut self.buf)
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Convert a char-index to a byte offset for `self.buf`.
    fn char_to_byte(&self, char_idx: usize) -> usize {
        self.buf
            .char_indices()
            .nth(char_idx)
            .map(|(b, _)| b)
            .unwrap_or(self.buf.len())
    }

    fn move_vertical(&mut self, width: usize, direction: isize) {
        let rows = self.visual_rows(width);
        let (row, column) = self.cursor_position(width);
        let target = row.saturating_add_signed(direction).min(rows.len() - 1);
        if target == row {
            return;
        }

        let chars: Vec<char> = self.buf.chars().collect();
        let range = &rows[target];
        let mut target_cursor = range.start;
        let mut target_column = 0;
        for (offset, ch) in chars[range.clone()].iter().copied().enumerate() {
            let next_column = target_column + Line::from(ch.to_string()).width();
            if next_column > column {
                break;
            }
            target_column = next_column;
            target_cursor = range.start + offset + 1;
        }
        self.cursor = target_cursor;
    }
}

impl Default for InputBuffer {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_buffer_is_empty() {
        let buf = InputBuffer::new();
        assert!(buf.content().is_empty());
        assert_eq!(buf.cursor(), 0);
        assert_eq!(buf.content(), "");
    }

    #[test]
    fn insert_appends_at_end() {
        let mut buf = InputBuffer::new();
        buf.insert('H');
        buf.insert('i');
        assert_eq!(buf.content(), "Hi");
        assert_eq!(buf.cursor(), 2);
    }

    #[test]
    fn insert_in_the_middle() {
        let mut buf = InputBuffer::new();
        buf.insert('a');
        buf.insert('c');
        buf.move_left(); // cursor at 1
        buf.insert('b'); // inserts 'b' between 'a' and 'c'
        assert_eq!(buf.content(), "abc");
        assert_eq!(buf.cursor(), 2);
    }

    #[test]
    fn backspace_removes_preceding_char() {
        let mut buf = InputBuffer::new();
        for ch in "hello".chars() {
            buf.insert(ch);
        }
        buf.backspace();
        assert_eq!(buf.content(), "hell");
        assert_eq!(buf.cursor(), 4);
    }

    #[test]
    fn backspace_at_zero_is_noop() {
        let mut buf = InputBuffer::new();
        buf.backspace(); // must not panic
        assert!(buf.content().is_empty());
        assert_eq!(buf.cursor(), 0);
    }

    #[test]
    fn take_returns_content_and_resets() {
        let mut buf = InputBuffer::new();
        buf.insert('x');
        buf.insert('y');
        let taken = buf.take();
        assert_eq!(taken, "xy");
        assert!(buf.content().is_empty());
        assert_eq!(buf.cursor(), 0);
    }

    #[test]
    fn move_left_clamps_to_zero() {
        let mut buf = InputBuffer::new();
        buf.insert('a');
        buf.move_left();
        buf.move_left(); // already at 0, should not underflow
        assert_eq!(buf.cursor(), 0);
    }

    #[test]
    fn move_right_clamps_to_len() {
        let mut buf = InputBuffer::new();
        buf.insert('a');
        buf.move_right(); // already at end
        assert_eq!(buf.cursor(), 1);
    }

    #[test]
    fn handles_multibyte_unicode() {
        let mut buf = InputBuffer::new();
        buf.insert('😀'); // 4-byte UTF-8 character
        buf.insert('!');
        assert_eq!(buf.content(), "😀!");
        assert_eq!(buf.cursor(), 2);

        buf.backspace();
        assert_eq!(buf.content(), "😀");
        assert_eq!(buf.cursor(), 1);

        buf.backspace();
        assert!(buf.content().is_empty());
        assert_eq!(buf.cursor(), 0);
    }

    #[test]
    fn wraps_moves_and_preserves_explicit_line_breaks() {
        let mut buf = InputBuffer::new();
        buf.insert_str("abcd\nef");

        assert_eq!(buf.visual_rows(3), vec![0..3, 3..4, 5..7]);
        assert_eq!(buf.cursor_position(3), (2, 2));

        buf.move_up(3);
        assert_eq!(buf.cursor(), 4);
        buf.move_up(3);
        assert_eq!(buf.cursor(), 1);
        buf.move_down(3);
        assert_eq!(buf.cursor(), 4);
    }

    #[test]
    fn keeps_zero_width_marks_with_their_wrapped_character() {
        let mut buf = InputBuffer::new();
        buf.insert_str("e\u{301}");
        assert_eq!(buf.visual_rows(1), vec![0..2, 2..2]);
    }

    #[test]
    fn blank_drafts_are_not_submittable() {
        let mut buf = InputBuffer::new();
        buf.insert_str(" \n\t");
        assert!(buf.is_blank());
        buf.insert('x');
        assert!(!buf.is_blank());
    }
}
