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

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("beeline")
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([640.0, 420.0]),
        ..Default::default()
    };
    // `beeline <folder>` opens that folder as the collection.
    let start = std::env::args_os().nth(1).map(PathBuf::from);
    eframe::run_native(
        "beeline",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc, start)))),
    )
}
