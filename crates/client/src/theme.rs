//! «Лаборатория ночью»: палитра, шрифты и рисованные элементы из design/mockup-phase3.html.

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect, Response, Sense,
    Shape, Stroke, StrokeKind, TextStyle, Ui, Vec2, pos2, vec2,
};

/// Смысловые цвета темы. Экраны берут цвета только отсюда: `t::p().text` и т. п.
#[derive(Debug)]
#[allow(dead_code)] // часть полей понадобится набору деталей (kit) — задачи 3–4
pub struct Palette {
    pub bg: Color32,
    pub surface: Color32,
    pub surface_2: Color32,
    pub surface_3: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub speaking: Color32,
    pub danger: Color32,
    pub danger_hover: Color32,
    pub danger_text: Color32,
    pub danger_soft: Color32,
    pub warn_bg: Color32,
    pub warn_border: Color32,
    pub error_bg: Color32,
    pub error_border: Color32,
    pub success: Color32,
    pub scrim: Color32,
    pub video_bg: Color32,
    pub row_hover: Color32,
    pub row_active: Color32,
    pub title_bg: Color32,
}

/// Тёмная: нейтральный графит, мягкий синий акцент.
pub static DARK: Palette = Palette {
    bg: Color32::from_rgb(0x11, 0x13, 0x17),
    surface: Color32::from_rgb(0x16, 0x19, 0x1E),
    surface_2: Color32::from_rgb(0x1C, 0x20, 0x26),
    surface_3: Color32::from_rgb(0x24, 0x29, 0x32),
    border: Color32::from_rgb(0x2A, 0x2F, 0x38),
    border_strong: Color32::from_rgb(0x3A, 0x41, 0x4D),
    text: Color32::from_rgb(0xE9, 0xEC, 0xF1),
    text_muted: Color32::from_rgb(0x8B, 0x93, 0xA1),
    text_faint: Color32::from_rgb(0x5E, 0x65, 0x73),
    accent: Color32::from_rgb(0x5B, 0x8C, 0xFF),
    accent_hover: Color32::from_rgb(0x7A, 0xA2, 0xFF),
    accent_soft: Color32::from_rgba_unmultiplied_const(0x5B, 0x8C, 0xFF, 40),
    on_accent: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    speaking: Color32::from_rgb(0x3D, 0xD6, 0x8C),
    danger: Color32::from_rgb(0xE5, 0x48, 0x4D),
    danger_hover: Color32::from_rgb(0xF2, 0x55, 0x5A),
    danger_text: Color32::from_rgb(0xFF, 0x9A, 0x9C),
    danger_soft: Color32::from_rgba_unmultiplied_const(0xE5, 0x48, 0x4D, 40),
    warn_bg: Color32::from_rgb(0x2B, 0x24, 0x16),
    warn_border: Color32::from_rgb(0x6B, 0x56, 0x26),
    error_bg: Color32::from_rgb(0x2C, 0x17, 0x19),
    error_border: Color32::from_rgb(0x6B, 0x2C, 0x30),
    success: Color32::from_rgb(0x3D, 0xD6, 0x8C),
    scrim: Color32::from_rgba_unmultiplied_const(0x05, 0x06, 0x08, 190),
    video_bg: Color32::from_rgb(0x0A, 0x0B, 0x0D),
    row_hover: Color32::from_rgb(0x1F, 0x23, 0x2A),
    row_active: Color32::from_rgb(0x25, 0x2B, 0x35),
    title_bg: Color32::from_rgb(0x0E, 0x10, 0x13),
};

