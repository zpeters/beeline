//! The strip of open request tabs above the editor. Each tab shows its
//! name, a dot when it has unsaved changes, and a close button. Pure
//! widgets: they report what was pressed and do nothing else.

use iced::widget::{button, column, container, row, scrollable, space, text};
use iced::{Center, Element, Fill, Theme};

use crate::ui::theme;

#[derive(Debug, Clone)]
pub enum Msg {
    Activate(u64),
    Close(u64),
    New,
}

/// What the strip needs to know about one open request.
pub struct Item {
    pub id: u64,
    pub label: String,
    pub dirty: bool,
}

/// One row of tabs, scrolling sideways when there are too many, with a
/// "+" button at the end for a new untitled request.
pub fn view(items: Vec<Item>, active: Option<u64>) -> Element<'static, Msg> {
    let mut tabs = row![].spacing(16).align_y(Center);
    for item in items {
        let selected = active == Some(item.id);
        let label = if item.dirty {
            format!("{} •", item.label)
        } else {
            item.label
        };
        let name = button(text(label).size(theme::TEXT))
            .style(theme::text_button)
            .on_press(Msg::Activate(item.id));
        let close = button(text("×").size(theme::TEXT))
            .style(theme::text_button)
            .on_press(Msg::Close(item.id));
        let bar =
            container(space::horizontal())
                .height(3)
                .width(Fill)
                .style(move |theme: &Theme| container::Style {
                    background: selected
                        .then(|| theme.extended_palette().primary.base.color.into()),
                    ..container::Style::default()
                });
        tabs = tabs
            .push(column![row![name, close].spacing(4).align_y(Center), bar].width(iced::Shrink));
    }
    let new = button(text("+").size(theme::TEXT))
        .style(theme::text_button)
        .on_press(Msg::New);
    scrollable(row![tabs, new].spacing(16).align_y(Center))
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::default(),
        ))
        .into()
}
