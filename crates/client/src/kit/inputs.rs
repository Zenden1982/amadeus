//! Поля ввода, переключатель, ползунок, выпадающий список, клавиша.

use eframe::egui::{self, Color32, CornerRadius, Response, Sense, Stroke, StrokeKind, TextEdit, Ui, pos2, vec2};

use super::HOVER_S;
use crate::theme as t;

/// Поле ввода в стиле набора (скругление — из визуалов темы, R_FIELD). Пароль/подсказка — методами TextEdit.
pub fn text_field(text: &mut String, width: f32) -> TextEdit<'_> {
    TextEdit::singleline(text).desired_width(width).margin(vec2(14.0, 10.0)).font(t::font(15.0))
}

/// Многострочное поле (чат).
pub fn text_area(text: &mut String, width: f32) -> TextEdit<'_> {
    TextEdit::multiline(text).desired_width(width).margin(vec2(14.0, 10.0)).font(t::font(15.0))
}

/// Переключатель 44×24: дорожка-«таблетка», белый кружок.
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(44.0, 24.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let k = ui.ctx().animate_bool_with_time(resp.id, *on, HOVER_S);
    let p = t::p();
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(12), p.border_strong.lerp_to_gamma(p.accent, k));
    let x = egui::lerp(rect.left() + 12.0..=rect.right() - 12.0, k);
    painter.circle_filled(pos2(x, rect.center().y + 0.5), 9.5, Color32::from_black_alpha(30));
    painter.circle_filled(pos2(x, rect.center().y), 9.0, Color32::WHITE);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Доля значения в диапазоне 0..1 (пустой диапазон — 0, без деления на ноль).
pub fn slider_fraction(value: f32, lo: f32, hi: f32) -> f32 {
    if hi <= lo {
        return 0.0;
    }
    ((value - lo) / (hi - lo)).clamp(0.0, 1.0)
}

/// Ползунок: тонкая дорожка, заполнение акцентом, белая ручка с обводкой.
pub fn slider(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, width: f32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, 22.0), Sense::click_and_drag());
    let (lo, hi) = (*range.start(), *range.end());
    if let Some(pos) = resp.interact_pointer_pos() {
        let k = ((pos.x - rect.left() - 9.0) / (rect.width() - 18.0)).clamp(0.0, 1.0);
        let nv = lo + k * (hi - lo);
        if (nv - *value).abs() > f32::EPSILON {
            *value = nv;
            resp.mark_changed();
        }
    }
    let k = slider_fraction(*value, lo, hi);
    let track = egui::Rect::from_center_size(rect.center(), vec2(rect.width() - 18.0, 6.0));
    let p = t::p();
    let painter = ui.painter();
    painter.rect_filled(track, CornerRadius::same(3), p.surface_3);
    let x = track.left() + track.width() * k;
    painter.rect_filled(egui::Rect::from_min_max(track.min, pos2(x, track.max.y)), CornerRadius::same(3), p.accent);
    let c = pos2(x, rect.center().y);
    painter.circle_filled(c, 9.0, Color32::WHITE);
    painter.circle_stroke(c, 9.0, Stroke::new(1.0, p.border_strong));
    resp
}

/// Выпадающий список. Возвращает true, если выбор изменился.
pub fn combo<T: PartialEq + Clone>(ui: &mut Ui, id: &str, selected: &mut T, items: &[(T, String)], width: f32) -> bool {
    let before = selected.clone();
    let current = items.iter().find(|(v, _)| v == selected).map(|(_, s)| s.as_str()).unwrap_or("");
    egui::ComboBox::from_id_salt(id).selected_text(current).width(width).height(320.0).show_ui(ui, |ui| {
        for (v, label) in items {
            ui.selectable_value(selected, v.clone(), label);
        }
    });
    *selected != before
}

/// Клавиша в рамке (назначение рации).
pub fn keycap(ui: &mut Ui, text: &str) -> Response {
    let p = t::p();
    let galley = ui.painter().layout_no_wrap(text.to_string(), t::bold(13.0), p.text);
    let size = galley.size() + vec2(18.0, 8.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect.translate(vec2(0.0, 2.0)), CornerRadius::same(8), p.border_strong);
    painter.rect(rect, CornerRadius::same(8), p.surface, Stroke::new(1.0, p.border_strong), StrokeKind::Inside);
    painter.galley(rect.center() - galley.size() / 2.0, galley, p.text);
    resp
}
