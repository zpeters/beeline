//! Top-level app state and frame layout.

use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver};

use eframe::egui;

use crate::model::RequestFile;
use crate::ui::{editor, response};
use crate::{http, resolve};

pub struct App {
    req: RequestFile,
    tab: editor::Tab,
    /// Set while a request is in flight; the worker sends its result here.
    pending: Option<Receiver<Result<http::Response, String>>>,
    result: Option<Result<response::View, String>>,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            req: RequestFile::default(),
            tab: editor::Tab::default(),
            pending: None,
            result: None,
        }
    }

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
}

impl eframe::App for App {
    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some(rx) = &self.pending
            && let Ok(res) = rx.try_recv()
        {
            self.result = Some(res.map(response::View::new));
            self.pending = None;
        }

        let ctx = root.ctx().clone();
        let busy = self.pending.is_some();
        let mut send = root.input_mut(|i| {
            i.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::Enter,
            ))
        });

        egui::Panel::top("request")
            .resizable(true)
            .default_size(280.0)
            .show(root, |ui| {
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

        if send && !busy {
            self.send(&ctx);
        }
    }
}
