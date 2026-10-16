//! The request editor: method, URL, Send, and the Params / Headers / Body /
//! Auth tabs. `update` applies messages to the request; `App` handles the
//! body text itself because it lives in an iced `text_editor::Content`.

use std::collections::BTreeMap;

use iced::widget::{column, pick_list, radio, row, scrollable, text, text_editor};
use iced::{Element, Fill};

use crate::model::{Auth, AuthKind, BodyKind, METHODS, RequestFile};
use crate::ui::{kv, theme};
use crate::{query, vars};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Params,
    Headers,
    Body,
    Auth,
}

/// Which key/value table a message is for.
#[derive(Debug, Clone, Copy)]
pub enum Table {
    Params,
    Headers,
    Form,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Method(&'static str),
    Url(String),
    Send,
    Tab(Tab),
    Kv(Table, kv::Msg),
    BodyKind(BodyKind),
    Body(text_editor::Action),
    FormatJson,
    AuthKind(AuthKind),
    Token(String),
    Username(String),
    Password(String),
}

fn count_label(name: &str, rows: &[crate::model::KeyValue]) -> String {
    match rows.iter().filter(|kv| kv.enabled).count() {
        0 => name.to_string(),
        n => format!("{name} ({n})"),
    }
}

/// Every `{{name}}` the request would send that `vars` lacks.
fn unknown_vars(req: &RequestFile, vars: &BTreeMap<String, String>) -> Vec<String> {
    let mut text = req.url.clone();
    let mut add = |rows: &[crate::model::KeyValue]| {
        for kv in rows.iter().filter(|kv| kv.enabled) {
            text.push_str(&kv.key);
            text.push_str(&kv.value);
        }
    };
    add(&req.headers);
    match req.body.kind {
        BodyKind::None => {}
        BodyKind::Json | BodyKind::Text => text.push_str(&req.body.text),
        BodyKind::Form => add(&req.body.fields),
    }
    match req.auth.kind {
        AuthKind::None => {}
        AuthKind::Bearer => text.push_str(&req.auth.token),
        AuthKind::Basic => {
            text.push_str(&req.auth.username);
            text.push_str(&req.auth.password);
        }
    }
    vars::missing(&text, vars)
}

/// The JSON text pretty-printed, or None when it is not valid JSON.
pub fn pretty(text: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
}

/// Applies a message to the request. Returns true when the user asked to
/// send. `Body` and `FormatJson` are left to `App`, which owns the text.
pub fn update(req: &mut RequestFile, tab: &mut Tab, msg: Msg) -> bool {
    match msg {
        Msg::Method(m) => req.method = m.to_string(),
        Msg::Url(s) => {
            req.url = s;
            req.params = query::params_from_url(&req.url, &req.params);
        }
        Msg::Send => return true,
        Msg::Tab(t) => *tab = t,
        Msg::Kv(Table::Params, m) => {
            kv::update(&mut req.params, m);
            req.url = query::url_from_params(&req.url, &req.params);
        }
        Msg::Kv(Table::Headers, m) => kv::update(&mut req.headers, m),
        Msg::Kv(Table::Form, m) => kv::update(&mut req.body.fields, m),
        Msg::BodyKind(k) => req.body.kind = k,
        Msg::Body(_) | Msg::FormatJson => {}
        Msg::AuthKind(k) => req.auth.kind = k,
        Msg::Token(s) => req.auth.token = s,
        Msg::Username(s) => req.auth.username = s,
        Msg::Password(s) => req.auth.password = s,
    }
    false
}

/// Draws the editor. `vars` are the selected environment's variables,
/// used only for hints. `busy` disables Send.
pub fn view<'a>(
    req: &'a RequestFile,
    tab: Tab,
    busy: bool,
    vars: &BTreeMap<String, String>,
    body: &'a text_editor::Content,
) -> Element<'a, Msg> {
    let method = METHODS.iter().copied().find(|m| *m == req.method);
    let send = theme::button(text(if busy { "…" } else { "Send" }).size(theme::TEXT))
        .on_press_maybe((!busy).then_some(Msg::Send));
    let url = theme::text_input("https://example.com or {{base}}/path", &req.url)
        .font(theme::MONO)
        .on_input(Msg::Url)
        .on_submit(Msg::Send)
        .width(Fill);
    let top = row![
        pick_list(&METHODS[..], method, Msg::Method)
            .style(theme::pick)
            .width(110),
        url,
        send,
    ]
    .spacing(8);

    let mut col = column![top].spacing(8);

    let unknown = unknown_vars(req, vars);
    if !unknown.is_empty() {
        let names: Vec<String> = unknown.iter().map(|n| format!("{{{{{n}}}}}")).collect();
        col = col.push(
            text(format!("Not in the environment: {}", names.join(", "))).style(text::warning),
        );
    }

    let tabs = row![
        tab_button(tab, Tab::Params, count_label("Params", &req.params)),
        tab_button(tab, Tab::Headers, count_label("Headers", &req.headers)),
        tab_button(tab, Tab::Body, body_label(req)),
        tab_button(tab, Tab::Auth, auth_label(req)),
    ]
    .spacing(6);
    col = col.push(tabs);

    let content: Element<'a, Msg> = match tab {
        Tab::Params => scrollable(kv::view(&req.params, true).map(|m| Msg::Kv(Table::Params, m)))
            .height(Fill)
            .into(),
        Tab::Headers => {
            scrollable(kv::view(&req.headers, true).map(|m| Msg::Kv(Table::Headers, m)))
                .height(Fill)
                .into()
        }
        Tab::Body => body_editor(req, body),
        Tab::Auth => scrollable(auth_editor(&req.auth)).height(Fill).into(),
    };
    col.push(content).height(Fill).into()
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

