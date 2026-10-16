//! Editable key/value rows, shared by params, headers, form bodies and
//! environments. Widgets only report messages; `update` applies them.

use iced::widget::{checkbox, column, row};
use iced::{Element, Fill};

use crate::model::KeyValue;
use crate::ui::theme;

#[derive(Debug, Clone)]
pub enum Msg {
    Toggle(usize, bool),
    Key(usize, String),
    Value(usize, String),
    Remove(usize),
    Add,
}

pub fn update(rows: &mut Vec<KeyValue>, msg: Msg) {
    match msg {
        Msg::Toggle(i, on) => {
            if let Some(kv) = rows.get_mut(i) {
                kv.enabled = on;
            }
        }
        Msg::Key(i, s) => {
            if let Some(kv) = rows.get_mut(i) {
                kv.key = s;
            }
        }
        Msg::Value(i, s) => {
            if let Some(kv) = rows.get_mut(i) {
                kv.value = s;
            }
        }
        Msg::Remove(i) => {
            if i < rows.len() {
                rows.remove(i);
            }
        }
        Msg::Add => rows.push(KeyValue::default()),
    }
}

/// One row per entry, then an "Add" button. `checkboxes` adds the
/// enable/disable toggle that params and headers use.
pub fn view(rows: &[KeyValue], checkboxes: bool) -> Element<'_, Msg> {
    let mut col = column![].spacing(6);
    for (i, kv) in rows.iter().enumerate() {
        let mut line = row![].spacing(6);
        if checkboxes {
            line = line.push(checkbox(kv.enabled).on_toggle(move |on| Msg::Toggle(i, on)));
        }
        line = line
            .push(
                theme::text_input("key", &kv.key)
                    .font(theme::MONO)
                    .on_input(move |s| Msg::Key(i, s))
                    .width(Fill),
            )
            .push(
                theme::text_input("value", &kv.value)
                    .font(theme::MONO)
                    .on_input(move |s| Msg::Value(i, s))
                    .width(Fill),
            )
            .push(theme::button("✕").on_press(Msg::Remove(i)));
        col = col.push(line);
    }
    col.push(theme::button("+ Add").on_press(Msg::Add)).into()
}
