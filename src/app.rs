//! Top-level app state, messages, update and frame layout. Widgets live in
//! `ui/`; this module decides what happens when they report something.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use iced::keyboard;
use iced::mouse;
use iced::theme::Mode;
use iced::widget::pane_grid::{self, Axis, Configuration, Content, PaneGrid};
use iced::widget::{
    column, container, mouse_area, row, rule, scrollable, space, stack, text, text_editor,
};
use iced::{Element, Fill, FillPortion, Point, Subscription, Task, Theme, event, window};
use serde::{Deserialize, Serialize};

use crate::browser;
use crate::collection::{self, Node};
use crate::export::postman;
use crate::history::History;
use crate::menu;
use crate::model::{Environment, KeyValue, RequestFile, to_json};
use crate::recents;
use crate::settings::{self, Settings};
use crate::spark::{self, Series};
use crate::tabs::Tabs;
use crate::ui::tab_strip;
use crate::ui::{dialog, editor, kv, response, sidebar, theme};
use crate::{curl, http, import, resolve, samples, seed};

const NOTICE_TIME: Duration = Duration::from_secs(4);
/// True where the native menu bar exists and owns the common shortcuts.
const MENU_BAR: bool = cfg!(target_os = "macos");
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
    /// Recently used collections, newest first.
    #[serde(default)]
    recents: Vec<PathBuf>,
}

/// The user's home folder: `HOME` on macOS and Linux, `USERPROFILE` on
/// Windows (which has no `HOME`).
fn home_dir() -> Option<PathBuf> {
    std::env::home_dir().filter(|p| !p.as_os_str().is_empty())
}

/// `~/.config/beeline/state.json`, or None when there is no home folder.
fn state_path() -> Option<PathBuf> {
    Some(home_dir()?.join(".config/beeline/state.json"))
}

fn load_state() -> SavedState {
    state_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// `~/.config/beeline/settings.json`, or None when there is no home folder.
fn settings_path() -> Option<PathBuf> {
    Some(home_dir()?.join(".config/beeline/settings.json"))
}

/// The saved settings, or the defaults when there is no file or it is
/// unreadable.
pub fn load_settings() -> Settings {
    settings_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .map(|s| Settings::from_json(&s))
        .unwrap_or_default()
}

fn save_settings(settings: &Settings) -> Result<(), String> {
    let path = settings_path().ok_or("No home folder to save settings in")?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
    }
    fs::write(&path, settings.to_json()).map_err(|e| format!("Cannot save settings: {e}"))
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
    /// What the response pane shows: the newest send, or one picked from
    /// the history.
    result: Option<Result<response::View, String>>,
    /// This tab's sends, for this session only. Requests that fail to
    /// resolve, or are cancelled, never went out and are not kept.
    history: History<response::Sent>,
    /// The history number of the send `result` shows.
    shown: Option<u64>,
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
            history: History::default(),
            shown: None,
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
    NewCollection(PathBuf),
    RenameCollection(PathBuf),
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
    /// The curl preview: the exact command for the active request.
    Curl(String),
    /// The API docs dialog: a page or spec URL, and the fetch in progress.
    ImportDocs {
        url: String,
        busy: bool,
        error: Option<String>,
    },
    /// The curl paste dialog. Its text lives in `App::curl_paste`.
    ImportCurl {
        error: Option<String>,
    },
    /// The settings form. `timeout` is the text being typed; `draft` holds
    /// the other choices until Save.
    Settings {
        draft: Settings,
        timeout: String,
        error: Option<String>,
    },
}

