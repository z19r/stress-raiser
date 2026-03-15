//! Simple line/multiline editor with cursor, Ctrl+A, arrows.

use ratatui::prelude::*;
use unicode_segmentation::UnicodeSegmentation;

/// Single-line or multi-line buffer with cursor. Cursor is grapheme index.
#[derive(Default, Clone)]
pub struct Editor {
    pub buf: String,
    pub cursor: usize,
}

impl Editor {
    /// Create an editor with initial content; cursor at end.
    pub fn new(s: impl Into<String>) -> Self {
        let buf = s.into();
        let cursor = buf.graphemes(true).count();
        Self { buf, cursor }
    }

    /// Buffer contents as a string slice.
    pub fn as_str(&self) -> &str {
        &self.buf
    }

    /// True if the buffer has no content.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    fn clamp_cursor(&mut self) {
        let len = self.buf.graphemes(true).count();
        self.cursor = self.cursor.min(len);
    }

    /// Byte index of the start of the grapheme at the given grapheme index.
    fn byte_index_at_grapheme(&self, grapheme_idx: usize) -> usize {
        self.buf
            .grapheme_indices(true)
            .nth(grapheme_idx)
            .map(|(i, _)| i)
            .unwrap_or(self.buf.len())
    }

    /// Byte range (start, end) for the grapheme at the given grapheme index.
    fn byte_range_at_grapheme(&self, grapheme_idx: usize) -> (usize, usize) {
        self.buf
            .grapheme_indices(true)
            .nth(grapheme_idx)
            .map(|(i, g)| (i, i + g.len()))
            .unwrap_or((self.buf.len(), self.buf.len()))
    }

    pub fn left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn right(&mut self) {
        self.clamp_cursor();
        if self.cursor < self.buf.graphemes(true).count() {
            self.cursor += 1;
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.buf.graphemes(true).count();
    }

    pub fn ctrl_a(&mut self) {
        self.home();
    }

    pub fn backspace(&mut self) {
        self.clamp_cursor();
        if self.cursor > 0 {
            let (idx, end) = self.byte_range_at_grapheme(self.cursor - 1);
            self.buf.drain(idx..end);
            self.cursor -= 1;
        }
    }

    pub fn delete(&mut self) {
        self.clamp_cursor();
        let len = self.buf.graphemes(true).count();
        if self.cursor < len {
            let idx = self.byte_index_at_grapheme(self.cursor);
            let (_, end) = self.byte_range_at_grapheme(self.cursor);
            self.buf.drain(idx..end);
        }
    }

    pub fn insert(&mut self, c: char) {
        self.clamp_cursor();
        let idx = self.byte_index_at_grapheme(self.cursor);
        self.buf.insert(idx, c);
        self.cursor += 1;
    }

    pub fn insert_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        self.clamp_cursor();
        let idx = self.byte_index_at_grapheme(self.cursor);
        let added = s.graphemes(true).count();
        self.buf.insert_str(idx, s);
        self.cursor += added;
    }

    /// Render as Line with visible block cursor. placeholder when empty.
    pub fn render_line(
        &self,
        _width: u16,
        placeholder: &str,
        focused: bool,
        style: Style,
        cursor_style: Style,
    ) -> Line<'static> {
        let placeholder = placeholder.to_string();
        let text = if self.buf.is_empty() {
            placeholder.clone()
        } else {
            self.buf.clone()
        };
        let graphemes: Vec<&str> = text.graphemes(true).collect();
        let cur = self.cursor.min(graphemes.len());
        let (before, at, after) = if focused && !text.is_empty() {
            let before: String = graphemes[..cur].concat();
            let at = graphemes.get(cur).copied().unwrap_or(" ").to_string();
            let after_start = (cur + 1).min(graphemes.len());
            let after: String = graphemes[after_start..].concat();
            (before, at, after)
        } else {
            (text.clone(), String::new(), String::new())
        };
        let mut spans = vec![];
        if !before.is_empty() {
            spans.push(Span::styled(before, style));
        }
        if focused && (!text.is_empty() || placeholder.is_empty()) {
            let c = if at.is_empty() { " " } else { &at };
            spans.push(Span::styled(c.to_string(), cursor_style));
        }
        if !after.is_empty() {
            spans.push(Span::styled(after, style));
        }
        if spans.is_empty() {
            let t = if focused { " ".into() } else { placeholder };
            let s = if focused { cursor_style } else { style };
            spans.push(Span::styled(t, s));
        }
        Line::from(spans)
    }

    /// Render multi-line (for headers/body). Returns Vec<Line>.
    pub fn render_lines(
        &self,
        _width: u16,
        max_lines: usize,
        placeholder: &str,
        focused: bool,
        style: Style,
        cursor_style: Style,
    ) -> Vec<Line<'static>> {
        let placeholder = placeholder.to_string();
        let lines: Vec<&str> = self.buf.lines().collect();
        let lines = if lines.is_empty() { vec![""] } else { lines };
        let (cursor_line, cursor_col) = self.cursor_line_col();
        let mut out = vec![];
        for (i, line) in lines.iter().take(max_lines).enumerate() {
            let line_cur = if focused && i == cursor_line {
                Some(cursor_col)
            } else {
                None
            };
            let graphemes: Vec<&str> = line.graphemes(true).collect();
            let cur = line_cur.unwrap_or(0).min(graphemes.len());
            let (before, at, after) = if line_cur.is_some() {
                let before: String = graphemes[..cur].concat();
                let at = graphemes.get(cur).copied().unwrap_or(" ").to_string();
                let after_start = (cur + 1).min(graphemes.len());
                let after: String = graphemes[after_start..].concat();
                (before, at, after)
            } else {
                (line.to_string(), String::new(), String::new())
            };
            let mut spans = vec![];
            if !before.is_empty() {
                spans.push(Span::styled(before, style));
            }
            if line_cur.is_some() {
                let c = if at.is_empty() { " " } else { &at };
                spans.push(Span::styled(c.to_string(), cursor_style));
            }
            if !after.is_empty() {
                spans.push(Span::styled(after, style));
            }
            if spans.is_empty() {
                let pl = if i == 0 && self.buf.is_empty() {
                    placeholder.clone()
                } else {
                    " ".to_string()
                };
                let s = if line_cur.is_some() {
                    cursor_style
                } else {
                    style
                };
                spans.push(Span::styled(pl, s));
            }
            out.push(Line::from(spans));
        }
        if self.buf.is_empty() && out.is_empty() {
            out.push(Line::from(Span::styled(placeholder.clone(), style)));
        }
        out
    }

    fn cursor_line_col(&self) -> (usize, usize) {
        let mut line = 0;
        let mut col = 0;
        for (idx, g) in self.buf.graphemes(true).enumerate() {
            if idx >= self.cursor {
                return (line, col);
            }
            if g == "\n" {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    /// Grapheme index at start of given line (0-based).
    fn line_start_grapheme(&self, line: usize) -> usize {
        let mut idx = 0;
        let mut cur_line = 0;
        for g in self.buf.graphemes(true) {
            if cur_line == line {
                return idx;
            }
            if g == "\n" {
                cur_line += 1;
            }
            idx += 1;
        }
        idx
    }

    pub fn up(&mut self) {
        let (line, col) = self.cursor_line_col();
        if line > 0 {
            let lines: Vec<&str> = self.buf.lines().collect();
            let prev = lines.get(line - 1).unwrap_or(&"");
            let new_col = col.min(prev.graphemes(true).count());
            self.cursor = self.line_start_grapheme(line - 1) + new_col;
        }
    }

    pub fn down(&mut self) {
        let (line, col) = self.cursor_line_col();
        let lines: Vec<&str> = self.buf.lines().collect();
        if line + 1 >= lines.len() {
            self.end();
            return;
        }
        let next_line = lines[line + 1];
        let new_col = col.min(next_line.graphemes(true).count());
        self.cursor = self.line_start_grapheme(line + 1) + new_col;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_cursor_at_end() {
        let e = Editor::new("hello");
        assert_eq!(e.as_str(), "hello");
        assert_eq!(e.cursor, 5);
    }

    #[test]
    fn left_right_home_end() {
        let mut e = Editor::new("ab");
        e.left();
        assert_eq!(e.cursor, 1);
        e.left();
        assert_eq!(e.cursor, 0);
        e.left();
        assert_eq!(e.cursor, 0);
        e.right();
        e.right();
        assert_eq!(e.cursor, 2);
        e.home();
        assert_eq!(e.cursor, 0);
        e.end();
        assert_eq!(e.cursor, 2);
    }

    #[test]
    fn backspace_deletes_before_cursor() {
        let mut e = Editor::new("abc");
        e.left();
        e.backspace();
        assert_eq!(e.as_str(), "ac");
        assert_eq!(e.cursor, 1);
    }

    #[test]
    fn insert_at_cursor() {
        let mut e = Editor::new("ac");
        e.left();
        e.insert('b');
        assert_eq!(e.as_str(), "abc");
        assert_eq!(e.cursor, 2);
    }

    #[test]
    fn delete_at_cursor() {
        let mut e = Editor::new("abc");
        e.left();
        e.delete();
        assert_eq!(e.as_str(), "ab");
        assert_eq!(e.cursor, 2);
    }

    #[test]
    fn insert_str() {
        let mut e = Editor::new("a");
        e.insert_str("bc");
        assert_eq!(e.as_str(), "abc");
        assert_eq!(e.cursor, 3);
    }
}
