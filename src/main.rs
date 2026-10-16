#![forbid(unsafe_code)]

mod app;
mod collection;
mod curl;
mod format;
mod http;
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
    .window_size((1100.0, 760.0))
    .exit_on_close_request(false)
    .subscription(app::App::subscription)
    .run()
}