pub struct App {
    panes: pane_grid::State<PaneKind>,
    collection: Option<Collection>,
    /// The open requests, one tab each.
    docs: Tabs<Doc>,
    /// The id the next tab gets. Responses find their tab by id.
    next_doc: u64,
    modal: Option<Modal>,
    settings: Settings,
    /// Recently used collections, newest first.
    recents: Vec<PathBuf>,
    /// The collection menu, open at this point under the collection name.
    collection_menu: Option<Point>,
    /// The ⋯ menu beside the collection name: the open collection's actions.
    actions_menu: Option<Point>,
    /// The environment menu, open at this point under the environment name.
    env_menu: Option<Point>,
    /// The "+" menu beside REQUESTS: new request or folder.
    new_menu: Option<Point>,
    /// Response times per request file, for the sidebar sparklines. Kept
    /// for this session only; never saved.
    sparks: HashMap<PathBuf, Series>,
    /// When each request's newest response arrived, to ease its point in.
    arrived: HashMap<PathBuf, Instant>,
    /// When the app started; animations measure their phase from it.
    epoch: Instant,
    /// The text of the curl paste dialog. iced edits it in place, so it
    /// lives here rather than in the modal.
    curl_paste: text_editor::Content,
    /// The macOS menu bar, kept alive while the app runs.
    menu_bar: menu::Bar,
    /// The operating system's light or dark mode, for the System theme.
    system_mode: Mode,
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
    SystemTheme(Mode),
    /// A redraw tick while a sparkline animates. Nothing to update; the
    /// view reads the clock.
    Frame,
    /// Installs the menu bar once the app has finished launching.
    InstallMenu,
    /// A menu bar item was chosen.
    Menu(menu::Command),
    NewTab,
    CloseTab,
    Send,
    Save,
    /// Show the active request, resolved with the current environment, as
    /// a curl command, ready to copy.
    ShowCurl,
    Sent(u64, u64, response::Sent),
    /// The API docs fetch finished: the spec it found, or why it failed.
    DocsFetched(Result<import::Imported, String>),
    CloseRequested(window::Id),
    CursorMoved(Point),
    MouseReleased,
    CloseContext,
    PaneResized(pane_grid::ResizeEvent),
}

impl App {
    /// Opens `start` if given, otherwise the last collection used.
    pub fn boot(start: Option<PathBuf>, settings: Settings) -> (Self, Task<Message>) {
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
            settings,
            recents: Vec::new(),
            collection_menu: None,
            actions_menu: None,
            env_menu: None,
            new_menu: None,
            sparks: HashMap::new(),
            arrived: HashMap::new(),
            epoch: Instant::now(),
            curl_paste: text_editor::Content::new(),
            menu_bar: menu::Bar::default(),
            system_mode: Mode::None,
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
        app.recents = saved.recents.clone();
        recents::prune(&mut app.recents, |p| p.is_dir());
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
        let tasks = Task::batch([
            iced::system::theme().map(Message::SystemTheme),
            Task::done(Message::InstallMenu),
        ]);
        (app, tasks)
    }

    /// The app theme for the current settings.
    pub fn theme(&self) -> Theme {
        theme::app_theme(self.settings.theme, self.system_mode)
    }

    fn open_settings(&mut self) {
        self.modal = Some(Modal::Settings {
            draft: self.settings.clone(),
            timeout: self.settings.request_timeout_secs.to_string(),
            error: None,
        });
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
        let Some(base) = self.collections_dir() else {
            return;
        };
        let root = base.join("Default");
        match seed::write_examples(&root) {
            Ok(()) => self.open_collection(root),
            Err(e) => self.notify(e, true),
        }
    }

