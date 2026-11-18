//! «Лаборатория ночью»: палитра, шрифты и рисованные элементы из design/mockup-phase3.html.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect, Response, Sense,
    Shape, Stroke, StrokeKind, TextStyle, Ui, Vec2, pos2, vec2,
};

pub const GLASS: Color32 = Color32::from_rgb(0x16, 0x12, 0x0F);
pub const BAKELITE: Color32 = Color32::from_rgb(0x21, 0x1B, 0x16);
pub const BAKELITE_2: Color32 = Color32::from_rgb(0x2A, 0x22, 0x1C);
pub const BRASS: Color32 = Color32::from_rgb(0x4A, 0x3B, 0x2D);
pub const BRASS_DIM: Color32 = Color32::from_rgb(0x2F, 0x26, 0x1E);
pub const BRASS_LOGO: Color32 = Color32::from_rgb(0x5C, 0x4A, 0x36); // ободки ламп рамки
pub const NIXIE: Color32 = Color32::from_rgb(0xFF, 0x9A, 0x3C);
pub const FILAMENT: Color32 = Color32::from_rgb(0xFF, 0xD9, 0xA8);
pub const PAPER: Color32 = Color32::from_rgb(0xEC, 0xE3, 0xD6);
pub const DUST: Color32 = Color32::from_rgb(0x9A, 0x8C, 0x7C);
pub const SIGNAL: Color32 = Color32::from_rgb(0xE5, 0x53, 0x4B);
pub const ERR_TEXT: Color32 = Color32::from_rgb(0xF0, 0xA3, 0x9E);
pub const DOCK: Color32 = Color32::from_rgb(0x1A, 0x15, 0x11);
pub const ME_BG: Color32 = Color32::from_rgb(0x1C, 0x17, 0x13);
pub const ON_NIXIE: Color32 = Color32::from_rgb(0x2A, 0x16, 0x06);

pub fn nixie_a(a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(0xFF, 0x9A, 0x3C, a)
}

pub const SEMIBOLD: &str = "manrope-semibold";
pub const BOLD: &str = "manrope-bold";
pub const XBOLD: &str = "manrope-xbold";
pub const NIXIE_FONT: &str = "nixie";
pub const PH_FILL: &str = "phosphor-fill";

pub fn font(size: f32) -> FontId {
    FontId::proportional(size)
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}
pub fn xbold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(XBOLD.into()))
}
pub fn nixie(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(NIXIE_FONT.into()))
}
pub fn icon_fill(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(PH_FILL.into()))
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.into(), FontData::from_static(bytes).into());
    };
    add(&mut fonts, "manrope", include_bytes!("../assets/fonts/Manrope-Regular.ttf"));
    add(&mut fonts, SEMIBOLD, include_bytes!("../assets/fonts/Manrope-SemiBold.ttf"));
    add(&mut fonts, BOLD, include_bytes!("../assets/fonts/Manrope-Bold.ttf"));
    add(&mut fonts, XBOLD, include_bytes!("../assets/fonts/Manrope-ExtraBold.ttf"));
    add(&mut fonts, NIXIE_FONT, include_bytes!("../assets/fonts/NixieOne-Regular.ttf"));
    add(&mut fonts, "phosphor", egui_phosphor::Variant::Regular.font_bytes());
    add(&mut fonts, PH_FILL, egui_phosphor::Variant::Fill.font_bytes());

    // Стандартные шрифты egui остаются запасными (символы, которых нет в Manrope).
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let family = |first: &str, icons: &str| {
        let mut v = vec![first.to_string(), icons.to_string()];
        v.extend(fallback.iter().cloned());
        v
    };
    fonts.families.insert(FontFamily::Proportional, family("manrope", "phosphor"));
    for name in [SEMIBOLD, BOLD, XBOLD] {
        fonts.families.insert(FontFamily::Name(name.into()), family(name, "phosphor"));
    }
    // Nixie One — только цифры и латиница; остальное — Manrope.
    fonts.families.insert(FontFamily::Name(NIXIE_FONT.into()), family(NIXIE_FONT, "manrope"));
    fonts.families.insert(FontFamily::Name(PH_FILL.into()), family(PH_FILL, "manrope"));
    ctx.set_fonts(fonts);

    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, font(12.5)),
            (TextStyle::Body, font(15.0)),
            (TextStyle::Button, semibold(14.0)),
            (TextStyle::Heading, xbold(18.0)),
            (TextStyle::Monospace, FontId::monospace(14.0)),
        ]
        .into();
        style.spacing.item_spacing = vec2(8.0, 8.0);
        style.spacing.button_padding = vec2(14.0, 8.0);
        style.spacing.interact_size.y = 34.0;
        style.spacing.combo_height = 300.0;
        style.interaction.selectable_labels = false;

        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.override_text_color = Some(PAPER);
        v.panel_fill = GLASS;
        v.window_fill = BAKELITE_2;
        v.window_stroke = Stroke::new(1.0, BRASS);
        v.window_corner_radius = CornerRadius::same(10);
        v.menu_corner_radius = CornerRadius::same(8);
        v.extreme_bg_color = GLASS;
        v.faint_bg_color = BAKELITE;
        v.code_bg_color = GLASS;
        v.hyperlink_color = PAPER;
        v.selection.bg_fill = nixie_a(70);
        v.selection.stroke = Stroke::new(1.5, NIXIE);
        v.text_cursor.stroke = Stroke::new(2.0, NIXIE);
        v.popup_shadow = egui::epaint::Shadow { offset: [0, 12], blur: 30, spread: 0, color: Color32::from_black_alpha(140) };
        v.window_shadow = v.popup_shadow;

        let w = &mut v.widgets;
        for (wv, fill, stroke) in [
            (&mut w.noninteractive, BAKELITE, BRASS_DIM),
            (&mut w.inactive, BAKELITE_2, Color32::from_rgb(0x3A, 0x2F, 0x25)),
            (&mut w.hovered, Color32::from_rgb(0x33, 0x29, 0x21), BRASS),
            (&mut w.active, Color32::from_rgb(0x3A, 0x2F, 0x25), NIXIE),
            (&mut w.open, BAKELITE_2, BRASS),
        ] {
            wv.bg_fill = fill;
            wv.weak_bg_fill = fill;
            wv.bg_stroke = Stroke::new(1.0, stroke);
            wv.corner_radius = CornerRadius::same(8);
            wv.fg_stroke = Stroke::new(1.5, PAPER);
        }
        w.noninteractive.fg_stroke = Stroke::new(1.0, DUST);
        w.inactive.fg_stroke = Stroke::new(1.5, PAPER);
    });
}

