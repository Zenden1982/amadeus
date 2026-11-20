//! Обрезка картинки в квадрат (аватар, значок сервера): масштаб колесом, перетаскивание, круглая маска (фаза 9.1).

use eframe::egui::{self, Color32, Rect, RichText, Sense, Stroke, TextureHandle, TextureOptions, Vec2, vec2};
use image::{DynamicImage, RgbaImage};

use crate::kit::{self, ButtonKind};
use crate::theme as t;

/// Сторона окна обрезки (точки).
const VIEW: f32 = 280.0;
/// Сторона результата (px).
pub const OUT: u32 = 256;
const ZOOM_MAX: f32 = 8.0;
/// Картинки больше — уменьшаются при открытии (текстура и память).
const SRC_MAX: u32 = 2048;

/// Положение картинки в окне обрезки: масштаб (1 — короткая сторона во всё окно) и сдвиг от центра (точки окна).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropState {
    pub zoom: f32,
    pub offset: Vec2,
}

impl Default for CropState {
    fn default() -> Self {
        Self { zoom: 1.0, offset: Vec2::ZERO }
    }
}

/// Квадрат `out`×`out` — то, что видно в окне обрезки стороной `view`.
pub fn crop_square(img: &DynamicImage, st: &CropState, view: f32, out: u32) -> RgbaImage {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let st = clamp(*st, w, h, view);
    let s = view / w.min(h) * st.zoom;
    // левый верхний угол картинки в координатах окна
    let left = (view - w * s) / 2.0 + st.offset.x;
    let top = (view - h * s) / 2.0 + st.offset.y;
    let side = (view / s).min(w).min(h);
    let x0 = (-left / s).clamp(0.0, w - side);
    let y0 = (-top / s).clamp(0.0, h - side);
    let side_px = (side.round() as u32).max(1);
    img.crop_imm(x0.round() as u32, y0.round() as u32, side_px, side_px)
        .resize_exact(out, out, image::imageops::FilterType::Lanczos3)
        .to_rgba8()
}

/// Масштаб 1..8, сдвиг — не дальше, чем картинка закрывает окно.
pub fn clamp(st: CropState, w: f32, h: f32, view: f32) -> CropState {
    let zoom = st.zoom.clamp(1.0, ZOOM_MAX);
    let s = view / w.min(h) * zoom;
    let mx = ((w * s - view) / 2.0).max(0.0);
    let my = ((h * s - view) / 2.0).max(0.0);
    CropState { zoom, offset: vec2(st.offset.x.clamp(-mx, mx), st.offset.y.clamp(-my, my)) }
}

/// Затемнение квадрата `rect` вне вписанного круга: лучи от центра, между кругом и краем квадрата.
fn circle_mask(rect: Rect, color: Color32) -> egui::Shape {
    const N: u32 = 96; // кратно 8 — углы квадрата попадают на лучи
    let c = rect.center();
    let half = rect.width() / 2.0;
    let mut mesh = egui::Mesh::default();
    for i in 0..=N {
        let a = i as f32 / N as f32 * std::f32::consts::TAU;
        let d = vec2(a.cos(), a.sin());
        let edge = half / d.x.abs().max(d.y.abs());
        mesh.colored_vertex(c + d * half, color);
        mesh.colored_vertex(c + d * edge, color);
        if i > 0 {
            let k = i * 2;
            mesh.add_triangle(k - 2, k - 1, k);
            mesh.add_triangle(k - 1, k + 1, k);
        }
    }
    egui::Shape::mesh(mesh)
}

/// Итог окна обрезки.
pub enum CropResult {
    Open,
    Cancel,
    /// PNG `OUT`×`OUT`.
    Done(Vec<u8>),
}

/// Окно обрезки: картинка в квадрате, видимая часть — круг.
pub struct CropDialog {
    title: String,
    img: DynamicImage,
    tex: TextureHandle,
    st: CropState,
}

/// Байты файла → картинка (уменьшенная до `SRC_MAX`). Тяжело — вызывать не в UI-потоке.
pub fn load(bytes: &[u8]) -> Result<DynamicImage, String> {
    let img = image::load_from_memory(bytes).map_err(|_| "Это не картинка (нужны PNG, JPEG, GIF, WebP).".to_string())?;
    Ok(if img.width().max(img.height()) > SRC_MAX { img.resize(SRC_MAX, SRC_MAX, image::imageops::FilterType::Triangle) } else { img })
}

