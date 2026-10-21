//! The collection sidebar: open a folder, browse the request tree, and
//! pick an environment. It only reports actions; `App` performs them.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use iced::widget::{canvas, column, container, mouse_area, row, scrollable, space, text};
use iced::{Element, Fill, Padding};

use crate::collection::{self, Node};
use crate::format;
use crate::spark::Series;
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
    /// Opens or closes the ⋯ menu beside the collection name: reload,
    /// import, rename and trash.
    ToggleActionsMenu,
    /// Opens or closes the environment menu under the environment name.
    ToggleEnvMenu,
    /// Opens or closes the "+" menu beside REQUESTS.
    ToggleNewMenu,
    /// A double-click on a request: open it and send it.
    Trigger(PathBuf),
    /// Opens the paste dialog for a curl command.
    ImportCurl,
    /// Picks an OpenAPI or Postman file to import.
    ImportFile,
    /// Picks a Bruno collection folder to import.
    ImportBruno,
    /// Saves the open collection as a Postman v2.1 file.
    ExportPostman,
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
    /// The row under the pointer. Only it and the selected row show ⋯.
    pub hovered: Option<&'a Path>,
    /// The selected environment's file, for the − on its label.
    pub env_file: Option<PathBuf>,
    /// Response times per request file, for this session only.
    pub sparks: &'a HashMap<PathBuf, Series>,
    /// When each request's newest response arrived, to animate its point.
    pub arrived: &'a HashMap<PathBuf, Instant>,
    /// Request files with a send in flight. Their end dot pulses.
    pub pending: Vec<&'a Path>,
    /// The time this frame is drawn at, for the animations.
    pub now: Instant,
    /// A fixed start time; the pulse's phase is measured from it.
    pub epoch: Instant,
}

/// How long a new point takes to ease in.
const ARRIVE: f32 = 0.4;
/// Sparkline size, in points: about one word wide and one line high.
const SPARK_W: f32 = 40.0;
const SPARK_H: f32 = 12.0;

