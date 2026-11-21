//! Проверка: окно eframe (wgpu) со стилем WS_EX_LAYERED и альфой 254 — рисуется ли и не «перекрывает» ли браузер.
//! cargo run -p client --example layered --release -- [альфа 0..255, по умолчанию 254; 0 — без layered]
use std::time::Instant;

use eframe::egui;
use msvc_shim as _;

struct App {
    t0: Instant,
    n: u64,
    hwnd: isize,
    alpha: u8,
    fixes: u32,
}

fn ensure_layered(hwnd: isize, alpha: u8) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongW, LWA_ALPHA, SetLayeredWindowAttributes, SetWindowLongW, WS_EX_LAYERED,
    };
    let hwnd = hwnd as windows_sys::Win32::Foundation::HWND;
    unsafe {
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
        if ex & WS_EX_LAYERED as i32 != 0 {
            return false;
        }
        SetWindowLongW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED as i32);
        SetLayeredWindowAttributes(hwnd, 0, alpha, LWA_ALPHA);
    }
    true
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        self.n += 1;
        if self.alpha > 0 && self.hwnd != 0 && ensure_layered(self.hwnd, self.alpha) {
            self.fixes += 1;
        }
        ui.heading(format!("LAYERED {} (стиль возвращали {} раз)", self.n, self.fixes));
        let r = ui.max_rect();
        let x = r.left() + (self.n as f32 * 7.0) % r.width();
        ui.painter().circle_filled(egui::pos2(x, r.center().y), 80.0, egui::Color32::from_rgb(20, 184, 166));
        if self.t0.elapsed().as_secs() >= 12 {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ui.ctx().request_repaint();
    }
}

fn main() -> eframe::Result<()> {
    let alpha: u8 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(254);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("LAYEREDTEST")
            .with_decorations(false)
            .with_position([0.0, 0.0])
            .with_inner_size([1500.0, 1000.0])
            .with_always_on_top(),
        ..Default::default()
    };
    eframe::run_native(
        "layered",
        options,
        Box::new(move |cc| {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let mut hwnd = 0;
            if let Ok(h) = cc.window_handle()
                && let RawWindowHandle::Win32(h) = h.as_raw()
            {
                hwnd = h.hwnd.get();
            }
            Ok(Box::new(App { t0: Instant::now(), n: 0, hwnd, alpha, fixes: 0 }))
        }),
    )
}
