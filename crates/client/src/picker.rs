//! Пикер источника трансляции (как в Discord): вкладки «Окна» / «Экраны», карточки с миниатюрой и иконкой,
//! выбор качества. Источники перечисляются в фоновом потоке (GDI-миниатюры — десятки мс на окно).

use std::collections::HashMap;
use std::sync::mpsc;

use capture_win::{Image, Source, SourceId};
use eframe::egui::{
    self, Align2, Color32, ColorImage, CornerRadius, Frame, Id, Margin, Rect, RichText, Sense, Stroke, StrokeKind,
    TextureHandle, TextureOptions, Ui, pos2, vec2,
};
use egui_phosphor::regular as ph;

use crate::stream::Preset;
use crate::theme as t;

const W: f32 = 780.0;
const COLS: usize = 3;
const GAP: f32 = 14.0;
const THUMB_W: u32 = 360;
const THUMB_H: u32 = 202;

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Windows,
    Screens,
}

pub enum Action {
    None,
    Cancel,
    Start { source: SourceId, title: String, width: u32, height: u32, preset: Preset, audio: bool },
}

pub struct Picker {
    tab: Tab,
    rx: Option<mpsc::Receiver<Vec<Source>>>,
    sources: Option<Vec<Source>>,
    tex: HashMap<SourceId, (Option<TextureHandle>, Option<TextureHandle>)>,
    selected: Option<SourceId>,
    pub preset: Preset,
    /// Передавать звук окна/системы.
    pub audio: bool,
    /// Уже транслируем — кнопка «Переключить».
    switching: bool,
}

impl Picker {
    pub fn open(ctx: &egui::Context, preset: Preset, audio: bool, current: Option<SourceId>) -> Self {
        let mut p = Picker {
            tab: if matches!(current, Some(SourceId::Monitor(_))) { Tab::Screens } else { Tab::Windows },
            rx: None,
            sources: None,
            tex: HashMap::new(),
            selected: current,
            preset,
            audio,
            switching: current.is_some(),
        };
        p.reload(ctx);
        p
    }

