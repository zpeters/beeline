//! Top-level app state, file actions, and frame layout.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use eframe::egui;

use crate::collection::{self, Node};
use crate::model::RequestFile;
use crate::ui::{editor, response, sidebar};
use crate::{http, resolve};

const KEY_COLLECTION: &str = "collection";
const NOTICE_TIME: Duration = Duration::from_secs(4);

struct Collection {
    root: PathBuf,
    tree: Vec<Node>,
}

struct Notice {
    text: String,
    error: bool,
    at: Instant,
}

/// What to do once unsaved changes are dealt with.
enum Then {
    Open(PathBuf),
    OpenCollection(PathBuf),
    Quit,
}

#[derive(Clone)]
enum NameFor {
    NewRequest(PathBuf),
    NewFolder(PathBuf),
    Rename(PathBuf),
    SaveAs(PathBuf),
}

enum Modal {
    Unsaved(Then),
    Name {
        purpose: NameFor,
        text: String,
        error: Option<String>,
        focus: bool,
    },
    Delete(PathBuf),
}

/// What the user chose in a modal; applied after it is drawn.
enum Choice {
    Keep(Modal),
    Cancel,
    Save(Then),
    Discard(Then),
    Name(NameFor, String),
    Delete(PathBuf),
}

pub struct App {
    collection: Option<Collection>,
    req: RequestFile,
    /// The file `req` was loaded from; `None` for the scratch request.
    open: Option<PathBuf>,
    /// The last loaded or saved copy, for the unsaved-changes check.
    saved: RequestFile,
    tab: editor::Tab,
    /// Set while a request is in flight; the worker sends its result here.
    pending: Option<Receiver<Result<http::Response, String>>>,
    result: Option<Result<response::View, String>>,
    modal: Option<Modal>,
    notice: Option<Notice>,
    quitting: bool,
}

impl App {
    /// Opens `start` if given, otherwise the last collection used.
    pub fn new(cc: &eframe::CreationContext<'_>, start: Option<PathBuf>) -> Self {
        let mut app = Self {
            collection: None,
            req: RequestFile::default(),
            open: None,
            saved: RequestFile::default(),
            tab: editor::Tab::default(),
            pending: None,
            result: None,
            modal: None,
            notice: None,
            quitting: false,
        };
        if let Some(start) = start {
            match start.canonicalize() {
                Ok(root) if root.is_dir() => app.open_collection(root),
                _ => app.notify(format!("Not a folder: {}", start.display()), true),
            }
        } else if let Some(root) = cc.storage.and_then(|s| s.get_string(KEY_COLLECTION))
            && Path::new(&root).is_dir()
        {
            app.open_collection(PathBuf::from(root));
        }
        app
    }

    fn notify(&mut self, text: impl Into<String>, error: bool) {
        self.notice = Some(Notice {
            text: text.into(),
            error,
            at: Instant::now(),
        });
    }

    fn dirty(&self) -> bool {
        self.req != self.saved
    }

    // --- collection and file actions ---

    fn open_collection(&mut self, root: PathBuf) {
        match collection::scan(&root) {
            Ok(tree) => {
                self.collection = Some(Collection { root, tree });
                self.open = None;
                self.req = RequestFile::default();
                self.saved = RequestFile::default();
                self.result = None;
            }
            Err(e) => self.notify(e, true),
        }
    }

    fn reload_tree(&mut self) {
        let Some(c) = &mut self.collection else {
            return;
        };
        match collection::scan(&c.root) {
            Ok(tree) => c.tree = tree,
            Err(e) => self.notify(e, true),
        }
    }

    fn load(&mut self, path: PathBuf) {
        match collection::read_request(&path) {
            Ok(req) => {
                self.saved = req.clone();
                self.req = req;
                self.open = Some(path);
                self.result = None;
            }
            Err(e) => self.notify(e, true),
        }
    }

