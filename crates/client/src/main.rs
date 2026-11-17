//! Amadeus — голосовой чат для своих. Десктоп-клиент (Windows).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod app;
mod config;
mod mictest;
mod msvc_shim;
mod ptt;
mod sounds;
mod theme;
mod voice;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,livekit=info,libwebrtc=warn,wgpu=warn,eframe=warn"))
        .init();
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .thread_name("amadeus-rt")
        .build()
        .expect("tokio runtime");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Amadeus")
            .with_inner_size([1120.0, 700.0])
            .with_min_inner_size([880.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native("Amadeus", options, Box::new(|cc| Ok(Box::new(app::App::new(cc, rt)))))
}
