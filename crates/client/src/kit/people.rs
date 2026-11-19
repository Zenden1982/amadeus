//! Люди: аватар (кольцо «говорит», значки mute/deaf), плитка участника, рамка трансляции, «В ЭФИРЕ».

use eframe::egui::{self, Align2, Color32, CornerRadius, Painter, Pos2, Rect, Stroke, StrokeKind, Ui, pos2, vec2};

use super::R_CARD;
use crate::theme as t;

/// Кого рисуем: имя (буква и цвет) и картинка, если есть.
pub struct AvatarInput<'a> {
    pub name: &'a str,
    pub texture: Option<egui::TextureId>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PeerBadge {
    None,
    MicOff,
    Deaf,
}

/// Круглый аватар. `speaking` 0..1 — кольцо цвета «говорит» (сила = громкость); `dim` — приглушить (заглушён).
pub fn avatar(painter: &Painter, center: Pos2, r: f32, who: &AvatarInput, speaking: f32, badge: PeerBadge, dim: bool) {
    let p = t::p();
    if speaking > 0.01 {
        let a = 0.45 + 0.55 * speaking.clamp(0.0, 1.0);
        painter.circle_filled(center, r + 6.0 + 3.0 * speaking, p.speaking.gamma_multiply(0.10 * a));
        painter.circle_stroke(center, r + 3.5, Stroke::new(2.5, p.speaking.gamma_multiply(a)));
    }
    match who.texture {
        Some(tex) => {
            let rect = Rect::from_center_size(center, vec2(r * 2.0, r * 2.0));
            let tint = if dim { Color32::from_gray(140) } else { Color32::WHITE };
            painter.add(egui::Shape::image(tex, rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint));
        }
        None => {
            let mut c = t::avatar_color(who.name);
            if dim {
                c = c.gamma_multiply(0.55);
            }
            painter.circle_filled(center, r, c);
            let letter: String = who.name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
            painter.text(center, Align2::CENTER_CENTER, letter, t::xbold(r * 0.85), Color32::WHITE);
        }
    }
    let icon = match badge {
        PeerBadge::None => return,
        PeerBadge::MicOff => egui_phosphor::fill::MICROPHONE_SLASH,
        PeerBadge::Deaf => egui_phosphor::fill::SPEAKER_SLASH,
    };
    let br = (r * 0.4).max(8.0);
    let bc = center + vec2(r * 0.72, r * 0.72);
    painter.circle_filled(bc, br + 2.0, p.surface);
    painter.circle_filled(bc, br, p.danger);
    painter.text(bc, Align2::CENTER_CENTER, icon, t::icon_fill(br * 1.2), Color32::WHITE);
}

/// Данные плитки участника.
pub struct PeerTile<'a> {
    pub who: AvatarInput<'a>,
    pub name: &'a str,
    /// 0..1 — сила кольца «говорит».
    pub speaking: f32,
    pub badge: PeerBadge,
    pub is_local: bool,
    pub streaming: bool,
}

/// Плитка участника в сетке голосового канала.
pub fn peer_tile(ui: &Ui, rect: Rect, tile: &PeerTile) {
    let p = t::p();
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(R_CARD), p.surface_2);
    let talk = tile.speaking > 0.05;
    let stroke = if talk { Stroke::new(2.0, p.speaking.gamma_multiply(0.4 + 0.6 * tile.speaking)) } else { Stroke::new(1.0, p.border) };
    painter.rect_stroke(rect, CornerRadius::same(R_CARD), stroke, StrokeKind::Inside);
    let r = (rect.height().min(rect.width()) * 0.22).clamp(24.0, 46.0);
    let ac = rect.center() - vec2(0.0, r * 0.35);
    avatar(painter, ac, r, &tile.who, tile.speaking, tile.badge, tile.badge != PeerBadge::None && !tile.is_local);
    painter.text(pos2(rect.center().x, ac.y + r + 20.0), Align2::CENTER_CENTER, tile.name, t::bold(15.0), p.text);
    if tile.streaming {
        live_badge(painter, pos2(rect.right() - 12.0, rect.top() + 12.0));
    }
    if tile.is_local {
        let tag = painter.layout_no_wrap("это вы".into(), t::semibold(12.0), p.text_muted);
        let tr = Rect::from_min_size(rect.min + vec2(12.0, 12.0), tag.size() + vec2(18.0, 8.0));
        painter.rect_filled(tr, CornerRadius::same(255), p.surface_3);
        painter.galley(tr.min + vec2(9.0, 4.0), tag, p.text_muted);
    }
}

/// Фон плитки трансляции (под кадр видео), `hovered` — обводка акцентом.
pub fn stream_tile_frame(painter: &Painter, rect: Rect, hovered: bool) {
    let p = t::p();
    painter.rect_filled(rect, CornerRadius::same(R_CARD), p.video_bg);
    let stroke = if hovered { Stroke::new(2.0, p.accent) } else { Stroke::new(1.0, p.border) };
    painter.rect_stroke(rect, CornerRadius::same(R_CARD), stroke, StrokeKind::Inside);
}

/// Плашка «В ЭФИРЕ» (правый верхний угол = `right_top`): красная «таблетка» с точкой.
pub fn live_badge(painter: &Painter, right_top: Pos2) {
    let galley = painter.layout_no_wrap("В ЭФИРЕ".into(), t::xbold(11.0), Color32::WHITE);
    let size = galley.size() + vec2(26.0, 8.0);
    let r = Rect::from_min_size(pos2(right_top.x - size.x, right_top.y), size);
    painter.rect_filled(r, CornerRadius::same(255), t::p().danger);
    painter.circle_filled(pos2(r.left() + 10.0, r.center().y), 3.0, Color32::WHITE);
    painter.galley(pos2(r.left() + 17.0, r.center().y - galley.size().y / 2.0), galley, Color32::WHITE);
}