    /// Save the open request. A scratch request asks for a name instead.
    /// Returns true when the request is now saved.
    fn save(&mut self) -> bool {
        if let Some(path) = &self.open {
            match collection::write_request(path, &self.req) {
                Ok(()) => {
                    self.saved = self.req.clone();
                    self.notify("Saved", false);
                    true
                }
                Err(e) => {
                    self.notify(e, true);
                    false
                }
            }
        } else if let Some(c) = &self.collection {
            self.modal = Some(name_modal(NameFor::SaveAs(c.root.clone()), ""));
            false
        } else {
            self.notify("Open a collection folder to save requests", true);
            false
        }
    }

    /// Run `then` now, or ask about unsaved changes first.
    fn guard(&mut self, then: Then, ctx: &egui::Context) {
        if self.dirty() {
            self.modal = Some(Modal::Unsaved(then));
        } else {
            self.run(then, ctx);
        }
    }

    fn run(&mut self, then: Then, ctx: &egui::Context) {
        match then {
            Then::Open(path) => self.load(path),
            Then::OpenCollection(root) => self.open_collection(root),
            Then::Quit => {
                self.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    fn apply_name(
        &mut self,
        purpose: &NameFor,
        text: &str,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        match purpose {
            NameFor::NewRequest(dir) => {
                let path = collection::create_request(dir, text, &RequestFile::default())?;
                self.reload_tree();
                self.guard(Then::Open(path), ctx);
            }
            NameFor::NewFolder(dir) => {
                collection::create_folder(dir, text)?;
                self.reload_tree();
            }
            NameFor::Rename(path) => {
                let new = collection::rename(path, text)?;
                if let Some(open) = &self.open
                    && let Some(moved) = collection::moved(open, path, &new)
                {
                    self.open = Some(moved);
                }
                self.reload_tree();
            }
            NameFor::SaveAs(dir) => {
                let path = collection::create_request(dir, text, &self.req)?;
                self.open = Some(path);
                self.saved = self.req.clone();
                self.reload_tree();
                self.notify("Saved", false);
            }
        }
        Ok(())
    }

    fn delete(&mut self, path: &Path) {
        if let Err(e) = collection::delete(path) {
            self.notify(e, true);
            return;
        }
        // Keep the editor's content as an unsaved scratch request.
        if self.open.as_deref().is_some_and(|o| o.starts_with(path)) {
            self.open = None;
            self.saved = RequestFile::default();
        }
        self.reload_tree();
        self.notify("Moved to Trash", false);
    }

    fn handle_sidebar(&mut self, action: sidebar::Action, ctx: &egui::Context) {
        use sidebar::Action;
        match action {
            Action::OpenCollection => {
                if let Some(root) = rfd::FileDialog::new()
                    .set_title("Open collection folder")
                    .pick_folder()
                {
                    self.guard(Then::OpenCollection(root), ctx);
                }
            }
            Action::Reload => self.reload_tree(),
            Action::Open(path) => {
                if self.open.as_ref() != Some(&path) {
                    self.guard(Then::Open(path), ctx);
                }
            }
            Action::NewRequest(dir) => self.modal = Some(name_modal(NameFor::NewRequest(dir), "")),
            Action::NewFolder(dir) => self.modal = Some(name_modal(NameFor::NewFolder(dir), "")),
            Action::Rename(path) => {
                let current = if path.is_dir() {
                    path.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                } else {
                    collection::stem(&path)
                };
                self.modal = Some(name_modal(NameFor::Rename(path), &current));
            }
            Action::Delete(path) => self.modal = Some(Modal::Delete(path)),
        }
    }

    // --- sending ---

    /// Resolve on the UI thread (cheap, and errors show at once), then
    /// send on a worker thread so the UI stays responsive.
    fn send(&mut self, ctx: &egui::Context) {
        let resolved = match resolve::resolve(&self.req, &BTreeMap::new()) {
            Ok(r) => r,
            Err(e) => {
                self.result = Some(Err(e));
                return;
            }
        };
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(http::send(&resolved));
            ctx.request_repaint();
        });
        self.pending = Some(rx);
        self.result = None;
    }

    // --- drawing ---

    fn title(&self) -> String {
        match (&self.open, &self.collection) {
            (Some(path), Some(c)) => {
                let rel = path.strip_prefix(&c.root).unwrap_or(path);
                let mut parts: Vec<String> = rel
                    .parent()
                    .into_iter()
                    .flat_map(|p| p.components())
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                parts.push(collection::stem(path));
                parts.join(" / ")
            }
            (Some(path), None) => collection::stem(path),
            (None, _) => "Scratch request".into(),
        }
    }

    fn show_modal(&mut self, ctx: &egui::Context) {
        let Some(modal) = self.modal.take() else {
            return;
        };
        let title = self.title();
        let scratch = self.open.is_none();
        let resp = egui::Modal::new(egui::Id::unique("beeline-modal")).show(ctx, |ui| {
            ui.set_width(380.0);
            modal_body(ui, modal, &title, scratch)
        });
        let choice = if resp.should_close() {
            Choice::Cancel
        } else {
            resp.inner
        };

        match choice {
            Choice::Keep(modal) => self.modal = Some(modal),
            Choice::Cancel => {}
            Choice::Save(then) => {
                if self.save() {
                    self.run(then, ctx);
                }
            }
            Choice::Discard(then) => self.run(then, ctx),
            Choice::Name(purpose, text) => {
                if let Err(e) = self.apply_name(&purpose, &text, ctx) {
                    self.modal = Some(Modal::Name {
                        purpose,
                        text,
                        error: Some(e),
                        focus: true,
                    });
                }
            }
            Choice::Delete(path) => self.delete(&path),
        }
    }
}

fn name_modal(purpose: NameFor, text: &str) -> Modal {
    Modal::Name {
        purpose,
        text: text.to_string(),
        error: None,
        focus: true,
    }
}

fn display_name(path: &Path) -> String {
    if path.is_dir() {
        path.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        collection::stem(path)
    }
}

fn modal_body(ui: &mut egui::Ui, modal: Modal, title: &str, scratch: bool) -> Choice {
    match modal {
        Modal::Unsaved(then) => {
            ui.heading("Unsaved changes");
            ui.label(format!("\"{title}\" has unsaved changes."));
            ui.add_space(8.0);
            let mut choice = None;
            ui.horizontal(|ui| {
                let save = if scratch { "Save as…" } else { "Save" };
                if ui.button(save).clicked() {
                    choice = Some(0);
                }
                if ui.button("Discard").clicked() {
                    choice = Some(1);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(2);
                }
            });
            match choice {
                Some(0) => Choice::Save(then),
                Some(1) => Choice::Discard(then),
                Some(_) => Choice::Cancel,
                None => Choice::Keep(Modal::Unsaved(then)),
            }
        }
        Modal::Name {
            purpose,
            mut text,
            error,
            focus,
        } => {
            ui.heading(match &purpose {
                NameFor::NewRequest(_) => "New request",
                NameFor::NewFolder(_) => "New folder",
                NameFor::Rename(_) => "Rename",
                NameFor::SaveAs(_) => "Save request as",
            });
            let edit = ui.add(
                egui::TextEdit::singleline(&mut text)
                    .hint_text("Name")
                    .desired_width(f32::INFINITY),
            );
            if focus {
                edit.request_focus();
            }
            let entered = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if let Some(e) = &error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            }
            ui.add_space(8.0);
            let (mut ok, mut cancel) = (entered, false);
            ui.horizontal(|ui| {
                ok |= ui.button("OK").clicked();
                cancel = ui.button("Cancel").clicked();
            });
            if cancel {
                Choice::Cancel
            } else if ok {
                Choice::Name(purpose, text)
            } else {
                Choice::Keep(Modal::Name {
                    purpose,
                    text,
                    error,
                    focus: false,
                })
            }
        }
        Modal::Delete(path) => {
            ui.heading("Move to Trash");
            let what = if path.is_dir() {
                format!(
                    "Move the folder \"{}\" and everything in it to the Trash?",
                    display_name(&path)
                )
            } else {
                format!("Move \"{}\" to the Trash?", display_name(&path))
            };
            ui.label(what);
            ui.add_space(8.0);
            let (mut yes, mut no) = (false, false);
            ui.horizontal(|ui| {
                yes = ui.button("Move to Trash").clicked();
                no = ui.button("Cancel").clicked();
            });
            if yes {
                Choice::Delete(path)
            } else if no {
                Choice::Cancel
            } else {
                Choice::Keep(Modal::Delete(path))
            }
        }
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        let root = self
            .collection
            .as_ref()
            .map(|c| c.root.to_string_lossy().into_owned())
            .unwrap_or_default();
        storage.set_string(KEY_COLLECTION, root);
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();

        if let Some(rx) = &self.pending
            && let Ok(res) = rx.try_recv()
        {
            self.result = Some(res.map(response::View::new));
            self.pending = None;
        }

        if ctx.input(|i| i.viewport().close_requested()) && self.dirty() && !self.quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.modal = Some(Modal::Unsaved(Then::Quit));
        }

