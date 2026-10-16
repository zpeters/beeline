//! Fonts and text sizes. Atkinson Hyperlegible was designed for low-vision
//! readers; it is bundled so the app looks the same on every machine.

use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId, TextStyle};

const SANS: &str = "atkinson";
const MONO: &str = "atkinson-mono";

/// Installs the bundled fonts and larger text sizes. Call once at startup.
pub fn apply(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        SANS.into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/AtkinsonHyperlegible-Regular.otf"
        ))),
    );
    fonts.font_data.insert(
        MONO.into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../../assets/fonts/AtkinsonHyperlegibleMono-Regular.ttf"
        ))),
    );
    // Our fonts come first; egui's defaults stay behind them as fallbacks
    // for symbols the bundled fonts lack.
    for (family, name) in [
        (FontFamily::Proportional, SANS),
        (FontFamily::Monospace, MONO),
    ] {
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, name.into());
    }
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (
                TextStyle::Small,
                FontId::new(13.0, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(16.0, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(16.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Heading,
                FontId::new(22.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(15.0, FontFamily::Monospace),
            ),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.interact_size.y = 30.0;
    });
}
