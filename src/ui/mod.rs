//! egui rendering. Widgets here only edit state they are handed; they do
//! no I/O.

pub mod editor;
pub mod response;
pub mod sidebar;
pub mod theme;

use eframe::egui;

use crate::model::KeyValue;

/// Editable key/value rows with enable checkboxes. Returns true when
/// anything changed.
pub fn kv_table(ui: &mut egui::Ui, id: &str, rows: &mut Vec<KeyValue>) -> bool {
    kv_rows(ui, id, rows, true)
}

/// Like `kv_table`, without the enable checkboxes.
pub fn kv_table_plain(ui: &mut egui::Ui, id: &str, rows: &mut Vec<KeyValue>) -> bool {
    kv_rows(ui, id, rows, false)
}

fn kv_rows(ui: &mut egui::Ui, id: &str, rows: &mut Vec<KeyValue>, checkboxes: bool) -> bool {
    let mut changed = false;
    let mut remove = None;
    let value_width = (ui.available_width() - 320.0).max(120.0);
    egui::Grid::new(id)
        .num_columns(if checkboxes { 4 } else { 3 })
        .spacing([6.0, 4.0])
        .show(ui, |ui| {
            for (i, row) in rows.iter_mut().enumerate() {
                if checkboxes {
                    changed |= ui.checkbox(&mut row.enabled, "").changed();
                }
                changed |= ui
                    .add(
                        egui::TextEdit::singleline(&mut row.key)
                            .hint_text("key")
                            .desired_width(220.0)
                            .margin(egui::Margin::symmetric(8, 6)),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::TextEdit::singleline(&mut row.value)
                            .hint_text("value")
                            .desired_width(value_width)
                            .margin(egui::Margin::symmetric(8, 6)),
                    )
                    .changed();
                if ui.button("🗙").on_hover_text("Remove").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    if let Some(i) = remove {
        rows.remove(i);
        changed = true;
    }
    if ui.button("+ Add").clicked() {
        rows.push(KeyValue::default());
        changed = true;
    }
    changed
}
