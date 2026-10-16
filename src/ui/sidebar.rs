//! The collection sidebar: open a folder, browse the request tree, and
//! pick an environment. It only reports actions; `App` performs them.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};

use iced::widget::{column, container, mouse_area, pick_list, row, scrollable, text};
use iced::{Element, Fill, Padding};

use crate::collection::{self, Node};
use crate::ui::theme;

#[derive(Debug, Clone)]
pub enum Action {
    OpenCollection,
    Reload,
    ToggleFolder(PathBuf),
    /// Right-click, or the ⋯ button: the app opens the context menu at the pointer.
    ContextMenu(PathBuf),
    Clone(PathBuf),
    /// The mouse went down on a row. Starts a drag, or a click.
    Press(PathBuf),
    /// The pointer entered a row. During a drag, this is the drop target.
    Enter(PathBuf),
    Exit(PathBuf),
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
    /// The folder a drop would move the dragged item into, for highlighting.
    pub drop: Option<&'a Path>,
    /// The item being dragged. Its row is marked while the drag is on.
    pub dragging: Option<&'a Path>,
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
    let header = row![text(title).size(theme::HEADING)];
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
                let count = collection::request_count(children);
                let label = format!("{arrow} {name} ({count})");
                let highlight = p.drop == Some(path.as_path());
                let dragging = p.dragging == Some(path.as_path());
                out.push(indented(
                    depth,
                    row_area(label, path, highlight, false, dragging),
                ));
                if open {
                    if children.is_empty() {
                        out.push(indented(depth + 1, text("Empty").size(theme::SMALL).into()));
                    }
                    tree_rows(children, depth + 1, p, out);
                }
            }
            Node::Request { name, path } => {
                let selected = p.selected == Some(path.as_path());
                let dragging = p.dragging == Some(path.as_path());
                out.push(indented(
                    depth,
                    row_area(name.clone(), path, false, selected, dragging),
                ));
            }
        }
    }
}

/// One tree row. Press starts a drag; releasing it on the same row is a
/// click, and releasing it on a folder moves the item there (`App` decides
/// which). Right-click and the ⋯ button open the context menu.
fn row_area<'a>(
    label: String,
    path: &Path,
    highlight: bool,
    selected: bool,
    dragging: bool,
) -> Element<'a, Action> {
    let line = container(text(label).size(theme::TEXT))
        .padding([4, 8])
        .width(Fill)
        .style(move |t| row_style(t, highlight, selected, dragging));
    let more = theme::button(text("⋯").size(theme::TEXT))
        .style(theme::text_button)
        .on_press(Action::ContextMenu(path.to_path_buf()));
    let content = row![line, more].spacing(4).align_y(iced::Alignment::Center);
    mouse_area(content)
        .on_press(Action::Press(path.to_path_buf()))
        .on_enter(Action::Enter(path.to_path_buf()))
        .on_exit(Action::Exit(path.to_path_buf()))
        .on_right_press(Action::ContextMenu(path.to_path_buf()))
        .into()
}

fn row_style(
    theme: &iced::Theme,
    highlight: bool,
    selected: bool,
    dragging: bool,
) -> container::Style {
    let palette = theme.extended_palette();
    let background = if highlight {
        Some(palette.primary.base.color.into())
    } else if dragging {
        Some(palette.background.strong.color.into())
    } else if selected {
        Some(palette.primary.weak.color.into())
    } else {
        None
    };
    let mut style = container::Style {
        background,
        ..container::Style::default()
    };
    style.border.radius = theme::RADIUS.into();
    style
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
