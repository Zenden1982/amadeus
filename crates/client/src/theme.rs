//! Тема Murmur: палитры (тёмная/светлая), шрифт Nunito, визуалы egui, знак. Детали интерфейса — модуль `kit`.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect, Sense,
    Shape, Stroke, TextStyle, Ui, Vec2, pos2, vec2,
};

/// Смысловые цвета темы. Экраны берут цвета только отсюда: `t::p().text` и т. п.
/// Значения — из пакета бренда Murmur (`palette.json`); производные (рамки, подложки ошибок) — смешением токенов.
/// Соответствие токенам бренда: bg = bg_main, rail = bg_rail, surface = bg_sidebar, surface_2/3 = bg_elevated,
/// text = text_primary, text_muted = text_secondary, text_faint = text_muted, accent_text = link.
#[derive(Debug)]
#[allow(dead_code)] // статусы и mention_bg — для будущих экранов (статус «не беспокоить», упоминания)
pub struct Palette {
    /// Основная область (чат, заголовок).
    pub bg: Color32,
    /// Колонка серверов — самый тёмный (в светлой — самый серый) слой. Заголовок окна: тёмная — тот же, светлая — как список каналов
    /// (на bg_rail ссылка бренда #0F766E даёт 4.3:1 — мало для мелкого текста).
    pub rail: Color32,
    /// Список каналов, док.
    pub surface: Color32,
    /// Поля ввода, карточки, всплывающие меню, модальные окна.
    pub surface_2: Color32,
    /// Значки серверов, подложки мелких элементов.
    pub surface_3: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    /// Фирменный бирюзовый: иконки, контуры, фокус, ползунки. Под белый текст не годится — для этого `accent_strong`.
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_soft: Color32,
    /// Заливка кнопок с белым текстом.
    pub accent_strong: Color32,
    pub accent_strong_hover: Color32,
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
    pub status_online: Color32,
    pub status_idle: Color32,
    pub status_dnd: Color32,
    pub status_offline: Color32,
    pub warning: Color32,
    /// Фон сообщения с упоминанием (премультиплицирован).
    pub mention_bg: Color32,
}

/// Тёмная: холодный серый, бирюзовый акцент.
pub static DARK: Palette = Palette {
    bg: Color32::from_rgb(0x22, 0x27, 0x2B),
    rail: Color32::from_rgb(0x15, 0x18, 0x1B),
    surface: Color32::from_rgb(0x1B, 0x1F, 0x23),
    surface_2: Color32::from_rgb(0x2A, 0x30, 0x35),
    surface_3: Color32::from_rgb(0x2A, 0x30, 0x35),
    border: Color32::from_rgb(0x3A, 0x42, 0x49),
    border_strong: Color32::from_rgb(0x4E, 0x58, 0x5F),
    text: Color32::from_rgb(0xE6, 0xED, 0xF0),
    text_muted: Color32::from_rgb(0xA9, 0xB4, 0xBA),
    text_faint: Color32::from_rgb(0x7E, 0x8A, 0x91),
    accent: Color32::from_rgb(0x14, 0xB8, 0xA6),
    accent_hover: Color32::from_rgb(0x2D, 0xD4, 0xBF),
    accent_soft: Color32::from_rgba_premultiplied(3, 27, 25, 38),
    accent_strong: Color32::from_rgb(0x0F, 0x76, 0x6E),
    accent_strong_hover: Color32::from_rgb(0x11, 0x5E, 0x59),
    accent_text: Color32::from_rgb(0x2D, 0xD4, 0xBF),
    on_accent: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    speaking: Color32::from_rgb(0x2D, 0xD4, 0xBF),
    danger: Color32::from_rgb(0xDC, 0x26, 0x26),
    danger_hover: Color32::from_rgb(0xB9, 0x1C, 0x1C),
    danger_text: Color32::from_rgb(0xF8, 0x71, 0x71),
    danger_soft: Color32::from_rgba_unmultiplied_const(0xDC, 0x26, 0x26, 40),
    warn_bg: Color32::from_rgb(0x3B, 0x35, 0x27),
    warn_border: Color32::from_rgb(0x81, 0x5D, 0x1D),
    error_bg: Color32::from_rgb(0x38, 0x27, 0x2A),
    error_border: Color32::from_rgb(0x76, 0x27, 0x29),
    success: Color32::from_rgb(0x22, 0xC5, 0x5E),
    scrim: Color32::from_rgba_unmultiplied_const(0x05, 0x06, 0x08, 190),
    video_bg: Color32::from_rgb(0x0A, 0x0B, 0x0D),
    row_hover: Color32::from_rgb(0x2A, 0x30, 0x35),
    row_active: Color32::from_rgb(0x32, 0x3A, 0x40),
    title_bg: Color32::from_rgb(0x15, 0x18, 0x1B),
    status_online: Color32::from_rgb(0x22, 0xC5, 0x5E),
    status_idle: Color32::from_rgb(0xF5, 0x9E, 0x0B),
    status_dnd: Color32::from_rgb(0xEF, 0x44, 0x44),
    status_offline: Color32::from_rgb(0x7E, 0x8A, 0x91),
    warning: Color32::from_rgb(0xF5, 0x9E, 0x0B),
    mention_bg: Color32::from_rgba_premultiplied(30, 19, 1, 31),
};

