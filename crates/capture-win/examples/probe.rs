//! Проверка захвата: список источников + 5 с захвата первого монитора (или окна по подстроке заголовка).
//! cargo run -p capture-win --example probe --release [-- "часть заголовка"]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use capture_win::{Capture, CaptureConfig, Event, I420Buffer, Sink, SourceId, list_sources};

struct Count(AtomicU64);

impl Sink for Count {
    fn frame(&self, _f: &I420Buffer, _ts: i64) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
    fn event(&self, ev: Event) {
        println!("событие: {ev:?}");
    }
}

fn main() {
    env_logger_lite();
    let t = Instant::now();
    let sources = list_sources(320, 180);
    println!("источников: {} за {:?}", sources.len(), t.elapsed());
    for s in &sources {
        println!(
            "  {:?} «{}» [{}] {}×{} min={} icon={:?} thumb={:?}",
            s.id, s.title, s.app, s.width, s.height, s.minimized, s.icon, s.thumb
        );
    }
    let want = std::env::args().nth(1);
    let src = match &want {
        Some(q) => sources.iter().find(|s| s.title.to_lowercase().contains(&q.to_lowercase())).map(|s| s.id),
        None => sources.iter().find(|s| matches!(s.id, SourceId::Monitor(_))).map(|s| s.id),
    }
    .expect("источник не найден");
    let sink = Arc::new(Count(AtomicU64::new(0)));
    let cfg = CaptureConfig { max_width: 1920, max_height: 1080, fps: 60 };
    let cap = Capture::start(src, cfg, sink.clone()).expect("старт");
    std::thread::sleep(Duration::from_secs(5));
    cap.stop();
    println!("кадров отправлено за 5 с: {}", sink.0.load(Ordering::Relaxed));
}

fn env_logger_lite() {
    struct L;
    impl log::Log for L {
        fn enabled(&self, m: &log::Metadata) -> bool {
            m.level() <= log::Level::Info
        }
        fn log(&self, r: &log::Record) {
            if r.target().starts_with("capture_win") {
                println!("[{}] {}", r.level(), r.args());
            }
        }
        fn flush(&self) {}
    }
    let _ = log::set_logger(&L).map(|()| log::set_max_level(log::LevelFilter::Info));
}
