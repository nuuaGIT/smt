// Keep the portable Windows release a GUI application so Windows does not
// create an extra console window next to SMT. Debug builds keep the console
// available for development diagnostics.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[path = "save_parser_core/decompress.rs"]
#[allow(dead_code, unexpected_cfgs)]
mod decompress;
#[path = "save_parser_core/error.rs"]
#[allow(dead_code)]
mod error;
#[path = "save_parser_core/level.rs"]
#[allow(dead_code)]
mod level;
#[path = "save_parser_core/object.rs"]
#[allow(dead_code)]
mod object;
#[path = "save_parser_core/properties.rs"]
#[allow(dead_code)]
mod properties;
#[path = "save_parser_core/reader.rs"]
#[allow(dead_code)]
mod reader;
#[path = "save_parser_core/save_header.rs"]
#[allow(dead_code)]
mod save_header;
#[path = "save_parser_core/store.rs"]
#[allow(dead_code)]
mod store;
#[path = "save_parser_core/version_data.rs"]
#[allow(dead_code)]
mod version_data;

mod app;
mod localization;
mod map;
mod save_parser;
mod storage;
mod world_data;

fn main() -> eframe::Result<()> {
    let app_icon =
        eframe::icon_data::from_png_bytes(include_bytes!("../data/logo.png")).unwrap_or_default();
    let options = eframe::NativeOptions {
        // Uncapped presentation while focused; the app still throttles when
        // unfocused or minimized.
        vsync: false,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([980.0, 720.0])
            .with_min_inner_size([760.0, 520.0])
            .with_icon(app_icon),
        ..Default::default()
    };

    eframe::run_native(
        "SMT",
        options,
        Box::new(|_creation_context| Ok(Box::new(app::TrackerApp::load()))),
    )
}