fn body_label(req: &RequestFile) -> String {
    match req.body.kind {
        BodyKind::None => "Body".to_string(),
        kind => format!("Body ({})", kind.label()),
    }
}

fn auth_label(req: &RequestFile) -> String {
    match req.auth.kind {
        AuthKind::None => "Auth".to_string(),
        kind => format!("Auth ({})", kind.label()),
    }
}

fn body_editor<'a>(req: &'a RequestFile, body: &'a text_editor::Content) -> Element<'a, Msg> {
    let kinds = BodyKind::ALL
        .into_iter()
        .fold(row![].spacing(12), |r, kind| {
            r.push(radio(
                kind.label(),
                kind,
                Some(req.body.kind),
                Msg::BodyKind,
            ))
        });
    let mut col = column![kinds].spacing(8);
    match req.body.kind {
        BodyKind::None => {
            col = col.push(text("This request has no body."));
        }
        BodyKind::Json | BodyKind::Text => {
            if req.body.kind == BodyKind::Json {
                let can_format = pretty(&req.body.text).is_some();
                col = col.push(
                    theme::button("Format").on_press_maybe(can_format.then_some(Msg::FormatJson)),
                );
            }
            col = col.push(
                text_editor(body)
                    .style(theme::editor)
                    .font(theme::MONO)
                    .on_action(Msg::Body)
                    .height(Fill),
            );
        }
        BodyKind::Form => {
            col = col.push(
                scrollable(kv::view(&req.body.fields, false).map(|m| Msg::Kv(Table::Form, m)))
                    .height(Fill),
            );
        }
    }
    col.height(Fill).into()
}

fn auth_editor(auth: &Auth) -> Element<'_, Msg> {
    let kinds = AuthKind::ALL
        .into_iter()
        .fold(row![].spacing(12), |r, kind| {
            r.push(radio(kind.label(), kind, Some(auth.kind), Msg::AuthKind))
        });
    let fields: Element<'_, Msg> = match auth.kind {
        AuthKind::None => {
            text("No auth. An Authorization header you set yourself is still sent.").into()
        }
        AuthKind::Bearer => column![
            text("Token"),
            theme::text_input("{{token}}", &auth.token)
                .font(theme::MONO)
                .on_input(Msg::Token),
        ]
        .spacing(4)
        .into(),
        AuthKind::Basic => column![
            text("Username"),
            theme::text_input("", &auth.username).on_input(Msg::Username),
            text("Password"),
            theme::text_input("", &auth.password)
                .secure(true)
                .on_input(Msg::Password),
        ]
        .spacing(4)
        .into(),
    };
    column![kinds, fields].spacing(8).into()
}
