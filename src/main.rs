#![cfg_attr(windows, windows_subsystem = "windows")]

use moza_xinput_bridge::gui;

fn init_logging() {
    use env_logger::{Builder, Target};
    use std::io::Write;

    // Append to %TEMP%\moza_mapper.log so windowed-app issues are diagnosable.
    let path = std::env::temp_dir().join("moza_mapper.log");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok();

    let mut builder = Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    if let Some(f) = file {
        builder.target(Target::Pipe(Box::new(f)));
    }
    builder.format(|buf, record| {
        writeln!(
            buf,
            "{} [{}] {}",
            buf.timestamp_millis(),
            record.level(),
            record.args()
        )
    });
    // try_init, not init: init() panics if a logger already exists.
    let _ = builder.try_init();
    log::info!("=== moza mapper starting, log file: {} ===", path.display());
}

fn main() -> eframe::Result<()> {
    init_logging();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([820.0, 720.0])
            .with_min_inner_size([640.0, 480.0])
            .with_title("Moza Mapper — R3 to XInput Bridge"),
        centered: true,
        ..Default::default()
    };

    eframe::run_native(
        "Moza Mapper",
        options,
        Box::new(|cc| Ok(Box::new(gui::MozaMapperApp::new(cc)))),
    )
}
