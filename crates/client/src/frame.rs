//! Своя рамка окна (без стандартной рамки Windows): полоса заголовка в стиле приложения, кнопки «свернуть /
//! развернуть / закрыть» (нарисованы кодом), перетаскивание и Aero Snap (`StartDrag` — системный цикл перемещения),
//! двойной клик — развернуть, ресайз за края (`BeginResize`). Тень и скруглённые углы Win11 — через DWM.

use eframe::egui::{
    self, Color32, CornerRadius, CursorIcon, Id, PointerButton, Pos2, Rect, ResizeDirection, Sense, Stroke, Ui,
    ViewportCommand, pos2, vec2,
};

use crate::theme as t;

pub const HEIGHT: f32 = 34.0;
const BTN_W: f32 = 34.0;
/// Толщина зоны ресайза у края окна (и больше — в углах).
const EDGE: f32 = 5.0;
const CORNER: f32 = 12.0;
const BG: Color32 = Color32::from_rgb(0x12, 0x0E, 0x0B);
/// Погашенная лампа — тёмное стекло.
const LAMP_OFF: Color32 = Color32::from_rgb(0x2A, 0x22, 0x1C);
const BEZEL_OFF: Color32 = Color32::from_rgb(0x3A, 0x2F, 0x25);
const SEPARATOR: Color32 = Color32::from_rgb(0x1E, 0x18, 0x13);
/// Радиусы лампы: ядро и латунный ободок.
const LAMP_R: f32 = 6.5;
const BEZEL_R: f32 = 8.5;

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

#[derive(Clone, Copy, PartialEq, Debug)]
enum Btn {
    Min,
    Max,
    Close,
}

/// Полоса заголовка — первой панелью кадра (на всю ширину окна). В полноэкранном режиме не показывать.
/// Вид «индикаторные лампы»: слева значок-осциллограф, кнопки — три лампы в латунных ободках,
/// при наведении загораются (янтарь; «закрыть» — красная).
/// `badge` — надпись-кнопка слева от ламп (есть обновление); возвращает true, если по ней кликнули.
pub fn title_bar(ui: &mut Ui, badge: Option<&str>) -> bool {
    let mut badge_clicked = false;
    egui::Panel::top("titlebar")
        .exact_size(HEIGHT)
        .resizable(false)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(BG))
        .show(ui, |ui| {
            let ctx = ui.ctx().clone();
            let r = ui.max_rect();
            let max = maximized(&ctx);
            let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true)) || forced_focus();
            let close = Rect::from_min_size(pos2(r.right() - BTN_W - 6.0, r.top()), vec2(BTN_W, r.height()));
            let maxb = close.translate(vec2(-BTN_W, 0.0));
            let minb = maxb.translate(vec2(-BTN_W, 0.0));

            // надпись «есть обновление»: латунная рамка, янтарный текст, при наведении светится
            let mut drag_right = minb.left();
            if let Some(text) = badge {
                let galley = ui.painter().layout_no_wrap(
                    format!("{}  {text}", egui_phosphor::regular::ARROW_CIRCLE_UP),
                    t::bold(12.5),
                    t::NIXIE,
                );
                let size = vec2(galley.size().x + 20.0, 22.0);
                let rect = Rect::from_min_size(pos2(minb.left() - 12.0 - size.x, r.center().y - size.y / 2.0), size);
                let resp = ui.interact(rect, Id::new("titlebar-badge"), Sense::click());
                let lit = ctx.animate_bool_with_time(Id::new("titlebar-badge-lit"), resp.hovered(), 0.12);
                let p = ui.painter();
                p.rect_filled(rect.expand(3.0 * lit), CornerRadius::same(13), t::NIXIE.gamma_multiply(0.10 * lit));
                p.rect(rect, CornerRadius::same(11), t::NIXIE.gamma_multiply(0.08 + 0.10 * lit), Stroke::new(1.0, t::BRASS), egui::StrokeKind::Inside);
                p.galley(pos2(rect.left() + 10.0, rect.center().y - galley.size().y / 2.0), galley, t::NIXIE);
                if resp.on_hover_text("Скачать и перезапустить").clicked() {
                    badge_clicked = true;
                }
                drag_right = rect.left() - 4.0;
            }

            let drag_rect = Rect::from_min_max(r.min, pos2(drag_right, r.bottom()));
            let drag = ui.interact(drag_rect, Id::new("titlebar-drag"), Sense::click_and_drag());
            if edge_dir(&ctx).is_none() {
                if drag.double_clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(!max));
                } else if drag.drag_started_by(PointerButton::Primary) {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
            }

            let p = ui.painter();
            p.hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, SEPARATOR));
            // только значок (без надписи — «Amadeus» есть в углу сайдбара)
            let logo = Rect::from_center_size(pos2(r.left() + 24.0, r.center().y), vec2(26.0, 26.0));
            t::logo(p, logo, false, focused);

            let forced = forced_hover();
            for (rect, kind) in [(minb, Btn::Min), (maxb, Btn::Max), (close, Btn::Close)] {
                let resp = ui.interact(rect, Id::new(("titlebar-btn", kind as u8)), Sense::click());
                let hover = resp.hovered() || forced == Some(kind);
                let mut lit = ctx.animate_bool_with_time(Id::new(("titlebar-lamp", kind as u8)), hover, 0.12);
                if resp.is_pointer_button_down_on() {
                    lit *= 0.8;
                }
                draw_lamp(p, rect.center(), kind, max, lit);
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
    badge_clicked
}

