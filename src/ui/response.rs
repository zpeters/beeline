//! The response viewer: status line, Body and Headers tabs.

use iced::widget::{column, row, scrollable, space, text, text_editor};
use iced::{Color, Element, Fill};

use crate::format;
use crate::http;
use crate::ui::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Body,
    Headers,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Tab(Tab),
    /// Selection and movement only; edits are ignored, the body is read-only.
    Edit(text_editor::Action),
    Copy,
}

/// A response plus its display text, computed once when it arrives
/// rather than every frame.
pub struct View {
    resp: http::Response,
    /// The full display text, for Copy.
    text: String,
    /// What the text view shows; capped so huge bodies stay responsive.
    content: text_editor::Content,
    truncated: bool,
    is_json: bool,
    tab: Tab,
}

/// Bodies bigger than this are cut in the view (Copy still copies all).
const MAX_SHOWN: usize = 2 * 1024 * 1024;

impl View {
    pub fn new(resp: http::Response) -> Self {
        let (text, is_json) = format::body_text(&resp.body);
        let mut shown = text.clone();
        let truncated = format::truncate(&mut shown, MAX_SHOWN);
        Self {
            resp,
            content: text_editor::Content::with_text(&shown),
            text,
            truncated,
            is_json,
            tab: Tab::default(),
        }
    }

    /// The whole body as text, for Copy.
    pub fn text(&self) -> &str {
        &self.text
    }
}

pub fn update(view: &mut View, msg: Msg) {
    match msg {
        Msg::Tab(t) => view.tab = t,
        Msg::Edit(action) => {
            if !action.is_edit() {
                view.content.perform(action);
            }
        }
        Msg::Copy => {}
    }
}

fn status_color(status: u16) -> Color {
    match status {
        200..=299 => Color::from_rgb8(70, 170, 90),
        300..=399 => Color::from_rgb8(200, 150, 40),
        _ => Color::from_rgb8(220, 80, 80),
    }
}

pub fn view(view: &View) -> Element<'_, Msg> {
    let resp = &view.resp;
    let mut info = row![
        text(format!("{} {}", resp.status, resp.reason))
            .size(theme::TEXT)
            .color(status_color(resp.status)),
        text(format::duration(resp.elapsed)).size(theme::SMALL),
        text(format::size(resp.body.len())).size(theme::SMALL),
    ]
    .spacing(12);
    if view.is_json {
        info = info.push(text("JSON").size(theme::SMALL));
    }
    info = info
        .push(space::horizontal().width(Fill))
        .push(theme::button(text("Copy body").size(theme::SMALL)).on_press(Msg::Copy));

    let tabs = row![
        tab_button(view.tab, Tab::Body, "Body".to_string()),
        tab_button(
            view.tab,
            Tab::Headers,
            format!("Headers ({})", resp.headers.len())
        ),
    ]
    .spacing(6);

    let body: Element<'_, Msg> = match view.tab {
        Tab::Body => {
            let mut col = column![].spacing(6);
            if view.truncated {
                col = col.push(
                    text(format!(
                        "Showing the first {}. Copy body copies all of it.",
                        format::size(MAX_SHOWN)
                    ))
                    .style(text::warning),
                );
            }
            col.push(
                text_editor(&view.content)
                    .style(theme::editor)
                    .font(theme::MONO)
                    .on_action(Msg::Edit)
                    .height(Fill),
            )
            .height(Fill)
            .into()
        }
        Tab::Headers => {
            let mut rows = column![].spacing(6);
            for (k, v) in &resp.headers {
                rows = rows.push(
                    row![
                        text(k.as_str()).size(theme::TEXT).width(220),
                        text(v.as_str()).size(theme::TEXT).width(Fill),
                    ]
                    .spacing(12),
                );
            }
            scrollable(rows).height(Fill).into()
        }
    };

    column![info, tabs, body].spacing(8).height(Fill).into()
}

fn tab_button<'a>(current: Tab, this: Tab, label: String) -> Element<'a, Msg> {
    let style: fn(&iced::Theme, iced::widget::button::Status) -> iced::widget::button::Style =
        if current == this {
            theme::primary
        } else {
            theme::secondary
        };
    theme::button(text(label).size(theme::TEXT))
        .style(style)
        .on_press(Msg::Tab(this))
        .into()
}
