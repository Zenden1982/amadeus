//! Окна поверх: модальное окно, пункты меню, плашки-уведомления.

use eframe::egui::{self, Align2, CornerRadius, Frame, Id, Margin, Response, RichText, Sense, Stroke, Ui, pos2, vec2};

use super::{HOVER_S, IconState, R_FIELD, R_ROW, icon_button};
use crate::theme as t;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ModalResult {
    Open,
    /// Закрыли: крестик, Esc или клик по затемнению.
    Closed,
}

/// Модальное окно: затемнение, карточка со скруглением 18, заголовок и крестик.
pub fn modal(ctx: &egui::Context, id: &str, title: &str, width: f32, body: impl FnOnce(&mut Ui)) -> ModalResult {
    let p = t::p();
    let frame = Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(18))
        .inner_margin(Margin::same(22))
        .shadow(ctx.global_style().visuals.window_shadow);
    let mut close = false;
    let resp = egui::Modal::new(Id::new(id)).frame(frame).backdrop_color(p.scrim).show(ctx, |ui| {
        ui.set_width(width);
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).font(t::xbold(19.0)).color(p.text));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_button(ui, egui_phosphor::regular::X, IconState::Normal, "Закрыть").clicked() {
                    close = true;
                }
            });
        });
        ui.add_space(10.0);
        body(ui);
    });
    if close || resp.should_close() { ModalResult::Closed } else { ModalResult::Open }
}

/// Пункт всплывающего меню (высота 34).
pub fn menu_item(ui: &mut Ui, icon: &str, text: &str, danger: bool) -> Response {
    let w = ui.available_width().max(180.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 34.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), HOVER_S);
    let p = t::p();
    let painter = ui.painter();
    let (bg, fg) = if danger { (p.danger_soft, p.danger_text) } else { (p.row_hover, p.text) };
    painter.rect_filled(rect, CornerRadius::same(R_ROW), egui::Color32::TRANSPARENT.lerp_to_gamma(bg, hover));
    painter.text(pos2(rect.left() + 18.0, rect.center().y), Align2::CENTER_CENTER, icon, t::icon_fill(17.0), fg);
    painter.text(pos2(rect.left() + 34.0, rect.center().y), Align2::LEFT_CENTER, text, t::semibold(14.5), fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Notice {
    Error,
    Warn,
    Info,
}

/// Плашка-уведомление на всю ширину. Возвращает true, если её закрыли крестиком.
pub fn notice(ui: &mut Ui, kind: Notice, text: &str, dismissible: bool) -> bool {
    let p = t::p();
    let (bg, border, fg, icon) = match kind {
        Notice::Error => (p.error_bg, p.error_border, p.danger_text, egui_phosphor::fill::WARNING_CIRCLE),
        Notice::Warn => (p.warn_bg, p.warn_border, p.text, egui_phosphor::fill::WARNING),
        Notice::Info => (p.accent_soft, p.accent.gamma_multiply(0.5), p.text, egui_phosphor::fill::INFO),
    };
    let mut closed = false;
    Frame::new()
        .fill(bg)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(CornerRadius::same(R_FIELD))
        .inner_margin(Margin::symmetric(14, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(icon).font(t::icon_fill(17.0)).color(fg));
                ui.label(RichText::new(text).color(fg).size(14.0));
                if dismissible {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_button(ui, egui_phosphor::regular::X, IconState::Normal, "Скрыть").clicked() {
                            closed = true;
                        }
                    });
                }
            });
        });
    closed
}
