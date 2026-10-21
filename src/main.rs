#![forbid(unsafe_code)]
// A release build on Windows is a GUI app: no console window opens with it.
// Debug builds keep the console so `cargo run` still shows panics and logs.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod collection;
mod curl;
mod export;
mod format;
mod highlight;
mod history;
mod http;
mod import;
mod menu;
mod model;
mod query;
mod recents;
mod resolve;
mod samples;
mod seed;
mod settings;
mod spark;
mod tabs;
mod ui;
mod vars;

use std::path::PathBuf;

fn main() -> iced::Result {
    // `beeline <folder>` opens that folder as the collection.
    let start = std::env::args_os().nth(1).map(PathBuf::from);
    let settings = app::load_settings();
    ui::theme::use_settings(&settings);
    let boot_settings = settings.clone();
    iced::application(
        move || app::App::boot(start.clone(), boot_settings.clone()),
        app::App::update,
        app::App::view,
    )
    .title("beeline")
    .theme(app::App::theme)
    .settings(iced::Settings {
        // Fonts are fixed at launch, so this is read once here.
        default_font: ui::theme::sans(),
        default_text_size: iced::Pixels(ui::theme::text_px()),
        fonts: vec![
            ui::theme::SANS_BYTES.into(),
            ui::theme::MONO_BYTES.into(),
            ui::theme::INTER_BYTES.into(),
            ui::theme::PLEX_BYTES.into(),
        ],
        ..Default::default()
    })
    .window(iced::window::Settings {
        size: iced::Size::new(1100.0, 760.0),
        exit_on_close_request: false,
        icon: window_icon(),
        ..Default::default()
    })
    .subscription(app::App::subscription)
    .run()
}

/// The title bar and taskbar icon (Windows, Linux; macOS uses the bundle's).
/// build.rs decodes it from `assets/icon/128x128.png`.
fn window_icon() -> Option<iced::window::Icon> {
    const RGBA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/icon.rgba"));
    iced::window::icon::from_rgba(RGBA.to_vec(), 128, 128).ok()
}
