//! The collection sidebar: open a folder, browse the request tree, and
//! ask for file operations. It only reports actions; `App` performs them.

use std::path::{Path, PathBuf};

use eframe::egui;

use crate::collection::Node;

pub enum Action {
    OpenCollection,
    Reload,
    Open(PathBuf),
    NewRequest(PathBuf),
    NewFolder(PathBuf),
    Rename(PathBuf),
    Delete(PathBuf),
}

pub fn show(
    ui: &mut egui::Ui,
    root: Option<&Path>,
    tree: &[Node],
    selected: Option<&Path>,
) -> Vec<Action> {
    let mut actions = Vec::new();

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let title = root
            .and_then(|r| r.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "beeline".into());
        ui.heading(title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button("Open…")
                .on_hover_text("Open a folder as a collection")
                .clicked()
            {
                actions.push(Action::OpenCollection);
            }
        });
    });

    let Some(root) = root else {
        ui.add_space(8.0);
        ui.weak(
            "Open a folder to use it as a collection. Each request is a JSON \
             file; subfolders group them.",
        );
        return actions;
    };

    ui.horizontal(|ui| {
        if ui.button("+ Request").clicked() {
            actions.push(Action::NewRequest(root.to_path_buf()));
        }
        if ui.button("+ Folder").clicked() {
            actions.push(Action::NewFolder(root.to_path_buf()));
        }
        if ui.button("⟳").on_hover_text("Reload from disk").clicked() {
            actions.push(Action::Reload);
        }
    });
    ui.separator();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if tree.is_empty() {
                ui.weak("No requests yet.");
            }
            nodes(ui, tree, selected, &mut actions);
        });

    actions
}

fn nodes(ui: &mut egui::Ui, tree: &[Node], selected: Option<&Path>, actions: &mut Vec<Action>) {
    for node in tree {
        match node {
            Node::Folder {
                name,
                path,
                children,
            } => {
                let resp = egui::CollapsingHeader::new(name.as_str())
                    .id_salt(path)
                    .show(ui, |ui| {
                        if children.is_empty() {
                            ui.weak("Empty");
                        }
                        nodes(ui, children, selected, actions);
                    });
                resp.header_response.context_menu(|ui| {
                    if ui.button("New request").clicked() {
                        actions.push(Action::NewRequest(path.clone()));
                    }
                    if ui.button("New folder").clicked() {
                        actions.push(Action::NewFolder(path.clone()));
                    }
                    ui.separator();
                    if ui.button("Rename…").clicked() {
                        actions.push(Action::Rename(path.clone()));
                    }
                    if ui.button("Move to Trash…").clicked() {
                        actions.push(Action::Delete(path.clone()));
                    }
                });
            }
            Node::Request { name, path } => {
                let is_selected = selected == Some(path.as_path());
                let resp = ui.selectable_label(is_selected, name);
                if resp.clicked() {
                    actions.push(Action::Open(path.clone()));
                }
                resp.context_menu(|ui| {
                    if ui.button("Rename…").clicked() {
                        actions.push(Action::Rename(path.clone()));
                    }
                    if ui.button("Move to Trash…").clicked() {
                        actions.push(Action::Delete(path.clone()));
                    }
                });
            }
        }
    }
}
