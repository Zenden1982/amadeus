//! Кнопки: «таблетки», иконки-кнопки, кнопки дока.

use eframe::egui::{self, Align2, Color32, CornerRadius, Response, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use super::{HOVER_S, R_PILL};
use crate::theme as t;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ButtonKind {
    /// Главное действие — акцент.
    Primary,
    /// Обычная — нейтральная подложка.
    Secondary,
    /// Опасное действие — красная.
    Danger,
    /// Без подложки, подсветка при наведении.
    Ghost,
}

fn colors(kind: ButtonKind, hover: f32, enabled: bool) -> (Color32, Color32) {
    let p = t::p();
    let (fill, text) = match kind {
        ButtonKind::Primary => (p.accent.lerp_to_gamma(p.accent_hover, hover), p.on_accent),
        ButtonKind::Secondary => (p.surface_3.lerp_to_gamma(p.row_active, hover), p.text),
        ButtonKind::Danger => (p.danger.lerp_to_gamma(p.danger_hover, hover), Color32::WHITE),
        ButtonKind::Ghost => (Color32::TRANSPARENT.lerp_to_gamma(p.row_hover, hover), p.text),
    };
    if enabled { (fill, text) } else { (fill.gamma_multiply(0.5), text.gamma_multiply(0.6)) }
}

fn pill(ui: &mut Ui, kind: ButtonKind, text: &str, icon: Option<&str>, width: Option<f32>, height: f32, enabled: bool) -> Response {
    let font = t::bold(14.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, Color32::PLACEHOLDER);
    let icon_w = if icon.is_some() { 18.0 + 8.0 } else { 0.0 };
    let content_w = icon_w + galley.size().x;
    let w = width.unwrap_or(content_w + 2.0 * 18.0);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, height), sense);
    let hover = ui.ctx().animate_bool_with_time(resp.id, enabled && resp.hovered(), HOVER_S);
    let (fill, fg) = colors(kind, hover, enabled);
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(R_PILL), fill);
    let mut x = rect.center().x - content_w / 2.0;
    if let Some(i) = icon {
        painter.text(pos2(x + 9.0, rect.center().y), Align2::CENTER_CENTER, i, t::icon_fill(18.0), fg);
        x += icon_w;
    }
    painter.galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, fg);
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

/// Кнопка-«таблетка» по размеру текста (высота 40).
pub fn button(ui: &mut Ui, kind: ButtonKind, text: &str, icon: Option<&str>) -> Response {
    pill(ui, kind, text, icon, None, 40.0, true)
}

/// Кнопка-«таблетка», которую можно выключить (серая, не нажимается).
pub fn button_ex(ui: &mut Ui, kind: ButtonKind, text: &str, icon: Option<&str>, enabled: bool) -> Response {
    pill(ui, kind, text, icon, None, 40.0, enabled)
}

/// Широкая кнопка заданной ширины (высота 44) — главное действие формы.
pub fn button_wide(ui: &mut Ui, kind: ButtonKind, text: &str, width: f32, enabled: bool) -> Response {
    pill(ui, kind, text, None, Some(width), 44.0, enabled)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IconState {
    Normal,
    /// Включено/выбрано — акцент.
    Active,
    /// Выключено (микрофон, звук) — красная.
    Off,
}

/// Круглая иконка-кнопка 36×36.
pub fn icon_button(ui: &mut Ui, icon: &str, state: IconState, tip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), HOVER_S);
    let p = t::p();
    let (bg, fg) = match state {
        IconState::Normal => (Color32::TRANSPARENT.lerp_to_gamma(p.row_hover, hover), p.text_muted.lerp_to_gamma(p.text, hover)),
        IconState::Active => (p.accent_soft, p.accent),
        IconState::Off => (p.danger_soft, p.danger),
    };
    let painter = ui.painter();
    painter.circle_filled(rect.center(), 18.0, bg);
    let font = if state == IconState::Normal { t::font(19.0) } else { t::icon_fill(19.0) };
    painter.text(rect.center(), Align2::CENTER_CENTER, icon, font, fg);
    resp.on_hover_text(tip)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DockStyle {
    Normal,
    /// Включено и передаётся (микрофон, трансляция).
    Live,
    /// Выключено.
    Off,
    /// Выйти из канала.
    Leave,
}

/// Кнопка дока: иконка + подпись, «таблетка» высотой 46.
pub fn dock_button(ui: &mut Ui, icon: &str, label: &str, style: DockStyle) -> Response {
    let galley = ui.painter().layout_no_wrap(label.to_string(), t::bold(14.0), Color32::PLACEHOLDER);
    let w = 18.0 + 20.0 + 9.0 + galley.size().x + 20.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 46.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), HOVER_S);
    let p = t::p();
    let (fill, icon_c, text_c, stroke) = match style {
        DockStyle::Normal => (p.surface_3.lerp_to_gamma(p.row_active, hover), p.text, p.text, Color32::TRANSPARENT),
        DockStyle::Live => (p.accent_soft, p.accent, p.text, p.accent.gamma_multiply(0.45)),
        DockStyle::Off => (p.danger_soft, p.danger, p.text, p.danger.gamma_multiply(0.4)),
        DockStyle::Leave => (p.danger.lerp_to_gamma(p.danger_hover, hover), Color32::WHITE, Color32::WHITE, Color32::TRANSPARENT),
    };
    let painter = ui.painter();
    painter.rect(rect, CornerRadius::same(R_PILL), fill, Stroke::new(1.0, stroke), StrokeKind::Inside);
    let font = if style == DockStyle::Normal { t::font(20.0) } else { t::icon_fill(20.0) };
    painter.text(pos2(rect.left() + 18.0 + 10.0, rect.center().y), Align2::CENTER_CENTER, icon, font, icon_c);
    painter.galley(pos2(rect.left() + 18.0 + 20.0 + 9.0, rect.center().y - galley.size().y / 2.0), galley, text_c);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
