//! Fonts, text sizes, the app theme, and rounded widget styles. The widget
//! wrappers below match iced's own widgets, with corners rounded to
//! `RADIUS`. Views call these instead of iced's helpers.

use std::sync::{PoisonError, RwLock};

use iced::widget::{self as w, column, container, pick_list, text, text_editor};
use iced::{Border, Color, Element, Fill, Font, Length, Theme, theme::Mode, theme::Palette};

use crate::settings::{Settings, ThemeChoice};

pub const SANS_BYTES: &[u8] = include_bytes!("../../assets/fonts/AtkinsonHyperlegible-Regular.otf");
pub const MONO_BYTES: &[u8] =
    include_bytes!("../../assets/fonts/AtkinsonHyperlegibleMono-Regular.ttf");
pub const INTER_BYTES: &[u8] = include_bytes!("../../assets/fonts/Inter-Variable.ttf");
pub const PLEX_BYTES: &[u8] = include_bytes!("../../assets/fonts/IBMPlexMono-Regular.ttf");

/// Corner radius for buttons, inputs and cards.
pub const RADIUS: f32 = 10.0;

// The settings a view reads on each frame. `use_settings` sets them at
// launch and whenever the settings dialog saves. Font families are read
// once at launch, so changing them takes a restart.
static TEXT_SIZE: RwLock<u16> = RwLock::new(16);
static SANS_FONT: RwLock<Font> = RwLock::new(Font::with_name("Atkinson Hyperlegible"));
static MONO_FONT: RwLock<Font> = RwLock::new(Font::with_name("Atkinson Hyperlegible Mono"));

/// Copies the text size and fonts from `settings` into what views read.
pub fn use_settings(settings: &Settings) {
    *TEXT_SIZE.write().unwrap_or_else(PoisonError::into_inner) = settings.text_size;
    *SANS_FONT.write().unwrap_or_else(PoisonError::into_inner) = font(settings.sans.family());
    *MONO_FONT.write().unwrap_or_else(PoisonError::into_inner) = font(settings.mono.family());
}

/// A font by family name. The name must be a `'static` string, which the
/// settings enums provide.
fn font(family: &'static str) -> Font {
    Font::with_name(family)
}

/// The body text size.
pub fn text_px() -> f32 {
    f32::from(*TEXT_SIZE.read().unwrap_or_else(PoisonError::into_inner))
}

/// Small text, such as the status line and table labels.
pub fn small_px() -> f32 {
    text_px() - 3.0
}

/// Headings.
pub fn heading_px() -> f32 {
    text_px() + 6.0
}

pub fn sans() -> Font {
    *SANS_FONT.read().unwrap_or_else(PoisonError::into_inner)
}

pub fn mono() -> Font {
    *MONO_FONT.read().unwrap_or_else(PoisonError::into_inner)
}

/// The app theme for a setting and the operating system's current mode.
/// The accent is #62b6cb on the matching built-in palette.
pub fn app_theme(choice: ThemeChoice, system: Mode) -> Theme {
    let dark = match choice {
        ThemeChoice::Light => false,
        ThemeChoice::Dark => true,
        ThemeChoice::System => system == Mode::Dark,
    };
    let base = if dark { Palette::DARK } else { Palette::LIGHT };
    Theme::custom(
        if dark { "Accent dark" } else { "Accent" }.to_string(),
        Palette {
            primary: Color::from_rgb8(98, 182, 203),
            ..base
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

/// Secondary text: the normal text colour at reduced strength, for
/// labels that support the data rather than compete with it.
pub fn muted(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.palette().text.scale_alpha(0.5)),
    }
}

/// A rounded panel with a visible border.
pub fn card(theme: &Theme) -> container::Style {
    let mut s = container::bordered_box(theme);
    round(&mut s.border);
    s
}

// --- tabs ---

/// A conventional tab: a label, with an accent bar under it when selected.
/// Tab strips are rows of these, with a rule drawn under the row.
pub fn tab<'a, T: PartialEq, M: Clone + 'a>(
    current: T,
    this: T,
    label: String,
    on_press: M,
) -> Element<'a, M> {
    let selected = current == this;
    let label = button(text(label).size(text_px()))
        .style(text_button)
        .on_press(on_press);
    let bar = container(w::space::horizontal())
        .height(3)
        .width(Fill)
        .style(move |theme: &Theme| container::Style {
            background: selected.then(|| theme.extended_palette().primary.base.color.into()),
            ..container::Style::default()
        });
    column![label, bar].width(Length::Shrink).into()
}