/// Светлая: белые панели на сером фоне, тот же акцент.
pub static LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xF2, 0xF3, 0xF6),
    surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    surface_2: Color32::from_rgb(0xF7, 0xF8, 0xFA),
    surface_3: Color32::from_rgb(0xEC, 0xEE, 0xF2),
    border: Color32::from_rgb(0xE3, 0xE6, 0xEC),
    border_strong: Color32::from_rgb(0xC9, 0xCE, 0xD8),
    text: Color32::from_rgb(0x16, 0x18, 0x1D),
    text_muted: Color32::from_rgb(0x6B, 0x71, 0x83),
    text_faint: Color32::from_rgb(0x9A, 0xA0, 0xAE),
    accent: Color32::from_rgb(0x5B, 0x8C, 0xFF),
    accent_hover: Color32::from_rgb(0x4A, 0x7B, 0xF7),
    accent_soft: Color32::from_rgba_unmultiplied_const(0x5B, 0x8C, 0xFF, 36),
    on_accent: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    speaking: Color32::from_rgb(0x1F, 0xAF, 0x6B),
    danger: Color32::from_rgb(0xE5, 0x48, 0x4D),
    danger_hover: Color32::from_rgb(0xD6, 0x3B, 0x40),
    danger_text: Color32::from_rgb(0xB4, 0x23, 0x18),
    danger_soft: Color32::from_rgba_unmultiplied_const(0xE5, 0x48, 0x4D, 30),
    warn_bg: Color32::from_rgb(0xFF, 0xF6, 0xE0),
    warn_border: Color32::from_rgb(0xF0, 0xC8, 0x70),
    error_bg: Color32::from_rgb(0xFD, 0xEC, 0xEC),
    error_border: Color32::from_rgb(0xF2, 0xB8, 0xB5),
    success: Color32::from_rgb(0x1F, 0xAF, 0x6B),
    scrim: Color32::from_rgba_unmultiplied_const(0x14, 0x16, 0x1C, 120),
    video_bg: Color32::from_rgb(0x0A, 0x0B, 0x0D),
    row_hover: Color32::from_rgb(0xEE, 0xF0, 0xF4),
    row_active: Color32::from_rgb(0xE6, 0xEA, 0xF7),
    title_bg: Color32::from_rgb(0xE9, 0xEB, 0xF0),
};

static CURRENT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Текущая палитра.
pub fn p() -> &'static Palette {
    if CURRENT.load(std::sync::atomic::Ordering::Relaxed) == 1 { &LIGHT } else { &DARK }
}

/// Палитра для настройки: «Как в Windows» без ответа системы — тёмная.
pub fn resolve(pref: crate::config::ThemePref, system: Option<egui::Theme>) -> &'static Palette {
    use crate::config::ThemePref;
    match pref {
        ThemePref::Dark => &DARK,
        ThemePref::Light => &LIGHT,
        ThemePref::System => match system {
            Some(egui::Theme::Light) => &LIGHT,
            _ => &DARK,
        },
    }
}

/// Выбрать тему и сразу перекрасить виджеты egui.
pub fn set_theme(ctx: &egui::Context, pref: crate::config::ThemePref) {
    let pal = resolve(pref, ctx.system_theme());
    let light = std::ptr::eq(pal, &LIGHT);
    CURRENT.store(light as u8, std::sync::atomic::Ordering::Relaxed);
    ctx.set_theme(if light { egui::Theme::Light } else { egui::Theme::Dark });
    apply_visuals(ctx, pal);
}

/// Подложка подписей и кнопок поверх видео (видео тёмное в обеих темах).
pub fn overlay_dark(a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(8, 9, 12, a)
}

