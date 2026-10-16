#![forbid(unsafe_code)]

mod request;

use std::sync::mpsc::{self, Receiver};

use eframe::egui;
use request::{Request, Response};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native("beeline", options, Box::new(|_cc| Ok(Box::new(App::new()))))
}

struct App {
    req: Request,
    /// Set while a request is in flight; the worker sends its result here.
    pending: Option<Receiver<Result<Response, String>>>,
    result: Option<Result<Response, String>>,
}

impl App {
    fn new() -> Self {
        Self {
            req: Request {
                method: "GET".into(),
                url: String::new(),
                body: String::new(),
            },
            pending: None,
            result: None,
        }
    }

    /// Run the request on a worker thread so the UI stays responsive.
    fn send(&mut self, ctx: &egui::Context) {
        let req = self.req.clone();
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(request::send(&req));
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
            self.result = Some(res);
            self.pending = None;
        }

        let ctx = root.ctx().clone();
        let mut send_clicked = false;
        egui::CentralPanel::default().show(root, |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("method")
                    .selected_text(&self.req.method)
                    .show_ui(ui, |ui| {
                        for m in request::METHODS {
                            ui.selectable_value(&mut self.req.method, m.to_string(), m);
                        }
                    });
                ui.add(
                    egui::TextEdit::singleline(&mut self.req.url)
                        .hint_text("https://example.com")
                        .desired_width(f32::INFINITY),
                );
                let busy = self.pending.is_some();
                send_clicked = ui.add_enabled(!busy, egui::Button::new("Send")).clicked();
            });

            ui.add(
                egui::TextEdit::multiline(&mut self.req.body)
                    .hint_text("Request body")
                    .desired_rows(6)
                    .desired_width(f32::INFINITY),
            );
            ui.separator();

            if self.pending.is_some() {
                ui.label("Sending...");
            }
            match &self.result {
                Some(Ok(resp)) => {
                    ui.label(format!("Status: {}", resp.status));
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(&resp.body).monospace()).wrap(),
                        );
                    });
                }
                Some(Err(err)) => {
                    ui.colored_label(egui::Color32::RED, err);
                }
                None => {}
            }
        });

        if send_clicked {
            self.send(&ctx);
        }
    }
}
