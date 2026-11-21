//! Мелочи: индикатор уровня, спиннер.

use eframe::egui::{self, CornerRadius, Rect, Sense, Ui, vec2};

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