pub fn accent_a(a: u8) -> Color32 {
    let c = p().accent;
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

pub const SEMIBOLD: &str = "nunito-semibold";
pub const BOLD: &str = "nunito-bold";
pub const XBOLD: &str = "nunito-xbold";
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
pub fn display_font(size: f32) -> FontId {
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
    add(&mut fonts, "nunito", include_bytes!("../assets/fonts/Nunito-Regular.ttf"));
    add(&mut fonts, SEMIBOLD, include_bytes!("../assets/fonts/Nunito-SemiBold.ttf"));
    add(&mut fonts, BOLD, include_bytes!("../assets/fonts/Nunito-Bold.ttf"));
    add(&mut fonts, XBOLD, include_bytes!("../assets/fonts/Nunito-ExtraBold.ttf"));
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
    fonts.families.insert(FontFamily::Proportional, family("nunito", "phosphor"));
    for name in [SEMIBOLD, BOLD, XBOLD] {
        fonts.families.insert(FontFamily::Name(name.into()), family(name, "phosphor"));
    }
    // Nixie One — только цифры и латиница; остальное — Manrope.
    fonts.families.insert(FontFamily::Name(NIXIE_FONT.into()), family(NIXIE_FONT, "nunito"));
    fonts.families.insert(FontFamily::Name(PH_FILL.into()), family(PH_FILL, "nunito"));
    ctx.set_fonts(fonts);

    set_theme(ctx, crate::config::ThemePref::System);
}

/// Визуальные стили egui под палитру (обе темы egui — одинаково, чтобы не зависеть от системной).
fn apply_visuals(ctx: &egui::Context, pal: &'static Palette) {
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
        *v = if std::ptr::eq(pal, &LIGHT) { egui::Visuals::light() } else { egui::Visuals::dark() };
        v.override_text_color = Some(pal.text);
        v.panel_fill = pal.bg;
        v.window_fill = pal.surface_2;
        v.window_stroke = Stroke::new(1.0, pal.border_strong);
        v.window_corner_radius = CornerRadius::same(10);
        v.menu_corner_radius = CornerRadius::same(8);
        v.extreme_bg_color = pal.bg;
        v.faint_bg_color = pal.surface;
        v.code_bg_color = pal.bg;
        v.hyperlink_color = pal.text;
        v.selection.bg_fill = accent_a(70);
        v.selection.stroke = Stroke::new(1.5, pal.accent);
        v.text_cursor.stroke = Stroke::new(2.0, pal.accent);
        v.popup_shadow = egui::epaint::Shadow { offset: [0, 12], blur: 30, spread: 0, color: if std::ptr::eq(pal, &LIGHT) { Color32::from_black_alpha(40) } else { Color32::from_black_alpha(140) } };
        v.window_shadow = v.popup_shadow;

        let w = &mut v.widgets;
        for (wv, fill, stroke) in [
            (&mut w.noninteractive, pal.surface, pal.border),
            (&mut w.inactive, pal.surface_2, pal.border_strong),
            (&mut w.hovered, pal.row_hover, pal.border_strong),
            (&mut w.active, pal.row_active, pal.accent),
            (&mut w.open, pal.surface_2, pal.border_strong),
        ] {
            wv.bg_fill = fill;
            wv.weak_bg_fill = fill;
            wv.bg_stroke = Stroke::new(1.0, stroke);
            wv.corner_radius = CornerRadius::same(12);
            wv.fg_stroke = Stroke::new(1.5, pal.text);
        }
        w.noninteractive.fg_stroke = Stroke::new(1.0, pal.text_muted);
        w.inactive.fg_stroke = Stroke::new(1.5, pal.text);
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
    avatar_glow(painter, center, r, glow);
    let mut c = avatar_color(name);
    if dim {
        c = c.gamma_multiply(0.55);
    }
    painter.circle_filled(center, r, c);
    let letter: String = name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
    painter.text(center, Align2::CENTER_CENTER, letter, xbold(r * 0.8), Color32::from_rgb(0x1B, 0x13, 0x0C));
}

/// Свечение и кольцо вокруг аватара говорящего.
pub fn avatar_glow(painter: &egui::Painter, center: Pos2, r: f32, glow: f32) {
    if glow > 0.01 {
        radial_glow(painter, center, r + 6.0 + 22.0 * glow, accent_a((150.0 * glow) as u8));
        painter.circle_stroke(center, r + 4.0, Stroke::new(2.5, self::p().accent.gamma_multiply(0.5 + 0.5 * glow)));
    }
}

/// Текст «газоразрядной лампой»: янтарь + мягкое свечение.
pub fn glow_text(painter: &egui::Painter, pos: Pos2, anchor: Align2, text: &str, size: f32) -> Rect {
    let fid = display_font(size);
    let spread = (size / 18.0).clamp(1.0, 5.0);
    for (i, (dx, dy)) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0), (0.7, 0.7), (-0.7, -0.7), (0.7, -0.7), (-0.7, 0.7)]
        .iter()
        .enumerate()
    {
        let a = if i < 4 { 46 } else { 30 };
        painter.text(pos + vec2(dx * spread, dy * spread), anchor, text, fid.clone(), Color32::from_rgba_unmultiplied(255, 120, 30, a));
    }
    painter.text(pos, anchor, text, fid, self::p().accent)
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
            painter.add(Shape::line(trace.clone(), Stroke::new(core * k, accent_a(a))));
            painter.add(Shape::line(wave.clone(), Stroke::new(core * k, accent_a(a))));
        }
    }
    let color = if lit { self::p().accent } else { self::p().accent.gamma_multiply(0.45) };
    painter.add(Shape::line(trace, Stroke::new(core, color)));
    painter.add(Shape::line(wave, Stroke::new(core, color)));
    if lit && s > 1.0 {
        painter.circle_filled(p(32.0, 13.0), 1.2 * s, self::p().accent_hover);
    }
}