pub fn view<'a>(p: Props<'a>) -> Element<'a, Action> {
    let title = p
        .root
        .and_then(|r| r.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "beeline".into());
    // The name always opens the collection menu, so recents, Open and New
    // stay reachable even with no collection open. The ⋯ beside it holds the
    // open collection's actions; on macOS those are in the menu bar instead.
    // A long name is cut short; the ▾ stays visible at the end.
    let name = theme::button(
        row![
            container(
                text(format::ellipsize(&title, 22))
                    .size(theme::heading_px())
                    .wrapping(iced::widget::text::Wrapping::None),
            )
            .width(Fill)
            .clip(true),
            text(" ▾").size(theme::heading_px()),
        ]
        .align_y(iced::Center),
    )
    .style(theme::text_button)
    .width(Fill)
    .on_press(Action::ToggleCollectionMenu);
    let mut header = row![name].align_y(iced::Center);
    if p.root.is_some() && !cfg!(target_os = "macos") {
        header = header.push(
            theme::button(text("⋯").size(theme::heading_px()))
                .style(theme::text_button)
                .on_press(Action::ToggleActionsMenu),
        );
    }
    let mut col = column![header].spacing(6);

    if p.root.is_none() {
        col = col
            .push(text(
                "Open a folder to use it as a collection. Each request is a JSON \
                 file; subfolders group them.",
            ))
            .push(
                row![
                    theme::button("Open collection…").on_press(Action::OpenCollection),
                    theme::button("New collection…").on_press(Action::NewCollection),
                ]
                .spacing(6),
            );
        // The tree's space stays empty, so the gear sits at the bottom as usual.
        return col
            .push(space::vertical().height(Fill))
            .push(gear())
            .height(Fill)
            .into();
    }

    // Layer two: the environment. A tiny label says what it is; the name
    // stays at full strength because it changes every request sent.
    let env_name = format::ellipsize(p.env.unwrap_or("No environment"), 22);
    let mut env_line = row![
        theme::button(
            text(format!("{env_name} ▾"))
                .size(theme::text_px())
                .wrapping(iced::widget::text::Wrapping::None),
        )
        .style(theme::text_button)
        .on_press(Action::ToggleEnvMenu),
    ]
    .spacing(2)
    .align_y(iced::Alignment::Center);
    if p.env.is_some() {
        env_line = env_line.push(text("·").style(theme::muted)).push(
            theme::button(text("Edit").size(theme::small_px()))
                .style(theme::text_button)
                .on_press(Action::EditEnv),
        );
    }
    let env_header = row![
        section_label("ENVIRONMENT"),
        space::horizontal().width(Fill),
        theme::button(text("+").size(theme::text_px()))
            .style(theme::text_button)
            .on_press(Action::NewEnv),
        theme::button(text("−").size(theme::text_px()))
            .style(theme::text_button)
            .on_press_maybe(p.env_file.clone().map(Action::DeleteEnv)),
    ]
    .align_y(iced::Alignment::Center);
    col = col.push(column![env_header, env_line]);

    // Layer three: the requests, labelled like the environment. New items
    // hide behind a "+" so the label leads.
    col = col.push(
        row![
            section_label("REQUESTS"),
            space::horizontal().width(Fill),
            theme::button(text("+").size(theme::text_px()))
                .style(theme::text_button)
                .on_press(Action::ToggleNewMenu),
        ]
        .align_y(iced::Alignment::Center),
    );

    let mut rows: Vec<Element<'a, Action>> = Vec::new();
    if p.tree.is_empty() {
        rows.push(text("No requests yet.").style(theme::muted).into());
    }
    tree_rows(p.tree, 0, &p, &mut rows);
    col = col.push(scrollable(column(rows).spacing(1).width(Fill)).height(Fill));

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
                let label: Element<'a, Action> = row![
                    text(format!("{arrow} {name}")).size(theme::text_px()),
                    text(count.to_string())
                        .size(theme::small_px())
                        .style(theme::muted),
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center)
                .into();
                let state = RowState {
                    highlight: p.drop == Some(path.as_path()),
                    selected: false,
                    dragging: p.dragging == Some(path.as_path()),
                    hovered: p.hovered == Some(path.as_path()),
                };
                out.push(indented(depth, row_area(label, path, state, None)));
                if open {
                    if children.is_empty() {
                        out.push(indented(
                            depth + 1,
                            text("Empty")
                                .size(theme::small_px())
                                .style(theme::muted)
                                .into(),
                        ));
                    }
                    tree_rows(children, depth + 1, p, out);
                }
            }
            Node::Request { name, path, method } => {
                let state = RowState {
                    highlight: false,
                    selected: p.selected == Some(path.as_path()),
                    dragging: p.dragging == Some(path.as_path()),
                    hovered: p.hovered == Some(path.as_path()),
                };
                out.push(indented(depth, request_row(name, method, path, p, state)));
            }
        }
    }
}

/// A request row: method, name, sparkline, and the last time.
fn request_row<'a>(
    name: &str,
    method: &str,
    path: &Path,
    p: &Props<'a>,
    state: RowState,
) -> Element<'a, Action> {
    let series = p.sparks.get(path);
    let pending = p.pending.contains(&path);
    let mut line = row![
        text(method.to_string())
            .font(theme::mono())
            .size(theme::small_px())
            .style(theme::muted)
            // Wide enough for the longest method, OPTIONS.
            .width(62),
        text(name.to_string()).size(theme::text_px()).width(Fill),
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center);
    if series.is_some() || pending {
        let t = p
            .arrived
            .get(path)
            .map_or(1.0, |at| p.now.duration_since(*at).as_secs_f32() / ARRIVE);
        let last = series.and_then(Series::last);
        let spark = Spark {
            points: series.map_or_else(Vec::new, |s| s.points(SPARK_W, SPARK_H, t)),
            ok: last.is_none_or(|l| l.ok),
            // A slow pulse, about once a second, while the send is in flight.
            pulse: pending.then(|| {
                let secs = p.now.duration_since(p.epoch).as_secs_f32();
                0.5 + 0.5 * (secs * std::f32::consts::TAU).sin()
            }),
        };
        line = line.push(canvas(spark).width(SPARK_W + 6.0).height(SPARK_H + 6.0));
        let time = last.map_or_else(String::new, |l| {
            format::duration(Duration::from_secs_f32(l.ms / 1000.0))
        });
        line = line.push(
            text(time)
                .size(theme::small_px())
                .style(theme::muted)
                .width(54)
                .align_x(iced::alignment::Horizontal::Right),
        );
    }
    row_area(
        line.into(),
        path,
        state,
        Some(Action::Trigger(path.to_path_buf())),
    )
}

