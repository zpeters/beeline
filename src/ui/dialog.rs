//! Overlay dialogs: unsaved changes, naming, move-to-Trash, the
//! environment editor and settings. They only report messages; `App` acts
//! on them.

use iced::widget::{
    column, container, pick_list, radio, row, scrollable, space, text, text_editor,
};
use iced::{Color, Element, Fill, Length};

use crate::model::KeyValue;
use crate::settings::{MonoFont, SansFont, Settings, TEXT_SIZES, ThemeChoice};
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
    SettingsTheme(ThemeChoice),
    SettingsSans(SansFont),
    SettingsMono(MonoFont),
    SettingsSize(u16),
    SettingsTimeout(String),
    SettingsSave,
    CurlEdit(text_editor::Action),
    CurlImport,
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
            text("Unsaved changes").size(theme::heading_px()),
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

/// The settings form. `timeout` is the text the user is typing; it is
/// checked when Save is pressed, so a half-typed number is not an error.
pub fn settings<'a>(
    draft: &'a Settings,
    timeout: &'a str,
    error: Option<&'a str>,
) -> Element<'a, Msg> {
    let themes = ThemeChoice::ALL
        .into_iter()
        .fold(row![].spacing(12), |r, choice| {
            r.push(radio(
                choice.to_string(),
                choice,
                Some(draft.theme),
                Msg::SettingsTheme,
            ))
        });
    let sizes: Vec<u16> = TEXT_SIZES.collect();
    let mut col = column![
        text("Settings").size(theme::heading_px()),
        text("Theme"),
        themes,
        text("Sans-serif font (applies after restart)"),
        pick_list(&SansFont::ALL[..], Some(draft.sans), Msg::SettingsSans).style(theme::pick),
        text("Monospace font (applies after restart)"),
        pick_list(&MonoFont::ALL[..], Some(draft.mono), Msg::SettingsMono).style(theme::pick),
        text("Text size"),
        pick_list(sizes, Some(draft.text_size), Msg::SettingsSize).style(theme::pick),
        text("Request timeout (seconds, 1 to 600)"),
        theme::text_input("30", timeout).on_input(Msg::SettingsTimeout),
    ]
    .spacing(8);
    if let Some(error) = error {
        col = col.push(text(error).style(text::danger));
    }
    let buttons = row![
        theme::button("Save").on_press(Msg::SettingsSave),
        theme::button("Cancel").on_press(Msg::Cancel),
    ]
    .spacing(8);
    overlay(col.push(buttons).into())
}

pub fn name<'a>(heading: &'static str, value: &'a str, error: Option<&'a str>) -> Element<'a, Msg> {
    let mut col = column![
        text(heading).size(theme::heading_px()),
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

/// Paste a curl command; Import turns it into a request.
pub fn import_curl<'a>(
    content: &'a text_editor::Content,
    error: Option<&'a str>,
) -> Element<'a, Msg> {
    let mut col = column![
        text("Import cURL").size(theme::heading_px()),
        text("Paste a curl command, such as one copied from a browser or API docs."),
        text_editor(content)
            .style(theme::editor)
            .font(theme::mono())
            .on_action(Msg::CurlEdit)
            .height(160),
    ]
    .spacing(12);
    if let Some(error) = error {
        col = col.push(text(error).style(text::danger));
    }
    let can_import = !content.text().trim().is_empty();
    col = col.push(
        row![
            theme::button("Import").on_press_maybe(can_import.then_some(Msg::CurlImport)),
            theme::button("Cancel").on_press(Msg::Cancel),
        ]
        .spacing(8),
    );
    overlay(col.into())
}

pub fn delete(what: String) -> Element<'static, Msg> {
    overlay(
        column![
            text("Move to Trash").size(theme::heading_px()),
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
            text(format!("Environment: {name}")).size(theme::heading_px()),
            text("Use {{name}} in the URL, params, headers, body and auth.")
                .size(theme::small_px()),
            scrollable(kv::view(rows, false).map(Msg::EnvKv)).height(Length::Fixed(360.0)),
            row![save, close, space::horizontal().width(Fill), trash].spacing(8),
        ]
        .spacing(12)
        .into(),
    )
}