/// Логотип + «Amadeus».
pub fn wordmark(ui: &mut Ui, logo_size: f32, text_size: f32) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(logo_size), Sense::hover());
        logo(ui.painter(), rect, true);
        ui.add_space(2.0);
        ui.label(egui::RichText::new("Amadeus").font(xbold(text_size)).color(self::p().text));
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
    let bg = Color32::from_rgb(0x3A, 0x2F, 0x25).lerp_to_gamma(self::p().accent, t);
    p.rect_filled(rect, CornerRadius::same(11), bg);
    let x = egui::lerp(rect.left() + 11.0..=rect.right() - 11.0, t);
    p.circle_filled(pos2(x, rect.center().y), 8.0, self::p().text_muted.lerp_to_gamma(self::p().on_accent, t));
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
    p.rect_filled(track, CornerRadius::same(3), self::p().bg);
    let x = track.left() + track.width() * t;
    p.rect_filled(Rect::from_min_max(track.min, pos2(x, track.max.y)), CornerRadius::same(3), self::p().accent);
    p.circle_filled(pos2(x, rect.center().y), 13.0, accent_a(40));
    p.circle_filled(pos2(x, rect.center().y), 8.0, self::p().accent_hover);
    resp
}

/// Клавиша в рамке (как `.key` в макете).
pub fn keycap(ui: &mut Ui, text: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), bold(12.5), self::p().text);
    let size = galley.size() + vec2(16.0, 6.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect.translate(vec2(0.0, 2.0)), CornerRadius::same(6), self::p().border_strong);
    p.rect(rect, CornerRadius::same(6), self::p().bg, Stroke::new(1.0, self::p().border_strong), StrokeKind::Inside);
    p.galley(rect.center() - galley.size() / 2.0, galley, self::p().text);
    resp
}

/// Квадратная иконка-кнопка (34×34). `hot` — красная (выключено).
pub fn icon_button(ui: &mut Ui, icon: &str, hot: bool, filled: bool, tooltip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
    let p = ui.painter();
    if hot {
        p.rect_filled(rect, CornerRadius::same(8), Color32::from_rgba_unmultiplied(229, 83, 75, 30));
    } else if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(8), self::p().surface_2);
    }
    let color = if hot { self::p().danger } else if resp.hovered() { self::p().text } else { self::p().text_muted };
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
    let galley = ui.painter().layout_no_wrap(label.to_string(), font_l, self::p().text);
    let w = 18.0 + 22.0 + 10.0 + galley.size().x + 18.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 46.0), Sense::click());
    let p = ui.painter();
    let (fill, stroke) = match style {
        DockStyle::Leave => (if resp.hovered() { Color32::from_rgb(0xF0, 0x62, 0x5A) } else { self::p().danger }, self::p().danger),
        _ => (if resp.hovered() { Color32::from_rgb(0x33, 0x29, 0x21) } else { self::p().surface_2 }, Color32::from_rgb(0x3A, 0x2F, 0x25)),
    };
    p.rect(rect, CornerRadius::same(10), fill, Stroke::new(1.0, stroke), StrokeKind::Inside);
    let icon_pos = pos2(rect.left() + 18.0 + 11.0, rect.center().y);
    let (icon_color, icon_font) = match style {
        DockStyle::Live => (self::p().accent, icon_fill(20.0)),
        DockStyle::Off => (self::p().danger, icon_fill(20.0)),
        DockStyle::Leave => (Color32::WHITE, font(20.0)),
        DockStyle::Normal => (self::p().text, font(20.0)),
    };
    if style == DockStyle::Live {
        p.circle_filled(icon_pos, 12.0, accent_a(35));
    }
    p.text(icon_pos, Align2::CENTER_CENTER, icon, icon_font, icon_color);
    let text_color = if style == DockStyle::Leave { Color32::WHITE } else { self::p().text };
    p.galley(pos2(rect.left() + 18.0 + 22.0 + 10.0, rect.center().y - galley.size().y / 2.0), galley, text_color);
    resp
}

