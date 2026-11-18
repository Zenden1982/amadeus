//! Своя рамка окна (без стандартной рамки Windows): полоса заголовка в стиле приложения, кнопки «свернуть /
//! развернуть / закрыть» (нарисованы кодом), перетаскивание и Aero Snap (`StartDrag` — системный цикл перемещения),
//! двойной клик — развернуть, ресайз за края (`BeginResize`). Тень и скруглённые углы Win11 — через DWM.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, Id, PointerButton, Pos2, Rect, ResizeDirection, Sense, Stroke, Ui,
    ViewportCommand, pos2, vec2,
};

use crate::theme as t;

pub const HEIGHT: f32 = 34.0;
const BTN_W: f32 = 46.0;
/// Толщина зоны ресайза у края окна (и больше — в углах).
const EDGE: f32 = 5.0;
const CORNER: f32 = 12.0;
const BG: Color32 = Color32::from_rgb(0x12, 0x0E, 0x0B);
const CLOSE_HOVER: Color32 = Color32::from_rgb(0xC4, 0x2B, 0x1C);

/// Скругление углов Win11 и цвет тонкой рамки DWM (на Win10 вызовы просто не действуют).
pub fn style_native_window(cc: &eframe::CreationContext<'_>) {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows_sys::Win32::Graphics::Dwm::{
            DWMWA_BORDER_COLOR, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmSetWindowAttribute,
        };
        let Ok(handle) = cc.window_handle() else { return };
        let RawWindowHandle::Win32(h) = handle.as_raw() else { return };
        let hwnd = h.hwnd.get() as windows_sys::Win32::Foundation::HWND;
        let corner: i32 = DWMWCP_ROUND;
        let c = t::BRASS_DIM;
        // COLORREF = 0x00BBGGRR
        let border: u32 = (c.b() as u32) << 16 | (c.g() as u32) << 8 | c.r() as u32;
        unsafe {
            DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE as u32, (&corner as *const i32).cast(), 4);
            DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR as u32, (&border as *const u32).cast(), 4);
        }
    }
    #[cfg(not(windows))]
    let _ = cc;
}

fn maximized(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().maximized.unwrap_or(false))
}

fn fullscreen(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().fullscreen.unwrap_or(false))
}

/// Направление ресайза, если курсор у края окна (развёрнутое/полноэкранное окно не тянется).
fn edge_dir(ctx: &egui::Context) -> Option<ResizeDirection> {
    if maximized(ctx) || fullscreen(ctx) {
        return None;
    }
    let pos = ctx.input(|i| i.pointer.hover_pos())?;
    edge_dir_at(ctx.content_rect(), pos)
}

fn edge_dir_at(r: Rect, p: Pos2) -> Option<ResizeDirection> {
    if !r.expand(1.0).contains(p) {
        return None;
    }
    let near = |d: f32, zone: f32| d < zone;
    let (l, rt, tp, b) = (p.x - r.left(), r.right() - p.x, p.y - r.top(), r.bottom() - p.y);
    use ResizeDirection as D;
    // углы — зона пошире, чтобы попадать было легко
    Some(match () {
        _ if near(l, CORNER) && near(tp, CORNER) => D::NorthWest,
        _ if near(rt, CORNER) && near(tp, CORNER) => D::NorthEast,
        _ if near(l, CORNER) && near(b, CORNER) => D::SouthWest,
        _ if near(rt, CORNER) && near(b, CORNER) => D::SouthEast,
        _ if near(l, EDGE) => D::West,
        _ if near(rt, EDGE) => D::East,
        _ if near(tp, EDGE) => D::North,
        _ if near(b, EDGE) => D::South,
        _ => return None,
    })
}

fn cursor_for(d: ResizeDirection) -> CursorIcon {
    use ResizeDirection as D;
    match d {
        D::North | D::South => CursorIcon::ResizeVertical,
        D::East | D::West => CursorIcon::ResizeHorizontal,
        D::NorthWest | D::SouthEast => CursorIcon::ResizeNwSe,
        D::NorthEast | D::SouthWest => CursorIcon::ResizeNeSw,
    }
}

