//! Top-level app state, messages, update and frame layout. Widgets live in
//! `ui/`; this module decides what happens when they report something.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use iced::keyboard;
use iced::mouse;
use iced::widget::pane_grid::{self, Axis, Configuration, Content, PaneGrid};
use iced::widget::{column, container, mouse_area, row, rule, space, stack, text, text_editor};
use iced::{Element, Fill, FillPortion, Point, Subscription, Task, event, window};
use serde::{Deserialize, Serialize};

use crate::collection::{self, Node};
use crate::model::{Environment, KeyValue, RequestFile};
use crate::tabs::Tabs;
use crate::ui::tab_strip;
use crate::ui::{dialog, editor, kv, response, sidebar, theme};
use crate::{http, resolve, seed};

const NOTICE_TIME: Duration = Duration::from_secs(4);
/// The two resizable panes: the collection sidebar and the request/response area.
#[derive(Debug, Clone, Copy)]
enum PaneKind {
    Sidebar,
    Main,
}

/// What the app remembers between runs.
#[derive(Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
struct SavedState {
    collection: Option<PathBuf>,
    env: Option<String>,
    /// Files of the open tabs, in tab order. Untitled tabs are not kept.
    #[serde(default)]
    tabs: Vec<PathBuf>,
    /// Index into `tabs` of the active one.
    #[serde(default)]
    active_tab: usize,
}

/// `~/.config/beeline/state.json`, or None when there is no home folder.
fn state_path() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("HOME")?).join(".config/beeline/state.json"))
}

