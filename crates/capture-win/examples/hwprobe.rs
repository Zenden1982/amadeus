//! Проверка аппаратного пути: 5 с захвата первого монитора (или окна по подстроке) → H.264 в файл.
//! cargo run -p capture-win --example hwprobe --release -- <out.h264> [заголовок] [пресет: 1080|1440|native]

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use capture_win::capture::CaptureConfig;
use capture_win::hw::{EncodedSink, HwCapture};
use capture_win::{Event, SourceId, list_sources};

struct FileSink {
    out: Mutex<std::fs::File>,
    frames: AtomicU64,
    keys: AtomicU64,
    bytes: AtomicU64,
    key_req: AtomicBool,
    first: Mutex<Option<Instant>>,
}

impl EncodedSink for FileSink {
    fn encoded(&self, data: &[u8], keyframe: bool, _ts: i64, w: u32, h: u32) {
        let n = self.frames.fetch_add(1, Ordering::Relaxed);
        if n == 0 {
            println!("первый кадр {w}×{h}, ключевой={keyframe}, SPS={}", capture_win::hw::has_nal(data, 7));
            *self.first.lock().unwrap() = Some(Instant::now());
        }
        if keyframe {
            self.keys.fetch_add(1, Ordering::Relaxed);
        }
        self.bytes.fetch_add(data.len() as u64, Ordering::Relaxed);
        self.out.lock().unwrap().write_all(data).unwrap();
    }
    fn take_keyframe_request(&self) -> bool {
        self.key_req.swap(false, Ordering::Relaxed)
    }
    fn take_bitrate_request(&self) -> Option<u32> {
        None
    }
    fn event(&self, ev: Event) {
        println!("событие: {ev:?}");
    }
}

fn main() {
    struct L;
    impl log::Log for L {
        fn enabled(&self, m: &log::Metadata) -> bool {
            m.level() <= log::Level::Debug
        }
        fn log(&self, r: &log::Record) {
            if r.target().starts_with("capture_win") {
                println!("[{}] {}", r.level(), r.args());
            }
        }
        fn flush(&self) {}
    }
    let _ = log::set_logger(&L).map(|()| log::set_max_level(log::LevelFilter::Debug));

    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).expect("путь к .h264");
    let want = args.get(2).filter(|s| !s.is_empty());
    let cfg = match args.get(3).map(|s| s.as_str()) {
        Some("1440") => CaptureConfig { max_width: 2560, max_height: 1440, fps: 60 },
        Some("native") => CaptureConfig { max_width: 0, max_height: 0, fps: 60 },
        _ => CaptureConfig { max_width: 1920, max_height: 1080, fps: 60 },
    };
    let sources = list_sources(16, 9);
    let src = match want {
        Some(q) => sources.iter().find(|s| s.title.to_lowercase().contains(&q.to_lowercase())).map(|s| s.id),
        None => sources.iter().find(|s| matches!(s.id, SourceId::Monitor(_))).map(|s| s.id),
    }
    .expect("источник не найден");
    let sink = Arc::new(FileSink {
        out: Mutex::new(std::fs::File::create(out).unwrap()),
        frames: AtomicU64::new(0),
        keys: AtomicU64::new(0),
        bytes: AtomicU64::new(0),
        key_req: AtomicBool::new(false),
        first: Mutex::new(None),
    });
    let t0 = Instant::now();
    let cap = match HwCapture::start(src, cfg, 8_000_000, sink.clone()) {
        Ok(c) => c,
        Err(e) => {
            println!("ОШИБКА старта: {e}");
            return;
        }
    };
    println!("старт за {:?}, энкодер «{}», {:?}", t0.elapsed(), cap.encoder_name, cap.size);
    std::thread::sleep(Duration::from_millis(2500));
    sink.key_req.store(true, Ordering::Relaxed); // как PLI от зрителя
    std::thread::sleep(Duration::from_millis(2500));
    let t1 = Instant::now();
    cap.stop();
    println!("стоп за {:?}", t1.elapsed());
    let f = sink.frames.load(Ordering::Relaxed);
    println!(
        "кадров {f}, ключевых {}, {:.0} кбит/с",
        sink.keys.load(Ordering::Relaxed),
        sink.bytes.load(Ordering::Relaxed) as f64 * 8.0 / 5.0 / 1000.0
    );
}
