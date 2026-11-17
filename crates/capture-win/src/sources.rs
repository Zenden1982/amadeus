//! Источники для стрима: мониторы и окна верхнего уровня, с миниатюрой и иконкой.
//!
//! Миниатюры — GDI (`PrintWindow` с PW_RENDERFULLCONTENT для окон, `StretchBlt` с экрана для мониторов):
//! быстро и без запуска WGC-сессии на каждое окно. Вызывать из фонового потока (десятки мс на окно).

use std::ffi::c_void;
use std::mem::size_of;

use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject,
    GetDC, GetMonitorInfoW, HALFTONE, HBITMAP, HDC, HGDIOBJ, HMONITOR, MONITORINFO, ReleaseDC,
    SRCCOPY, SelectObject, SetStretchBltMode, StretchBlt,
};
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::{
    DI_NORMAL, DrawIconEx, GCLP_HICON, GCLP_HICONSM, GW_OWNER, GWL_EXSTYLE, GetClassLongPtrW, GetClassNameW, GetWindow,
    GetWindowLongPtrW, GetWindowRect, HICON, ICON_BIG, ICON_SMALL2, IsIconic, IsWindow, SMTO_ABORTIFHUNG,
    SendMessageTimeoutW, WM_GETICON, WS_EX_APPWINDOW,
};
use windows_capture::monitor::Monitor;
use windows_capture::window::Window;

/// PW_RENDERFULLCONTENT: окна с DirectComposition (Chrome, Electron, UWP) иначе рисуются чёрными.
const PW_RENDERFULLCONTENT: u32 = 2;
const ICON_SIZE: i32 = 32;
const MONITORINFOF_PRIMARY: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SourceId {
    /// HMONITOR.
    Monitor(isize),
    /// HWND.
    Window(isize),
}

impl SourceId {
    pub fn is_window(&self) -> bool {
        matches!(self, SourceId::Window(_))
    }

    /// Источник ещё существует (окно не закрыто).
    pub fn is_alive(&self) -> bool {
        match *self {
            SourceId::Window(h) => unsafe { IsWindow(Some(hwnd(h))).as_bool() },
            SourceId::Monitor(h) => Monitor::enumerate().map(|v| v.iter().any(|m| m.as_raw_hmonitor() as isize == h)).unwrap_or(false),
        }
    }

    /// Окно свёрнуто.
    pub fn is_minimized(&self) -> bool {
        match *self {
            SourceId::Window(h) => unsafe { IsIconic(hwnd(h)).as_bool() },
            SourceId::Monitor(_) => false,
        }
    }
}

/// Картинка RGBA8 (без отступов строк).
#[derive(Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Image {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Image({}×{})", self.width, self.height)
    }
}

#[derive(Clone, Debug)]
pub struct Source {
    pub id: SourceId,
    /// Заголовок окна или «Экран N».
    pub title: String,
    /// Имя программы (для окна) или разрешение (для экрана).
    pub app: String,
    pub width: u32,
    pub height: u32,
    pub minimized: bool,
    pub primary: bool,
    pub icon: Option<Image>,
    pub thumb: Option<Image>,
}

fn hwnd(h: isize) -> HWND {
    HWND(h as *mut c_void)
}