    fn reload(&mut self, ctx: &egui::Context) {
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("list-sources".into())
            .spawn(move || {
                let v = capture_win::list_sources(THUMB_W, THUMB_H);
                let _ = tx.send(v);
                ctx.request_repaint();
            })
            .ok();
        self.rx = Some(rx);
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.rx else { return };
        let Ok(list) = rx.try_recv() else { return };
        self.rx = None;
        self.tex.clear();
        for s in &list {
            let load = |img: &Option<Image>, kind: &str| {
                img.as_ref().map(|i| {
                    ctx.load_texture(
                        format!("src-{kind}-{:?}", s.id),
                        ColorImage::from_rgba_unmultiplied([i.width as usize, i.height as usize], &i.rgba),
                        TextureOptions::LINEAR,
                    )
                })
            };
            self.tex.insert(s.id, (load(&s.thumb, "thumb"), load(&s.icon, "icon")));
        }
        if let Some(sel) = self.selected
            && !list.iter().any(|s| s.id == sel) {
                self.selected = None;
            }
        // Без окон — сразу экраны.
        if !list.iter().any(|s| s.id.is_window()) {
            self.tab = Tab::Screens;
        }
        self.sources = Some(list);
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> Action {
        self.poll(ctx);
        let mut action = Action::None;
        let modal = egui::Modal::new(Id::new("stream-picker"))
            .backdrop_color(t::SCRIM)
            .frame(
                Frame::new()
                    .fill(t::SURFACE)
                    .stroke(Stroke::new(1.0, t::BORDER))
                    .corner_radius(CornerRadius::same(14))
                    .inner_margin(Margin::symmetric(28, 24))
                    .shadow(ctx.global_style().visuals.window_shadow),
            )
            .show(ctx, |ui| {
                ui.set_width(W);
                // заголовок
                ui.horizontal(|ui| {
                    ui.label(RichText::new(if self.switching { "Сменить источник" } else { "Трансляция экрана" }).font(t::xbold(20.0)));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        if t::icon_button(ui, ph::X, false, false, "Закрыть").clicked() {
                            action = Action::Cancel;
                        }
                        if t::icon_button(ui, ph::ARROWS_CLOCKWISE, false, false, "Обновить список").clicked() && self.rx.is_none() {
                            self.reload(ctx);
                        }
                    });
                });
                ui.add_space(12.0);
                self.ui_tabs(ui);
                ui.add_space(14.0);
                let max_h = (ctx.content_rect().height() - 300.0).clamp(220.0, 560.0);
                egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
                    if let Some(a) = self.ui_grid(ui) {
                        action = a;
                    }
                });
                ui.add_space(16.0);
                if let Some(a) = self.ui_footer(ui) {
                    action = a;
                }
            });
        if modal.should_close() && matches!(action, Action::None) {
            action = Action::Cancel;
        }
        action
    }

    fn ui_tabs(&mut self, ui: &mut Ui) {
        let (nw, ns) = match &self.sources {
            Some(v) => (v.iter().filter(|s| s.id.is_window()).count(), v.iter().filter(|s| !s.id.is_window()).count()),
            None => (0, 0),
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            for (tab, icon, name, n) in [(Tab::Windows, ph::APP_WINDOW, "Окна приложений", nw), (Tab::Screens, ph::MONITOR, "Экраны", ns)] {
                let on = self.tab == tab;
                let text = if self.sources.is_some() { format!("{icon}  {name}  {n}") } else { format!("{icon}  {name}") };
                let galley = ui.painter().layout_no_wrap(text, t::semibold(14.0), t::TEXT);
                let (rect, resp) = ui.allocate_exact_size(galley.size() + vec2(28.0, 16.0), Sense::click());
                let p = ui.painter();
                let fill = if on { t::accent_a(30) } else if resp.hovered() { t::SURFACE_2 } else { Color32::TRANSPARENT };
                let stroke = if on { Stroke::new(1.0, t::accent_a(150)) } else { Stroke::new(1.0, t::BORDER_DIM) };
                p.rect(rect, CornerRadius::same(255), fill, stroke, StrokeKind::Inside);
                p.galley(rect.center() - galley.size() / 2.0, galley, if on { t::ACCENT_BRIGHT } else { t::TEXT_MUTED });
                if resp.clicked() {
                    self.tab = tab;
                }
            }
        });
    }

    fn ui_grid(&mut self, ui: &mut Ui) -> Option<Action> {
        let Some(sources) = &self.sources else {
            ui.add_space(60.0);
            ui.vertical_centered(|ui| {
                ui.add(egui::Spinner::new().size(26.0).color(t::ACCENT));
                ui.add_space(8.0);
                ui.label(RichText::new("Ищем окна…").color(t::TEXT_MUTED));
            });
            ui.add_space(60.0);
            return None;
        };
        let list: Vec<&Source> = sources.iter().filter(|s| s.id.is_window() == (self.tab == Tab::Windows)).collect();
        if list.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| ui.label(RichText::new("Здесь пусто — откройте нужное окно и нажмите «Обновить»").color(t::TEXT_MUTED)));
            ui.add_space(40.0);
            return None;
        }
        let cw = (W - 12.0 - GAP * (COLS - 1) as f32) / COLS as f32;
        let thumb_h = cw * 9.0 / 16.0;
        let ch = thumb_h + 58.0;
        let mut out = None;
        let mut select = None;
        for row in list.chunks(COLS) {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                for s in row {
                    let (rect, resp) = ui.allocate_exact_size(vec2(cw, ch), Sense::click());
                    let sel = self.selected == Some(s.id);
                    self.card(ui, rect, s, sel, resp.hovered(), thumb_h);
                    if resp.clicked() {
                        select = Some(s.id);
                    }
                    if resp.double_clicked() {
                        out = Some(Action::Start { source: s.id, title: s.title.clone(), width: s.width, height: s.height, preset: self.preset, audio: self.audio });
                    }
                    resp.on_hover_text(&s.title).on_hover_cursor(egui::CursorIcon::PointingHand);
                }
            });
            ui.add_space(GAP);
        }
        if select.is_some() {
            self.selected = select;
        }
        out
    }

    fn card(&self, ui: &Ui, rect: Rect, s: &Source, selected: bool, hovered: bool, thumb_h: f32) {
        let p = ui.painter();
        let (thumb, icon) = self.tex.get(&s.id).cloned().unwrap_or((None, None));
        if selected {
            for i in 0..3 {
                p.rect_filled(rect.expand(2.0 + i as f32 * 3.0), CornerRadius::same(14 + 3 * i as u8), t::accent_a(9));
            }
        }
        let fill = if hovered || selected { t::SURFACE_2 } else { t::SURFACE_CARD };
        p.rect_filled(rect, CornerRadius::same(12), fill);
        let tr = Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(rect.width() - 16.0, thumb_h - 8.0));
        p.rect_filled(tr, CornerRadius::same(8), t::BG);
        match &thumb {
            Some(tex) => {
                let size = tex.size_vec2();
                let k = (tr.width() / size.x).min(tr.height() / size.y);
                let ir = Rect::from_center_size(tr.center(), size * k);
                p.image(tex.id(), ir, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                let c = tr.center() - vec2(0.0, 8.0);
                match &icon {
                    Some(ic) => {
                        p.image(ic.id(), Rect::from_center_size(c, vec2(40.0, 40.0)), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    None => {
                        p.text(c, Align2::CENTER_CENTER, ph::APP_WINDOW, t::font(36.0), t::TEXT_MUTED);
                    }
                }
                let note = if s.minimized { "свёрнуто — разверните окно" } else { "нет превью" };
                p.text(pos2(tr.center().x, tr.bottom() - 16.0), Align2::CENTER_CENTER, note, t::font(12.0), t::TEXT_MUTED);
            }
        }
        let stroke = if selected { Stroke::new(2.0, t::ACCENT) } else if hovered { Stroke::new(1.0, t::BORDER) } else { Stroke::new(1.0, t::BORDER_DIM) };
        p.rect_stroke(rect, CornerRadius::same(12), stroke, StrokeKind::Inside);

        // подпись: иконка + заголовок, ниже — программа/разрешение
        let ty = rect.top() + thumb_h + 10.0;
        let mut tx = rect.left() + 12.0;
        if s.id.is_window() {
            if let Some(ic) = &icon {
                p.image(ic.id(), Rect::from_min_size(pos2(tx, ty + 1.0), vec2(18.0, 18.0)), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            } else {
                p.text(pos2(tx + 9.0, ty + 10.0), Align2::CENTER_CENTER, ph::APP_WINDOW, t::font(16.0), t::TEXT_MUTED);
            }
        } else {
            p.text(pos2(tx + 9.0, ty + 10.0), Align2::CENTER_CENTER, ph::MONITOR, t::font(17.0), if selected { t::ACCENT } else { t::TEXT_MUTED });
        }
        tx += 26.0;
        let max_w = rect.right() - 12.0 - tx;
        let title = elided(p, &s.title, t::semibold(13.5), if selected { t::ACCENT_BRIGHT } else { t::TEXT }, max_w);
        p.galley(pos2(tx, ty + 10.0 - title.size().y / 2.0), title, t::TEXT);
        let sub = elided(p, &s.app, t::font(12.0), t::TEXT_MUTED, rect.width() - 24.0);
        p.galley(pos2(rect.left() + 12.0, ty + 24.0), sub, t::TEXT_MUTED);
    }

    fn ui_footer(&mut self, ui: &mut Ui) -> Option<Action> {
        let mut out = None;
        let top = ui.cursor().top() - 8.0;
        ui.painter().hline(ui.max_rect().x_range(), top, Stroke::new(1.0, t::BORDER_DIM));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Качество").size(13.0).color(t::TEXT_MUTED));
            egui::ComboBox::from_id_salt("preset").selected_text(self.preset.label()).width(170.0).show_ui(ui, |ui| {
                for p in Preset::ALL {
                    ui.selectable_value(&mut self.preset, p, p.label());
                }
            });
            ui.add_space(14.0);
            let hint = if self.tab == Tab::Screens {
                "Весь звук компьютера, кроме голосов из Amadeus"
            } else {
                "Только звук выбранного приложения (музыка и Discord в других окнах не попадут)"
            };
            ui.checkbox(&mut self.audio, RichText::new("Передавать звук").size(13.0)).on_hover_text(hint);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if self.switching { "Переключить" } else { "Начать трансляцию" };
                let chosen = self.selected.and_then(|id| self.sources.as_ref()?.iter().find(|s| s.id == id));
                if t::primary_button(ui, label, 210.0, chosen.is_some()).clicked()
                    && let Some(s) = chosen {
                        out = Some(Action::Start { source: s.id, title: s.title.clone(), width: s.width, height: s.height, preset: self.preset, audio: self.audio });
                    }
                ui.add_space(6.0);
                if ui.button("Отмена").clicked() {
                    out = Some(Action::Cancel);
                }
            });
        });
        ui.label(
            RichText::new("Кодирует видеокарта (NVIDIA/AMD/Intel); если она не умеет — процессор, тогда 60 к/с могут проседать.")
                .size(12.0)
                .color(t::TEXT_MUTED),
        );
        out
    }
}

fn elided(p: &egui::Painter, text: &str, font: egui::FontId, color: Color32, max_w: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_w.max(10.0));
    p.layout_job(job)
}
