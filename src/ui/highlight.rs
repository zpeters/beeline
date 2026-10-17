//! JSON syntax colours for `text_editor`, on top of the pure tokenizer in
//! `crate::highlight`.

use std::ops::Range;

use iced::advanced::text::highlighter::{self, Format};
use iced::{Color, Font, Theme};

use crate::highlight::{self as json, Token};

/// Colours JSON when its setting is true; otherwise colours nothing, so
/// one editor type serves both JSON and plain text.
pub struct Json {
    on: bool,
    current: usize,
}

impl highlighter::Highlighter for Json {
    type Settings = bool;
    type Highlight = Token;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Token)>;

    fn new(on: &bool) -> Self {
        Self {
            on: *on,
            current: 0,
        }
    }

    fn update(&mut self, on: &bool) {
        self.on = *on;
        self.current = 0;
    }

    fn change_line(&mut self, line: usize) {
        self.current = self.current.min(line);
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        self.current += 1;
        if self.on {
            json::line(line).into_iter()
        } else {
            Vec::new().into_iter()
        }
    }

    fn current_line(&self) -> usize {
        self.current
    }
}

/// A token's colour, in a light and a dark variant.
pub fn format(token: &Token, theme: &Theme) -> Format<Font> {
    let dark = theme.extended_palette().is_dark;
    let pick = |light: [u8; 3], dark_rgb: [u8; 3]| {
        let [r, g, b] = if dark { dark_rgb } else { light };
        Color::from_rgb8(r, g, b)
    };
    let color = match token {
        Token::Key => pick([31, 111, 139], [98, 182, 203]),
        Token::String => pick([46, 125, 50], [143, 209, 143]),
        Token::Number => pick([181, 93, 0], [240, 163, 94]),
        Token::Literal => pick([142, 68, 173], [199, 146, 234]),
        Token::Punct => theme.palette().text.scale_alpha(0.55),
    };
    Format {
        color: Some(color),
        font: None,
    }
}