        let busy = self.pending.is_some();
        let modal_open = self.modal.is_some();
        let (mut send, save) = root.input_mut(|i| {
            (
                i.consume_shortcut(&egui::KeyboardShortcut::new(
                    egui::Modifiers::COMMAND,
                    egui::Key::Enter,
                )),
                i.consume_shortcut(&egui::KeyboardShortcut::new(
                    egui::Modifiers::COMMAND,
                    egui::Key::S,
                )),
            )
        });
        if save && !modal_open {
            self.save();
        }

        egui::Panel::bottom("status").show(root, |ui| {
            ui.horizontal(|ui| {
                if let Some(c) = &self.collection {
                    ui.weak(c.root.display().to_string());
                }
                if let Some(n) = &self.notice
                    && let Some(left) = NOTICE_TIME.checked_sub(n.at.elapsed())
                {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if n.error {
                            ui.colored_label(ui.visuals().error_fg_color, &n.text);
                        } else {
                            ui.label(&n.text);
                        }
                    });
                    // Redraw once more when it expires so it disappears.
                    ui.ctx().request_repaint_after(left);
                }
            });
        });

        let mut actions = Vec::new();
        egui::Panel::left("sidebar")
            .resizable(true)
            .default_size(240.0)
            .show(root, |ui| {
                let (root_dir, tree) = match &self.collection {
                    Some(c) => (Some(c.root.as_path()), c.tree.as_slice()),
                    None => (None, &[][..]),
                };
                actions = sidebar::show(ui, root_dir, tree, self.open.as_deref());
            });

        let title = self.title();
        let dirty = self.dirty();
        let mut save_clicked = false;
        egui::Panel::top("request")
            .resizable(true)
            .default_size(300.0)
            .show(root, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.strong(&title);
                    if dirty {
                        ui.label("•").on_hover_text("Unsaved changes");
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        save_clicked = ui
                            .add_enabled(dirty, egui::Button::new("Save"))
                            .on_hover_text("Cmd/Ctrl+S")
                            .clicked();
                    });
                });
                send |= editor::show(ui, &mut self.req, &mut self.tab, busy);
            });

        egui::CentralPanel::default().show(root, |ui| {
            if busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Sending…");
                });
            }
            match &mut self.result {
                Some(Ok(view)) => response::show(ui, view),
                Some(Err(err)) => {
                    ui.colored_label(ui.visuals().error_fg_color, err.as_str());
                }
                None if !busy => {
                    ui.weak("Send a request to see the response here. Cmd/Ctrl+Enter sends.");
                }
                None => {}
            }
        });

        for action in actions {
            self.handle_sidebar(action, &ctx);
        }
        if save_clicked {
            self.save();
        }
        if send && !busy && !modal_open {
            self.send(&ctx);
        }
        self.show_modal(&ctx);
    }
}
