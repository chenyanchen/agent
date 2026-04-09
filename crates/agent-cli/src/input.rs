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

    /// Return the current content as a `&str`.
    pub fn content(&self) -> &str {
        &self.buf
    }

    /// Return the current cursor position (in chars).
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Return `true` when the buffer contains no text.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
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
        assert!(buf.is_empty());
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
        assert!(buf.is_empty());
        assert_eq!(buf.cursor(), 0);
    }

    #[test]
    fn take_returns_content_and_resets() {
        let mut buf = InputBuffer::new();
        buf.insert('x');
        buf.insert('y');
        let taken = buf.take();
        assert_eq!(taken, "xy");
        assert!(buf.is_empty());
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
        assert!(buf.is_empty());
        assert_eq!(buf.cursor(), 0);
    }
}