/// Все мониторы и подходящие окна (в порядке Z — сверху вниз). Миниатюры вписаны в `thumb_w`×`thumb_h`.
pub fn list_sources(thumb_w: u32, thumb_h: u32) -> Vec<Source> {
    let mut out = Vec::new();
    for (i, m) in Monitor::enumerate().unwrap_or_default().into_iter().enumerate() {
        let hmon = HMONITOR(m.as_raw_hmonitor());
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !unsafe { GetMonitorInfoW(hmon, &mut info) }.as_bool() {
            continue;
        }
        let r = info.rcMonitor;
        let (w, h) = ((r.right - r.left).max(0) as u32, (r.bottom - r.top).max(0) as u32);
        let primary = info.dwFlags & MONITORINFOF_PRIMARY != 0;
        out.push(Source {
            id: SourceId::Monitor(hmon.0 as isize),
            title: format!("Экран {}", i + 1),
            app: format!("{w}×{h}{}", if primary { " · основной" } else { "" }),
            width: w,
            height: h,
            minimized: false,
            primary,
            icon: None,
            thumb: screen_thumb(r, thumb_w, thumb_h),
        });
    }
    let own_pid = unsafe { GetCurrentProcessId() };
    for w in Window::enumerate().unwrap_or_default() {
        let h = HWND(w.as_raw_hwnd());
        let Ok(title) = w.title() else { continue };
        if title.trim().is_empty() || w.process_id().map(|p| p == own_pid).unwrap_or(true) {
            continue;
        }
        if is_cloaked(h) || is_shell(h) || is_owned_popup(h) {
            continue;
        }
        let minimized = unsafe { IsIconic(h).as_bool() };
        let bounds = frame_bounds(h);
        let (bw, bh) = (bounds.right - bounds.left, bounds.bottom - bounds.top);
        if !minimized && (bw < 64 || bh < 48) {
            continue;
        }
        let app = w.process_name().map(|n| n.trim_end_matches(".exe").trim_end_matches(".EXE").to_string()).unwrap_or_default();
        out.push(Source {
            id: SourceId::Window(h.0 as isize),
            title,
            app,
            width: bw.max(0) as u32,
            height: bh.max(0) as u32,
            minimized,
            primary: false,
            icon: window_icon(h),
            thumb: if minimized { None } else { window_thumb(h, thumb_w, thumb_h) },
        });
    }
    out
}

fn is_cloaked(h: HWND) -> bool {
    let mut cloaked: u32 = 0;
    let ok = unsafe {
        DwmGetWindowAttribute(h, DWMWA_CLOAKED, (&raw mut cloaked).cast(), size_of::<u32>() as u32)
    };
    ok.is_ok() && cloaked != 0
}

fn is_shell(h: HWND) -> bool {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(h, &mut buf) };
    let class = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
    matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
}

/// Диалоги/поповеры, принадлежащие другому окну (если сами не просятся на панель задач).
fn is_owned_popup(h: HWND) -> bool {
    let owner = unsafe { GetWindow(h, GW_OWNER) }.unwrap_or_default();
    if owner.0.is_null() {
        return false;
    }
    let ex = unsafe { GetWindowLongPtrW(h, GWL_EXSTYLE) } as u32;
    ex & WS_EX_APPWINDOW.0 == 0
}

/// Видимые границы окна (без невидимых рамок изменения размера Win10+).
fn frame_bounds(h: HWND) -> RECT {
    let mut r = RECT::default();
    let ok = unsafe {
        DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS, (&raw mut r).cast(), size_of::<RECT>() as u32)
    };
    if ok.is_err() {
        let _ = unsafe { GetWindowRect(h, &mut r) };
    }
    r
}

/// Вписать (w,h) в (bw,bh) с сохранением пропорций.
pub fn fit(w: u32, h: u32, bw: u32, bh: u32) -> (u32, u32) {
    if w == 0 || h == 0 {
        return (0, 0);
    }
    let k = (bw as f64 / w as f64).min(bh as f64 / h as f64);
    (((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1))
}

/// 32-битный DIB сверху вниз. Возвращает битмап и указатель на пиксели (BGRA).
struct Dib {
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    w: i32,
    h: i32,
}

impl Dib {
    fn new(screen: HDC, w: i32, h: i32) -> Option<Dib> {
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        unsafe {
            let bmp = CreateDIBSection(Some(screen), &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
            if bits.is_null() {
                let _ = DeleteObject(bmp.into());
                return None;
            }
            let dc = CreateCompatibleDC(Some(screen));
            let old = SelectObject(dc, bmp.into());
            Some(Dib { dc, bmp, old, bits: bits.cast(), w, h })
        }
    }

    fn pixels(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.bits, (self.w * self.h * 4) as usize) }
    }

    /// BGRA → RGBA; `force_opaque` — альфа = 255 (GDI её не заполняет).
    fn to_image(&self, force_opaque: bool) -> Image {
        let src = self.pixels();
        let mut rgba = Vec::with_capacity(src.len());
        let any_alpha = !force_opaque && src.as_chunks::<4>().0.iter().any(|p| p[3] != 0);
        for p in src.as_chunks::<4>().0 {
            let a = if any_alpha { p[3] } else { 255 };
            rgba.extend_from_slice(&[p[2], p[1], p[0], a]);
        }
        Image { width: self.w as u32, height: self.h as u32, rgba }
    }
}

