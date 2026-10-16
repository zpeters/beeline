//! User settings: theme, fonts, text size and request timeout. Pure: the
//! file is read and written by `app.rs`.

use std::fmt;
use std::ops::RangeInclusive;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::model::to_json;

/// Text sizes the settings dialog offers, in points.
pub const TEXT_SIZES: RangeInclusive<u16> = 12..=22;
/// Request timeouts the settings dialog accepts, in seconds.
pub const TIMEOUT_SECS: RangeInclusive<u64> = 1..=600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    Light,
    Dark,
    #[default]
    System,
}

impl ThemeChoice {
    pub const ALL: [Self; 3] = [Self::Light, Self::Dark, Self::System];
}

impl fmt::Display for ThemeChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::System => "System",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SansFont {
    #[default]
    Atkinson,
    Inter,
}

impl SansFont {
    pub const ALL: [Self; 2] = [Self::Atkinson, Self::Inter];

    /// The family name the bundled font registers under.
    pub fn family(self) -> &'static str {
        match self {
            Self::Atkinson => "Atkinson Hyperlegible",
            Self::Inter => "Inter",
        }
    }
}

impl fmt::Display for SansFont {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.family())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MonoFont {
    #[default]
    Atkinson,
    Plex,
}

impl MonoFont {
    pub const ALL: [Self; 2] = [Self::Atkinson, Self::Plex];

    pub fn family(self) -> &'static str {
        match self {
            Self::Atkinson => "Atkinson Hyperlegible Mono",
            Self::Plex => "IBM Plex Mono",
        }
    }
}

impl fmt::Display for MonoFont {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.family())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeChoice,
    pub sans: SansFont,
    pub mono: MonoFont,
    /// The base text size. Headings and small text scale from it.
    pub text_size: u16,
    /// How long a request may take in total before it is given up.
    pub request_timeout_secs: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::default(),
            sans: SansFont::default(),
            mono: MonoFont::default(),
            text_size: 16,
            request_timeout_secs: 30,
        }
    }
}

impl Settings {
    /// Keeps numbers inside the ranges the dialog offers, so a hand-edited
    /// file cannot break the layout or make every request time out at once.
    pub fn clamped(mut self) -> Self {
        self.text_size = self.text_size.clamp(*TEXT_SIZES.start(), *TEXT_SIZES.end());
        self.request_timeout_secs = self
            .request_timeout_secs
            .clamp(*TIMEOUT_SECS.start(), *TIMEOUT_SECS.end());
        self
    }

    /// Reads a settings file. Anything unreadable gives the defaults.
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str::<Self>(json)
            .map(Self::clamped)
            .unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        to_json(self)
    }

    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_app_look() {
        let s = Settings::default();
        assert_eq!(s.theme, ThemeChoice::System);
        assert_eq!(s.sans, SansFont::Atkinson);
        assert_eq!(s.mono, MonoFont::Atkinson);
        assert_eq!(s.text_size, 16);
        assert_eq!(s.request_timeout_secs, 30);
    }

    #[test]
    fn missing_or_broken_file_gives_defaults() {
        assert_eq!(Settings::from_json(""), Settings::default());
        assert_eq!(Settings::from_json("{not json"), Settings::default());
    }

    #[test]
    fn partial_file_keeps_the_rest_at_defaults() {
        let s = Settings::from_json(r#"{"theme": "dark", "text_size": 18}"#);
        assert_eq!(s.theme, ThemeChoice::Dark);
        assert_eq!(s.text_size, 18);
        assert_eq!(s.request_timeout_secs, 30);
    }

    #[test]
    fn out_of_range_numbers_are_clamped() {
        let s = Settings::from_json(r#"{"text_size": 99, "request_timeout_secs": 0}"#);
        assert_eq!(s.text_size, 22);
        assert_eq!(s.request_timeout_secs, 1);
    }

    #[test]
    fn round_trips() {
        let s = Settings {
            theme: ThemeChoice::Light,
            sans: SansFont::Inter,
            mono: MonoFont::Plex,
            text_size: 14,
            request_timeout_secs: 90,
        };
        assert_eq!(Settings::from_json(&s.to_json()), s);
    }

    #[test]
    fn timeout_is_a_duration() {
        let s = Settings {
            request_timeout_secs: 45,
            ..Settings::default()
        };
        assert_eq!(s.timeout(), Duration::from_secs(45));
    }
}
