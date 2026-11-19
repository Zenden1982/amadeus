//! Amadeus — голосовой чат для своих. Десктоп-клиент (Windows).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod app;
mod chat;
mod config;
mod desktop;
mod frame;
mod metrics;
mod mictest;
mod people;
mod picker;
mod ptt;
mod sounds;
mod stream;
mod theme;
mod tray;
mod update;
mod video;
mod voice;

// Символ STL 14.44 для предкомпилированного libwebrtc (см. crates/msvc-shim).
use msvc_shim as _;

fn main() -> eframe::Result {
    let mut logger = env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,livekit=info,libwebrtc=warn,wgpu=warn,eframe=warn"),
    );
    // Лог — в файл (релиз без консоли): по нему разбираем зависания.
    if let Some(p) = config::log_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::metadata(&p).is_ok_and(|m| m.len() > 2_000_000) {
            let _ = std::fs::rename(&p, p.with_file_name("amadeus.old.log"));
        }
        if let Ok(f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
            logger.target(env_logger::Target::Pipe(Box::new(f)));
        }
    }
    logger.init();
    let args = desktop::Args::parse(std::env::args().skip(1));
    // После обновления: дождаться, пока старая копия отпустит «единственный экземпляр».
    if let Some(pid) = args.after_update {
        desktop::wait_pid_exit(pid, 15_000);
    }
    if !desktop::claim_single_instance() {
        log::info!("Amadeus уже запущен — показываю его окно");
        return Ok(());
    }
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .thread_name("amadeus-rt")
        .build()
        .expect("tokio runtime");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Amadeus")
            // своя рамка и заголовок — frame.rs
            .with_decorations(false)
            .with_inner_size([1120.0, 700.0])
            .with_min_inner_size([880.0, 560.0])
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png")).unwrap_or_default()),
        ..Default::default()
    };
    eframe::run_native("Amadeus", options, Box::new(|cc| Ok(Box::new(app::App::new(cc, rt)))))
}
