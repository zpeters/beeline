//! The collection sidebar: open a folder, browse the request tree, and
//! pick an environment. It only reports actions; `App` performs them.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use iced::widget::{column, container, mouse_area, row, scrollable, space, text};
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
    /// The gear at the bottom of the sidebar: opens the settings dialog.
    Settings,
    /// Adds the httpbin sample folder inside a folder.
    AddSamples(PathBuf),
    /// Opens or closes the collection menu under the collection name.
    ToggleCollectionMenu,
    /// Opens or closes the environment menu under the environment name.
    ToggleEnvMenu,
    /// Moves an environment file to the Trash.
    DeleteEnv(PathBuf),
    /// Opens a collection picked from the recents list.
    SwitchCollection(PathBuf),
    NewCollection,
    RenameCollection(PathBuf),
    DeleteCollection(PathBuf),
}

/// The settings gear, at the bottom-left of the sidebar.
fn gear<'a>() -> Element<'a, Action> {
    row![
        theme::button(text("⚙").size(theme::heading_px() * 1.4))
            .style(theme::text_button)
            .on_press(Action::Settings),
        space::horizontal().width(Fill),
    ]
    .into()
}

/// What the sidebar shows; `root` is `None` when no collection is open.
pub struct Props<'a> {
    pub root: Option<&'a Path>,
    pub tree: &'a [Node],
    pub selected: Option<&'a Path>,
    pub env: Option<&'a str>,
    /// Folders that are open. Folders start closed, as they did in egui.
    pub expanded: &'a BTreeSet<PathBuf>,
    /// The folder a drop would move the dragged item into, for highlighting.
    pub drop: Option<&'a Path>,
    /// The item being dragged. Its row is marked while the drag is on.
    pub dragging: Option<&'a Path>,
}

pub fn view<'a>(p: Props<'a>) -> Element<'a, Action> {
    let title = p
        .root
        .and_then(|r| r.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "beeline".into());
    let name: Element<'a, Action> = match p.root {
        Some(_) => theme::button(text(format!("{title} ▾")).size(theme::heading_px()))
            .style(theme::text_button)
            .on_press(Action::ToggleCollectionMenu)
            .into(),
        None => text(title).size(theme::heading_px()).into(),
    };
    let mut header = column![name].spacing(2);
    if let Some(root) = p.root {
        header = header.push(text(root.display().to_string()).size(theme::small_px()));
    }
    let mut col = column![header].spacing(10);

    let Some(root) = p.root else {
        col = col.push(text(
            "Open a folder to use it as a collection. Each request is a JSON \
             file; subfolders group them.",
        ));
        return col.push(gear()).height(Fill).into();
    };

    let env_name = p.env.unwrap_or("No environment");
    col = col.push(
        row![
            theme::button(text(format!("{env_name} ▾")).size(theme::text_px()))
                .style(theme::text_button)
                .on_press(Action::ToggleEnvMenu),
            theme::button("Edit environment").on_press_maybe(p.env.map(|_| Action::EditEnv)),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center),
    );

    col = col.push(
        row![
            theme::button("+ Request").on_press(Action::NewRequest(root.to_path_buf())),
            theme::button("+ Folder").on_press(Action::NewFolder(root.to_path_buf())),
            theme::button("⟳").on_press(Action::Reload),
        ]
        .spacing(6),
    );

    let mut rows: Vec<Element<'a, Action>> = Vec::new();
    if p.tree.is_empty() {
        rows.push(text("No requests yet.").into());
    }
    tree_rows(p.tree, 0, &p, &mut rows);
    col = col.push(scrollable(column(rows).spacing(2).width(Fill)).height(Fill));

    col.push(gear()).height(Fill).into()
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
                        out.push(indented(
                            depth + 1,
                            text("Empty").size(theme::small_px()).into(),
                        ));
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
    let line = container(text(label).size(theme::text_px()))
        .padding([4, 8])
        .width(Fill)
        .style(move |t| row_style(t, highlight, selected, dragging));
    let more = theme::button(text("⋯").size(theme::text_px()))
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