/// Светлая: белая основная область, серо-голубые боковые панели.
pub static LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    rail: Color32::from_rgb(0xDF, 0xE6, 0xE9),
    surface: Color32::from_rgb(0xED, 0xF2, 0xF4),
    surface_2: Color32::from_rgb(0xF4, 0xF7, 0xF8),
    surface_3: Color32::from_rgb(0xF4, 0xF7, 0xF8),
    border: Color32::from_rgb(0xD3, 0xDC, 0xE0),
    border_strong: Color32::from_rgb(0xB8, 0xC2, 0xC7),
    text: Color32::from_rgb(0x0F, 0x1A, 0x1E),
    text_muted: Color32::from_rgb(0x46, 0x55, 0x5C),
    text_faint: Color32::from_rgb(0x66, 0x75, 0x7C),
    accent: Color32::from_rgb(0x0D, 0x94, 0x88),
    accent_hover: Color32::from_rgb(0x0F, 0x76, 0x6E),
    accent_soft: Color32::from_rgba_premultiplied(2, 18, 17, 31),
    accent_strong: Color32::from_rgb(0x0F, 0x76, 0x6E),
    accent_strong_hover: Color32::from_rgb(0x11, 0x5E, 0x59),
    accent_text: Color32::from_rgb(0x0F, 0x76, 0x6E),
    on_accent: Color32::from_rgb(0xFF, 0xFF, 0xFF),
    speaking: Color32::from_rgb(0x0D, 0x94, 0x88),
    danger: Color32::from_rgb(0xDC, 0x26, 0x26),
    danger_hover: Color32::from_rgb(0xB9, 0x1C, 0x1C),
    danger_text: Color32::from_rgb(0xB9, 0x1C, 0x1C),
    danger_soft: Color32::from_rgba_unmultiplied_const(0xDC, 0x26, 0x26, 30),
    warn_bg: Color32::from_rgb(0xF8, 0xEE, 0xE6),
    warn_border: Color32::from_rgb(0xDD, 0xB2, 0x90),
    error_bg: Color32::from_rgb(0xFC, 0xE9, 0xE9),
    error_border: Color32::from_rgb(0xEF, 0x9D, 0x9D),
    success: Color32::from_rgb(0x15, 0x80, 0x3D),
    scrim: Color32::from_rgba_unmultiplied_const(0x14, 0x16, 0x1C, 120),
    video_bg: Color32::from_rgb(0x0A, 0x0B, 0x0D),
    row_hover: Color32::from_rgb(0xE6, 0xED, 0xF0),
    row_active: Color32::from_rgb(0xDD, 0xE5, 0xE8),
    title_bg: Color32::from_rgb(0xED, 0xF2, 0xF4),
    status_online: Color32::from_rgb(0x16, 0xA3, 0x4A),
    status_idle: Color32::from_rgb(0xD9, 0x77, 0x06),
    status_dnd: Color32::from_rgb(0xDC, 0x26, 0x26),
    status_offline: Color32::from_rgb(0x8A, 0x97, 0x9D),
    warning: Color32::from_rgb(0xB4, 0x53, 0x09),
    mention_bg: Color32::from_rgba_premultiplied(37, 24, 2, 38),
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
    // уже нарисованная часть кадра — в старой палитре: перерисовать целиком
    ctx.request_repaint();
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
            // Белый текст — только на accent_strong (бренд: на accent контраста не хватает).
            assert!(contrast(p.on_accent, p.accent_strong) >= 4.5, "{name}: on_accent/accent_strong {}", contrast(p.on_accent, p.accent_strong));
            assert!(contrast(p.on_accent, p.danger) >= 4.5, "{name}: on_accent/danger");
            assert!(contrast(p.text_muted, p.surface) >= 4.5, "{name}: muted/surface");
            assert!(contrast(p.text_faint, p.bg) >= 3.0, "{name}: faint/bg");
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