fn load_state() -> SavedState {
    state_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

struct Collection {
    root: PathBuf,
    tree: Vec<Node>,
    envs: Vec<(String, PathBuf)>,
}

/// The environment editor. Rows are edited as a list and written back as
/// a map, so duplicate keys keep the last value.
struct EnvEditor {
    name: String,
    path: PathBuf,
    rows: Vec<KeyValue>,
    saved: Vec<KeyValue>,
}

fn env_rows(env: &Environment) -> Vec<KeyValue> {
    env.variables
        .iter()
        .map(|(k, v)| KeyValue::new(k.clone(), v.clone()))
        .collect()
}

fn rows_env(rows: &[KeyValue]) -> Environment {
    Environment {
        variables: rows
            .iter()
            .filter(|kv| !kv.key.trim().is_empty())
            .map(|kv| (kv.key.trim().to_string(), kv.value.clone()))
            .collect(),
    }
}

struct Notice {
    text: String,
    error: bool,
    at: Instant,
}

/// What to do once unsaved changes are dealt with.
#[derive(Debug, Clone)]
pub enum Then {
    Close(u64),
    OpenCollection(PathBuf),
    Quit(window::Id),
}

/// One open request: its own text, file, editor state and send.
struct Doc {
    id: u64,
    req: RequestFile,
    /// The file `req` was loaded from; `None` for an untitled request.
    open: Option<PathBuf>,
    /// The last loaded or saved copy, for the unsaved-changes check.
    saved: RequestFile,
    /// The body's text, which iced edits in place. `req.body.text` mirrors it.
    body: text_editor::Content,
    tab: editor::Tab,
    /// True while this request is in flight.
    pending: bool,
    /// Bumped on each load, so a late response for an old request is dropped.
    generation: u64,
    result: Option<Result<response::View, String>>,
}

impl Doc {
    fn new(id: u64, req: RequestFile, open: Option<PathBuf>, saved: RequestFile) -> Self {
        Self {
            id,
            body: text_editor::Content::with_text(&req.body.text),
            req,
            open,
            saved,
            tab: editor::Tab::default(),
            pending: false,
            generation: 0,
            result: None,
        }
    }

    fn dirty(&self) -> bool {
        self.req != self.saved
    }

    /// Replaces the request and keeps the body editor in step with it.
    fn replace_req(&mut self, req: RequestFile) {
        self.body = text_editor::Content::with_text(&req.body.text);
        self.req = req;
    }
}

#[derive(Debug, Clone)]
enum NameFor {
    NewRequest(PathBuf),
    NewFolder(PathBuf),
    Rename(PathBuf),
    SaveAs(PathBuf),
    NewEnv(PathBuf),
}

#[derive(Debug, Clone)]
enum Modal {
    Unsaved(Then),
    Name {
        purpose: NameFor,
        text: String,
        error: Option<String>,
    },
    Delete(PathBuf),
}

pub struct App {
    panes: pane_grid::State<PaneKind>,
    collection: Option<Collection>,
    /// The open requests, one tab each.
    docs: Tabs<Doc>,
    /// The id the next tab gets. Responses find their tab by id.
    next_doc: u64,
    modal: Option<Modal>,
    /// Set when a dialog opens that needs its name field focused.
    focus_name: bool,
    /// The action waiting on a scratch request being named and saved.
    after_save: Option<Then>,
    notice: Option<Notice>,
    quitting: bool,
    /// Selected environment name and its variables.
    env: Option<String>,
    vars: BTreeMap<String, String>,
    env_editor: Option<EnvEditor>,
    /// Folders that are open in the sidebar.
    expanded: BTreeSet<PathBuf>,
    /// The open context menu: the row it is for, and where to draw it.
    context: Option<(PathBuf, Point)>,
    /// Last known pointer position, so a menu can open where it was clicked.
    cursor: Point,
    /// The item being dragged, from the press that started it.
    drag: Option<PathBuf>,
    /// The row the pointer is over, which is the drop target during a drag.
    drop_target: Option<PathBuf>,
    /// Where the pointer was when the drag's press happened.
    press_at: Point,
}

#[derive(Debug, Clone)]
pub enum Message {
    Sidebar(sidebar::Action),
    Editor(editor::Msg),
    Response(response::Msg),
    Dialog(dialog::Msg),
    Tabs(tab_strip::Msg),
    NewTab,
    CloseTab,
    Send,
    Save,
    Sent(u64, u64, Result<http::Response, String>),
    CloseRequested(window::Id),
    CursorMoved(Point),
    MouseReleased,
    CloseContext,
    PaneResized(pane_grid::ResizeEvent),
}

impl App {
    /// Opens `start` if given, otherwise the last collection used.
    pub fn boot(start: Option<PathBuf>) -> (Self, Task<Message>) {
        let mut app = Self {
            panes: pane_grid::State::with_configuration(Configuration::Split {
                axis: Axis::Vertical,
                ratio: 0.26,
                a: Box::new(Configuration::Pane(PaneKind::Sidebar)),
                b: Box::new(Configuration::Pane(PaneKind::Main)),
            }),
            collection: None,
            docs: Tabs::default(),
            next_doc: 0,
            modal: None,
            focus_name: false,
            after_save: None,
            notice: None,
            quitting: false,
            env: None,
            vars: BTreeMap::new(),
            env_editor: None,
            expanded: BTreeSet::new(),
            context: None,
            cursor: Point::ORIGIN,
            drag: None,
            drop_target: None,
            press_at: Point::ORIGIN,
        };
        // No state file means this is the first run.
        let first_run = state_path().is_some_and(|p| !p.exists());
        let saved = load_state();
        match start {
            Some(start) => match start.canonicalize() {
                Ok(root) if root.is_dir() => app.open_collection(root),
                _ => app.notify(format!("Not a folder: {}", start.display()), true),
            },
            None => match &saved.collection {
                Some(root) if root.is_dir() => {
                    app.open_collection(root.clone());
                    app.restore_tabs(&saved);
                }
                _ if first_run => app.open_default_collection(),
                _ => {}
            },
        }
        if let Some(name) = saved.env
            && app.has_env(&name)
        {
            app.select_env(Some(name));
        }
        (app, Task::none())
    }

    /// Reopens the tabs saved for this collection. Files that are gone are
    /// skipped.
    fn restore_tabs(&mut self, saved: &SavedState) {
        let mut active = None;
        for (i, path) in saved.tabs.iter().enumerate() {
            if path.is_file()
                && let Ok(req) = collection::read_request(path)
            {
                let at = self.push_doc(req, Some(path.clone()));
                if i == saved.active_tab {
                    active = Some(at);
                }
            }
        }
        if let Some(at) = active {
            self.docs.activate(at);
        }
    }

    /// Adds a tab for `req` and makes it active. Returns its index.
    fn push_doc(&mut self, req: RequestFile, open: Option<PathBuf>) -> usize {
        let id = self.next_doc;
        self.next_doc += 1;
        let saved = req.clone();
        self.docs.push(Doc::new(id, req, open, saved))
    }

    /// The active request, if any tab is open.
    fn doc(&self) -> Option<&Doc> {
        self.docs.active()
    }

    fn doc_mut(&mut self) -> Option<&mut Doc> {
        self.docs.active_mut()
    }

    fn any_dirty(&self) -> bool {
        self.docs.iter().any(Doc::dirty)
    }

    /// First run: create the Default collection under ~/Documents/beeline
    /// and fill it with the examples.
    fn open_default_collection(&mut self) {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let root = PathBuf::from(home).join("Documents/beeline/Default");
        match seed::write_examples(&root) {
            Ok(()) => self.open_collection(root),
            Err(e) => self.notify(e, true),
        }
    }

    fn notify(&mut self, text: impl Into<String>, error: bool) {
        self.notice = Some(Notice {
            text: text.into(),
            error,
            at: Instant::now(),
        });
    }

    fn save_state(&self) {
        let Some(path) = state_path() else {
            return;
        };
        // Only tabs with a file are kept. The active index counts among them.
        let mut tabs = Vec::new();
        let mut active_tab = 0;
        for (i, doc) in self.docs.iter().enumerate() {
            if let Some(file) = &doc.open {
                if Some(i) == self.docs.active_index() {
                    active_tab = tabs.len();
                }
                tabs.push(file.clone());
            }
        }
        let state = SavedState {
            collection: self.collection.as_ref().map(|c| c.root.clone()),
            env: self.env.clone(),
            tabs,
            active_tab,
        };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(&state) {
            let _ = fs::write(path, json);
        }
    }

    fn open_name(&mut self, purpose: NameFor, text: &str) {
        self.modal = Some(Modal::Name {
            purpose,
            text: text.to_string(),
            error: None,
        });
        self.focus_name = true;
    }

    // --- collection and file actions ---

    fn open_collection(&mut self, root: PathBuf) {
        let scanned = collection::scan(&root)
            .and_then(|tree| collection::environments(&root).map(|envs| (tree, envs)));
        match scanned {
            Ok((tree, envs)) => {
                self.collection = Some(Collection { root, tree, envs });
                self.env = None;
                self.vars.clear();
                self.env_editor = None;
                // Tabs belong to the collection they came from.
                self.docs.clear();
                self.expanded.clear();
                self.context = None;
                self.save_state();
            }
            Err(e) => self.notify(e, true),
        }
    }

    /// Re-read the tree, the environment list and the selected
    /// environment's variables from disk.
    fn reload_tree(&mut self) {
        let Some(c) = &mut self.collection else {
            return;
        };
        let scanned = collection::scan(&c.root)
            .and_then(|tree| collection::environments(&c.root).map(|envs| (tree, envs)));
        match scanned {
            Ok((tree, envs)) => {
                c.tree = tree;
                c.envs = envs;
            }
            Err(e) => self.notify(e, true),
        }
        let env = self.env.take().filter(|name| self.has_env(name));
        self.select_env(env);
        if let Some(editor) = &self.env_editor
            && !editor.path.exists()
        {
            self.env_editor = None;
        }
    }

    fn has_env(&self, name: &str) -> bool {
        self.env_path(name).is_some()
    }

    fn env_path(&self, name: &str) -> Option<PathBuf> {
        self.collection
            .as_ref()?
            .envs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, p)| p.clone())
    }

    fn select_env(&mut self, name: Option<String>) {
        self.vars.clear();
        if let Some(path) = name.as_deref().and_then(|n| self.env_path(n)) {
            match collection::read_env(&path) {
                Ok(env) => self.vars = env.variables,
                Err(e) => self.notify(e, true),
            }
        }
        self.env = name;
        self.save_state();
    }

    fn edit_env(&mut self) {
        let Some(name) = self.env.clone() else {
            return;
        };
        let Some(path) = self.env_path(&name) else {
            return;
        };
        match collection::read_env(&path) {
            Ok(env) => {
                let rows = env_rows(&env);
                self.env_editor = Some(EnvEditor {
                    name,
                    path,
                    saved: rows.clone(),
                    rows,
                });
            }
            Err(e) => self.notify(e, true),
        }
    }

    /// Opens `path` in a tab, or switches to its tab if it is already open.
    fn open_tab(&mut self, path: PathBuf) {
        if let Some(at) = self.docs.position(|d| d.open.as_ref() == Some(&path)) {
            self.docs.activate(at);
        } else {
            match collection::read_request(&path) {
                Ok(req) => {
                    self.push_doc(req, Some(path));
                }
                Err(e) => {
                    self.notify(e, true);
                    return;
                }
            }
        }
        self.save_state();
    }

    /// Opens a new untitled request in a tab.
    fn new_tab(&mut self) {
        self.push_doc(RequestFile::default(), None);
        self.save_state();
    }

    /// Asks before closing a tab with unsaved changes; closes it otherwise.
    fn close_tab(&mut self, id: u64) -> Task<Message> {
        let Some(at) = self.docs.position(|d| d.id == id) else {
            return Task::none();
        };
        if self.docs.get(at).is_some_and(Doc::dirty) {
            self.docs.activate(at);
            self.modal = Some(Modal::Unsaved(Then::Close(id)));
            Task::none()
        } else {
            self.run(Then::Close(id))
        }
    }

    /// Save the active request. An untitled request asks for a name instead.
    /// Returns true when the request is now saved.
    fn save(&mut self) -> bool {
        let Some(doc) = self.doc() else {
            self.notify("Open a request first", true);
            return false;
        };
        if let Some(path) = doc.open.clone() {
            let req = doc.req.clone();
            match collection::write_request(&path, &req) {
                Ok(()) => {
                    if let Some(doc) = self.doc_mut() {
                        doc.saved = req;
                    }
                    self.notify("Saved", false);
                    true
                }
                Err(e) => {
                    self.notify(e, true);
                    false
                }
            }
        } else if let Some(c) = &self.collection {
            let root = c.root.clone();
            self.open_name(NameFor::SaveAs(root), "");
            false
        } else {
            self.notify("Open a collection folder to save requests", true);
            false
        }
    }

    /// Run `then` now, or ask about unsaved changes first. Quitting and
    /// switching collections check every tab, one dialog at a time.
    fn guard(&mut self, then: Then) -> Task<Message> {
        let dirty = match &then {
            Then::Close(_) => None,
            _ => self.docs.position(Doc::dirty),
        };
        match dirty {
            Some(at) => {
                self.docs.activate(at);
                self.modal = Some(Modal::Unsaved(then));
                Task::none()
            }
            None => self.run(then),
        }
    }

    /// Does `then` now, without asking again.
    fn run(&mut self, then: Then) -> Task<Message> {
        match then {
            Then::Close(id) => {
                if let Some(at) = self.docs.position(|d| d.id == id) {
                    self.docs.close(at);
                    self.save_state();
                }
                Task::none()
            }
            Then::OpenCollection(root) => {
                self.open_collection(root);
                Task::none()
            }
            Then::Quit(id) => {
                self.quitting = true;
                window::close(id)
            }
        }
    }

    fn apply_name(&mut self, purpose: &NameFor, text: &str) -> Result<Task<Message>, String> {
        match purpose {
            NameFor::NewRequest(dir) => {
                let path = collection::create_request(dir, text, &RequestFile::default())?;
                self.reload_tree();
                self.open_tab(path);
                Ok(Task::none())
            }
            NameFor::NewFolder(dir) => {
                collection::create_folder(dir, text)?;
                self.reload_tree();
                Ok(Task::none())
            }
            NameFor::Rename(path) => {
                let new = collection::rename(path, text)?;
                self.retarget(path, &new);
                self.reload_tree();
                Ok(Task::none())
            }
            NameFor::SaveAs(dir) => {
                let req = self.doc().map(|d| d.req.clone()).unwrap_or_default();
                let path = collection::create_request(dir, text, &req)?;
                if let Some(doc) = self.doc_mut() {
                    doc.open = Some(path);
                    doc.saved = req;
                }
                self.reload_tree();
                self.save_state();
                self.notify("Saved", false);
                Ok(match self.after_save.take() {
                    Some(then) => self.guard(then),
                    None => Task::none(),
                })
            }
            NameFor::NewEnv(root) => {
                let path = collection::create_env(root, text)?;
                self.reload_tree();
                self.select_env(Some(collection::stem(&path)));
                self.edit_env();
                Ok(Task::none())
            }
        }
    }

    fn delete(&mut self, path: &Path) {
        if let Err(e) = collection::delete(path) {
            self.notify(e, true);
            return;
        }
        // Keep each affected tab's content as an unsaved untitled request.
        for doc in self.docs.iter_mut() {
            if doc.open.as_deref().is_some_and(|o| o.starts_with(path)) {
                doc.open = None;
                doc.saved = RequestFile::default();
            }
        }
        self.save_state();
        self.reload_tree();
        self.notify("Moved to Trash", false);
    }

    /// Points every open tab that was at `from` (or inside it) at its new
    /// place after a rename or move.
    fn retarget(&mut self, from: &Path, to: &Path) {
        for doc in self.docs.iter_mut() {
            if let Some(open) = &doc.open
                && let Some(moved) = collection::moved(open, from, to)
            {
                doc.open = Some(moved);
            }
        }
        self.save_state();
    }

    fn sidebar_action(&mut self, action: sidebar::Action) -> Task<Message> {
        use sidebar::Action;
        if !matches!(action, Action::Enter(_) | Action::Exit(_)) {
            self.context = None;
        }
        match action {
            Action::OpenCollection => {
                // Start where the open collection is, if there is one.
                let mut dialog = rfd::FileDialog::new().set_title("Open collection folder");
                if let Some(c) = &self.collection {
                    dialog = dialog.set_directory(&c.root);
                }
                if let Some(root) = dialog.pick_folder() {
                    return self.guard(Then::OpenCollection(root));
                }
            }
            Action::Reload => self.reload_tree(),
            Action::ToggleFolder(path) => {
                if !self.expanded.remove(&path) {
                    self.expanded.insert(path);
                }
            }
            Action::ContextMenu(path) => self.context = Some((path, self.cursor)),
            Action::Press(path) => {
                self.drop_target = Some(path.clone());
                self.drag = Some(path);
                self.press_at = self.cursor;
            }
            Action::Enter(path) => {
                self.drop_target = Some(path);
            }
            Action::Exit(path) => {
                if self.drop_target.as_ref() == Some(&path) {
                    self.drop_target = None;
                }
            }
            Action::Clone(path) => match collection::duplicate(&path) {
                Ok(copy) => {
                    self.reload_tree();
                    self.notify(format!("Cloned as \"{}\"", collection::stem(&copy)), false);
                }
                Err(e) => self.notify(e, true),
            },
            Action::Open(path) => self.open_tab(path),
            Action::NewRequest(dir) => self.open_name(NameFor::NewRequest(dir), ""),
            Action::NewFolder(dir) => self.open_name(NameFor::NewFolder(dir), ""),
            Action::Rename(path) => {
                let current = display_name(&path);
                self.open_name(NameFor::Rename(path), &current);
            }
            Action::Delete(path) => self.modal = Some(Modal::Delete(path)),
            Action::SelectEnv(name) => {
                self.select_env(name);
                if self.env_editor.is_some() {
                    self.edit_env();
                }
            }
            Action::NewEnv => {
                if let Some(c) = &self.collection {
                    let root = c.root.clone();
                    self.open_name(NameFor::NewEnv(root), "");
                }
            }
            Action::EditEnv => self.edit_env(),
        }
        Task::none()
    }

    /// A release ends a drag. Released on the item it started on, it is a
    /// click. Released on a folder, it moves the item there.
    fn finish_drag(&mut self) -> Task<Message> {
        let Some(origin) = self.drag.take() else {
            return Task::none();
        };
        let Some(target) = self.drop_target.take() else {
            return Task::none();
        };
        if target == origin {
            let action = if origin.is_dir() {
                sidebar::Action::ToggleFolder(origin)
            } else {
                sidebar::Action::Open(origin)
            };
            return self.sidebar_action(action);
        }
        // Dropping on a request means dropping into the folder it sits in.
        let dir = if target.is_dir() {
            Some(target)
        } else {
            target.parent().map(Path::to_path_buf)
        };
        if let Some(dir) = dir
            && origin.parent() != Some(dir.as_path())
        {
            self.move_item(&origin, &dir);
        }
        Task::none()
    }

    fn move_item(&mut self, from: &Path, dir: &Path) {
        match collection::move_into(from, dir) {
            Ok(to) => {
                self.retarget(from, &to);
                self.expanded.insert(dir.to_path_buf());
                self.reload_tree();
                self.notify(format!("Moved to \"{}\"", collection::stem(dir)), false);
            }
            Err(e) => self.notify(e, true),
        }
    }

    fn env_save(&mut self) {
        let Some(editor) = &mut self.env_editor else {
            return;
        };
        let env = rows_env(&editor.rows);
        match collection::write_env(&editor.path, &env) {
            Ok(()) => {
                editor.rows = env_rows(&env);
                editor.saved = editor.rows.clone();
                let name = editor.name.clone();
                if self.env.as_deref() == Some(name.as_str()) {
                    self.vars = env.variables;
                }
                self.notify("Environment saved", false);
            }
            Err(e) => self.notify(e, true),
        }
    }

    // --- sending ---

    /// Resolve on the UI thread (cheap, and errors show at once), then
    /// send on a worker thread so the UI stays responsive.
    fn send(&mut self) -> Task<Message> {
        if self.modal.is_some() {
            return Task::none();
        }
        let vars = &self.vars;
        let Some(doc) = self.docs.active_mut() else {
            return Task::none();
        };
        if doc.pending {
            return Task::none();
        }
        let resolved = match resolve::resolve(&doc.req, vars) {
            Ok(r) => r,
            Err(e) => {
                doc.result = Some(Err(e));
                return Task::none();
            }
        };
        doc.pending = true;
        doc.result = None;
        let (id, generation) = (doc.id, doc.generation);
        // A plain thread keeps the blocking HTTP call off the UI executor.
        // The oneshot channel works with any executor.
        let (tx, rx) = iced::futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(http::send(&resolved));
        });
        Task::perform(
            async move {
                rx.await
                    .unwrap_or_else(|_| Err("request thread stopped".into()))
            },
            move |res| Message::Sent(id, generation, res),
        )
    }

    // --- update ---

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = match message {
            Message::Sidebar(action) => self.sidebar_action(action),
            Message::PaneResized(event) => {
                self.panes.resize(event.split, event.ratio.clamp(0.15, 0.5));
                Task::none()
            }
            Message::Editor(msg) => self.editor_msg(msg),
            Message::Response(msg) => self.response_msg(msg),
            Message::Dialog(msg) => self.dialog_msg(msg),
            Message::Send => self.send(),
            Message::Save => {
                if self.modal.is_none() {
                    self.save();
                }
                Task::none()
            }
            Message::CursorMoved(at) => {
                self.cursor = at;
                Task::none()
            }
            Message::MouseReleased => self.finish_drag(),
            Message::CloseContext => {
                self.context = None;
                Task::none()
            }
            Message::Tabs(msg) => self.tab_msg(msg),
            Message::NewTab => {
                if self.modal.is_none() {
                    self.new_tab();
                }
                Task::none()
            }
            Message::CloseTab => match self.docs.active().map(|d| d.id) {
                Some(id) if self.modal.is_none() => self.close_tab(id),
                _ => Task::none(),
            },
            Message::Sent(id, generation, res) => {
                // A tab closed or cancelled since the send drops its response.
                if let Some(doc) = self.docs.iter_mut().find(|d| d.id == id)
                    && doc.generation == generation
                {
                    doc.pending = false;
                    doc.result = Some(res.map(response::View::new));
                }
                Task::none()
            }
            Message::CloseRequested(id) => {
                if self.any_dirty() && !self.quitting {
                    self.guard(Then::Quit(id))
                } else {
                    window::close(id)
                }
            }
        };
        let focus = if std::mem::take(&mut self.focus_name) {
            iced::widget::operation::focus(dialog::NAME_ID)
        } else {
            Task::none()
        };
        Task::batch([task, focus])
    }

    fn editor_msg(&mut self, msg: editor::Msg) -> Task<Message> {
        let Some(doc) = self.docs.active_mut() else {
            return Task::none();
        };
        match msg {
            editor::Msg::Body(action) => {
                doc.body.perform(action);
                doc.req.body.text = doc.body.text();
                Task::none()
            }
            editor::Msg::FormatJson => {
                if let Some(pretty) = editor::pretty(&doc.req.body.text) {
                    doc.body = text_editor::Content::with_text(&pretty);
                    doc.req.body.text = pretty;
                }
                Task::none()
            }
            editor::Msg::Cancel => {
                self.cancel_request();
                Task::none()
            }
            other => {
                if editor::update(&mut doc.req, &mut doc.tab, other) {
                    self.send()
                } else {
                    Task::none()
                }
            }
        }
    }

    /// Gives up on the active tab's request in flight. Its response, if it
    /// arrives, is dropped because the generation has moved on.
    fn cancel_request(&mut self) {
        if let Some(doc) = self.docs.active_mut()
            && doc.pending
        {
            doc.generation += 1;
            doc.pending = false;
            doc.result = Some(Err("Request cancelled".into()));
        }
    }

    fn tab_msg(&mut self, msg: tab_strip::Msg) -> Task<Message> {
        match msg {
            tab_strip::Msg::Activate(id) => {
                if let Some(at) = self.docs.position(|d| d.id == id) {
                    self.docs.activate(at);
                    self.save_state();
                }
                Task::none()
            }
            tab_strip::Msg::Close(id) => {
                if self.modal.is_none() {
                    self.close_tab(id)
                } else {
                    Task::none()
                }
            }
            tab_strip::Msg::New => {
                if self.modal.is_none() {
                    self.new_tab();
                }
                Task::none()
            }
        }
    }

    fn response_msg(&mut self, msg: response::Msg) -> Task<Message> {
        let Some(doc) = self.docs.active_mut() else {
            return Task::none();
        };
        match msg {
            response::Msg::Copy => {
                let text = match &doc.result {
                    Some(Ok(view)) => Some(view.text().to_string()),
                    _ => None,
                };
                match text {
                    Some(text) => {
                        self.notify("Copied", false);
                        iced::clipboard::write(text)
                    }
                    None => Task::none(),
                }
            }
            other => {
                if let Some(Ok(view)) = &mut doc.result {
                    response::update(view, other);
                }
                Task::none()
            }
        }
    }

    fn dialog_msg(&mut self, msg: dialog::Msg) -> Task<Message> {
        match msg {
            dialog::Msg::UnsavedSave => {
                let Some(Modal::Unsaved(then)) = self.modal.take() else {
                    return Task::none();
                };
                if self.save() {
                    self.guard(then)
                } else {
                    // An untitled request asks for a name first. Remember the
                    // action; it runs once the name is saved.
                    if self.modal.is_some() {
                        self.after_save = Some(then);
                    }
                    Task::none()
                }
            }
            dialog::Msg::UnsavedDiscard => {
                let Some(Modal::Unsaved(then)) = self.modal.take() else {
                    return Task::none();
                };
                match then {
                    Then::Close(_) => self.run(then),
                    then => {
                        // Throw away this tab's edits, then ask about the next.
                        if let Some(doc) = self.doc_mut() {
                            let saved = doc.saved.clone();
                            doc.replace_req(saved);
                        }
                        self.guard(then)
                    }
                }
            }
            dialog::Msg::Cancel => {
                if self.context.take().is_none() && self.modal.take().is_none() {
                    self.env_editor = None;
                }
                self.after_save = None;
                Task::none()
            }
            dialog::Msg::NameInput(value) => {
                if let Some(Modal::Name { text, .. }) = &mut self.modal {
                    *text = value;
                }
                Task::none()
            }
            dialog::Msg::NameOk => {
                let Some(Modal::Name {
                    purpose,
                    text,
                    error: _,
                }) = self.modal.take()
                else {
                    return Task::none();
                };
                match self.apply_name(&purpose, &text) {
                    Ok(task) => task,
                    Err(e) => {
                        self.modal = Some(Modal::Name {
                            purpose,
                            text,
                            error: Some(e),
                        });
                        Task::none()
                    }
                }
            }
            dialog::Msg::DeleteOk => {
                if let Some(Modal::Delete(path)) = self.modal.take() {
                    self.delete(&path);
                }
                Task::none()
            }
            dialog::Msg::EnvKv(msg) => {
                if let Some(editor) = &mut self.env_editor {
                    kv::update(&mut editor.rows, msg);
                }
                Task::none()
            }
            dialog::Msg::EnvSave => {
                self.env_save();
                Task::none()
            }
            dialog::Msg::EnvClose => {
                self.env_editor = None;
                Task::none()
            }
            dialog::Msg::EnvTrash => {
                if let Some(editor) = &self.env_editor {
                    self.modal = Some(Modal::Delete(editor.path.clone()));
                }
                Task::none()
            }
        }
    }

    // --- subscriptions ---

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            event::listen_with(shortcut),
            window::close_requests().map(Message::CloseRequested),
        ])
    }

    // --- drawing ---

    fn title(&self) -> String {
        match (self.doc().and_then(|d| d.open.as_ref()), &self.collection) {
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
            (None, _) => "Untitled request".into(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let grid = PaneGrid::new(&self.panes, |_, pane, _| match pane {
            PaneKind::Sidebar => Content::new(self.sidebar_pane()),
            PaneKind::Main => Content::new(self.main_pane()),
        })
        .on_resize(6, Message::PaneResized)
        .spacing(6);

        let c = self.collection.as_ref();
        let root_path = c.map(|c| c.root.display().to_string()).unwrap_or_default();
        let notice = self
            .notice
            .as_ref()
            .filter(|n| n.at.elapsed() < NOTICE_TIME)
            .map(|n| {
                let t = text(n.text.as_str()).size(theme::SMALL);
                if n.error { t.style(text::danger) } else { t }
            });
        let mut status_row = row![
            text(root_path).size(theme::SMALL),
            space::horizontal().width(Fill)
        ]
        .spacing(8);
        if let Some(n) = notice {
            status_row = status_row.push(n);
        }
        let status = container(status_row).padding([4, 12]).width(Fill);

        let base: Element<'_, Message> = column![
            container(grid).padding(8).height(Fill).width(Fill),
            rule::horizontal(1),
            status,
        ]
        .into();

        let mut layers: Vec<Element<'_, Message>> = vec![base];
        if let Some(editor) = &self.env_editor {
            let env_dirty = editor.rows != editor.saved;
            layers.push(dialog::env(&editor.name, &editor.rows, env_dirty).map(Message::Dialog));
        }
        if let Some((name, at)) = self.drag_ghost() {
            layers.push(drag_ghost_layer(name, at));
        }
        if let Some((path, at)) = &self.context {
            layers.push(self.context_menu(path, *at));
        }
        if let Some(modal) = &self.modal {
            layers.push(self.modal_view(modal));
        }
        stack(layers).into()
    }

    /// The collection tree and environment controls, in a rounded card.
    fn sidebar_pane(&self) -> Element<'_, Message> {
        let c = self.collection.as_ref();
        let side = sidebar::view(sidebar::Props {
            root: c.map(|c| c.root.as_path()),
            tree: c.map(|c| c.tree.as_slice()).unwrap_or_default(),
            selected: self.doc().and_then(|d| d.open.as_deref()),
            envs: c.map(|c| c.envs.as_slice()).unwrap_or_default(),
            env: self.env.as_deref(),
            expanded: &self.expanded,
            drop: self.drop_folder(),
            dragging: self.drag.as_deref(),
        })
        .map(Message::Sidebar);
        container(side)
            .style(theme::card)
            .padding(12)
            .width(Fill)
            .height(Fill)
            .into()
    }

    /// The request editor above the response viewer, each in a rounded card.
    fn main_pane(&self) -> Element<'_, Message> {
        let items = self
            .docs
            .iter()
            .map(|d| tab_strip::Item {
                id: d.id,
                label: d
                    .open
                    .as_deref()
                    .map_or("Untitled".into(), collection::stem),
                dirty: d.dirty(),
            })
            .collect();
        let strip = tab_strip::view(items, self.doc().map(|d| d.id)).map(Message::Tabs);

        let Some(doc) = self.doc() else {
            let hint =
                text("No request open. Pick one in the sidebar, or press Cmd+T for a new one.")
                    .size(theme::TEXT);
            return container(
                column![strip, container(hint).center(Fill)]
                    .spacing(12)
                    .height(Fill),
            )
            .style(theme::card)
            .padding(12)
            .width(Fill)
            .height(Fill)
            .into();
        };

        let dirty = doc.dirty();
        let header = row![
            text(self.title()).size(theme::HEADING),
            if dirty {
                text("•").size(theme::HEADING)
            } else {
                text("")
            },
            space::horizontal().width(Fill),
            theme::button(text("Open…").size(theme::TEXT))
                .on_press(Message::Sidebar(sidebar::Action::OpenCollection)),
            theme::button(text("Save").size(theme::TEXT))
                .on_press_maybe(dirty.then_some(Message::Save)),
        ]
        .spacing(8);
        let request = column![
            strip,
            header,
            editor::view(
                &doc.req,
                doc.tab,
                doc.pending,
                &resolve::effective(&doc.req, &self.vars),
                &doc.body,
            )
            .map(Message::Editor),
        ]
        .spacing(10)
        .height(Fill);

        let response: Element<'_, Message> = if doc.pending {
            text("Sending…").size(theme::TEXT).into()
        } else {
            match &doc.result {
                Some(Ok(view)) => response::view(view).map(Message::Response),
                Some(Err(err)) => text(err.as_str()).style(text::danger).into(),
                None => text("Send a request to see the response here. Cmd+Enter sends.").into(),
            }
        };

        column![
            container(request)
                .style(theme::card)
                .padding(12)
                .width(Fill)
                .height(FillPortion(1)),
            container(response)
                .style(theme::card)
                .padding(12)
                .width(Fill)
                .height(FillPortion(1)),
        ]
        .spacing(8)
        .height(Fill)
        .into()
    }

    /// The folder a drop would move the dragged item into, for highlighting.
    fn drop_folder(&self) -> Option<&Path> {
        let origin = self.drag.as_deref()?;
        let target = self.drop_target.as_deref()?;
        if target == origin {
            return None;
        }
        let dir = if target.is_dir() {
            target
        } else {
            target.parent()?
        };
        (origin.parent() != Some(dir)).then_some(dir)
    }

    /// The dragged item's name and the pointer, once the pointer has moved
    /// far enough from the press to count as a drag rather than a click.
    fn drag_ghost(&self) -> Option<(String, Point)> {
        let origin = self.drag.as_ref()?;
        let dx = self.cursor.x - self.press_at.x;
        let dy = self.cursor.y - self.press_at.y;
        (dx.hypot(dy) > 6.0).then(|| (display_name(origin), self.cursor))
    }

    /// A context menu at `at`. A click anywhere outside it closes it.
    fn context_menu(&self, path: &Path, at: Point) -> Element<'_, Message> {
        let item = |label: &'static str, action: sidebar::Action| -> Element<'_, Message> {
            theme::button(text(label).size(theme::TEXT))
                .style(theme::text_button)
                .width(Fill)
                .on_press(Message::Sidebar(action))
                .into()
        };
        let items: Vec<Element<'_, Message>> = if path.is_dir() {
            vec![
                item(
                    "New request",
                    sidebar::Action::NewRequest(path.to_path_buf()),
                ),
                item("New folder", sidebar::Action::NewFolder(path.to_path_buf())),
                item("Rename…", sidebar::Action::Rename(path.to_path_buf())),
                item(
                    "Move to Trash…",
                    sidebar::Action::Delete(path.to_path_buf()),
                ),
            ]
        } else {
            vec![
                item("Clone", sidebar::Action::Clone(path.to_path_buf())),
                item("Rename…", sidebar::Action::Rename(path.to_path_buf())),
                item(
                    "Move to Trash…",
                    sidebar::Action::Delete(path.to_path_buf()),
                ),
            ]
        };
        let card = container(column(items).spacing(2))
            .style(theme::card)
            .padding(6)
            .width(200);
        let placed = container(card)
            .padding(iced::Padding {
                top: at.y,
                right: 0.0,
                bottom: 0.0,
                left: at.x,
            })
            .width(Fill)
            .height(Fill);
        mouse_area(placed)
            .on_press(Message::CloseContext)
            .on_right_press(Message::CloseContext)
            .into()
    }

    fn modal_view<'a>(&'a self, modal: &'a Modal) -> Element<'a, Message> {
        match modal {
            Modal::Unsaved(_) => {
                let untitled = self.doc().is_some_and(|d| d.open.is_none());
                dialog::unsaved(self.title(), untitled).map(Message::Dialog)
            }
            Modal::Name {
                purpose,
                text,
                error,
            } => {
                let heading = match purpose {
                    NameFor::NewRequest(_) => "New request",
                    NameFor::NewFolder(_) => "New folder",
                    NameFor::Rename(_) => "Rename",
                    NameFor::SaveAs(_) => "Save request as",
                    NameFor::NewEnv(_) => "New environment",
                };
                dialog::name(heading, text, error.as_deref()).map(Message::Dialog)
            }
            Modal::Delete(path) => {
                let what = if path.is_dir() {
                    format!(
                        "Move the folder \"{}\" and everything in it to the Trash?",
                        display_name(path)
                    )
                } else {
                    format!("Move \"{}\" to the Trash?", display_name(path))
                };
                dialog::delete(what).map(Message::Dialog)
            }
        }
    }
}