/// Главная янтарная кнопка (на всю ширину).
pub fn primary_button(ui: &mut Ui, text: &str, width: f32, enabled: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 46.0), if enabled { Sense::click() } else { Sense::hover() });
    let p = ui.painter();
    let fill = if !enabled {
        self::p().accent.gamma_multiply(0.45)
    } else if resp.hovered() {
        Color32::from_rgb(0xFF, 0xAA, 0x55)
    } else {
        self::p().accent
    };
    if enabled {
        for i in 0..3 {
            p.rect_filled(rect.expand(2.0 + i as f32 * 3.0), CornerRadius::same(10 + 3 * i as u8), accent_a(10));
        }
    }
    p.rect_filled(rect, CornerRadius::same(8), fill);
    p.text(rect.center(), Align2::CENTER_CENTER, text, bold(15.0), self::p().on_accent);
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
    use super::*;
    use crate::config::ThemePref;

    /// Отношение контраста WCAG 2.x.
    fn contrast(a: Color32, b: Color32) -> f32 {
        fn lum(c: Color32) -> f32 {
            let ch = |v: u8| {
                let s = v as f32 / 255.0;
                if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
        }
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn resolve_follows_system() {
        assert!(std::ptr::eq(resolve(ThemePref::System, Some(egui::Theme::Light)), &LIGHT));
        assert!(std::ptr::eq(resolve(ThemePref::System, None), &DARK));
        assert!(std::ptr::eq(resolve(ThemePref::Dark, Some(egui::Theme::Light)), &DARK));
        assert!(std::ptr::eq(resolve(ThemePref::Light, Some(egui::Theme::Dark)), &LIGHT));
    }

    #[test]
    fn contrast_ok() {
        for (name, p) in [("dark", &DARK), ("light", &LIGHT)] {
            assert!(contrast(p.text, p.bg) >= 4.5, "{name}: text/bg");
            assert!(contrast(p.text, p.surface) >= 4.5, "{name}: text/surface");
            // Текст на акценте — только жирный (кнопки), для него хватает 3:1; акцент #5B8CFF — из спеки.
            assert!(contrast(p.on_accent, p.accent) >= 3.0, "{name}: on_accent/accent {}", contrast(p.on_accent, p.accent));
            assert!(contrast(p.text_muted, p.surface) >= 3.0, "{name}: muted/surface");
            assert!(contrast(p.danger_text, p.error_bg) >= 4.5, "{name}: danger_text/error_bg");
        }
    }

    #[test]
    fn apply_visuals_switches() {
        let ctx = egui::Context::default();
        set_theme(&ctx, ThemePref::Light);
        assert_eq!(ctx.global_style().visuals.panel_fill, LIGHT.bg);
        assert!(std::ptr::eq(p(), &LIGHT));
        set_theme(&ctx, ThemePref::Dark);
        assert_eq!(ctx.global_style().visuals.panel_fill, DARK.bg);
        assert!(std::ptr::eq(p(), &DARK));
    }

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
