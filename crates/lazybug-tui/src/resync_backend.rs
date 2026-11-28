//! Ratatui backend wrapper that resyncs terminal cursor position after wide/ambiguous Unicode glyphs
//! (inspired by Mistral Vibe's ResyncBackend) so emoji/symbols never drift modal borders on Windows Terminal.

use crossterm::cursor::MoveTo;
use crossterm::queue;
use ratatui::backend::{Backend, ClearType, CrosstermBackend, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};
use std::io::{self, Write};

pub struct ResyncBackend<W: Write> {
    inner: CrosstermBackend<W>,
}

impl<W: Write> ResyncBackend<W> {
    pub fn new(writer: W) -> Self {
        Self {
            inner: CrosstermBackend::new(writer),
        }
    }
}

impl<W: Write> Write for ResyncBackend<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Write::flush(&mut self.inner)
    }
}

/// Returns true for glyphs whose rendered width in Windows Terminal / ConPTY can disagree
/// with `unicode-width`, requiring an explicit `MoveTo` resync on the next cell.
fn needs_cursor_resync(symbol: &str) -> bool {
    let mut chars = symbol.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => matches!(
            c,
            '\u{2100}'..='\u{24FF}'
                | '\u{25A0}'..='\u{27FF}'
                | '\u{2900}'..='\u{2BFF}'
                | '\u{10000}'..
        ),
        (Some(_), Some(_)) => !symbol.is_ascii(),
        _ => false,
    }
}

impl<W: Write> Backend for ResyncBackend<W> {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        let mut batch: Vec<(u16, u16, &'a Cell)> = Vec::new();
        for (x, y, cell) in content {
            let resync = needs_cursor_resync(cell.symbol());
            batch.push((x, y, cell));
            if resync {
                self.inner.draw(batch.drain(..))?;
                let next_x = x.saturating_add(1);
                queue!(&mut self.inner, MoveTo(next_x, y))?;
            }
        }
        if !batch.is_empty() {
            self.inner.draw(batch.into_iter())?;
        }
        Ok(())
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> io::Result<()> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> io::Result<Size> {
        self.inner.size()
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> io::Result<()> {
        Backend::flush(&mut self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_emoji_and_ambiguous_symbols_for_resync() {
        assert!(!needs_cursor_resync("a"));
        assert!(!needs_cursor_resync("─"));
        assert!(needs_cursor_resync("⚡"));
        assert!(needs_cursor_resync("📎"));
        assert!(needs_cursor_resync("★"));
    }
}