    /// The folder collections are created in, from the settings.
    fn collections_dir(&self) -> Option<PathBuf> {
        Some(self.settings.collections_dir(&home_dir()?))
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
            recents: self.recents.clone(),
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
                recents::touch(&mut self.recents, &root);
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
            NameFor::NewCollection(parent) => {
                let root = collection::create_folder(parent, text)?;
                self.open_collection(root);
                Ok(Task::none())
            }
            NameFor::RenameCollection(root) => {
                let new = collection::rename(root, text)?;
                recents::retarget(&mut self.recents, root, &new);
                self.retarget(root, &new);
                if let Some(c) = &mut self.collection {
                    c.root = new.clone();
                }
                self.reload_tree();
                self.save_state();
                self.notify("Renamed", false);
                Ok(Task::none())
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
        // An environment file is deleted by path, so note whether it is the
        // selected one before the file is gone.
        let selected_env_gone = self
            .env
            .clone()
            .filter(|n| self.env_path(n).as_deref() == Some(path));
        if let Err(e) = collection::delete(path) {
            self.notify(e, true);
            return;
        }
        if selected_env_gone.is_some() {
            self.reload_tree();
            self.select_env(None);
            self.notify("Moved to Trash", false);
            return;
        }
        recents::forget(&mut self.recents, path);
        // The current collection is gone: clear everything that belonged to it.
        if self.collection.as_ref().is_some_and(|c| c.root == path) {
            self.collection = None;
            self.env = None;
            self.vars.clear();
            self.env_editor = None;
            self.docs.clear();
            self.expanded.clear();
            self.save_state();
            self.notify("Moved to Trash", false);
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
        // Sparklines follow their request to its new path.
        let keys: Vec<PathBuf> = self
            .sparks
            .keys()
            .chain(self.arrived.keys())
            .cloned()
            .collect();
        for key in keys {
            if let Some(new) = collection::moved(&key, from, to) {
                if let Some(series) = self.sparks.remove(&key) {
                    self.sparks.insert(new.clone(), series);
                }
                if let Some(at) = self.arrived.remove(&key) {
                    self.arrived.insert(new, at);
                }
            }
        }
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
        if !matches!(
            action,
            Action::Enter(_)
                | Action::Exit(_)
                | Action::ToggleCollectionMenu
                | Action::ToggleActionsMenu
                | Action::ToggleEnvMenu
                | Action::ToggleNewMenu
        ) {
            self.context = None;
            self.collection_menu = None;
            self.actions_menu = None;
            self.env_menu = None;
            self.new_menu = None;
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
            Action::Settings => self.open_settings(),
            Action::ToggleCollectionMenu => {
                self.collection_menu = match self.collection_menu {
                    Some(_) => None,
                    None => Some(self.cursor),
                };
            }
            Action::ToggleActionsMenu => {
                self.actions_menu = match self.actions_menu {
                    Some(_) => None,
                    None => Some(self.cursor),
                };
            }
            Action::ToggleEnvMenu => {
                self.env_menu = match self.env_menu {
                    Some(_) => None,
                    None => Some(self.cursor),
                };
            }
            Action::Trigger(path) => {
                // The first click of the double-click already opened the tab;
                // send only if it is the active one, so a failed open sends nothing.
                self.open_tab(path.clone());
                if self.doc().and_then(|d| d.open.as_ref()) == Some(&path) {
                    return self.send();
                }
            }
            Action::ImportFile => {
                if let Some(c) = &self.collection {
                    let root = c.root.clone();
                    let picked = rfd::FileDialog::new()
                        .set_title("Import an OpenAPI, Postman, Insomnia or HAR file")
                        .add_filter("API file", &["json", "yaml", "yml", "har"])
                        .pick_file();
                    if let Some(file) = picked {
                        self.import_file(&root, &file);
                    }
                }
            }
            Action::ImportBruno => {
                if let Some(c) = &self.collection {
                    let root = c.root.clone();
                    let picked = rfd::FileDialog::new()
                        .set_title("Import a Bruno collection folder")
                        .pick_folder();
                    if let Some(dir) = picked {
                        let result = import::read_bruno(&dir);
                        self.finish_import(&root, result);
                    }
                }
            }
            Action::ExportPostman => {
                if let Some(c) = &self.collection {
                    let root = c.root.clone();
                    let name = collection::stem(&root);
                    let picked = rfd::FileDialog::new()
                        .set_title("Export as a Postman collection")
                        .set_file_name(format!("{name}.postman_collection.json"))
                        .add_filter("Postman collection", &["json"])
                        .save_file();
                    if let Some(file) = picked {
                        self.export_postman(&root, &file);
                    }
                }
            }
            Action::ImportDocs => {
                self.modal = Some(Modal::ImportDocs {
                    url: String::new(),
                    busy: false,
                    error: None,
                });
            }
            Action::ImportCurl => {
                self.curl_paste = text_editor::Content::new();
                self.modal = Some(Modal::ImportCurl { error: None });
            }
            Action::ToggleNewMenu => {
                self.new_menu = match self.new_menu {
                    Some(_) => None,
                    None => Some(self.cursor),
                };
            }
            Action::DeleteEnv(path) => self.modal = Some(Modal::Delete(path)),
            Action::SwitchCollection(root) => {
                if self.collection.as_ref().is_none_or(|c| c.root != root) {
                    return self.guard(Then::OpenCollection(root));
                }
            }
            // New collections always go in the collections folder, so they
            // never end up nested inside another collection.
            Action::NewCollection => match self.collections_dir() {
                Some(base) => match fs::create_dir_all(&base) {
                    Ok(()) => self.open_name(NameFor::NewCollection(base), ""),
                    Err(e) => {
                        self.notify(format!("Could not create {}: {e}", base.display()), true)
                    }
                },
                None => self.notify("No home folder to keep collections in", true),
            },
            Action::RenameCollection(root) => {
                let current = display_name(&root);
                self.open_name(NameFor::RenameCollection(root), &current);
            }
            Action::DeleteCollection(root) => self.modal = Some(Modal::Delete(root)),
            Action::AddSamples(dir) => match samples::write(&dir) {
                Ok(folder) => {
                    self.expanded.insert(dir);
                    self.reload_tree();
                    self.notify(
                        format!("Added samples to \"{}\"", collection::stem(&folder)),
                        false,
                    );
                }
                Err(e) => self.notify(e, true),
            },
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
        let timeout = self.settings.timeout();
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
        let (method, url) = (resolved.method.clone(), resolved.url.clone());
        // A plain thread keeps the blocking HTTP call off the UI executor.
        // The oneshot channel works with any executor.
        let (tx, rx) = iced::futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(http::send(&resolved, timeout));
        });
        Task::perform(
            async move {
                rx.await
                    .unwrap_or_else(|_| Err("request thread stopped".into()))
            },
            move |result| {
                Message::Sent(
                    id,
                    generation,
                    response::Sent {
                        method,
                        url,
                        result,
                    },
                )
            },
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
            Message::ShowCurl => {
                match self.doc().map(|d| resolve::resolve(&d.req, &self.vars)) {
                    Some(Ok(resolved)) => {
                        self.modal = Some(Modal::Curl(curl::command(&resolved)));
                    }
                    Some(Err(e)) => self.notify(e, true),
                    None => {}
                }
                Task::none()
            }
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
                self.collection_menu = None;
                self.actions_menu = None;
                self.env_menu = None;
                self.new_menu = None;
                Task::none()
            }
            Message::Tabs(msg) => self.tab_msg(msg),
            Message::Frame => Task::none(),
            Message::InstallMenu => {
                match menu::install() {
                    Ok(bar) => self.menu_bar = bar,
                    Err(e) => self.notify(e, true),
                }
                Task::none()
            }
            Message::Menu(command) => self.menu_command(command),
            Message::SystemTheme(mode) => {
                self.system_mode = mode;
                Task::none()
            }
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
            Message::DocsFetched(result) => {
                self.docs_fetched(result);
                Task::none()
            }
            Message::Sent(id, generation, sent) => {
                // A tab closed or cancelled since the send drops its response.
                if let Some(doc) = self.docs.iter_mut().find(|d| d.id == id)
                    && doc.generation == generation
                {
                    // A response with a status adds a point to the request's
                    // sparkline. A transport error has no time to show.
                    if let (Ok(resp), Some(path)) = (&sent.result, &doc.open) {
                        self.sparks
                            .entry(path.clone())
                            .or_default()
                            .push(spark::Sample {
                                ms: resp.elapsed.as_secs_f32() * 1000.0,
                                ok: resp.status < 400,
                            });
                        self.arrived.insert(path.clone(), Instant::now());
                    }
                    doc.pending = false;
                    doc.result = Some(sent.result.clone().map(response::View::new));
                    doc.shown = Some(doc.history.push(sent));
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

    /// Imports an API file into a new folder in the collection, then shows
    /// the folder open in the tree.
    fn import_file(&mut self, root: &Path, file: &Path) {
        let result = fs::read_to_string(file)
            .map_err(|e| format!("Could not read {}: {e}", file.display()))
            .and_then(|text| import::parse_file(&text));
        self.finish_import(root, result);
    }

    /// Writes the open collection, read fresh from disk, to `file` as a
    /// Postman v2.1 collection.
    fn export_postman(&mut self, root: &Path, file: &Path) {
        let result = collection::scan(root).and_then(|nodes| {
            let entries = collection::read_entries(&nodes)?;
            let doc = postman::collection(&collection::stem(root), &entries);
            collection::write_export(file, &to_json(&doc))?;
            Ok(collection::request_count(&nodes))
        });
        match result {
            Ok(count) => {
                let noun = if count == 1 { "request" } else { "requests" };
                self.notify(
                    format!("Exported {count} {noun} to \"{}\"", collection::stem(file)),
                    false,
                );
            }
            Err(e) => self.notify(e, true),
        }
    }

    /// Fetches the docs URL in the dialog on a plain thread, so the blocking
    /// HTTP calls stay off the UI executor. A second press while a fetch is
    /// running does nothing.
    fn start_docs_import(&mut self) -> Task<Message> {
        let timeout = self.settings.timeout();
        let Some(Modal::ImportDocs { url, busy, error }) = &mut self.modal else {
            return Task::none();
        };
        if *busy || url.trim().is_empty() {
            return Task::none();
        }
        *busy = true;
        *error = None;
        let url = url.trim().to_string();
        let (tx, rx) = iced::futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(import::docs::fetch(&url, timeout));
        });
        Task::perform(
            async move {
                rx.await
                    .unwrap_or_else(|_| Err("fetch thread stopped".into()))
            },
            Message::DocsFetched,
        )
    }

    /// Opens a prefilled GitHub issue for a docs URL that did not import. The
    /// issue holds the URL and the reason only.
    fn report_docs_failure(&mut self) {
        let Some(Modal::ImportDocs {
            url,
            error: Some(reason),
            ..
        }) = &self.modal
        else {
            return;
        };
        let (title, body) = import::docs::report_text(url, reason);
        if let Err(e) = browser::open(&browser::new_issue_url(&title, &body)) {
            self.notify(e, true);
        }
    }

    /// Imports the spec a docs fetch found, or shows why it found none.
    fn docs_fetched(&mut self, result: Result<import::Imported, String>) {
        match result {
            Ok(imported) => {
                self.modal = None;
                if let Some(root) = self.collection.as_ref().map(|c| c.root.clone()) {
                    self.finish_import(&root, Ok(imported));
                }
            }
            Err(e) => {
                if let Some(Modal::ImportDocs { busy, error, .. }) = &mut self.modal {
                    *busy = false;
                    *error = Some(e);
                }
            }
        }
    }

    /// Writes a parsed import into a new folder and shows it open in the tree.
    fn finish_import(&mut self, root: &Path, parsed: Result<import::Imported, String>) {
        match parsed.and_then(|imported| import::write(root, &imported)) {
            Ok((folder, count)) => {
                self.expanded.insert(folder.clone());
                self.reload_tree();
                let noun = if count == 1 { "request" } else { "requests" };
                self.notify(
                    format!(
                        "Imported {count} {noun} into \"{}\"",
                        collection::stem(&folder)
                    ),
                    false,
                );
            }
            Err(e) => self.notify(e, true),
        }
    }

    /// Turns the pasted curl command into a request at the collection's top
    /// level and opens it. A command that can't be read keeps the dialog
    /// open with the reason.
    fn import_curl(&mut self) {
        let Some(root) = self.collection.as_ref().map(|c| c.root.clone()) else {
            return;
        };
        let result = import::curl::parse(&self.curl_paste.text())
            .and_then(|(name, req)| import::write_one(&root, &name, &req));
        match result {
            Ok(path) => {
                self.modal = None;
                self.reload_tree();
                self.notify(format!("Imported \"{}\"", collection::stem(&path)), false);
                self.open_tab(path);
            }
            Err(e) => {
                if let Some(Modal::ImportCurl { error }) = &mut self.modal {
                    *error = Some(e);
                }
            }
        }
    }

    /// Checks the settings form, saves it and applies it. The form stays
    /// open, with an error, when the timeout is not a number in range.
    fn settings_save(&mut self) {
        let Some(Modal::Settings {
            draft,
            timeout,
            error,
        }) = &mut self.modal
        else {
            return;
        };
        let Ok(secs) = timeout.trim().parse::<u64>() else {
            *error = Some("Enter a whole number of seconds".into());
            return;
        };
        if !settings::TIMEOUT_SECS.contains(&secs) {
            *error = Some("The timeout must be from 1 to 600 seconds".into());
            return;
        }
        if let Some(problem) = Settings::collections_dir_problem(&draft.collections_dir) {
            *error = Some(problem.into());
            return;
        }
        draft.request_timeout_secs = secs;
        let saved = draft.clone().clamped();
        match save_settings(&saved) {
            Ok(()) => {
                theme::use_settings(&saved);
                self.settings = saved;
                self.modal = None;
                self.notify("Settings saved", false);
            }
            Err(e) => *error = Some(e),
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
            response::Msg::Show(n) => {
                if let Some(sent) = doc.history.get(n) {
                    doc.result = Some(sent.result.clone().map(response::View::new));
                    doc.shown = Some(n);
                }
                Task::none()
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
            dialog::Msg::SettingsTheme(choice) => {
                if let Some(Modal::Settings { draft, .. }) = &mut self.modal {
                    draft.theme = choice;
                }
                Task::none()
            }
            dialog::Msg::SettingsSans(font) => {
                if let Some(Modal::Settings { draft, .. }) = &mut self.modal {
                    draft.sans = font;
                }
                Task::none()
            }
            dialog::Msg::SettingsMono(font) => {
                if let Some(Modal::Settings { draft, .. }) = &mut self.modal {
                    draft.mono = font;
                }
                Task::none()
            }
            dialog::Msg::SettingsSize(size) => {
                if let Some(Modal::Settings { draft, .. }) = &mut self.modal {
                    draft.text_size = size;
                }
                Task::none()
            }
            dialog::Msg::SettingsCollectionsDir(text) => {
                if let Some(Modal::Settings { draft, error, .. }) = &mut self.modal {
                    draft.collections_dir = text;
                    *error = None;
                }
                Task::none()
            }
            dialog::Msg::SettingsTimeout(text) => {
                if let Some(Modal::Settings { timeout, error, .. }) = &mut self.modal {
                    *timeout = text;
                    *error = None;
                }
                Task::none()
            }
            dialog::Msg::CurlEdit(action) => {
                self.curl_paste.perform(action);
                if let Some(Modal::ImportCurl { error }) = &mut self.modal {
                    *error = None;
                }
                Task::none()
            }
            dialog::Msg::CurlCopy => {
                let Some(Modal::Curl(command)) = self.modal.take() else {
                    return Task::none();
                };
                self.notify("Copied as cURL", false);
                iced::clipboard::write(command)
            }
            dialog::Msg::CurlImport => {
                self.import_curl();
                Task::none()
            }
            dialog::Msg::DocsUrl(text) => {
                if let Some(Modal::ImportDocs { url, .. }) = &mut self.modal {
                    *url = text;
                }
                Task::none()
            }
            dialog::Msg::DocsImport => self.start_docs_import(),
            dialog::Msg::DocsReport => {
                self.report_docs_failure();
                Task::none()
            }
            dialog::Msg::SettingsSave => {
                self.settings_save();
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
            iced::system::theme_changes().map(Message::SystemTheme),
            menu::commands().map(Message::Menu),
            if self.animating() {
                window::frames().map(|_| Message::Frame)
            } else {
                Subscription::none()
            },
        ])
    }

    /// Runs a menu bar command through the same paths as the in-app controls.
    fn menu_command(&mut self, command: menu::Command) -> Task<Message> {
        use menu::Command;
        use sidebar::Action;
        let root = self.collection.as_ref().map(|c| c.root.clone());
        let needs_collection = |app: &mut Self| {
            app.notify("Open a collection first", true);
            Task::none()
        };
        match command {
            Command::Settings => {
                self.open_settings();
                Task::none()
            }
            Command::NewRequest => self.update(Message::NewTab),
            Command::Save => self.update(Message::Save),
            Command::CloseTab => self.update(Message::CloseTab),
            Command::NewCollection => self.sidebar_action(Action::NewCollection),
            Command::OpenCollection => self.sidebar_action(Action::OpenCollection),
            Command::NewFolder => match root {
                Some(root) => self.sidebar_action(Action::NewFolder(root)),
                None => needs_collection(self),
            },
            Command::ImportCurl
            | Command::ImportFile
            | Command::ImportBruno
            | Command::ImportDocs
            | Command::ExportPostman
            | Command::Reload
                if root.is_none() =>
            {
                needs_collection(self)
            }
            Command::ImportCurl => self.sidebar_action(Action::ImportCurl),
            Command::ImportFile => self.sidebar_action(Action::ImportFile),
            Command::ImportBruno => self.sidebar_action(Action::ImportBruno),
            Command::ImportDocs => self.sidebar_action(Action::ImportDocs),
            Command::ExportPostman => self.sidebar_action(Action::ExportPostman),
            Command::Reload => self.sidebar_action(Action::Reload),
            Command::RenameCollection => match root {
                Some(root) => self.sidebar_action(Action::RenameCollection(root)),
                None => needs_collection(self),
            },
            Command::TrashCollection => match root {
                Some(root) => self.sidebar_action(Action::DeleteCollection(root)),
                None => needs_collection(self),
            },
        }
    }

    /// True while a sparkline moves: a point easing in, or a send in flight.
    fn animating(&self) -> bool {
        self.docs.iter().any(|d| d.pending && d.open.is_some())
            || self
                .arrived
                .values()
                .any(|at| at.elapsed() < Duration::from_millis(400))
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

        let notice = self
            .notice
            .as_ref()
            .filter(|n| n.at.elapsed() < NOTICE_TIME)
            .map(|n| {
                let t = text(n.text.as_str()).size(theme::small_px());
                if n.error { t.style(text::danger) } else { t }
            });
        // The collection path is shown at the top of the sidebar.
        let mut status_row = row![space::horizontal().width(Fill)].spacing(8);
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
        if let Some(at) = self.collection_menu {
            layers.push(self.collection_menu_view(at));
        }
        if let Some(at) = self.actions_menu {
            layers.push(self.actions_menu_view(at));
        }
        if let Some(at) = self.env_menu {
            layers.push(self.env_menu_view(at));
        }
        if let Some(at) = self.new_menu {
            layers.push(self.new_menu_view(at));
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
            env: self.env.as_deref(),
            expanded: &self.expanded,
            drop: self.drop_folder(),
            dragging: self.drag.as_deref(),
            hovered: self.drop_target.as_deref(),
            env_file: self.env.as_deref().and_then(|n| self.env_path(n)),
            sparks: &self.sparks,
            arrived: &self.arrived,
            pending: self
                .docs
                .iter()
                .filter(|d| d.pending)
                .filter_map(|d| d.open.as_deref())
                .collect(),
            now: Instant::now(),
            epoch: self.epoch,
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
                    .size(theme::text_px());
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
            text(self.title()).size(theme::heading_px()),
            if dirty {
                text("•").size(theme::heading_px())
            } else {
                text("")
            },
            space::horizontal().width(Fill),
            theme::button(text("cURL…").size(theme::text_px())).on_press(Message::ShowCurl),
            theme::button(text("Save").size(theme::text_px()))
                .on_press_maybe(dirty.then_some(Message::Save)),
        ]
        .spacing(8);
        let request = column![
            header,
            strip,
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
            text("Sending…").size(theme::text_px()).into()
        } else {
            match &doc.result {
                Some(Ok(view)) => {
                    response::view(view, &doc.history, doc.shown).map(Message::Response)
                }
                // An error has no tabs; earlier sends are listed below it.
                Some(Err(err)) if !doc.history.is_empty() => column![
                    text(err.as_str()).style(text::danger),
                    text(format!("History ({})", doc.history.len())).size(theme::small_px()),
                    response::history_list(&doc.history, doc.shown).map(Message::Response),
                ]
                .spacing(8)
                .into(),
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

    /// The menu under the collection name chooses a collection: recents,
    /// then Open and New. The open collection's actions are in the ⋯ menu.
    fn collection_menu_view(&self, at: Point) -> Element<'_, Message> {
        let item = menu_item;
        let current = self.collection.as_ref().map(|c| c.root.as_path());
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        if let Some(root) = current {
            items.push(
                text(root.display().to_string())
                    .size(theme::small_px())
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                    .into(),
            );
            items.push(text("").into());
        }
        for recent in &self.recents {
            let mark = if Some(recent.as_path()) == current {
                "✓ "
            } else {
                ""
            };
            items.push(item(
                format!("{mark}{}", collection::stem(recent)),
                sidebar::Action::SwitchCollection(recent.clone()),
            ));
        }
        // On macOS Open and New live in the File menu, so this menu only
        // chooses a collection.
        if cfg!(target_os = "macos") {
            return popup(items, at);
        }
        if !self.recents.is_empty() {
            items.push(text("").into());
        }
        items.push(item(
            "Open collection…".into(),
            sidebar::Action::OpenCollection,
        ));
        items.push(item(
            "New collection…".into(),
            sidebar::Action::NewCollection,
        ));
        popup(items, at)
    }

    /// The ⋯ menu beside the collection name: the open collection's actions.
    fn actions_menu_view(&self, at: Point) -> Element<'_, Message> {
        let item = menu_item;
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        if let Some(c) = &self.collection {
            let root = c.root.as_path();
            items.push(item("Reload".into(), sidebar::Action::Reload));
            items.push(item("Import cURL…".into(), sidebar::Action::ImportCurl));
            items.push(item(
                "Import OpenAPI or Postman file…".into(),
                sidebar::Action::ImportFile,
            ));
            items.push(item(
                "Import Bruno folder…".into(),
                sidebar::Action::ImportBruno,
            ));
            items.push(item(
                "Export as Postman collection…".into(),
                sidebar::Action::ExportPostman,
            ));
            items.push(item(
                "Import from API docs URL…".into(),
                sidebar::Action::ImportDocs,
            ));
            items.push(item(
                "Rename…".into(),
                sidebar::Action::RenameCollection(root.to_path_buf()),
            ));
            items.push(item(
                "Move to Trash…".into(),
                sidebar::Action::DeleteCollection(root.to_path_buf()),
            ));
        }
        popup(items, at)
    }

    /// The "+" menu beside REQUESTS: new items at the collection's top level.
    fn new_menu_view(&self, at: Point) -> Element<'_, Message> {
        let Some(c) = &self.collection else {
            return text("").into();
        };
        let item = |label: &'static str, action: sidebar::Action| -> Element<'_, Message> {
            theme::button(text(label).size(theme::text_px()))
                .style(theme::text_button)
                .width(Fill)
                .on_press(Message::Sidebar(action))
                .into()
        };
        popup(
            vec![
                item("New request…", sidebar::Action::NewRequest(c.root.clone())),
                item("New folder…", sidebar::Action::NewFolder(c.root.clone())),
            ],
            at,
        )
    }

    /// The menu under the environment name, laid out like the collection
    /// menu: the choices first, then the actions for the selected one.
    fn env_menu_view(&self, at: Point) -> Element<'_, Message> {
        let item = |label: String, action: sidebar::Action| -> Element<'_, Message> {
            theme::button(
                text(label)
                    .size(theme::text_px())
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            )
            .style(theme::text_button)
            .width(Fill)
            .on_press(Message::Sidebar(action))
            .into()
        };
        let mut items: Vec<Element<'_, Message>> = Vec::new();
        let none_mark = if self.env.is_none() { "✓ " } else { "" };
        items.push(item(
            format!("{none_mark}No environment"),
            sidebar::Action::SelectEnv(None),
        ));
        if let Some(c) = &self.collection {
            for (name, _) in &c.envs {
                let mark = if self.env.as_deref() == Some(name.as_str()) {
                    "✓ "
                } else {
                    ""
                };
                items.push(item(
                    format!("{mark}{name}"),
                    sidebar::Action::SelectEnv(Some(name.clone())),
                ));
            }
        }
        // Adding and removing live on the ENVIRONMENT label's + and −, so the
        // menu only chooses.
        popup(items, at)
    }

    fn context_menu(&self, path: &Path, at: Point) -> Element<'_, Message> {
        let item = |label: &'static str, action: sidebar::Action| -> Element<'_, Message> {
            theme::button(
                text(label)
                    .size(theme::text_px())
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
            )
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
                item(
                    "Add sample requests",
                    sidebar::Action::AddSamples(path.to_path_buf()),
                ),
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
                    NameFor::NewCollection(_) => "New collection",
                    NameFor::RenameCollection(_) => "Rename collection",
                };
                dialog::name(heading, text, error.as_deref()).map(Message::Dialog)
            }
            Modal::Settings {
                draft,
                timeout,
                error,
            } => dialog::settings(draft, timeout, error.as_deref()).map(Message::Dialog),
            Modal::Curl(command) => dialog::curl_preview(command).map(Message::Dialog),
            Modal::ImportDocs { url, busy, error } => {
                dialog::import_docs(url, *busy, error.as_deref()).map(Message::Dialog)
            }
            Modal::ImportCurl { error } => {
                dialog::import_curl(&self.curl_paste, error.as_deref()).map(Message::Dialog)
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
/// saves, Cmd+T (or Cmd+N) and Cmd+W open and close tabs, Cmd+Enter sends, and Escape
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
                // On macOS the menu bar owns ⌘S and ⌘W; handling them here too
                // would run them twice.
                keyboard::Key::Character("s") if modifiers.command() && !MENU_BAR => {
                    Some(Message::Save)
                }
                keyboard::Key::Character("t") if modifiers.command() => Some(Message::NewTab),
                // New Request: the menu bar's ⌘N on macOS.
                keyboard::Key::Character("n") if modifiers.command() && !MENU_BAR => {
                    Some(Message::NewTab)
                }
                keyboard::Key::Character("w") if modifiers.command() && !MENU_BAR => {
                    Some(Message::CloseTab)
                }
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
    let card = container(text(name).size(theme::text_px()))
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

/// One row of a sidebar menu.
fn menu_item<'a>(label: String, action: sidebar::Action) -> Element<'a, Message> {
    theme::button(
        text(label)
            .size(theme::text_px())
            .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
    )
    .style(theme::text_button)
    .width(Fill)
    .on_press(Message::Sidebar(action))
    .into()
}

/// A sidebar menu card, opened just below `at`. Pressing anywhere outside
/// it closes it. A menu taller than the room below `at` scrolls.
fn popup<'a>(items: Vec<Element<'a, Message>>, at: Point) -> Element<'a, Message> {
    let card = container(scrollable(column(items).spacing(2)).spacing(4))
        .style(theme::card)
        .padding(6)
        .width(240);
    let placed = container(card)
        .padding(iced::Padding {
            top: at.y + 12.0,
            right: 0.0,
            bottom: 12.0,
            left: 12.0,
        })
        .width(Fill)
        .height(Fill);
    mouse_area(placed)
        .on_press(Message::CloseContext)
        .on_right_press(Message::CloseContext)
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
            recents: vec![PathBuf::from("/work/api")],
        };
        let json = serde_json::to_string(&state).unwrap();
        let back: SavedState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, state);
    }
}
