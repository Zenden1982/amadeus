//! Витрина набора деталей (debug, `AMADEUS_GALLERY=1`): все детали в одном окне, тема переключается сверху.

use eframe::egui::{self, CornerRadius, Frame, Margin, Rect, RichText, Sense, Ui, vec2};
use egui_phosphor::regular as ph;

use crate::config::ThemePref;
use crate::kit::{self, ButtonKind, DockStyle, IconState, MascotPose, Notice, PeerBadge};
use crate::theme as t;

#[derive(Default)]
pub struct GalleryState {
    text: String,
    pass: String,
    search: String,
    area: String,
    on: bool,
    vol: f32,
    pick: u8,
    modal: bool,
    notice_closed: bool,
}

fn section(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    ui.add_space(14.0);
    ui.label(RichText::new(title).font(t::bold(13.0)).color(t::p().text_muted));
    ui.add_space(6.0);
    Frame::new()
        .fill(t::p().surface)
        .stroke(egui::Stroke::new(1.0, t::p().border))
        .corner_radius(CornerRadius::same(kit::R_CARD))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui)
        });
}

pub fn ui(ui: &mut Ui, s: &mut GalleryState, theme: &mut ThemePref) {
    if s.vol == 0.0 {
        s.vol = 1.0;
        s.text = "Наша тусовка".into();
        s.pass = "секрет".into();
    }
    egui::CentralPanel::default().frame(Frame::new().fill(t::p().bg).inner_margin(Margin::same(24))).show(ui, |ui| {
        let mut area = egui::ScrollArea::vertical();
        // Отладка: AMADEUS_GALLERY=2 — сразу прокрутить к нижней половине (для скриншота).
        if std::env::var("AMADEUS_GALLERY").as_deref() == Ok("2") {
            area = area.vertical_scroll_offset(1000.0);
        }
        area.show(ui, |ui| {
            ui.horizontal(|ui| {
                t::wordmark(ui, 36.0);
                ui.add_space(16.0);
                ui.label(RichText::new("витрина набора деталей").color(t::p().text_muted));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let items: Vec<(ThemePref, String)> = ThemePref::ALL.iter().map(|v| (*v, v.label().to_string())).collect();
                    if kit::combo(ui, "gallery-theme", theme, &items, 170.0) {
                        t::set_theme(ui.ctx(), *theme);
                    }
                });
            });
            section(ui, "Кнопки", |ui| {
                ui.horizontal(|ui| {
                    kit::button(ui, ButtonKind::Primary, "Войти", None);
                    kit::button(ui, ButtonKind::Secondary, "Пригласить", Some(egui_phosphor::fill::USER_PLUS));
                    kit::button(ui, ButtonKind::Danger, "Удалить", Some(egui_phosphor::fill::TRASH));
                    kit::button(ui, ButtonKind::Ghost, "Отмена", None);
                    kit::button_wide(ui, ButtonKind::Primary, "Недоступно", 150.0, false);
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    kit::icon_button(ui, ph::GEAR_SIX, IconState::Normal, "Обычная");
                    kit::icon_button(ui, egui_phosphor::fill::MICROPHONE, IconState::Active, "Включено");
                    kit::icon_button(ui, egui_phosphor::fill::MICROPHONE_SLASH, IconState::Off, "Выключено");
                    ui.add_space(20.0);
                    kit::dock_button(ui, ph::MICROPHONE, "Микрофон", DockStyle::Live);
                    kit::dock_button(ui, ph::HEADPHONES, "Звук", DockStyle::Normal);
                    kit::dock_button(ui, ph::SPEAKER_SLASH, "Звук", DockStyle::Off);
                    kit::dock_button(ui, ph::PHONE_X, "Отключиться", DockStyle::Leave);
                });
            });
            section(ui, "Поля и переключатели", |ui| {
                ui.horizontal(|ui| {
                    ui.add(kit::text_field(&mut s.text, 220.0));
                    ui.add(kit::text_field(&mut s.pass, 180.0).password(true));
                    ui.add(kit::text_field(&mut s.search, 200.0).hint_text(format!("{}  Поиск", ph::MAGNIFYING_GLASS)));
                });
                ui.add_space(8.0);
                ui.add(kit::text_area(&mut s.area, 620.0).hint_text("Написать в #общий").desired_rows(2));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    kit::switch(ui, &mut s.on);
                    ui.add_space(16.0);
                    kit::slider(ui, &mut s.vol, 0.0..=2.0, 220.0);
                    ui.add_space(16.0);
                    let items = vec![(0u8, "Нейросеть (DeepFilterNet)".to_string()), (1, "Обычное (WebRTC)".into()), (2, "Выключено".into())];
                    kit::combo(ui, "gallery-combo", &mut s.pick, &items, 220.0);
                    ui.add_space(16.0);
                    kit::keycap(ui, "V");
                });
                ui.add_space(8.0);
                kit::level_meter(ui, Some(0.55), 300.0);
            });
            section(ui, "Списки и серверы", |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(260.0);
                        kit::group_label(ui, "Голосовые каналы");
                        kit::row(ui, ph::SPEAKER_HIGH, "Катка", true, Some("4"), true);
                        kit::row(ui, ph::SPEAKER_HIGH, "Тихая", false, None, true);
                        kit::group_label(ui, "Текстовые каналы");
                        kit::row(ui, ph::HASH, "общий", false, Some("12"), true);
                        kit::row(ui, ph::HASH, "архив", false, None, false);
                    });
                    ui.add_space(30.0);
                    ui.vertical(|ui| {
                        kit::server_icon(ui, &kit::ServerMark::Letter("A".into()), true, "Наша тусовка");
                        kit::server_icon(ui, &kit::ServerMark::Letter("У".into()), false, "Учёба");
                        kit::server_icon(ui, &kit::ServerMark::Letter("G".into()), false, "Games");
                    });
                    ui.add_space(30.0);
                    ui.vertical(|ui| {
                        ui.set_width(220.0);
                        kit::menu_item(ui, egui_phosphor::fill::SPEAKER_HIGH, "Громкость", false);
                        kit::menu_item(ui, egui_phosphor::fill::USER, "Профиль", false);
                        kit::menu_item(ui, egui_phosphor::fill::PROHIBIT, "Выгнать", true);
                    });
                });
            });
            section(ui, "Люди", |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(360.0, 80.0), Sense::hover());
                    let pnt = ui.painter();
                    let who = |n| kit::AvatarInput { name: n, texture: None };
                    kit::avatar(pnt, rect.left_center() + vec2(40.0, 0.0), 26.0, &who("mayuri"), 0.8, PeerBadge::None, false);
                    kit::avatar(pnt, rect.left_center() + vec2(120.0, 0.0), 26.0, &who("kurisu"), 0.0, PeerBadge::MicOff, true);
                    kit::avatar(pnt, rect.left_center() + vec2(200.0, 0.0), 26.0, &who("daru"), 0.0, PeerBadge::Deaf, false);
                    kit::avatar(pnt, rect.left_center() + vec2(280.0, 0.0), 26.0, &who("tester"), 0.0, PeerBadge::None, false);
                });
                ui.add_space(8.0);
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width().min(760.0), 170.0), Sense::hover());
                let w = (row.width() - 24.0) / 3.0;
                let r = |i: f32| Rect::from_min_size(row.min + vec2(i * (w + 12.0), 0.0), vec2(w, row.height()));
                kit::peer_tile(ui, r(0.0), &kit::PeerTile { who: kit::AvatarInput { name: "mayuri", texture: None }, name: "mayuri", speaking: 0.8, badge: PeerBadge::None, is_local: false, streaming: false });
                kit::peer_tile(ui, r(1.0), &kit::PeerTile { who: kit::AvatarInput { name: "tester", texture: None }, name: "Тестер", speaking: 0.0, badge: PeerBadge::None, is_local: true, streaming: true });
                kit::stream_tile_frame(ui.painter(), r(2.0), false);
                kit::live_badge(ui.painter(), r(2.0).right_top() + vec2(-12.0, 12.0));
            });
            section(ui, "Уведомления и окна", |ui| {
                if !s.notice_closed && kit::notice(ui, Notice::Error, "Не удалось войти в канал: нет связи с сервером голоса.", true) {
                    s.notice_closed = true;
                }
                ui.add_space(6.0);
                kit::notice(ui, Notice::Warn, "Связь пропала — переподключаемся…", false);
                ui.add_space(6.0);
                kit::notice(ui, Notice::Info, "Доступна версия 0.3.0", false);
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if kit::button(ui, ButtonKind::Secondary, "Открыть окно", None).clicked() {
                        s.modal = true;
                    }
                    kit::spinner(ui, 24.0);
                });
            });
            section(ui, "Пустое состояние", |ui| {
                kit::empty_state(ui, MascotPose::Sleeping, "Тут тихо", "Позови своих", Some("Пригласить"));
            });
        });
    });
    if s.modal
        && kit::modal(ui.ctx(), "gallery-modal", "Пригласить друзей", 420.0, |ui| {
            ui.label("Отправьте ссылку — по ней можно войти на сервер.");
            ui.add_space(10.0);
            kit::button_wide(ui, ButtonKind::Primary, "Скопировать ссылку", 420.0, true);
        }) == kit::ModalResult::Closed
    {
        s.modal = false;
    }
}
