//! The macOS menu bar. Each item has a string id; a subscription turns
//! clicks (and their shortcuts) into [`Command`]s for `App::update`.
//! Other platforms get no menu bar; the in-app controls cover everything.
//!
//! There is no Edit menu, on purpose. muda's standard Edit items take ⌘A,
//! ⌘C and ⌘V as menu shortcuts and send them as AppKit actions (`copy:`,
//! `paste:`) to winit's view, which ignores them, so iced's text fields
//! never see the keys. Tried and confirmed by hand: with the Edit menu,
//! select all, copy and paste stop working; without it, they work. A real
//! Edit menu needs its items wired to iced's focused field instead.

use iced::Subscription;

/// What a menu item asks the app to do. Only the macOS menu bar sends
/// these, so elsewhere most of it goes unused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub enum Command {
    Settings,
    NewRequest,
    NewFolder,
    NewCollection,
    OpenCollection,
    ImportCurl,
    ImportFile,
    ImportBruno,
    Save,
    CloseTab,
    Reload,
    RenameCollection,
    TrashCollection,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
impl Command {
    const ALL: [Command; 13] = [
        Command::Settings,
        Command::NewRequest,
        Command::NewFolder,
        Command::NewCollection,
        Command::OpenCollection,
        Command::ImportCurl,
        Command::ImportFile,
        Command::ImportBruno,
        Command::Save,
        Command::CloseTab,
        Command::Reload,
        Command::RenameCollection,
        Command::TrashCollection,
    ];

    /// The menu item id for this command.
    pub fn id(self) -> &'static str {
        match self {
            Command::Settings => "settings",
            Command::NewRequest => "new-request",
            Command::NewFolder => "new-folder",
            Command::NewCollection => "new-collection",
            Command::OpenCollection => "open-collection",
            Command::ImportCurl => "import-curl",
            Command::ImportFile => "import-file",
            Command::ImportBruno => "import-bruno",
            Command::Save => "save",
            Command::CloseTab => "close-tab",
            Command::Reload => "reload",
            Command::RenameCollection => "rename-collection",
            Command::TrashCollection => "trash-collection",
        }
    }

    pub fn from_id(id: &str) -> Option<Command> {
        Command::ALL.into_iter().find(|c| c.id() == id)
    }
}

/// The installed menu bar. Dropping it would remove the menus, so `App`
/// keeps it for as long as it runs.
#[derive(Default)]
pub struct Bar {
    #[cfg(target_os = "macos")]
    _menu: Option<muda::Menu>,
}

/// Builds the menu bar and makes it the app's. Call once the app has
/// finished launching; before that, winit can replace it with its default.
pub fn install() -> Result<Bar, String> {
    #[cfg(target_os = "macos")]
    {
        let menu = mac::build().map_err(|e| format!("Could not build the menu bar: {e}"))?;
        menu.init_for_nsapp();
        Ok(Bar { _menu: Some(menu) })
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(Bar::default())
    }
}

/// Menu clicks as commands. Nothing on platforms without a menu bar.
pub fn commands() -> Subscription<Command> {
    #[cfg(target_os = "macos")]
    {
        Subscription::run(mac::events)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Subscription::none()
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use iced::futures::channel::mpsc;
    use iced::futures::{SinkExt, Stream, StreamExt};
    use muda::accelerator::Accelerator;
    use muda::{AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

    use super::Command;

    fn item(command: Command, label: &str, shortcut: Option<&str>) -> MenuItem {
        let accelerator = shortcut.and_then(|s| s.parse::<Accelerator>().ok());
        MenuItem::with_id(command.id(), label, true, accelerator)
    }

    pub fn build() -> muda::Result<Menu> {
        let about = PredefinedMenuItem::about(
            None,
            Some(AboutMetadata {
                name: Some("beeline".into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                ..AboutMetadata::default()
            }),
        );
        let app = Submenu::with_items(
            "beeline",
            true,
            &[
                &about,
                &PredefinedMenuItem::separator(),
                &item(Command::Settings, "Settings…", Some("CmdOrCtrl+Comma")),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::quit(None),
            ],
        )?;
        let import = Submenu::with_items(
            "Import",
            true,
            &[
                &item(Command::ImportCurl, "cURL Command…", None),
                &item(Command::ImportFile, "OpenAPI or Postman File…", None),
                &item(Command::ImportBruno, "Bruno Folder…", None),
            ],
        )?;
        let file = Submenu::with_items(
            "File",
            true,
            &[
                &item(Command::NewRequest, "New Request", Some("CmdOrCtrl+N")),
                &item(Command::NewFolder, "New Folder…", None),
                &item(
                    Command::NewCollection,
                    "New Collection…",
                    Some("CmdOrCtrl+Shift+N"),
                ),
                &PredefinedMenuItem::separator(),
                &item(
                    Command::OpenCollection,
                    "Open Collection…",
                    Some("CmdOrCtrl+O"),
                ),
                &import,
                &PredefinedMenuItem::separator(),
                &item(Command::Save, "Save", Some("CmdOrCtrl+S")),
                &item(Command::CloseTab, "Close Tab", Some("CmdOrCtrl+W")),
            ],
        )?;
        let collection = Submenu::with_items(
            "Collection",
            true,
            &[
                &item(Command::Reload, "Reload", Some("CmdOrCtrl+R")),
                &item(Command::RenameCollection, "Rename…", None),
                &PredefinedMenuItem::separator(),
                &item(Command::TrashCollection, "Move to Trash…", None),
            ],
        )?;
        let window = Submenu::with_items(
            "Window",
            true,
            &[
                &PredefinedMenuItem::minimize(None),
                &PredefinedMenuItem::maximize(None),
                &PredefinedMenuItem::fullscreen(None),
            ],
        )?;
        window.set_as_windows_menu_for_nsapp();
        Menu::with_items(&[&app, &file, &collection, &window])
    }

    /// Forwards menu clicks, which muda reports through a callback, into a
    /// stream the subscription can read.
    pub fn events() -> impl Stream<Item = Command> {
        iced::stream::channel(32, async |mut output: mpsc::Sender<Command>| {
            let (tx, mut rx) = mpsc::unbounded::<String>();
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                let _ = tx.unbounded_send(event.id.0.clone());
            }));
            while let Some(id) = rx.next().await {
                if let Some(command) = Command::from_id(&id) {
                    let _ = output.send(command).await;
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_round_trips_through_its_id() {
        for command in Command::ALL {
            assert_eq!(Command::from_id(command.id()), Some(command));
        }
        assert_eq!(Command::from_id("nope"), None);
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<&str> = Command::ALL.iter().map(|c| c.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), Command::ALL.len());
    }
}
