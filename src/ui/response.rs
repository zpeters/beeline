//! The response viewer: status line, Body, Headers and History tabs.

use iced::widget::{button, column, row, rule, scrollable, space, text, text_editor};
use iced::{Color, Element, Fill, Theme};

use crate::format;
use crate::history::History;
use crate::http;
use crate::ui::{highlight, theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Body,
    Headers,
    History,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Tab(Tab),
    /// Selection and movement only; edits are ignored, the body is read-only.
    Edit(text_editor::Action),
    Copy,
    /// Show this earlier send from the history. Left to `App`, which owns
    /// the history.
    Show(u64),
}

/// One send, as the history keeps it: what went on the wire and what
/// came back.
#[derive(Debug, Clone)]
pub struct Sent {
    pub method: String,
    pub url: String,
    pub result: Result<http::Response, String>,
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
        Msg::Copy | Msg::Show(_) => {}
    }
}

fn status_color(status: u16) -> Color {
    match status {
        200..=299 => Color::from_rgb8(70, 170, 90),
        300..=399 => Color::from_rgb8(200, 150, 40),
        _ => Color::from_rgb8(220, 80, 80),
    }
}

/// `shown` is the history number of the send on screen; an earlier one
/// than the newest gets a note in the status line.
pub fn view<'a>(
    view: &'a View,
    history: &'a History<Sent>,
    shown: Option<u64>,
) -> Element<'a, Msg> {
    let resp = &view.resp;
    let mut info = row![
        text(format!("{} {}", resp.status, resp.reason))
            .size(theme::text_px())
            .color(status_color(resp.status)),
        text(format::duration(resp.elapsed)).size(theme::small_px()),
        text(format::size(resp.body.len())).size(theme::small_px()),
    ]
    .spacing(12);
    if view.is_json {
        info = info.push(text("JSON").size(theme::small_px()));
    }
    if let Some(n) = shown
        && history.latest() != Some(n)
    {
        info = info.push(
            text(format!("Send #{n}, not the newest"))
                .size(theme::small_px())
                .style(text::warning),
        );
    }
    info = info
        .push(space::horizontal().width(Fill))
        .push(theme::button(text("Copy body").size(theme::small_px())).on_press(Msg::Copy));

    let tabs = row![
        tab_button(view.tab, Tab::Body, "Body".to_string()),
        tab_button(
            view.tab,
            Tab::Headers,
            format!("Headers ({})", resp.headers.len())
        ),
        tab_button(
            view.tab,
            Tab::History,
            format!("History ({})", history.len())
        ),
    ]
    .spacing(16);

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
                    .font(theme::mono())
                    .on_action(Msg::Edit)
                    .height(Fill)
                    .highlight_with::<highlight::Json>(
                        highlight::Settings {
                            json: view.is_json,
                            vars: None,
                        },
                        highlight::format,
                    ),
            )
            .height(Fill)
            .into()
        }
        Tab::Headers => {
            let mut rows = column![].spacing(6);
            for (k, v) in &resp.headers {
                rows = rows.push(
                    row![
                        text(k.as_str()).size(theme::text_px()).width(220),
                        text(v.as_str()).size(theme::text_px()).width(Fill),
                    ]
                    .spacing(12),
                );
            }
            scrollable(rows).height(Fill).into()
        }
        Tab::History => history_list(history, shown),
    };

    column![info, column![tabs, rule::horizontal(1)], body]
        .spacing(8)
        .height(Fill)
        .into()
}

fn tab_button<'a>(current: Tab, this: Tab, label: String) -> Element<'a, Msg> {
    theme::tab(current, this, label, Msg::Tab(this))
}

/// This session's sends of the request, newest first. A click on one
/// shows it.
pub fn history_list(history: &History<Sent>, shown: Option<u64>) -> Element<'_, Msg> {
    let mut rows = column![].spacing(2);
    for (n, sent) in history.newest_first() {
        let (status, color, detail) = match &sent.result {
            Ok(resp) => (
                format!("{} {}", resp.status, resp.reason),
                Some(status_color(resp.status)),
                format!(
                    "{} · {}",
                    format::duration(resp.elapsed),
                    format::size(resp.body.len())
                ),
            ),
            Err(e) => ("Error".to_string(), None, e.clone()),
        };
        let mut status = text(status).size(theme::small_px()).width(120);
        status = match color {
            Some(c) => status.color(c),
            None => status.style(text::danger),
        };
        let line = row![
            text(format!("#{n}")).size(theme::small_px()).width(40),
            status,
            text(detail).size(theme::small_px()).width(160),
            text(format!("{} {}", sent.method, sent.url))
                .size(theme::small_px())
                .font(theme::mono())
                .wrapping(text::Wrapping::None)
                .width(Fill),
        ]
        .spacing(12);
        let selected = shown == Some(n);
        rows = rows.push(
            button(line)
                .style(move |theme: &Theme, status| {
                    let mut s = theme::text_button(theme, status);
                    if selected {
                        s.background =
                            Some(theme.extended_palette().background.strong.color.into());
                    }
                    s
                })
                .width(Fill)
                .on_press(Msg::Show(n)),
        );
    }
    scrollable(rows).height(Fill).into()
}
