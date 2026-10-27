//! JSON syntax colours and `{{variable}}` colours for `text_editor`, on top
//! of the pure tokenizer in `crate::highlight`.

use std::collections::BTreeSet;
use std::ops::Range;

use iced::advanced::text::highlighter::{self, Format};
use iced::{Color, Font, Theme};

use crate::highlight::{self as json, Token};

/// What to colour. `json` turns on JSON colours. `vars` turns on
/// `{{name}}` colours: the names that have a value. `None` leaves
/// variables plain, as in a response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub json: bool,
    pub vars: Option<BTreeSet<String>>,
}

pub struct Json {
    settings: Settings,
    current: usize,
}

impl highlighter::Highlighter for Json {
    type Settings = Settings;
    type Highlight = Token;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Token)>;

    fn new(settings: &Settings) -> Self {
        Self {
            settings: settings.clone(),
            current: 0,
        }
    }

    fn update(&mut self, settings: &Settings) {
        self.settings = settings.clone();
        self.current = 0;
    }

    fn change_line(&mut self, line: usize) {
        self.current = self.current.min(line);
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        self.current += 1;
        let json = self.settings.json;
        match &self.settings.vars {
            Some(names) => {
                json::line_with_vars(line, json, |name| names.contains(name)).into_iter()
            }
            None if json => json::line(line).into_iter(),
            None => Vec::new().into_iter(),
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
        Token::VarOk => pick([46, 125, 50], [143, 209, 143]),
        Token::VarMissing => pick([198, 40, 40], [255, 123, 123]),
    };
    Format {
        color: Some(color),
        font: None,
    }
}
