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
///
/// The active tab is set apart by the smallest differences that still read:
/// a faint fill, full-strength text and an accent underline. Inactive tabs
/// are muted, with hairlines between them so each reads as its own tab.
pub fn view(items: Vec<Item>, active: Option<u64>) -> Element<'static, Msg> {
    let mut tabs = row![].align_y(Center);
    for (i, item) in items.into_iter().enumerate() {
        let selected = active == Some(item.id);
        if i > 0 {
            tabs = tabs.push(hairline());
        }
        let label = if item.dirty {
            format!("{} •", item.label)
        } else {
            item.label
        };
        let mut name = text(label).size(theme::text_px());
        if !selected {
            name = name.style(theme::muted);
        }
        let name = button(name)
            .style(theme::text_button)
            .padding([4, 2])
            .on_press(Msg::Activate(item.id));
        let close = button(text("×").size(theme::small_px()).style(theme::muted))
            .style(theme::text_button)
            .padding([4, 4])
            .on_press(Msg::Close(item.id));
        let face = container(row![name, close].spacing(2).align_y(Center))
            .padding([2, 8])
            .style(move |t: &Theme| container::Style {
                background: selected.then(|| t.extended_palette().background.weak.color.into()),
                border: iced::Border {
                    radius: iced::border::top(6.0),
                    ..iced::Border::default()
                },
                ..container::Style::default()
            });
        let bar = container(space::horizontal())
            .height(2)
            .width(Fill)
            .style(move |t: &Theme| container::Style {
                background: selected.then(|| t.extended_palette().primary.base.color.into()),
                ..container::Style::default()
            });
        tabs = tabs.push(column![face, bar].width(iced::Shrink));
    }
    let new = button(text("+").size(theme::text_px()).style(theme::muted))
        .style(theme::text_button)
        .on_press(Msg::New);
    scrollable(row![tabs, new].spacing(8).align_y(Center))
        // A thin rail below the tabs, only as heavy as it needs to be to find.
        .direction(scrollable::Direction::Horizontal(
            scrollable::Scrollbar::new()
                .width(3)
                .scroller_width(3)
                .margin(0)
                .spacing(4),
        ))
        .into()
}

/// A faint vertical rule between two inactive-looking tabs.
fn hairline() -> Element<'static, Msg> {
    container(space::horizontal())
        .width(1)
        .height(iced::Length::Fixed(theme::text_px()))
        .style(|t: &Theme| container::Style {
            background: Some(t.palette().text.scale_alpha(0.15).into()),
            ..container::Style::default()
        })
        .into()
}