/// Цвет аватара — стабильный по имени (палитра из макета).
pub fn avatar_color(name: &str) -> Color32 {
    const P: [Color32; 6] = [
        Color32::from_rgb(0xE9, 0xB4, 0x6A),
        Color32::from_rgb(0x8F, 0xB3, 0xC9),
        Color32::from_rgb(0xC7, 0xA2, 0xD8),
        Color32::from_rgb(0xA8, 0xC7, 0x9A),
        Color32::from_rgb(0xD9, 0x9A, 0x8F),
        Color32::from_rgb(0xC9, 0xC2, 0x8F),
    ];
    let h = name.to_lowercase().bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    P[(h % P.len() as u32) as usize]
}

/// Плавное радиальное свечение (меш с градиентом по вершинам — без «колец»).
pub fn radial_glow(painter: &egui::Painter, center: Pos2, radius: f32, color: Color32) {
    let mut mesh = egui::Mesh::default();
    let segs = 48;
    let rings = [(0.0, 1.0), (0.35, 0.6), (0.7, 0.22), (1.0, 0.0)];
    mesh.colored_vertex(center, color);
    for &(r, a) in &rings[1..] {
        for i in 0..segs {
            let ang = i as f32 / segs as f32 * std::f32::consts::TAU;
            mesh.colored_vertex(center + vec2(ang.cos(), ang.sin()) * radius * r, color.gamma_multiply(a));
        }
    }
    let seg = segs as u32;
    for i in 0..seg {
        let j = (i + 1) % seg;
        mesh.add_triangle(0, 1 + i, 1 + j);
    }
    for ring in 0..(rings.len() as u32 - 2) {
        let a0 = 1 + ring * seg;
        let b0 = a0 + seg;
        for i in 0..seg {
            let j = (i + 1) % seg;
            mesh.add_triangle(a0 + i, b0 + i, b0 + j);
            mesh.add_triangle(a0 + i, b0 + j, a0 + j);
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// Круглый аватар с буквой. `glow` 0..1 — янтарное кольцо говорящего.
pub fn avatar(painter: &egui::Painter, center: Pos2, r: f32, name: &str, glow: f32, dim: bool) {
    if glow > 0.01 {
        radial_glow(painter, center, r + 6.0 + 22.0 * glow, nixie_a((150.0 * glow) as u8));
        painter.circle_stroke(center, r + 4.0, Stroke::new(2.5, NIXIE.gamma_multiply(0.5 + 0.5 * glow)));
    }
    let mut c = avatar_color(name);
    if dim {
        c = c.gamma_multiply(0.55);
    }
    painter.circle_filled(center, r, c);
    let letter: String = name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
    painter.text(center, Align2::CENTER_CENTER, letter, xbold(r * 0.8), Color32::from_rgb(0x1B, 0x13, 0x0C));
}

/// Текст «газоразрядной лампой»: янтарь + мягкое свечение.
pub fn nixie_text(painter: &egui::Painter, pos: Pos2, anchor: Align2, text: &str, size: f32) -> Rect {
    let fid = nixie(size);
    let spread = (size / 18.0).clamp(1.0, 5.0);
    for (i, (dx, dy)) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0), (0.7, 0.7), (-0.7, -0.7), (0.7, -0.7), (-0.7, 0.7)]
        .iter()
        .enumerate()
    {
        let a = if i < 4 { 46 } else { 30 };
        painter.text(pos + vec2(dx * spread, dy * spread), anchor, text, fid.clone(), Color32::from_rgba_unmultiplied(255, 120, 30, a));
    }
    painter.text(pos, anchor, text, fid, NIXIE)
}

/// Логотип «Осциллограф»: «A» — всплеск на линии сигнала, перекладина — короткая волна. Без фона (минимализм).
/// Координаты как в design/logo.svg (64×64). `lit` = false — приглушённый (неактивное окно).
pub fn logo(painter: &egui::Painter, rect: Rect, lit: bool) {
    let s = rect.width() / 64.0;
    let p = |x: f32, y: f32| pos2(rect.left() + x * s, rect.top() + y * s);
    let trace = vec![p(7.0, 46.0), p(18.0, 46.0), p(32.0, 13.0), p(46.0, 46.0), p(57.0, 46.0)];
    let wave = vec![p(25.5, 34.0), p(29.5, 34.0), p(31.0, 31.0), p(33.0, 37.0), p(34.5, 34.0), p(37.5, 34.0)];
    let core = (3.2 * s).max(1.7);
    if lit {
        for (k, a) in [(3.0, 22u8), (2.0, 40), (1.4, 70)] {
            painter.add(Shape::line(trace.clone(), Stroke::new(core * k, nixie_a(a))));
            painter.add(Shape::line(wave.clone(), Stroke::new(core * k, nixie_a(a))));
        }
    }
    let color = if lit { NIXIE } else { NIXIE.gamma_multiply(0.45) };
    painter.add(Shape::line(trace, Stroke::new(core, color)));
    painter.add(Shape::line(wave, Stroke::new(core, color)));
    if lit && s > 1.0 {
        painter.circle_filled(p(32.0, 13.0), 1.2 * s, FILAMENT);
    }
}

/// Логотип + «Amadeus».
pub fn wordmark(ui: &mut Ui, logo_size: f32, text_size: f32) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(logo_size), Sense::hover());
        logo(ui.painter(), rect, true);
        ui.add_space(2.0);
        ui.label(egui::RichText::new("Amadeus").font(xbold(text_size)).color(PAPER));
    });
}

