//! Мелочи: индикатор уровня, спиннер, пустое состояние, маскот.

use eframe::egui::{self, CornerRadius, Painter, Rect, RichText, Sense, Ui, pos2, vec2};

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

/// Маскот. До выбора владельцем (задача 6) — знак в круге.
pub fn mascot(painter: &Painter, rect: Rect, pose: MascotPose) {
    let p = t::p();
    let r = rect.width().min(rect.height()) / 2.0;
    painter.circle_filled(rect.center(), r, p.surface_3);
    let inner = Rect::from_center_size(rect.center(), vec2(r * 1.2, r * 1.2));
    t::logo(painter, inner, pose != MascotPose::Sleeping);
}

/// Пустое состояние: маскот, заголовок, подпись, кнопка. Возвращает true по нажатию кнопки.
pub fn empty_state(ui: &mut Ui, pose: MascotPose, title: &str, subtitle: &str, action: Option<&str>) -> bool {
    let mut clicked = false;
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(120.0, 120.0), Sense::hover());
        mascot(ui.painter(), rect, pose);
        ui.add_space(14.0);
        ui.label(RichText::new(title).font(t::xbold(20.0)).color(t::p().text));
        ui.add_space(4.0);
        ui.label(RichText::new(subtitle).size(14.5).color(t::p().text_muted));
        if let Some(a) = action {
            ui.add_space(14.0);
            clicked = button(ui, ButtonKind::Primary, a, None).clicked();
        }
    });
    let _ = pos2;
    clicked
}
