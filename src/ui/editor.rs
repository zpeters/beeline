//! The request editor: method, URL, Send, and the Params / Headers / Body /
//! Auth / Vars tabs. `update` applies messages to the request; `App` handles the
//! body text itself because it lives in an iced `text_editor::Content`.

use std::collections::BTreeMap;

use iced::widget::text_editor::{Action, Content, Edit};
use iced::widget::{column, pick_list, radio, row, rule, scrollable, text, text_editor};
use iced::{Element, Fill};

use crate::model::{Auth, AuthKind, BodyKind, METHODS, RequestFile};
use crate::ui::{field, highlight, kv, theme};
use crate::{query, vars};

/// The editors for the request's single-line fields. Each one shows the
/// request's text. `sync` brings them back in line after any change.
#[derive(Default)]
pub struct Fields {
    pub url: Content,
    pub token: Content,
    pub username: Content,
    pub password: Content,
    pub params: Vec<kv::Row>,
    pub headers: Vec<kv::Row>,
    pub form: Vec<kv::Row>,
    pub vars: Vec<kv::Row>,
}

impl Fields {
    pub fn new(req: &RequestFile) -> Self {
        let mut fields = Self::default();
        fields.sync(req);
        fields
    }

    pub fn sync(&mut self, req: &RequestFile) {
        field::sync(&mut self.url, &req.url);
        field::sync(&mut self.token, &req.auth.token);
        field::sync(&mut self.username, &req.auth.username);
        field::sync(&mut self.password, &req.auth.password);
        kv::sync_rows(&req.params, &mut self.params);
        kv::sync_rows(&req.headers, &mut self.headers);
        kv::sync_rows(&req.body.fields, &mut self.form);
        kv::sync_rows(&req.vars, &mut self.vars);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Params,
    Headers,
    Body,
    Auth,
    Vars,
}

/// Which key/value table a message is for.
#[derive(Debug, Clone, Copy)]
pub enum Table {
    Params,
    Headers,
    Form,
    Vars,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Method(&'static str),
    UrlEdit(Action),
    Send,
    /// Cancels the request in flight. `App` handles it.
    Cancel,
    Tab(Tab),
    Kv(Table, kv::Msg),
    BodyKind(BodyKind),
    Body(text_editor::Action),
    FormatJson,
    AuthKind(AuthKind),
    TokenEdit(Action),
    UsernameEdit(Action),
    PasswordEdit(Action),
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
pub fn update(req: &mut RequestFile, fields: &mut Fields, tab: &mut Tab, msg: Msg) -> bool {
    match msg {
        Msg::Method(m) => req.method = m.to_string(),
        Msg::UrlEdit(action) => {
            field::apply(&mut fields.url, action);
            req.url = fields.url.text();
            req.params = query::params_from_url(&req.url, &req.params);
        }
        Msg::Send => return true,
        Msg::Tab(t) => *tab = t,
        Msg::Kv(Table::Params, m) => {
            kv::update(&mut req.params, &mut fields.params, m);
            req.url = query::url_from_params(&req.url, &req.params);
        }
        Msg::Kv(Table::Headers, m) => kv::update(&mut req.headers, &mut fields.headers, m),
        Msg::Kv(Table::Form, m) => kv::update(&mut req.body.fields, &mut fields.form, m),
        Msg::Kv(Table::Vars, m) => kv::update(&mut req.vars, &mut fields.vars, m),
        Msg::BodyKind(k) => req.body.kind = k,
        Msg::Body(_) | Msg::FormatJson | Msg::Cancel => {}
        Msg::AuthKind(k) => req.auth.kind = k,
        Msg::TokenEdit(action) => {
            field::apply(&mut fields.token, action);
            req.auth.token = fields.token.text();
        }
        Msg::UsernameEdit(action) => {
            field::apply(&mut fields.username, action);
            req.auth.username = fields.username.text();
        }
        Msg::PasswordEdit(action) => {
            field::apply(&mut fields.password, action);
            req.auth.password = fields.password.text();
        }
    }
    fields.sync(req);
    false
}

/// Draws the editor. `vars` are the variables the request sends with
/// (see `resolve::effective`), used only for hints. `busy` disables Send.
pub fn view<'a>(
    req: &'a RequestFile,
    tab: Tab,
    busy: bool,
    vars: &BTreeMap<String, String>,
    body: &'a text_editor::Content,
    fields: &'a Fields,
) -> Element<'a, Msg> {
    let method = METHODS.iter().copied().find(|m| *m == req.method);
    // While a request is in flight, the same button cancels it.
    let send = if busy {
        theme::button(text("Cancel").size(theme::text_px())).on_press(Msg::Cancel)
    } else {
        theme::button(text("Send").size(theme::text_px())).on_press(Msg::Send)
    };
    let url = field::view(&fields.url, Some(vars), |action| {
        if matches!(action, Action::Edit(Edit::Enter)) {
            Msg::Send
        } else {
            Msg::UrlEdit(action)
        }
    });
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
        col = col.push(text(format!("Not defined: {}", names.join(", "))).style(text::warning));
    }