impl Drop for Dib {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bmp.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

struct ScreenDc(HDC);

impl ScreenDc {
    fn get() -> ScreenDc {
        ScreenDc(unsafe { GetDC(None) })
    }
}

impl Drop for ScreenDc {
    fn drop(&mut self) {
        unsafe { ReleaseDC(None, self.0) };
    }
}

fn screen_thumb(r: RECT, bw: u32, bh: u32) -> Option<Image> {
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let (tw, th) = fit(w.max(0) as u32, h.max(0) as u32, bw, bh);
    if tw == 0 {
        return None;
    }
    let screen = ScreenDc::get();
    let dib = Dib::new(screen.0, tw as i32, th as i32)?;
    unsafe {
        SetStretchBltMode(dib.dc, HALFTONE);
        StretchBlt(dib.dc, 0, 0, tw as i32, th as i32, Some(screen.0), r.left, r.top, w, h, SRCCOPY).ok().ok()?;
    }
    Some(dib.to_image(true))
}

fn window_thumb(h: HWND, bw: u32, bh: u32) -> Option<Image> {
    let mut wr = RECT::default();
    unsafe { GetWindowRect(h, &mut wr) }.ok()?;
    let fb = frame_bounds(h);
    let (ww, wh) = (wr.right - wr.left, wr.bottom - wr.top);
    let (fw, fh) = (fb.right - fb.left, fb.bottom - fb.top);
    if ww <= 0 || wh <= 0 || fw <= 0 || fh <= 0 {
        return None;
    }
    let (tw, th) = fit(fw as u32, fh as u32, bw, bh);
    let screen = ScreenDc::get();
    let full = Dib::new(screen.0, ww, wh)?;
    let thumb = Dib::new(screen.0, tw as i32, th as i32)?;
    unsafe {
        if !PrintWindow(h, full.dc, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool() {
            return None;
        }
        SetStretchBltMode(thumb.dc, HALFTONE);
        let (ox, oy) = ((fb.left - wr.left).clamp(0, ww - 1), (fb.top - wr.top).clamp(0, wh - 1));
        StretchBlt(thumb.dc, 0, 0, tw as i32, th as i32, Some(full.dc), ox, oy, fw.min(ww - ox), fh.min(wh - oy), SRCCOPY)
            .ok()
            .ok()?;
    }
    Some(thumb.to_image(true))
}

fn window_icon(h: HWND) -> Option<Image> {
    let mut res: usize = 0;
    unsafe {
        for kind in [ICON_BIG, ICON_SMALL2] {
            let r = SendMessageTimeoutW(
                h,
                WM_GETICON,
                WPARAM(kind as usize),
                LPARAM(0),
                SMTO_ABORTIFHUNG,
                50,
                Some(&mut res),
            );
            if r.0 != 0 && res != 0 {
                break;
            }
            res = 0;
        }
        if res == 0 {
            res = GetClassLongPtrW(h, GCLP_HICON);
        }
        if res == 0 {
            res = GetClassLongPtrW(h, GCLP_HICONSM);
        }
    }
    if res == 0 {
        return None;
    }
    let screen = ScreenDc::get();
    let dib = Dib::new(screen.0, ICON_SIZE, ICON_SIZE)?;
    unsafe {
        DrawIconEx(dib.dc, 0, 0, HICON(res as *mut c_void), ICON_SIZE, ICON_SIZE, 0, None, DI_NORMAL).ok()?;
    }
    Some(dib.to_image(false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_aspect() {
        assert_eq!(fit(1920, 1080, 320, 180), (320, 180));
        assert_eq!(fit(1080, 1920, 320, 180), (101, 180));
        assert_eq!(fit(0, 10, 320, 180), (0, 0));
    }

    #[test]
    fn lists_monitors() {
        // На любом ПК с рабочим столом есть хотя бы один монитор.
        let s = list_sources(160, 90);
        assert!(s.iter().any(|s| matches!(s.id, SourceId::Monitor(_))));
    }
}
