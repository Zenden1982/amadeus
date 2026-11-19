//! Тема Murmur: палитры (тёмная/светлая), шрифт Nunito, визуалы egui, знак. Детали интерфейса — модуль `kit`.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect, Sense,
    Shape, Stroke, TextStyle, Ui, Vec2, pos2, vec2,
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
    /// Акцент для мелкого текста (ссылки, «доступна версия», бейдж) — читаемый на фоне (≥ 4.5:1).
    pub accent_text: Color32,
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
    accent_text: Color32::from_rgb(0x8F, 0xB0, 0xFF),
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
    accent_text: Color32::from_rgb(0x2F, 0x5F, 0xD8),
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
        Color32::from_rgb(0x5B, 0x8C, 0xFF),
        Color32::from_rgb(0xF2, 0x99, 0x4A),
        Color32::from_rgb(0x2F, 0xB5, 0x9A),
        Color32::from_rgb(0xB5, 0x65, 0xE6),
        Color32::from_rgb(0xE5, 0x48, 0x8D),
        Color32::from_rgb(0xD9, 0xA4, 0x1E),
    ];
    let h = name.to_lowercase().bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    P[(h % P.len() as u32) as usize]
}

/// Точки знака «M-волна с глазами» (design/logo.svg, 64×64): линия звука, из которой вырастает M (пики — уши),
/// две точки-глаза; толщина линии. На ≤24 px линия толще, иначе знак «рассыпается».
pub fn logo_points(rect: Rect) -> (Vec<Pos2>, [Pos2; 2], f32) {
    let s = rect.width() / 64.0;
    let p = |x: f32, y: f32| pos2(rect.left() + x * s, rect.top() + y * s);
    let line = [(2.0, 42.0), (10.0, 42.0), (14.0, 18.0), (24.0, 32.0), (32.0, 22.0), (40.0, 32.0), (50.0, 18.0), (54.0, 42.0), (62.0, 42.0)]
        .iter()
        .map(|&(x, y)| p(x, y))
        .collect();
    let width = if rect.width() <= 24.0 { 5.5 * s * 1.3 } else { 5.5 * s };
    (line, [p(26.0, 44.0), p(38.0, 44.0)], width)
}

/// Знак Murmur. `lit` = false — приглушённый (неактивное окно).
pub fn logo(painter: &egui::Painter, rect: Rect, lit: bool) {
    let (line, eyes, width) = logo_points(rect);
    let color = if lit { self::p().accent } else { self::p().text_muted };
    painter.add(Shape::line(line.clone(), Stroke::new(width, color)));
    // скруглённые стыки: кружки в вершинах
    for q in &line {
        painter.circle_filled(*q, width / 2.0, color);
    }
    let er = 2.6 * rect.width() / 64.0 * if rect.width() <= 24.0 { 1.3 } else { 1.0 };
    for e in eyes {
        painter.circle_filled(e, er, color);
    }
}

/// Знак + «Murmur».
pub fn wordmark(ui: &mut Ui, logo_size: f32, text_size: f32) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(logo_size), Sense::hover());
        logo(ui.painter(), rect, true);
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Murmur").font(xbold(text_size)).color(self::p().text));
    });
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
    fn logo_points_in_box() {
        for size in [16.0, 24.0, 64.0, 110.0] {
            let rect = Rect::from_min_size(pos2(10.0, 20.0), Vec2::splat(size));
            let (line, eyes, width) = logo_points(rect);
            assert_eq!(line.len(), 9);
            for q in line.iter().chain(eyes.iter()) {
                assert!(rect.contains(*q), "{size}: {q:?} вне {rect:?}");
            }
            // мелкий знак — линия толще относительно размера
            assert!(width / size >= 5.5 / 64.0 - 1e-6);
        }
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
    fn accent_text_readable() {
        for (name, p) in [("dark", &DARK), ("light", &LIGHT)] {
            for (bg_name, bg) in [("bg", p.bg), ("surface", p.surface), ("title_bg", p.title_bg), ("surface_2", p.surface_2)] {
                assert!(contrast(p.accent_text, bg) >= 4.5, "{name}: accent_text/{bg_name} {}", contrast(p.accent_text, bg));
            }
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
