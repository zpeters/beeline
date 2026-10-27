//! Editable key/value rows, shared by params, headers, form bodies, request
//! vars and environments. Widgets only report messages; `update` applies them.
//! Each row's key and value are single-line fields (see `ui::field`), kept
//! in `Row`s that line up with the rows by index.

use std::collections::BTreeMap;

use iced::Element;
use iced::widget::text_editor::{Action, Content};
use iced::widget::{checkbox, column, row};

use crate::model::KeyValue;
use crate::ui::{field, theme};

/// The editors for one row.
#[derive(Default)]
pub struct Row {
    pub key: Content,
    pub value: Content,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Toggle(usize, bool),
    Key(usize, Action),
    Value(usize, Action),
    Remove(usize),
    Add,
}

/// Makes `editors` line up with `rows`, one per row, each showing its text.
pub fn sync_rows(rows: &[KeyValue], editors: &mut Vec<Row>) {
    editors.truncate(rows.len());
    while editors.len() < rows.len() {
        editors.push(Row::default());
    }
    for (kv, row) in rows.iter().zip(editors.iter_mut()) {
        field::sync(&mut row.key, &kv.key);
        field::sync(&mut row.value, &kv.value);
    }
}

pub fn update(rows: &mut Vec<KeyValue>, editors: &mut Vec<Row>, msg: Msg) {
    match msg {
        Msg::Toggle(i, on) => {
            if let Some(kv) = rows.get_mut(i) {
                kv.enabled = on;
            }
        }
        Msg::Key(i, action) => {
            if let (Some(kv), Some(row)) = (rows.get_mut(i), editors.get_mut(i)) {
                field::apply(&mut row.key, action);
                kv.key = row.key.text();
            }
        }
        Msg::Value(i, action) => {
            if let (Some(kv), Some(row)) = (rows.get_mut(i), editors.get_mut(i)) {
                field::apply(&mut row.value, action);
                kv.value = row.value.text();
            }
        }
        Msg::Remove(i) => {
            if i < rows.len() {
                rows.remove(i);
            }
        }
        Msg::Add => rows.push(KeyValue::default()),
    }
    sync_rows(rows, editors);
}

/// One row per entry, then an "Add" button. `checkboxes` adds the
/// enable/disable toggle that params and headers use. `vars` colours the
/// `{{name}}`s in keys and values; None leaves them plain.
pub fn view<'a>(
    rows: &'a [KeyValue],
    editors: &'a [Row],
    checkboxes: bool,
    vars: Option<&BTreeMap<String, String>>,
) -> Element<'a, Msg> {
    let mut col = column![].spacing(6);
    for (i, (kv, editor)) in rows.iter().zip(editors).enumerate() {
        let mut line = row![].spacing(6);
        if checkboxes {
            line = line.push(checkbox(kv.enabled).on_toggle(move |on| Msg::Toggle(i, on)));
        }
        line = line
            .push(field::view(&editor.key, vars, move |a| Msg::Key(i, a)))
            .push(field::view(&editor.value, vars, move |a| Msg::Value(i, a)))
            .push(theme::button("✕").on_press(Msg::Remove(i)));
        col = col.push(line);
    }
    col.push(theme::button("+ Add").on_press(Msg::Add)).into()
}