#[derive(Clone, Copy)]
struct RowState {
    /// A drop would land here.
    highlight: bool,
    selected: bool,
    dragging: bool,
    hovered: bool,
}

/// One tree row. Press starts a drag; releasing it on the same row is a
/// click, and releasing it on a folder moves the item there (`App` decides
/// which). Right-click and the ⋯ button open the context menu. The ⋯ shows
/// only on the hovered or selected row; its space is kept so rows don't shift.
/// `on_double_click` is set for request rows, where a double-click sends.
fn row_area<'a>(
    content: Element<'a, Action>,
    path: &Path,
    state: RowState,
    on_double_click: Option<Action>,
) -> Element<'a, Action> {
    let RowState {
        highlight,
        selected,
        dragging,
        hovered,
    } = state;
    // A thin accent rule marks the selection: the smallest difference that reads.
    let rule = container(space::horizontal())
        .width(2)
        .height(iced::Length::Fixed(theme::text_px() + 8.0))
        .style(move |t: &iced::Theme| container::Style {
            background: selected.then(|| t.palette().primary.into()),
            ..container::Style::default()
        });
    let line = container(content)
        .padding([4, 8])
        .width(Fill)
        .style(move |t| row_style(t, highlight, selected, dragging));
    let more: Element<'a, Action> = if hovered || selected {
        theme::button(text("⋯").size(theme::text_px()))
            .style(theme::text_button)
            .width(28)
            .on_press(Action::ContextMenu(path.to_path_buf()))
            .into()
    } else {
        space::horizontal().width(28).into()
    };
    let content = row![rule, line, more]
        .spacing(2)
        .align_y(iced::Alignment::Center);
    let mut area = mouse_area(content)
        .on_press(Action::Press(path.to_path_buf()))
        .on_enter(Action::Enter(path.to_path_buf()))
        .on_exit(Action::Exit(path.to_path_buf()))
        .on_right_press(Action::ContextMenu(path.to_path_buf()));
    if let Some(action) = on_double_click {
        area = area.on_double_click(action);
    }
    area.into()
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
        // A faint tint: enough to find the row, not enough to shout.
        Some(palette.primary.base.color.scale_alpha(0.12).into())
    } else {
        None
    };
    let mut style = container::Style {
        background,
        ..container::Style::default()
    };
    style.border.radius = 4.0.into();
    style
}

/// A tiny muted capital label for a sidebar section, inset to line up with
/// the text inside the buttons below it.
fn section_label<'a>(label: &'static str) -> Element<'a, Action> {
    container(
        text(label)
            .size(theme::small_px() - 2.0)
            .style(theme::muted),
    )
    .padding(Padding {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 10.0,
    })
    .into()
}

/// Pushes `content` right by one indent step per `depth`.
fn indented<'a>(depth: u16, content: Element<'a, Action>) -> Element<'a, Action> {
    let left = f32::from(depth) * 14.0;
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

/// A word-sized line of response times with a dot on the newest one.
struct Spark {
    points: Vec<(f32, f32)>,
    /// The newest response was a success. Red dot otherwise.
    ok: bool,
    /// While a send is in flight, the dot's strength, 0 to 1.
    pulse: Option<f32>,
}

impl<M> canvas::Program<M> for Spark {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        // Inset so the end dot is not clipped at the edges.
        let pad = 3.0;
        let at = |(x, y): (f32, f32)| iced::Point::new(x + pad, y + pad);
        let palette = theme.palette();
        if let Some((first, rest)) = self.points.split_first() {
            let line = canvas::Path::new(|b| {
                b.move_to(at(*first));
                for point in rest {
                    b.line_to(at(*point));
                }
            });
            frame.stroke(
                &line,
                canvas::Stroke::default()
                    .with_color(palette.text.scale_alpha(0.45))
                    .with_width(1.0),
            );
        }
        let end = self
            .points
            .last()
            .copied()
            .unwrap_or((SPARK_W, SPARK_H / 2.0));
        let base = if self.ok {
            palette.primary
        } else {
            palette.danger
        };
        let color = match self.pulse {
            Some(strength) => base.scale_alpha(0.25 + 0.75 * strength),
            None => base,
        };
        frame.fill(&canvas::Path::circle(at(end), 2.0), color);
        vec![frame.into_geometry()]
    }
}