/// Отладка вёрстки (только debug): AMADEUS_FRAME_HOVER=min|max|close — лампа горит, как при наведении.
fn forced_hover() -> Option<Btn> {
    #[cfg(debug_assertions)]
    {
        match std::env::var("AMADEUS_FRAME_HOVER").ok()?.as_str() {
            "min" => Some(Btn::Min),
            "max" => Some(Btn::Max),
            "close" => Some(Btn::Close),
            _ => None,
        }
    }
    #[cfg(not(debug_assertions))]
    None
}

/// Отладка (только debug): AMADEUS_FRAME_FOCUS=1 — рисовать как активное окно (для скриншотов из фона).
fn forced_focus() -> bool {
    cfg!(debug_assertions) && std::env::var_os("AMADEUS_FRAME_FOCUS").is_some()
}

struct LampColors {
    /// Цвет стекла лампы.
    core: Color32,
    /// Свечение вокруг (прозрачное, пока лампа погашена).
    glow: Color32,
    /// Значок внутри — проступает, когда лампа горит.
    glyph: Color32,
}

/// Цвета лампы; `lit` 0..1 — насколько она зажглась.
fn lamp_colors(kind: Btn, lit: f32) -> LampColors {
    let lit = lit.clamp(0.0, 1.0);
    let on = if kind == Btn::Close { t::SIGNAL } else { t::NIXIE };
    let a = (lit * 255.0).round() as u8;
    LampColors {
        core: LAMP_OFF.lerp_to_gamma(on, lit),
        glow: Color32::from_rgba_unmultiplied(on.r(), on.g(), on.b(), (lit * 120.0).round() as u8),
        glyph: Color32::from_rgba_unmultiplied(t::ON_NIXIE.r(), t::ON_NIXIE.g(), t::ON_NIXIE.b(), a),
    }
}

fn draw_lamp(p: &egui::Painter, c: Pos2, kind: Btn, maximized: bool, lit: f32) {
    let col = lamp_colors(kind, lit);
    if lit > 0.01 {
        t::radial_glow(p, c, 16.0, col.glow);
    }
    // латунный ободок: тускнеет, пока лампа не горит
    let bezel = BEZEL_OFF.lerp_to_gamma(t::BRASS_LOGO, lit);
    p.circle_filled(c, BEZEL_R, Color32::from_rgb(0x15, 0x11, 0x0D));
    p.circle_stroke(c, BEZEL_R - 1.0, Stroke::new(2.0, bezel));
    p.circle_filled(c, LAMP_R, col.core);
    // блик на стекле (сверху слева)
    let shine = if lit > 0.01 { t::FILAMENT.gamma_multiply(0.75 * lit) } else { Color32::from_white_alpha(14) };
    p.circle_filled(c + vec2(-2.0, -2.2), 2.2, shine);
    if col.glyph.a() > 0 {
        draw_icon(p, c, kind, maximized, col.glyph);
    }
}

/// Значки 6×6 (внутри лампы).
fn draw_icon(p: &egui::Painter, c: Pos2, kind: Btn, maximized: bool, color: Color32) {
    let s = Stroke::new(1.4, color);
    let c = pos2(c.x.round() + 0.5, c.y.round() + 0.5);
    match kind {
        Btn::Min => {
            p.hline(c.x - 3.0..=c.x + 3.0, c.y, s);
        }
        Btn::Max if !maximized => {
            p.rect_stroke(Rect::from_center_size(c, vec2(6.0, 6.0)), CornerRadius::ZERO, s, egui::StrokeKind::Middle);
        }
        Btn::Max => {
            // «восстановить»: два квадрата, задний виден уголком
            p.rect_stroke(Rect::from_min_size(pos2(c.x - 3.0, c.y - 1.5), vec2(4.5, 4.5)), CornerRadius::ZERO, s, egui::StrokeKind::Middle);
            p.line_segment([pos2(c.x - 1.5, c.y - 3.0), pos2(c.x + 3.0, c.y - 3.0)], s);
            p.line_segment([pos2(c.x + 3.0, c.y - 3.0), pos2(c.x + 3.0, c.y + 1.5)], s);
        }
        Btn::Close => {
            p.line_segment([pos2(c.x - 3.0, c.y - 3.0), pos2(c.x + 3.0, c.y + 3.0)], s);
            p.line_segment([pos2(c.x - 3.0, c.y + 3.0), pos2(c.x + 3.0, c.y - 3.0)], s);
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

    #[test]
    fn lamp_colors_states() {
        for k in [Btn::Min, Btn::Max, Btn::Close] {
            let off = lamp_colors(k, 0.0);
            assert_eq!(off.glyph.a(), 0);
            assert_eq!(off.glow.a(), 0);
            assert_eq!(off.core, LAMP_OFF);
            let on = lamp_colors(k, 1.0);
            assert_eq!(on.glyph.a(), 255);
            assert_eq!(on.core, if k == Btn::Close { t::SIGNAL } else { t::NIXIE });
        }
    }
}
