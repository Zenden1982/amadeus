//! Мелочи: индикатор уровня, спиннер, пустое состояние, маскот.

use eframe::egui::{self, Align2, CornerRadius, Painter, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, pos2, vec2};

use super::{ButtonKind, button};
use crate::theme as t;

/// Индикатор уровня микрофона (None — неизвестно, серый).
pub fn level_meter(ui: &mut Ui, level: Option<f32>, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 10.0), Sense::hover());
    let p = t::p();
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(255), p.surface_3);
    if let Some(l) = level {
        let w = rect.width() * l.clamp(0.0, 1.0);
        if w > 1.0 {
            painter.rect_filled(Rect::from_min_size(rect.min, vec2(w.max(10.0), rect.height())), CornerRadius::same(255), p.speaking);
        }
    }
}

/// Спиннер цвета акцента.
pub fn spinner(ui: &mut Ui, size: f32) {
    ui.add(egui::Spinner::new().size(size).color(t::p().accent));
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MascotPose {
    Sitting,
    Sleeping,
    Waving,
    Breathing,
}

/// Точки квадратичной кривой Безье (для контура маскота).
fn quad(out: &mut Vec<Pos2>, a: Pos2, c: Pos2, b: Pos2) {
    for i in 1..=12 {
        let k = i as f32 / 12.0;
        let u = 1.0 - k;
        out.push(pos2(u * u * a.x + 2.0 * u * k * c.x + k * k * b.x, u * u * a.y + 2.0 * u * k * c.y + k * k * b.y));
    }
}

/// Маскот — простой контурный кот (design/mascot.svg, 120×120): уши как пики знака, глаза-точки, «ω», хвост завитком.
/// `Sleeping` — глаза закрыты; `Breathing` — мягко «дышит» (масштаб 1.0↔1.03, ~3 с).
pub fn mascot(painter: &Painter, rect: Rect, pose: MascotPose) {
    let p = t::p();
    let mut scale = rect.width().min(rect.height()) / 120.0;
    if pose == MascotPose::Breathing {
        let time = painter.ctx().input(|i| i.time);
        scale *= 1.0 + 0.015 * (1.0 + (time * std::f64::consts::TAU / 3.0).sin() as f32);
        painter.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    }
    let o = rect.center() - vec2(60.0, 60.0) * scale;
    let q = |x: f32, y: f32| o + vec2(x, y) * scale;
    let w = (6.0 * scale).max(1.5);
    let stroke = Stroke::new(w, p.accent);
    // тело: M30 104 Q24 70 34 52 L30 22 L48 38 Q60 34 72 38 L90 22 L86 52 Q96 70 90 104 Z
    let mut body = vec![q(30.0, 104.0)];
    quad(&mut body, q(30.0, 104.0), q(24.0, 70.0), q(34.0, 52.0));
    body.push(q(30.0, 22.0));
    body.push(q(48.0, 38.0));
    quad(&mut body, q(48.0, 38.0), q(60.0, 34.0), q(72.0, 38.0));
    body.push(q(90.0, 22.0));
    body.push(q(86.0, 52.0));
    quad(&mut body, q(86.0, 52.0), q(96.0, 70.0), q(90.0, 104.0));
    for v in &body {
        painter.circle_filled(*v, w / 2.0, p.accent); // скруглённые стыки
    }
    painter.add(Shape::closed_line(body, stroke));
    // хвост: M90 96 Q112 92 104 72
    let mut tail = vec![q(90.0, 96.0)];
    quad(&mut tail, q(90.0, 96.0), q(112.0, 92.0), q(104.0, 72.0));
    painter.circle_filled(q(104.0, 72.0), w / 2.0, p.accent);
    painter.add(Shape::line(tail, stroke));
    // глаза
    if pose == MascotPose::Sleeping {
        for x in [50.0, 70.0] {
            let mut lid = vec![q(x - 5.0, 59.0)];
            quad(&mut lid, q(x - 5.0, 59.0), q(x, 64.0), q(x + 5.0, 59.0));
            painter.add(Shape::line(lid, Stroke::new(w * 0.55, p.accent)));
        }
        painter.text(q(94.0, 30.0), Align2::CENTER_CENTER, "z", t::xbold(16.0 * scale), p.text_muted);
    } else {
        painter.circle_filled(q(50.0, 60.0), 4.5 * scale, p.accent);
        painter.circle_filled(q(70.0, 60.0), 4.5 * scale, p.accent);
    }
    // рот «ω»: M54 72 q3 3 6 0 q3 3 6 0
    let mut mouth = vec![q(54.0, 72.0)];
    quad(&mut mouth, q(54.0, 72.0), q(57.0, 75.0), q(60.0, 72.0));
    quad(&mut mouth, q(60.0, 72.0), q(63.0, 75.0), q(66.0, 72.0));
    painter.add(Shape::line(mouth, Stroke::new(w * 0.6, p.accent)));
}

/// Пустое состояние: кот-маскот (только там, где он к месту — пустой канал, пустой чат, вход), заголовок, подпись,
/// кнопка. Возвращает true по нажатию кнопки.
pub fn empty_state(ui: &mut Ui, pose: MascotPose, title: &str, subtitle: &str, action: Option<&str>) -> bool {
    let mut clicked = false;
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(110.0, 110.0), Sense::hover());
        mascot(ui.painter(), rect, pose);
        let p = t::p();
        ui.add_space(12.0);
        ui.label(RichText::new(title).font(t::xbold(20.0)).color(p.text));
        ui.add_space(4.0);
        ui.label(RichText::new(subtitle).size(14.5).color(p.text_muted));
        if let Some(a) = action {
            ui.add_space(14.0);
            clicked = button(ui, ButtonKind::Primary, a, None).clicked();
        }
    });
    clicked
}
