//! The collection sidebar: open a folder, browse the request tree, and
//! pick an environment. It only reports actions; `App` performs them.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

use iced::widget::{column, pick_list, row, scrollable, space, text};
use iced::{Element, Fill, Padding};

use crate::collection::Node;
use crate::ui::theme;

#[derive(Debug, Clone)]
pub enum Action {
    OpenCollection,
    Reload,
    ToggleFolder(PathBuf),
    /// Shows or hides the row of actions under a folder or request.
    ToggleMenu(PathBuf),
    Open(PathBuf),
    NewRequest(PathBuf),
    NewFolder(PathBuf),
    Rename(PathBuf),
    Delete(PathBuf),
    SelectEnv(Option<String>),
    NewEnv,
    EditEnv,
}

/// What the sidebar shows; `root` is `None` when no collection is open.
pub struct Props<'a> {
    pub root: Option<&'a Path>,
    pub tree: &'a [Node],
    pub selected: Option<&'a Path>,
    pub envs: &'a [(String, PathBuf)],
    pub env: Option<&'a str>,
    /// Folders that are open. Folders start closed, as they did in egui.
    pub expanded: &'a BTreeSet<PathBuf>,
    /// The row whose action menu is open, if any.
    pub menu: Option<&'a Path>,
}

#[derive(Debug, Clone, PartialEq)]
enum EnvChoice {
    None,
    Named(String),
}

impl fmt::Display for EnvChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EnvChoice::None => write!(f, "No environment"),
            EnvChoice::Named(name) => write!(f, "{name}"),
        }
    }
}

pub fn view<'a>(p: Props<'a>) -> Element<'a, Action> {
    let title = p
        .root
        .and_then(|r| r.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "beeline".into());
    let header = row![
        text(title).size(theme::HEADING),
        space::horizontal().width(Fill),
        theme::button(text("Open…").size(theme::TEXT)).on_press(Action::OpenCollection),
    ]
    .spacing(8);
    let mut col = column![header].spacing(10);

    let Some(root) = p.root else {
        col = col.push(text(
            "Open a folder to use it as a collection. Each request is a JSON \
             file; subfolders group them.",
        ));
        return col.into();
    };

    col = col.push(
        row![
            theme::button("+ Request").on_press(Action::NewRequest(root.to_path_buf())),
            theme::button("+ Folder").on_press(Action::NewFolder(root.to_path_buf())),
            theme::button("⟳").on_press(Action::Reload),
        ]
        .spacing(6),
    );

    let mut choices = vec![EnvChoice::None];
    choices.extend(
        p.envs
            .iter()
            .map(|(name, _)| EnvChoice::Named(name.clone())),
    );
    let selected = Some(match p.env {
        None => EnvChoice::None,
        Some(name) => EnvChoice::Named(name.to_string()),
    });
    col = col.push(
        row![
            text("Env").size(theme::TEXT),
            pick_list(choices, selected, |choice| {
                Action::SelectEnv(match choice {
                    EnvChoice::None => None,
                    EnvChoice::Named(name) => Some(name),
                })
            })
            .style(theme::pick)
            .width(Fill),
        ]
        .spacing(8),
    );
    col = col.push(
        row![
            theme::button("New env").on_press(Action::NewEnv),
            theme::button("Edit env").on_press_maybe(p.env.map(|_| Action::EditEnv)),
        ]
        .spacing(6),
    );

    let mut rows: Vec<Element<'a, Action>> = Vec::new();
    if p.tree.is_empty() {
        rows.push(text("No requests yet.").into());
    }
    tree_rows(p.tree, 0, &p, &mut rows);
    col = col.push(scrollable(column(rows).spacing(2).width(Fill)).height(Fill));

    col.height(Fill).into()
}

fn tree_rows<'a>(nodes: &'a [Node], depth: u16, p: &Props<'a>, out: &mut Vec<Element<'a, Action>>) {
    for node in nodes {
        match node {
            Node::Folder {
                name,
                path,
                children,
            } => {
                let open = p.expanded.contains(path);
                let arrow = if open { "▾" } else { "▸" };
                out.push(indented(
                    depth,
                    row![
                        theme::button(text(format!("{arrow} {name}")).size(theme::TEXT))
                            .style(theme::text_button)
                            .on_press(Action::ToggleFolder(path.clone()))
                            .width(Fill),
                        menu_toggle(path),
                    ]
                    .spacing(4)
                    .into(),
                ));
                if p.menu == Some(path.as_path()) {
                    out.push(indented(
                        depth + 1,
                        action_row(&[
                            ("New request", Action::NewRequest(path.clone())),
                            ("New folder", Action::NewFolder(path.clone())),
                            ("Rename…", Action::Rename(path.clone())),
                            ("Move to Trash…", Action::Delete(path.clone())),
                        ]),
                    ));
                }
                if open {
                    if children.is_empty() {
                        out.push(indented(depth + 1, text("Empty").size(theme::SMALL).into()));
                    }
                    tree_rows(children, depth + 1, p, out);
                }
            }
            Node::Request { name, path } => {
                let selected = p.selected == Some(path.as_path());
                let style: fn(
                    &iced::Theme,
                    iced::widget::button::Status,
                ) -> iced::widget::button::Style = if selected {
                    theme::primary
                } else {
                    theme::text_button
                };
                out.push(indented(
                    depth,
                    row![
                        theme::button(text(name.as_str()).size(theme::TEXT))
                            .style(style)
                            .on_press(Action::Open(path.clone()))
                            .width(Fill),
                        menu_toggle(path),
                    ]
                    .spacing(4)
                    .into(),
                ));
                if p.menu == Some(path.as_path()) {
                    out.push(indented(
                        depth + 1,
                        action_row(&[
                            ("Rename…", Action::Rename(path.clone())),
                            ("Move to Trash…", Action::Delete(path.clone())),
                        ]),
                    ));
                }
            }
        }
    }
}

fn menu_toggle<'a>(path: &Path) -> Element<'a, Action> {
    theme::button(text("⋯").size(theme::TEXT))
        .style(theme::text_button)
        .on_press(Action::ToggleMenu(path.to_path_buf()))
        .into()
}

fn action_row<'a>(items: &[(&'static str, Action)]) -> Element<'a, Action> {
    let mut r = row![].spacing(6);
    for (label, action) in items {
        r = r.push(
            theme::button(text(*label).size(theme::SMALL))
                .style(theme::secondary)
                .on_press(action.clone()),
        );
    }
    r.into()
}

/// Pushes `content` right by one indent step per `depth`.
fn indented<'a>(depth: u16, content: Element<'a, Action>) -> Element<'a, Action> {
    let left = f32::from(depth) * 18.0;
    iced::widget::container(content)
        .padding(Padding {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left,
        })
        .width(Fill)
        .into()
}
