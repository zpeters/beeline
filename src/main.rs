#![forbid(unsafe_code)]

mod app;
mod format;
mod http;
mod model;
mod query;
mod resolve;
mod ui;
mod vars;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("beeline")
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([640.0, 420.0]),
        ..Default::default()
    };
    eframe::run_native(
        "beeline",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
