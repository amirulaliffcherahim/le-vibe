//! OSC 133 shell-integration prompt zones.
//!
//! pi renders inline and wraps each user/assistant turn in `ESC ]133;A` … `ESC ]133;B ESC ]133;C`
//! so shell-integration aware terminals (WezTerm, Ghostty, iTerm2, Windows Terminal) can jump
//! between prompts.
//!
//! `le-vibe` draws a full-screen alternate buffer, and the alternate buffer has no scrollback, so
//! these marks are currently inert. They are exposed as a ready-to-wire capability rather than
//! injected into the render loop: writing escape bytes into a cell symbol would corrupt
//! `unicode-width` cell math and the `ResyncBackend` diff path, which is a real regression for a
//! purely cosmetic (and here, invisible) gain. Wiring them up only becomes worthwhile together
//! with an inline (non-alternate-screen) renderer.

use std::io::{self, Write};

use crate::state::TranscriptBlock;

/// Prompt start (beginning of a user turn).
pub const OSC133_ZONE_START: &str = "\x1b]133;A\x07";
/// Prompt end (the user has submitted).
pub const OSC133_ZONE_END: &str = "\x1b]133;B\x07";
/// Command output start.
pub const OSC133_ZONE_FINAL: &str = "\x1b]133;C\x07";

/// Whether OSC 133 emission is requested (`LE_VIBE_OSC133=1`).
pub fn osc133_enabled() -> bool {
    matches!(
        std::env::var("LE_VIBE_OSC133").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

/// Write the three-mark wrapper for one zone around an already-rendered block of text.
pub fn write_zone<W: Write>(writer: &mut W, body: &str) -> io::Result<()> {
    writer.write_all(OSC133_ZONE_START.as_bytes())?;
    writer.write_all(body.as_bytes())?;
    writer.write_all(OSC133_ZONE_END.as_bytes())?;
    writer.write_all(OSC133_ZONE_FINAL.as_bytes())?;
    Ok(())
}

/// Whether `blocks[idx]` may open a prompt zone.
///
/// pi skips the zone on an assistant message that is followed by tool activity: the zone is meant
/// to cover a complete prompt/response pair, and tool output keeps streaming into the same block.
pub fn prompt_zone_for(blocks: &[TranscriptBlock], idx: usize) -> bool {
    match blocks.get(idx) {
        Some(TranscriptBlock::User(_)) => true,
        Some(TranscriptBlock::Assistant(text)) => {
            !text.trim().is_empty()
                && !matches!(blocks.get(idx + 1), Some(TranscriptBlock::Tool(_)))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_marks_in_pi_order() {
        let mut out: Vec<u8> = Vec::new();
        write_zone(&mut out, "hi").unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "\x1b]133;A\x07hi\x1b]133;B\x07\x1b]133;C\x07"
        );
    }

    #[test]
    fn marks_are_bell_terminated_ascii_escapes() {
        for mark in [OSC133_ZONE_START, OSC133_ZONE_END, OSC133_ZONE_FINAL] {
            assert!(mark.starts_with("\x1b]133;"), "{mark:?}");
            assert!(mark.ends_with('\u{7}'), "{mark:?}");
            assert!(mark.is_ascii());
        }
    }
}
