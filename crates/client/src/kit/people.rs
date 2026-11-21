//! Люди: аватар (буква на цвете по хешу ника или картинка), плитки участника (обычная и компактная), рамка трансляции, «В ЭФИРЕ».
//! Красный здесь не используется: выключенный микрофон — значок цвета `text_faint`, стример — значок трансляции акцентом,
//! «В ЭФИРЕ» — одна метка только на окне трансляции.

use eframe::egui::{self, Align2, Color32, CornerRadius, Painter, Pos2, Rect, Stroke, StrokeKind, Ui, pos2, vec2};

use super::{R_CARD, WaveStyle, wave};
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

/// Круглый аватар. `ring` 0..1 — обводка «говорит» (индикатор «Кольцо»); `dim` — приглушить (заглушён).
pub fn avatar(painter: &Painter, center: Pos2, r: f32, who: &AvatarInput, ring: f32, badge: PeerBadge, dim: bool) {
    let p = t::p();
    if ring > 0.01 {
        let a = 0.45 + 0.55 * ring.clamp(0.0, 1.0);
        painter.circle_filled(center, r + 6.0 + 3.0 * ring, p.speaking.gamma_multiply(0.10 * a));
        painter.circle_stroke(center, r + 3.5, Stroke::new(2.5, p.speaking.gamma_multiply(a)));
    }
    match who.texture {
        Some(tex) => {
            let rect = Rect::from_center_size(center, vec2(r * 2.0, r * 2.0));
            let tint = if dim { Color32::from_gray(140) } else { Color32::WHITE };
            painter.add(egui::Shape::image(tex, rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint));
        }
        None => {
            let (mut bg, mut fg) = t::avatar_colors(who.name);
            if dim {
                bg = bg.gamma_multiply(0.6);
                fg = fg.gamma_multiply(0.6);
            }
            painter.circle_filled(center, r, bg);
            let letter: String = who.name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
            painter.text(center, Align2::CENTER_CENTER, letter, t::xbold(r * 0.85), fg);
        }
    }
    let icon = match badge {
        PeerBadge::None => return,
        PeerBadge::MicOff => egui_phosphor::fill::MICROPHONE_SLASH,
        PeerBadge::Deaf => egui_phosphor::fill::SPEAKER_SLASH,
    };
    let br = (r * 0.4).max(8.0);
    let bc = center + vec2(r * 0.72, r * 0.72);
    painter.circle_filled(bc, br + 1.5, p.border);
    painter.circle_filled(bc, br, p.surface);
    painter.text(bc, Align2::CENTER_CENTER, icon, t::icon_fill(br * 1.2), p.text_faint);
}

/// Данные плитки участника.
pub struct PeerTile<'a> {
    pub who: AvatarInput<'a>,
    pub name: &'a str,
    /// Сглаженная громкость речи 0..1 (0 — молчит).
    pub speaking: f32,
    /// Индикатор речи: Some(сдвиг, px) — волна под ником; None — «кольцо» вокруг аватара.
    pub wave_phase: Option<f32>,
    pub mic_off: bool,
    pub is_local: bool,
    pub streaming: bool,
}

impl PeerTile<'_> {
    fn talking(&self) -> bool {
        self.speaking > 0.02
    }
}

/// Рамка плитки: говорящий — обводка 2 px цвета «говорит» и мягкое свечение.
fn tile_frame(painter: &Painter, rect: Rect, k: f32) {
    let p = t::p();
    if k > 0.02 {
        for (grow, a) in [(6.0, 0.05), (3.0, 0.09)] {
            painter.rect_filled(rect.expand(grow * k), CornerRadius::same(R_CARD + grow as u8), p.speaking.gamma_multiply(a * k));
        }
    }
    painter.rect_filled(rect, CornerRadius::same(R_CARD), p.surface_2);
    let stroke = if k > 0.02 { Stroke::new(2.0, p.speaking.gamma_multiply(0.5 + 0.5 * k)) } else { Stroke::new(1.0, p.border) };
    painter.rect_stroke(rect, CornerRadius::same(R_CARD), stroke, StrokeKind::Inside);
}

/// Строка под ником: волна речи или «микрофон выкл.». `left` — левый край (по центру строки), `centered` — по центру `left.x`.
fn status_line(painter: &Painter, tile: &PeerTile, anchor: Pos2, centered: bool, wave_w: f32) {
    let p = t::p();
    if tile.mic_off {
        let text = format!("{}  микрофон выкл.", egui_phosphor::regular::MICROPHONE_SLASH);
        let align = if centered { Align2::CENTER_CENTER } else { Align2::LEFT_CENTER };
        painter.text(anchor, align, text, t::font(12.5), p.text_faint);
    } else if let (true, Some(phase)) = (tile.talking(), tile.wave_phase) {
        let left = if centered { anchor - vec2(wave_w / 2.0, 0.0) } else { anchor };
        wave(painter, left, wave_w, 1.5 + 5.5 * tile.speaking.clamp(0.0, 1.0), phase, p.speaking, WaveStyle::Solid);
    }
}

/// Значок «транслирует» (акцент) в правом верхнем углу.
fn stream_icon(painter: &Painter, right_top: Pos2) {
    painter.text(right_top, Align2::RIGHT_TOP, egui_phosphor::fill::MONITOR_ARROW_UP, t::icon_fill(18.0), t::p().accent);
}

