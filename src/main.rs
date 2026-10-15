#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod ui;
fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Schema Compare",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([1380.0, 860.0])
                .with_min_inner_size([1050.0, 700.0])
                .with_icon(
                    eframe::icon_data::from_png_bytes(include_bytes!(
                        "../assets/schema-compare.png"
                    ))
                    .expect("Embedded application icon must be valid"),
                ),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(ui::App::new(cc)))),
    )
}