/// Переключатель как в макете (янтарь = вкл).
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(40.0, 22.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = ui.ctx().animate_bool(resp.id, *on);
    let p = ui.painter();
    let bg = Color32::from_rgb(0x3A, 0x2F, 0x25).lerp_to_gamma(NIXIE, t);
    p.rect_filled(rect, CornerRadius::same(11), bg);
    let x = egui::lerp(rect.left() + 11.0..=rect.right() - 11.0, t);
    p.circle_filled(pos2(x, rect.center().y), 8.0, DUST.lerp_to_gamma(ON_NIXIE, t));
    resp
}

/// Ползунок громкости: янтарная дорожка + светящаяся «нить».
pub fn slider(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, width: f32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, 20.0), Sense::click_and_drag());
    let (lo, hi) = (*range.start(), *range.end());
    if let Some(pos) = resp.interact_pointer_pos() {
        let t = ((pos.x - rect.left() - 8.0) / (rect.width() - 16.0)).clamp(0.0, 1.0);
        let nv = lo + t * (hi - lo);
        if (nv - *value).abs() > f32::EPSILON {
            *value = nv;
            resp.mark_changed();
        }
    }
    let t = ((*value - lo) / (hi - lo)).clamp(0.0, 1.0);
    let track = Rect::from_center_size(rect.center(), vec2(rect.width() - 16.0, 6.0));
    let p = ui.painter();
    p.rect_filled(track, CornerRadius::same(3), GLASS);
    let x = track.left() + track.width() * t;
    p.rect_filled(Rect::from_min_max(track.min, pos2(x, track.max.y)), CornerRadius::same(3), NIXIE);
    p.circle_filled(pos2(x, rect.center().y), 13.0, nixie_a(40));
    p.circle_filled(pos2(x, rect.center().y), 8.0, FILAMENT);
    resp
}