/// Ресайз за края. Вызывать в конце кадра (курсор ресайза перекрывает курсоры виджетов).
pub fn resize_edges(ctx: &egui::Context) {
    let Some(dir) = edge_dir(ctx) else { return };
    ctx.set_cursor_icon(cursor_for(dir));
    if ctx.input(|i| i.pointer.button_pressed(PointerButton::Primary)) {
        ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Btn {
    Min,
    Max,
    Close,
}

/// Полоса заголовка — первой панелью кадра (на всю ширину окна). В полноэкранном режиме не показывать.
pub fn title_bar(ui: &mut Ui) {
    egui::Panel::top("titlebar")
        .exact_size(HEIGHT)
        .resizable(false)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(BG))
        .show(ui, |ui| {
            let ctx = ui.ctx().clone();
            let r = ui.max_rect();
            let max = maximized(&ctx);
            let close = Rect::from_min_size(pos2(r.right() - BTN_W, r.top()), vec2(BTN_W, r.height()));
            let maxb = close.translate(vec2(-BTN_W, 0.0));
            let minb = maxb.translate(vec2(-BTN_W, 0.0));

            let drag_rect = Rect::from_min_max(r.min, pos2(minb.left(), r.bottom()));
            let drag = ui.interact(drag_rect, Id::new("titlebar-drag"), Sense::click_and_drag());
            if edge_dir(&ctx).is_none() {
                if drag.double_clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(!max));
                } else if drag.drag_started_by(PointerButton::Primary) {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
            }

            let p = ui.painter();
            let logo = Rect::from_center_size(pos2(r.left() + 22.0, r.center().y), vec2(20.0, 20.0));
            t::logo(p, logo, false);
            p.text(pos2(logo.right() + 6.0, r.center().y), Align2::LEFT_CENTER, "Amadeus", t::semibold(12.5), t::DUST);

            for (rect, kind) in [(minb, Btn::Min), (maxb, Btn::Max), (close, Btn::Close)] {
                let resp = ui.interact(rect, Id::new(("titlebar-btn", kind as u8)), Sense::click());
                let hover = resp.hovered();
                let (bg, fg) = match (kind, hover) {
                    (Btn::Close, true) => (CLOSE_HOVER, Color32::WHITE),
                    (_, true) => (t::BAKELITE_2, t::PAPER),
                    _ => (Color32::TRANSPARENT, t::DUST),
                };
                if resp.is_pointer_button_down_on() {
                    p.rect_filled(rect, CornerRadius::ZERO, bg.gamma_multiply(0.8));
                } else {
                    p.rect_filled(rect, CornerRadius::ZERO, bg);
                }
                draw_icon(p, rect.center(), kind, max, fg);
                let tip = match kind {
                    Btn::Min => "Свернуть",
                    Btn::Max if max => "Восстановить",
                    Btn::Max => "Развернуть",
                    Btn::Close => "Закрыть",
                };
                if resp.on_hover_text(tip).clicked() {
                    match kind {
                        Btn::Min => ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
                        Btn::Max => ctx.send_viewport_cmd(ViewportCommand::Maximized(!max)),
                        Btn::Close => ctx.send_viewport_cmd(ViewportCommand::Close),
                    }
                }
            }
        });
}

/// Значки 10×10 тонкой линией, как у системных кнопок Windows 11.
fn draw_icon(p: &egui::Painter, c: Pos2, kind: Btn, maximized: bool, color: Color32) {
    let s = Stroke::new(1.0, color);
    // привязка к пиксельной сетке, чтобы линии были чёткими
    let c = pos2(c.x.round() + 0.5, c.y.round() + 0.5);
    match kind {
        Btn::Min => {
            p.hline(c.x - 5.0..=c.x + 5.0, c.y, s);
        }
        Btn::Max if !maximized => {
            p.rect_stroke(Rect::from_center_size(c, vec2(10.0, 10.0)), CornerRadius::same(1), s, egui::StrokeKind::Middle);
        }
        Btn::Max => {
            // «восстановить»: два квадрата, задний виден уголком
            let front = Rect::from_min_size(pos2(c.x - 5.0, c.y - 3.0), vec2(8.0, 8.0));
            p.rect_stroke(front, CornerRadius::same(1), s, egui::StrokeKind::Middle);
            p.line_segment([pos2(c.x - 3.0, c.y - 3.0), pos2(c.x - 3.0, c.y - 5.0)], s);
            p.line_segment([pos2(c.x - 3.0, c.y - 5.0), pos2(c.x + 5.0, c.y - 5.0)], s);
            p.line_segment([pos2(c.x + 5.0, c.y - 5.0), pos2(c.x + 5.0, c.y + 3.0)], s);
            p.line_segment([pos2(c.x + 5.0, c.y + 3.0), pos2(c.x + 3.0, c.y + 3.0)], s);
        }
        Btn::Close => {
            p.line_segment([pos2(c.x - 5.0, c.y - 5.0), pos2(c.x + 5.0, c.y + 5.0)], s);
            p.line_segment([pos2(c.x - 5.0, c.y + 5.0), pos2(c.x + 5.0, c.y - 5.0)], s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_and_corners() {
        let r = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
        assert_eq!(edge_dir_at(r, pos2(400.0, 300.0)), None);
        assert_eq!(edge_dir_at(r, pos2(2.0, 300.0)), Some(ResizeDirection::West));
        assert_eq!(edge_dir_at(r, pos2(798.0, 300.0)), Some(ResizeDirection::East));
        assert_eq!(edge_dir_at(r, pos2(400.0, 1.0)), Some(ResizeDirection::North));
        assert_eq!(edge_dir_at(r, pos2(400.0, 597.0)), Some(ResizeDirection::South));
        assert_eq!(edge_dir_at(r, pos2(8.0, 8.0)), Some(ResizeDirection::NorthWest));
        assert_eq!(edge_dir_at(r, pos2(795.0, 590.0)), Some(ResizeDirection::SouthEast));
        assert_eq!(edge_dir_at(r, pos2(900.0, 300.0)), None);
    }
}