    let tabs = row![
        tab_button(tab, Tab::Params, count_label("Params", &req.params)),
        tab_button(tab, Tab::Headers, count_label("Headers", &req.headers)),
        tab_button(tab, Tab::Body, body_label(req)),
        tab_button(tab, Tab::Auth, auth_label(req)),
        tab_button(tab, Tab::Vars, count_label("Vars", &req.vars)),
    ]
    .spacing(16);
    col = col.push(tabs).push(rule::horizontal(1));

    let content: Element<'a, Msg> = match tab {
        Tab::Params => scrollable(
            kv::view(&req.params, &fields.params, true, Some(vars))
                .map(|m| Msg::Kv(Table::Params, m)),
        )
        .height(Fill)
        .into(),
        Tab::Headers => scrollable(
            kv::view(&req.headers, &fields.headers, true, Some(vars))
                .map(|m| Msg::Kv(Table::Headers, m)),
        )
        .height(Fill)
        .into(),
        Tab::Body => body_editor(req, fields, vars, body),
        Tab::Auth => scrollable(auth_editor(&req.auth, fields, vars))
            .height(Fill)
            .into(),
        Tab::Vars => column![
            text(
                "Overrides the environment's variable of the same name. Unchecked rows are ignored."
            ),
            scrollable(
                kv::view(&req.vars, &fields.vars, true, Some(vars))
                    .map(|m| Msg::Kv(Table::Vars, m)),
            )
            .height(Fill),
        ]
        .spacing(8)
        .height(Fill)
        .into(),
    };
    col.push(content).height(Fill).into()
}

fn tab_button<'a>(current: Tab, this: Tab, label: String) -> Element<'a, Msg> {
    theme::tab(current, this, label, Msg::Tab(this))
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

fn body_editor<'a>(
    req: &'a RequestFile,
    fields: &'a Fields,
    vars: &BTreeMap<String, String>,
    body: &'a text_editor::Content,
) -> Element<'a, Msg> {
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
                    .font(theme::mono())
                    .on_action(Msg::Body)
                    .height(Fill)
                    .highlight_with::<highlight::Json>(
                        highlight::Settings {
                            json: req.body.kind == BodyKind::Json,
                            vars: Some(vars.keys().cloned().collect()),
                        },
                        highlight::format,
                    ),
            );
        }
        BodyKind::Form => {
            col = col.push(
                scrollable(
                    kv::view(&req.body.fields, &fields.form, false, Some(vars))
                        .map(|m| Msg::Kv(Table::Form, m)),
                )
                .height(Fill),
            );
        }
    }
    col.height(Fill).into()
}

fn auth_editor<'a>(
    auth: &'a Auth,
    fields: &'a Fields,
    vars: &BTreeMap<String, String>,
) -> Element<'a, Msg> {
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
            field::view(&fields.token, Some(vars), Msg::TokenEdit),
        ]
        .spacing(4)
        .into(),
        AuthKind::Basic => column![
            text("Username"),
            field::view(&fields.username, Some(vars), Msg::UsernameEdit),
            text("Password"),
            field::view(&fields.password, Some(vars), Msg::PasswordEdit),
        ]
        .spacing(4)
        .into(),
    };
    column![kinds, fields].spacing(8).into()
}