impl CropDialog {
    pub fn new(ctx: &egui::Context, title: &str, img: DynamicImage) -> Self {
        let rgba = img.to_rgba8();
        let ci = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
        let tex = ctx.load_texture("crop-source", ci, TextureOptions::LINEAR);
        Self { title: title.into(), img, tex, st: CropState::default() }
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> CropResult {
        let mut result = CropResult::Open;
        let (w, h) = (self.img.width() as f32, self.img.height() as f32);
        let res = kit::modal(ctx, "crop", &self.title, VIEW + 40.0, |ui| {
            ui.vertical_centered(|ui| {
                let (rect, resp) = ui.allocate_exact_size(vec2(VIEW, VIEW), Sense::drag());
                if resp.dragged() {
                    self.st.offset += resp.drag_delta();
                }
                if resp.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                    if scroll != 0.0 {
                        self.st.zoom *= (scroll * 0.003).exp();
                    }
                }
                self.st = clamp(self.st, w, h, VIEW);
                let s = VIEW / w.min(h) * self.st.zoom;
                let size = vec2(w * s, h * s);
                let img_rect = Rect::from_center_size(rect.center() + self.st.offset, size);
                let painter = ui.painter_at(rect);
                painter.rect_filled(rect, 0.0, Color32::BLACK);
                painter.image(self.tex.id(), img_rect, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                // затемнить всё вне круга
                let r = VIEW / 2.0;
                painter.add(circle_mask(rect, Color32::from_black_alpha(150)));
                painter.circle_stroke(rect.center(), r, Stroke::new(1.5, Color32::from_white_alpha(180)));
                resp.on_hover_cursor(egui::CursorIcon::Grab);
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new(egui_phosphor::regular::MAGNIFYING_GLASS_MINUS).color(t::p().text_muted));
                    kit::slider(ui, &mut self.st.zoom, 1.0..=ZOOM_MAX, VIEW - 60.0);
                    ui.label(RichText::new(egui_phosphor::regular::MAGNIFYING_GLASS_PLUS).color(t::p().text_muted));
                });
                ui.label(RichText::new("Перетащите картинку, колесо — масштаб").size(12.5).color(t::p().text_muted));
            });
            ui.add_space(14.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                if kit::button(ui, ButtonKind::Primary, "Готово", None).clicked() {
                    let out = crop_square(&self.img, &self.st, VIEW, OUT);
                    let mut png = std::io::Cursor::new(Vec::new());
                    result = match out.write_to(&mut png, image::ImageFormat::Png) {
                        Ok(()) => CropResult::Done(png.into_inner()),
                        Err(e) => {
                            log::warn!("обрезка: PNG {e}");
                            CropResult::Cancel
                        }
                    };
                }
                if kit::button(ui, ButtonKind::Ghost, "Отмена", None).clicked() {
                    result = CropResult::Cancel;
                }
            });
        });
        if res == kit::ModalResult::Closed {
            return CropResult::Cancel;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::vec2;

    /// 800×300: левая половина красная, правая синяя.
    fn halves() -> DynamicImage {
        DynamicImage::ImageRgba8(RgbaImage::from_fn(800, 300, |x, _| {
            if x < 400 { image::Rgba([255, 0, 0, 255]) } else { image::Rgba([0, 0, 255, 255]) }
        }))
    }

    fn is_red(p: &image::Rgba<u8>) -> bool {
        p[0] > 200 && p[2] < 50
    }

    #[test]
    fn default_is_center_square() {
        let out = crop_square(&halves(), &CropState::default(), 240.0, 256);
        assert!(is_red(out.get_pixel(10, 128)), "слева от центра — красное");
        assert!(!is_red(out.get_pixel(245, 128)), "справа от центра — синее");
    }

    #[test]
    fn offset_to_edge_is_clamped() {
        let st = CropState { zoom: 1.0, offset: vec2(10_000.0, 0.0) };
        let out = crop_square(&halves(), &st, 240.0, 256);
        // сдвиг вправо до упора = виден левый край (0..300) — весь красный
        assert!(is_red(out.get_pixel(10, 128)));
        assert!(is_red(out.get_pixel(245, 128)), "дальше края не уезжает");
    }

    #[test]
    fn zoom_shows_less() {
        let st = CropState { zoom: 4.0, offset: Vec2::ZERO };
        let out = crop_square(&halves(), &st, 240.0, 256);
        // по центру 800×300 при ×4 видно 75 px вокруг x=400: половина красная, половина синяя
        assert!(is_red(out.get_pixel(20, 128)));
        assert!(!is_red(out.get_pixel(236, 128)));
    }

    #[test]
    fn not_an_image() {
        assert!(load(b"hello").is_err());
    }

    #[test]
    fn output_is_256() {
        let out = crop_square(&halves(), &CropState::default(), 240.0, 256);
        assert_eq!(out.dimensions(), (256, 256));
    }
}
