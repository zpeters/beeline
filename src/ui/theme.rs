//! Fonts, text sizes, the app theme, and rounded widget styles. The widget
//! wrappers below match iced's own widgets, with corners rounded to
//! `RADIUS`. Views call these instead of iced's helpers.

use iced::widget::{self as w, container, pick_list, text_editor};
use iced::{Border, Element, Font, Theme};

use crate::app::App;

pub const SANS_BYTES: &[u8] = include_bytes!("../../assets/fonts/AtkinsonHyperlegible-Regular.otf");
pub const MONO_BYTES: &[u8] =
    include_bytes!("../../assets/fonts/AtkinsonHyperlegibleMono-Regular.ttf");

pub const SANS: Font = Font::with_name("Atkinson Hyperlegible");
pub const MONO: Font = Font::with_name("Atkinson Hyperlegible Mono");

pub const TEXT: f32 = 16.0;
pub const SMALL: f32 = 13.0;
pub const HEADING: f32 = 22.0;

/// Corner radius for buttons, inputs and cards.
pub const RADIUS: f32 = 10.0;

/// Iced's light palette with a #62b6cb accent.
pub fn app(_: &App) -> Theme {
    Theme::custom(
        "Accent".to_string(),
        iced::theme::Palette {
            primary: iced::Color::from_rgb8(98, 182, 203),
            ..iced::theme::Palette::LIGHT
        },
    )
}

fn round(border: &mut Border) {
    border.radius = RADIUS.into();
}

// --- buttons ---

pub fn button<'a, M: Clone + 'a>(content: impl Into<Element<'a, M>>) -> w::button::Button<'a, M> {
    w::button::Button::new(content).style(primary)
}

pub fn primary(theme: &Theme, status: w::button::Status) -> w::button::Style {
    let mut s = w::button::primary(theme, status);
    round(&mut s.border);
    s
}

pub fn secondary(theme: &Theme, status: w::button::Status) -> w::button::Style {
    let mut s = w::button::secondary(theme, status);
    round(&mut s.border);
    s
}

pub fn danger(theme: &Theme, status: w::button::Status) -> w::button::Style {
    let mut s = w::button::danger(theme, status);
    round(&mut s.border);
    s
}

/// A borderless button, for tree rows and menu toggles.
pub fn text_button(theme: &Theme, status: w::button::Status) -> w::button::Style {
    let mut s = w::button::text(theme, status);
    round(&mut s.border);
    s
}

// --- inputs ---

pub fn text_input<'a, M: Clone + 'a>(
    placeholder: &str,
    value: &str,
) -> w::text_input::TextInput<'a, M> {
    w::text_input::TextInput::new(placeholder, value).style(input)
}

pub fn input(theme: &Theme, status: w::text_input::Status) -> w::text_input::Style {
    let mut s = w::text_input::default(theme, status);
    round(&mut s.border);
    s
}

pub fn editor(theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let mut s = text_editor::default(theme, status);
    round(&mut s.border);
    s
}

pub fn pick(theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let mut s = pick_list::default(theme, status);
    round(&mut s.border);
    s
}

// --- containers ---

/// A rounded panel with a visible border.
pub fn card(theme: &Theme) -> container::Style {
    let mut s = container::bordered_box(theme);
    round(&mut s.border);
    s
}
