//! Single-line fields that colour `{{name}}`. iced's `text_input` cannot
//! colour part of its text, so a field is a `text_editor` that never holds
//! a line break. Its `Content` is kept in step with the request by `sync`.

use std::collections::BTreeMap;

use iced::widget::text::Wrapping;
use iced::widget::text_editor::{self, Action, Content, Edit};
use iced::{Element, Length};

use crate::ui::{highlight, theme};

/// Applies an action to a single-line field. Enter does nothing, and a
/// line break that gets in (a paste) becomes a space.
pub fn apply(content: &mut Content, action: Action) {
    if matches!(action, Action::Edit(Edit::Enter)) {
        return;
    }
    content.perform(action);
    let text = content.text();
    if text.contains('\n') {
        *content = Content::with_text(&text.replace('\n', " "));
    }
}

/// Makes `content` show `value`. Does nothing when it already does, so the
/// cursor stays put while the user types.
pub fn sync(content: &mut Content, value: &str) {
    if content.text() != value {
        *content = Content::with_text(&value.replace('\n', " "));
    }
}

/// A single-line field. With `vars` set, every `{{name}}` is coloured:
/// green when `vars` has the name, red when not. With `None` the text is
/// plain.
pub fn view<'a, M: Clone + 'a>(
    content: &'a Content,
    vars: Option<&BTreeMap<String, String>>,
    on_action: impl Fn(Action) -> M + 'a,
) -> Element<'a, M> {
    let settings = highlight::Settings {
        json: false,
        vars: vars.map(|v| v.keys().cloned().collect()),
    };
    text_editor::TextEditor::new(content)
        .font(theme::mono())
        .padding(5)
        .wrapping(Wrapping::None)
        .height(Length::Shrink)
        .style(theme::editor)
        .on_action(on_action)
        .highlight_with::<highlight::Json>(settings, highlight::format)
        .into()
}
