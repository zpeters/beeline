//! The response viewer: status line, Body and Headers tabs.

use eframe::egui;

use crate::{format, http};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Body,
    Headers,
}

/// A response plus its display text, computed once when it arrives
/// rather than every frame.
pub struct View {
    resp: http::Response,
    text: String,
    is_json: bool,
    tab: Tab,
}

impl View {
    pub fn new(resp: http::Response) -> Self {
        let (text, is_json) = format::body_text(&resp.body);
        Self {
            resp,
            text,
            is_json,
            tab: Tab::default(),
        }
    }
}

fn status_color(ui: &egui::Ui, status: u16) -> egui::Color32 {
    match status {
        200..=299 => egui::Color32::from_rgb(70, 170, 90),
        300..=399 => egui::Color32::from_rgb(200, 150, 40),
        _ => ui.visuals().error_fg_color,
    }
}

pub fn show(ui: &mut egui::Ui, view: &mut View) {
    let resp = &view.resp;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{} {}", resp.status, resp.reason))
                .strong()
                .color(status_color(ui, resp.status)),
        );
        ui.separator();
        ui.label(format::duration(resp.elapsed));
        ui.separator();
        ui.label(format::size(resp.body.len()));
        if view.is_json {
            ui.separator();
            ui.weak("JSON");
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Copy body").clicked() {
                ui.ctx().copy_text(view.text.clone());
            }
        });
    });

    ui.horizontal(|ui| {
        ui.selectable_value(&mut view.tab, Tab::Body, "Body");
        ui.selectable_value(
            &mut view.tab,
            Tab::Headers,
            format!("Headers ({})", resp.headers.len()),
        );
    });
    ui.separator();

    match view.tab {
        Tab::Body => {
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut view.text.as_str())
                            .code_editor()
                            .desired_width(f32::INFINITY),
                    );
                });
        }
        Tab::Headers => {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    egui::Grid::new("response-headers")
                        .num_columns(2)
                        .striped(true)
                        .show(ui, |ui| {
                            for (k, v) in &resp.headers {
                                ui.add(egui::Label::new(egui::RichText::new(k).strong()));
                                ui.add(egui::Label::new(v).wrap());
                                ui.end_row();
                            }
                        });
                });
        }
    }
}
