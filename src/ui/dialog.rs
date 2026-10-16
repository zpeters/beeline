//! Overlay dialogs: unsaved changes, naming, move-to-Trash, and the
//! environment editor. They only report messages; `App` acts on them.

use iced::widget::{column, container, row, scrollable, space, text};
use iced::{Color, Element, Fill, Length};

use crate::model::KeyValue;
use crate::ui::{kv, theme};

/// The name field's id, so `App` can focus it when the dialog opens.
pub const NAME_ID: &str = "beeline-name";

#[derive(Debug, Clone)]
pub enum Msg {
    UnsavedSave,
    UnsavedDiscard,
    Cancel,
    NameInput(String),
    NameOk,
    DeleteOk,
    EnvKv(kv::Msg),
    EnvSave,
    EnvClose,
    EnvTrash,
}

/// Puts a dimmed backdrop behind `content`, centred.
pub fn overlay<'a>(content: Element<'a, Msg>) -> Element<'a, Msg> {
    let card = container(content).padding(20).width(480).style(theme::card);
    container(card)
        .center(Fill)
        .style(|_| container::Style {
            background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.5).into()),
            ..container::Style::default()
        })
        .into()
}

pub fn unsaved(title: String, scratch: bool) -> Element<'static, Msg> {
    let save = if scratch { "Save as…" } else { "Save" };
    overlay(
        column![
            text("Unsaved changes").size(theme::HEADING),
            text(format!("\"{title}\" has unsaved changes.")),
            row![
                theme::button(save).on_press(Msg::UnsavedSave),
                theme::button("Discard").on_press(Msg::UnsavedDiscard),
                theme::button("Cancel").on_press(Msg::Cancel),
            ]
            .spacing(8),
        ]
        .spacing(12)
        .into(),
    )
}

pub fn name<'a>(heading: &'static str, value: &'a str, error: Option<&'a str>) -> Element<'a, Msg> {
    let mut col = column![
        text(heading).size(theme::HEADING),
        theme::text_input("Name", value)
            .id(NAME_ID)
            .on_input(Msg::NameInput)
            .on_submit(Msg::NameOk)
            .width(Fill),
    ]
    .spacing(12);
    if let Some(e) = error {
        col = col.push(text(e).style(text::danger));
    }
    col = col.push(
        row![
            theme::button("OK").on_press(Msg::NameOk),
            theme::button("Cancel").on_press(Msg::Cancel),
        ]
        .spacing(8),
    );
    overlay(col.into())
}

pub fn delete(what: String) -> Element<'static, Msg> {
    overlay(
        column![
            text("Move to Trash").size(theme::HEADING),
            text(what),
            row![
                theme::button(text("Move to Trash"))
                    .style(theme::danger)
                    .on_press(Msg::DeleteOk),
                theme::button("Cancel").on_press(Msg::Cancel),
            ]
            .spacing(8),
        ]
        .spacing(12)
        .into(),
    )
}

pub fn env<'a>(name: &'a str, rows: &'a [KeyValue], dirty: bool) -> Element<'a, Msg> {
    let save = theme::button("Save").on_press_maybe(dirty.then_some(Msg::EnvSave));
    let close = theme::button(if dirty { "Discard" } else { "Close" }).on_press(Msg::EnvClose);
    let trash = theme::button("Move to Trash…")
        .style(theme::danger)
        .on_press(Msg::EnvTrash);
    overlay(
        column![
            text(format!("Environment: {name}")).size(theme::HEADING),
            text("Use {{name}} in the URL, params, headers, body and auth.").size(theme::SMALL),
            scrollable(kv::view(rows, false).map(Msg::EnvKv)).height(Length::Fixed(360.0)),
            row![save, close, space::horizontal().width(Fill), trash].spacing(8),
        ]
        .spacing(12)
        .into(),
    )
}