/// Клавиша в рамке (как `.key` в макете).
pub fn keycap(ui: &mut Ui, text: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), bold(12.5), PAPER);
    let size = galley.size() + vec2(16.0, 6.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect.translate(vec2(0.0, 2.0)), CornerRadius::same(6), BRASS);
    p.rect(rect, CornerRadius::same(6), GLASS, Stroke::new(1.0, BRASS), StrokeKind::Inside);
    p.galley(rect.center() - galley.size() / 2.0, galley, PAPER);
    resp
}

/// Квадратная иконка-кнопка (34×34). `hot` — красная (выключено).
pub fn icon_button(ui: &mut Ui, icon: &str, hot: bool, filled: bool, tooltip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
    let p = ui.painter();
    if hot {
        p.rect_filled(rect, CornerRadius::same(8), Color32::from_rgba_unmultiplied(229, 83, 75, 30));
    } else if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(8), BAKELITE_2);
    }
    let color = if hot { SIGNAL } else if resp.hovered() { PAPER } else { DUST };
    let f = if filled { icon_fill(20.0) } else { font(20.0) };
    p.text(rect.center(), Align2::CENTER_CENTER, icon, f, color);
    resp.on_hover_text(tooltip)
}

#[derive(Clone, Copy, PartialEq)]
pub enum DockStyle {
    Normal,
    Live,
    Off,
    Leave,
}

/// Большая кнопка дока: иконка + подпись.
pub fn dock_button(ui: &mut Ui, icon: &str, label: &str, style: DockStyle) -> Response {
    let font_l = semibold(14.0);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font_l, PAPER);
    let w = 18.0 + 22.0 + 10.0 + galley.size().x + 18.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 46.0), Sense::click());
    let p = ui.painter();
    let (fill, stroke) = match style {
        DockStyle::Leave => (if resp.hovered() { Color32::from_rgb(0xF0, 0x62, 0x5A) } else { SIGNAL }, SIGNAL),
        _ => (if resp.hovered() { Color32::from_rgb(0x33, 0x29, 0x21) } else { BAKELITE_2 }, Color32::from_rgb(0x3A, 0x2F, 0x25)),
    };
    p.rect(rect, CornerRadius::same(10), fill, Stroke::new(1.0, stroke), StrokeKind::Inside);
    let icon_pos = pos2(rect.left() + 18.0 + 11.0, rect.center().y);
    let (icon_color, icon_font) = match style {
        DockStyle::Live => (NIXIE, icon_fill(20.0)),
        DockStyle::Off => (SIGNAL, icon_fill(20.0)),
        DockStyle::Leave => (Color32::WHITE, font(20.0)),
        DockStyle::Normal => (PAPER, font(20.0)),
    };
    if style == DockStyle::Live {
        p.circle_filled(icon_pos, 12.0, nixie_a(35));
    }
    p.text(icon_pos, Align2::CENTER_CENTER, icon, icon_font, icon_color);
    let text_color = if style == DockStyle::Leave { Color32::WHITE } else { PAPER };
    p.galley(pos2(rect.left() + 18.0 + 22.0 + 10.0, rect.center().y - galley.size().y / 2.0), galley, text_color);
    resp
}

/// Главная янтарная кнопка (на всю ширину).
pub fn primary_button(ui: &mut Ui, text: &str, width: f32, enabled: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 46.0), if enabled { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    let fill = if !enabled {
        NIXIE.gamma_multiply(0.45)
    } else if resp.hovered() {
        Color32::from_rgb(0xFF, 0xAA, 0x55)
    } else {
        NIXIE
    };
    if enabled {
        for i in 0..3 {
            p.rect_filled(rect.expand(2.0 + i as f32 * 3.0), CornerRadius::same(10 + 3 * i as u8), nixie_a(10));
        }
    }
    p.rect_filled(rect, CornerRadius::same(8), fill);
    p.text(rect.center(), Align2::CENTER_CENTER, text, bold(15.0), ON_NIXIE);
    resp
}

/// Русское склонение: 1 участник, 2 участника, 5 участников.
pub fn plural(n: usize, one: &str, few: &str, many: &str) -> String {
    let m10 = n % 10;
    let m100 = n % 100;
    let w = if m10 == 1 && m100 != 11 {
        one
    } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
        few
    } else {
        many
    };
    format!("{n} {w}")
}

#[cfg(test)]
mod tests {
    use super::plural;

    #[test]
    fn plural_ru() {
        let p = |n| plural(n, "участник", "участника", "участников");
        assert_eq!(p(1), "1 участник");
        assert_eq!(p(3), "3 участника");
        assert_eq!(p(5), "5 участников");
        assert_eq!(p(11), "11 участников");
        assert_eq!(p(22), "22 участника");
    }
}
