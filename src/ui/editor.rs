//! The request editor: method, URL, Send, and the Params / Headers /
//! Body / Auth tabs.

use std::collections::BTreeMap;

use eframe::egui;

use crate::model::{Auth, AuthKind, Body, BodyKind, KeyValue, METHODS, RequestFile};
use crate::ui::kv_table;
use crate::{query, vars};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Params,
    Headers,
    Body,
    Auth,
}

fn count_label(name: &str, rows: &[KeyValue]) -> String {
    match rows.iter().filter(|kv| kv.enabled).count() {
        0 => name.to_string(),
        n => format!("{name} ({n})"),
    }
}

/// Every `{{name}}` the request would send that `vars` lacks.
fn unknown_vars(req: &RequestFile, vars: &BTreeMap<String, String>) -> Vec<String> {
    let mut text = req.url.clone();
    let mut add = |rows: &[KeyValue]| {
        for kv in rows.iter().filter(|kv| kv.enabled) {
            text.push_str(&kv.key);
            text.push_str(&kv.value);
        }
    };
    add(&req.headers);
    match req.body.kind {
        BodyKind::None => {}
        BodyKind::Json | BodyKind::Text => text.push_str(&req.body.text),
        BodyKind::Form => add(&req.body.fields),
    }
    match req.auth.kind {
        AuthKind::None => {}
        AuthKind::Bearer => text.push_str(&req.auth.token),
        AuthKind::Basic => {
            text.push_str(&req.auth.username);
            text.push_str(&req.auth.password);
        }
    }
    vars::missing(&text, vars)
}

/// Draws the editor. `vars` are the selected environment's variables,
/// used only for hints. Returns true when the user asked to send.
pub fn show(
    ui: &mut egui::Ui,
    req: &mut RequestFile,
    tab: &mut Tab,
    busy: bool,
    vars: &BTreeMap<String, String>,
) -> bool {
    let mut send = false;

    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("method")
            .width(90.0)
            .selected_text(&req.method)
            .show_ui(ui, |ui| {
                for m in METHODS {
                    ui.selectable_value(&mut req.method, m.to_string(), m);
                }
            });
        let send_width = 70.0;
        let url = ui.add(
            egui::TextEdit::singleline(&mut req.url)
                .hint_text("https://example.com or {{base}}/path")
                .font(egui::TextStyle::Monospace)
                .desired_width(ui.available_width() - send_width - 8.0),
        );
        if url.changed() {
            req.params = query::params_from_url(&req.url, &req.params);
        }
        if url.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            send = true;
        }
        if req.url.contains("{{") {
            url.on_hover_text(vars::interpolate(&req.url, vars));
        }
        let label = if busy { "…" } else { "Send" };
        send |= ui
            .add_enabled(
                !busy,
                egui::Button::new(label).min_size(egui::vec2(send_width, 0.0)),
            )
            .clicked();
    });

    let unknown = unknown_vars(req, vars);
    if !unknown.is_empty() {
        let names: Vec<String> = unknown.iter().map(|n| format!("{{{{{n}}}}}")).collect();
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!("Not in the environment: {}", names.join(", ")),
        );
    }

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.selectable_value(tab, Tab::Params, count_label("Params", &req.params));
        ui.selectable_value(tab, Tab::Headers, count_label("Headers", &req.headers));
        let body = match req.body.kind {
            BodyKind::None => "Body".to_string(),
            kind => format!("Body ({})", kind.label()),
        };
        ui.selectable_value(tab, Tab::Body, body);
        let auth = match req.auth.kind {
            AuthKind::None => "Auth".to_string(),
            kind => format!("Auth ({})", kind.label()),
        };
        ui.selectable_value(tab, Tab::Auth, auth);
    });
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| match tab {
            Tab::Params => {
                if kv_table(ui, "params", &mut req.params) {
                    req.url = query::url_from_params(&req.url, &req.params);
                }
            }
            Tab::Headers => {
                kv_table(ui, "headers", &mut req.headers);
            }
            Tab::Body => body_editor(ui, &mut req.body),
            Tab::Auth => auth_editor(ui, &mut req.auth),
        });

    send && !busy
}

fn body_editor(ui: &mut egui::Ui, body: &mut Body) {
    ui.horizontal(|ui| {
        for kind in BodyKind::ALL {
            ui.radio_value(&mut body.kind, kind, kind.label());
        }
    });
    ui.add_space(4.0);
    match body.kind {
        BodyKind::None => {
            ui.weak("This request has no body.");
        }
        BodyKind::Json | BodyKind::Text => {
            if body.kind == BodyKind::Json {
                let pretty = serde_json::from_str::<serde_json::Value>(&body.text)
                    .ok()
                    .and_then(|v| serde_json::to_string_pretty(&v).ok());
                if ui
                    .add_enabled(pretty.is_some(), egui::Button::new("Format"))
                    .on_disabled_hover_text("Not valid JSON (or it has {{vars}} outside strings)")
                    .clicked()
                    && let Some(p) = pretty
                {
                    body.text = p;
                }
            }
            ui.add(
                egui::TextEdit::multiline(&mut body.text)
                    .code_editor()
                    .desired_rows(10)
                    .desired_width(f32::INFINITY),
            );
        }
        BodyKind::Form => {
            kv_table(ui, "form", &mut body.fields);
        }
    }
}

fn auth_editor(ui: &mut egui::Ui, auth: &mut Auth) {
    ui.horizontal(|ui| {
        for kind in AuthKind::ALL {
            ui.radio_value(&mut auth.kind, kind, kind.label());
        }
    });
    ui.add_space(4.0);
    egui::Grid::new("auth")
        .num_columns(2)
        .show(ui, |ui| match auth.kind {
            AuthKind::None => {
                ui.weak("No auth. An Authorization header you set yourself is still sent.");
            }
            AuthKind::Bearer => {
                ui.label("Token");
                ui.add(
                    egui::TextEdit::singleline(&mut auth.token)
                        .hint_text("{{token}}")
                        .desired_width(400.0),
                );
                ui.end_row();
            }
            AuthKind::Basic => {
                ui.label("Username");
                ui.add(egui::TextEdit::singleline(&mut auth.username).desired_width(300.0));
                ui.end_row();
                ui.label("Password");
                ui.add(
                    egui::TextEdit::singleline(&mut auth.password)
                        .password(true)
                        .desired_width(300.0),
                );
                ui.end_row();
            }
        });
}
