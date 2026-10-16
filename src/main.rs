#![forbid(unsafe_code)]

mod app;
mod collection;
mod format;
mod http;
mod model;
mod query;
mod resolve;
mod ui;
mod vars;

use std::path::PathBuf;

fn main() -> iced::Result {
    // `beeline <folder>` opens that folder as the collection.
    let start = std::env::args_os().nth(1).map(PathBuf::from);
    iced::application(
        move || app::App::boot(start.clone()),
        app::App::update,
        app::App::view,
    )
    .title("beeline")
    .theme(ui::theme::app)
    .settings(iced::Settings {
        default_font: ui::theme::SANS,
        default_text_size: iced::Pixels(ui::theme::TEXT),
        fonts: vec![ui::theme::SANS_BYTES.into(), ui::theme::MONO_BYTES.into()],
        ..Default::default()
    })
    .window_size((1100.0, 760.0))
    .exit_on_close_request(false)
    .subscription(app::App::subscription)
    .run()
}