/// Плитка участника в сетке голосового канала (крупная).
pub fn peer_tile(ui: &Ui, rect: Rect, tile: &PeerTile) {
    let p = t::p();
    let painter = ui.painter();
    let k = if tile.talking() { tile.speaking.clamp(0.0, 1.0).max(0.35) } else { 0.0 };
    tile_frame(painter, rect, k);
    let r = (rect.height().min(rect.width()) * 0.22).clamp(24.0, 46.0);
    let ac = rect.center() - vec2(0.0, r * 0.45);
    let ring = if tile.wave_phase.is_none() { tile.speaking } else { 0.0 };
    avatar(painter, ac, r, &tile.who, ring, PeerBadge::None, tile.mic_off && !tile.is_local);
    let name_y = ac.y + r + 18.0;
    painter.text(pos2(rect.center().x, name_y), Align2::CENTER_CENTER, tile.name, t::bold(15.0), p.text);
    status_line(painter, tile, pos2(rect.center().x, name_y + 20.0), true, 64.0);
    if tile.streaming {
        stream_icon(painter, pos2(rect.right() - 12.0, rect.top() + 11.0));
    }
    if tile.is_local {
        let tag = painter.layout_no_wrap("это вы".into(), t::semibold(12.0), p.text_muted);
        let tr = Rect::from_min_size(rect.min + vec2(12.0, 12.0), tag.size() + vec2(18.0, 8.0));
        painter.rect_filled(tr, CornerRadius::same(255), p.surface_3);
        painter.galley(tr.min + vec2(9.0, 4.0), tag, p.text_muted);
    }
}

/// Размер компактной плитки.
pub const COMPACT_TILE: egui::Vec2 = vec2(200.0, 68.0);

/// Компактная плитка (≈200×68): аватар 40 px, ник, под ником волна речи или «микрофон выкл.».
pub fn peer_tile_compact(ui: &Ui, rect: Rect, tile: &PeerTile) {
    let p = t::p();
    let painter = ui.painter();
    let k = if tile.talking() { tile.speaking.clamp(0.0, 1.0).max(0.35) } else { 0.0 };
    tile_frame(painter, rect, k);
    let ac = pos2(rect.left() + 14.0 + 20.0, rect.center().y);
    let ring = if tile.wave_phase.is_none() { tile.speaking } else { 0.0 };
    avatar(painter, ac, 20.0, &tile.who, ring, PeerBadge::None, tile.mic_off && !tile.is_local);
    let x = ac.x + 20.0 + 12.0;
    let right = rect.right() - if tile.streaming { 36.0 } else { 12.0 };
    let has_line = tile.mic_off || (tile.talking() && tile.wave_phase.is_some());
    let name_y = if has_line || tile.is_local { rect.center().y - 9.0 } else { rect.center().y };
    let name = painter.layout(tile.name.to_string(), t::bold(14.0), p.text, right - x);
    let name = if name.rows.len() > 1 {
        let mut s: String = tile.name.chars().take(14).collect();
        s.push('…');
        painter.layout_no_wrap(s, t::bold(14.0), p.text)
    } else {
        name
    };
    painter.galley(pos2(x, name_y - name.size().y / 2.0), name, p.text);
    if has_line {
        status_line(painter, tile, pos2(x, rect.center().y + 11.0), false, 64.0);
    } else if tile.is_local {
        painter.text(pos2(x, rect.center().y + 11.0), Align2::LEFT_CENTER, "это вы", t::font(12.5), p.text_faint);
    }
    if tile.streaming {
        painter.text(pos2(rect.right() - 14.0, rect.center().y), Align2::RIGHT_CENTER, egui_phosphor::fill::MONITOR_ARROW_UP, t::icon_fill(18.0), p.accent);
    }
}

/// Фон плитки трансляции (под кадр видео), `hovered` — обводка акцентом.
pub fn stream_tile_frame(painter: &Painter, rect: Rect, hovered: bool) {
    let p = t::p();
    painter.rect_filled(rect, CornerRadius::same(R_CARD), p.video_bg);
    let stroke = if hovered { Stroke::new(2.0, p.accent) } else { Stroke::new(1.0, p.border) };
    painter.rect_stroke(rect, CornerRadius::same(R_CARD), stroke, StrokeKind::Inside);
}

/// Плашка «В ЭФИРЕ» (правый верхний угол = `right_top`): красная «таблетка» с белой точкой. Только на окне трансляции.
pub fn live_badge(painter: &Painter, right_top: Pos2) {
    let galley = painter.layout_no_wrap("В ЭФИРЕ".into(), t::xbold(11.0), Color32::WHITE);
    let size = galley.size() + vec2(26.0, 8.0);
    let r = Rect::from_min_size(pos2(right_top.x - size.x, right_top.y), size);
    painter.rect_filled(r, CornerRadius::same(255), t::p().danger);
    painter.circle_filled(pos2(r.left() + 10.0, r.center().y), 3.0, Color32::WHITE);
    painter.galley(pos2(r.left() + 17.0, r.center().y - galley.size().y / 2.0), galley, Color32::WHITE);
}