/// Window-level input: the pointer position (for context menus), Cmd+S
/// saves, Cmd+T and Cmd+W open and close tabs, Cmd+Enter sends, and Escape
/// closes a menu or dialog.
fn shortcut(event: iced::Event, _status: event::Status, _id: window::Id) -> Option<Message> {
    match event {
        iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
            Some(Message::CursorMoved(position))
        }
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
            Some(Message::MouseReleased)
        }
        iced::Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
            match key.as_ref() {
                keyboard::Key::Named(keyboard::key::Named::Escape) => {
                    Some(Message::Dialog(dialog::Msg::Cancel))
                }
                keyboard::Key::Character("s") if modifiers.command() => Some(Message::Save),
                keyboard::Key::Character("t") if modifiers.command() => Some(Message::NewTab),
                keyboard::Key::Character("w") if modifiers.command() => Some(Message::CloseTab),
                keyboard::Key::Named(keyboard::key::Named::Enter) if modifiers.command() => {
                    Some(Message::Send)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// A small card with the dragged item's name, drawn just below and to the
/// right of the pointer. It takes no input, so it never blocks the rows.
fn drag_ghost_layer(name: String, at: Point) -> Element<'static, Message> {
    let card = container(text(name).size(theme::TEXT))
        .padding([4, 10])
        .style(theme::card);
    container(card)
        .padding(iced::Padding {
            top: at.y + 12.0,
            right: 0.0,
            bottom: 0.0,
            left: at.x + 12.0,
        })
        .width(Fill)
        .height(Fill)
        .into()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_state_file_has_no_tabs() {
        let old = r#"{"collection": "/work/api", "env": "dev"}"#;
        let state: SavedState = serde_json::from_str(old).unwrap();
        assert_eq!(state.collection, Some(PathBuf::from("/work/api")));
        assert!(state.tabs.is_empty());
        assert_eq!(state.active_tab, 0);
    }

    #[test]
    fn tabs_round_trip() {
        let state = SavedState {
            collection: Some(PathBuf::from("/work/api")),
            env: None,
            tabs: vec![
                PathBuf::from("/work/api/a.json"),
                PathBuf::from("/work/api/b.json"),
            ],
            active_tab: 1,
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: SavedState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, state);
    }
}
