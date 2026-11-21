//! Отладка «трансляция замирает»: аппаратный захват окна по подстроке заголовка, кадры энкодера по секундам.
//! cargo run -p capture-win --example occlusion --release -- <заголовок> [секунд]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use capture_win::capture::CaptureConfig;
use capture_win::hw::{EncodedSink, HwCapture};
use capture_win::{Event, list_sources};

struct Count(AtomicU64);

impl EncodedSink for Count {
    fn encoded(&self, _: &[u8], _: bool, _: i64, _: u32, _: u32) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
    fn take_keyframe_request(&self) -> bool {
        false
    }
    fn take_bitrate_request(&self) -> Option<u32> {
        None
    }
    fn event(&self, ev: Event) {
        println!("событие: {ev:?}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let q = args.get(1).expect("подстрока заголовка").to_lowercase();
    let secs: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(20);
    let src = list_sources(16, 9).into_iter().find(|s| s.title.to_lowercase().contains(&q)).expect("окно не найдено");
    println!("окно: {}", src.title);
    let sink = Arc::new(Count(AtomicU64::new(0)));
    let cap = HwCapture::start(src.id, CaptureConfig { max_width: 1920, max_height: 1080, fps: 30 }, 8_000_000, sink.clone())
        .expect("старт");
    for s in 1..=secs {
        std::thread::sleep(Duration::from_secs(1));
        println!("{s:>2} с: {} кадров", sink.0.swap(0, Ordering::Relaxed));
    }
    cap.stop();
}
