//! Окно клиента: вход по инвайту → каналы → комната, настройки звука. Вёрстка — design/mockup-phase3.html.

use std::collections::HashMap;
use std::sync::mpsc as std_mpsc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Frame, Id, Layout, Margin, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, TextEdit, Ui, pos2, vec2,
};
use egui_phosphor::regular as ph;
use proto::{AuthResponse, Channel, ChannelKind, Invite, User};

use crate::api::{self, Api, ApiError, ApiResult};
use crate::config::Settings;
use crate::mictest::{self, MicTest};
use crate::picker::{self, Picker};
use crate::ptt::{self, Ptt};
use crate::theme::{self as t, DockStyle};
use crate::voice::{self, AudioPrefs, Cmd, Conn, Peer, VoiceHandle, VoiceState};

/// (микрофон, вывод, шумоподавление, автогромкость, слышать себя).
type MicTestKey = (Option<String>, Option<String>, bool, bool, bool);

const CHANNELS_REFRESH: Duration = Duration::from_secs(60);

enum Msg {
    Me(ApiResult<User>),
    Auth(ApiResult<AuthResponse>),
    Channels(ApiResult<Vec<Channel>>),
    Invite(ApiResult<Invite>),
}

#[derive(PartialEq)]
enum Screen {
    /// Проверяем сохранённую сессию.
    Boot,
    Login,
    Main,
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Login,
    Invite,
}

struct LoginForm {
    tab: Tab,
    name: String,
    password: String,
    invite: [String; 3],
    error: Option<String>,
    busy: bool,
    server_edit: Option<String>,
}

struct VolumePopup {
    identity: String,
    name: String,
    pos: Pos2,
    opened_at: Instant,
}

pub struct App {
    rt: tokio::runtime::Runtime,
    ctx: egui::Context,
    settings: Settings,
    api: Api,
    tx: std_mpsc::Sender<Msg>,
    rx: std_mpsc::Receiver<Msg>,
    voice: VoiceHandle,
    ptt: Ptt,
    ptt_rx: std_mpsc::Receiver<u32>,
    screen: Screen,
    boot_error: Option<String>,
    login: LoginForm,
    me: Option<User>,
    channels: Vec<Channel>,
    channels_error: Option<String>,
    channels_at: Instant,
    settings_open: bool,
    popup: Option<VolumePopup>,
    invite: Option<Result<Invite, String>>,
    invite_busy: bool,
    mic_test: Option<MicTest>,
    /// Параметры, с которыми запущена проверка: (микрофон, вывод, ns, agc, слышать себя).
    mic_test_key: Option<MicTestKey>,
    mic_test_error: Option<String>,
    mic_level: Arc<AtomicU32>,
    mic_playback: bool,
    /// Пикер источника трансляции.
    picker: Option<Picker>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, rt: tokio::runtime::Runtime) -> Self {
        t::install(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let settings = Settings::load();
        let api = Api::new(&settings.server_url, settings.session_token.clone());
        let repaint: Arc<dyn Fn() + Send + Sync> = {
            let ctx = ctx.clone();
            Arc::new(move || ctx.request_repaint())
        };
        let prefs = AudioPrefs {
            input: settings.input_device.clone(),
            output: settings.output_device.clone(),
            aec: settings.echo_cancellation,
            ns: settings.noise_suppression,
            agc: settings.auto_gain,
            ptt_enabled: settings.ptt_enabled,
            sounds: settings.sounds,
            volumes: settings.user_volume.keys().map(|k| (k.clone(), settings.effective_volume(k))).collect::<HashMap<_, _>>(),
        };
        let voice = voice::spawn(rt.handle(), api.clone(), prefs, repaint.clone());
        let (ptt_tx, ptt_rx) = std_mpsc::channel();
        let ptt = Ptt::start(settings.ptt_key, settings.ptt_enabled, voice.sender(), ptt_tx, repaint);
        // Отладка: AMADEUS_START_DEAF=1 — второй клиент на том же ПК без заводки через колонки.
        #[cfg(debug_assertions)]
        if std::env::var_os("AMADEUS_START_DEAF").is_some() {
            voice.send(Cmd::SetDeafened(true));
        }
        let (tx, rx) = std_mpsc::channel();
        let has_token = settings.session_token.is_some();
        let login = LoginForm {
            tab: if settings.last_name.is_empty() { Tab::Invite } else { Tab::Login },
            name: settings.last_name.clone(),
            password: String::new(),
            invite: Default::default(),
            error: None,
            busy: false,
            server_edit: None,
        };
        let mut app = Self {
            rt,
            ctx,
            settings,
            api,
            tx,
            rx,
            voice,
            ptt,
            ptt_rx,
            screen: if has_token { Screen::Boot } else { Screen::Login },
            boot_error: None,
            login,
            me: None,
            channels: Vec::new(),
            channels_error: None,
            channels_at: Instant::now(),
            settings_open: false,
            popup: None,
            invite: None,
            invite_busy: false,
            mic_test: None,
            mic_test_key: None,
            mic_test_error: None,
            mic_level: Arc::new(AtomicU32::new(0)),
            mic_playback: false,
            picker: None,
        };
        if has_token {
            app.fetch_me();
        }
        #[cfg(debug_assertions)]
        if std::env::var_os("AMADEUS_OPEN_SETTINGS").is_some() {
            app.open_settings();
        }
        app
    }

    // ------------------------------------------------------------ фон

    fn spawn<T: Send + 'static>(
        &self,
        fut: impl std::future::Future<Output = T> + Send + 'static,
        wrap: impl FnOnce(T) -> Msg + Send + 'static,
    ) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        self.rt.spawn(async move {
            let r = fut.await;
            let _ = tx.send(wrap(r));
            ctx.request_repaint();
        });
    }

    fn fetch_me(&mut self) {
        self.boot_error = None;
        let api = self.api.clone();
        self.spawn(async move { api.me().await }, Msg::Me);
    }

    fn fetch_channels(&mut self) {
        self.channels_at = Instant::now();
        let api = self.api.clone();
        self.spawn(async move { api.channels().await }, Msg::Channels);
    }

    fn set_api(&mut self, api: Api) {
        self.api = api.clone();
        self.voice.send(Cmd::SetApi(api));
    }

    fn handle_msgs(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Me(Ok(user)) => {
                    self.me = Some(user);
                    self.screen = Screen::Main;
                    self.fetch_channels();
                }
                Msg::Me(Err(e)) => {
                    if e.is_unauthorized() {
                        self.settings.session_token = None;
                        self.settings.save();
                        self.set_api(self.api.with_token(None));
                        self.screen = Screen::Login;
                    } else {
                        self.boot_error = Some(e.message);
                    }
                }
                Msg::Auth(res) => {
                    self.login.busy = false;
                    match res {
                        Ok(auth) => {
                            self.settings.session_token = Some(auth.token.clone());
                            self.settings.last_name = auth.user.name.clone();
                            self.settings.save();
                            self.set_api(self.api.with_token(Some(auth.token)));
                            self.me = Some(auth.user);
                            self.login.password.clear();
                            self.login.invite = Default::default();
                            self.login.error = None;
                            self.screen = Screen::Main;
                            self.fetch_channels();
                        }
                        Err(e) => self.login.error = Some(e.message),
                    }
                }
                Msg::Channels(Ok(list)) => {
                    // Отладка: AMADEUS_AUTOJOIN=<id канала> — сразу зайти (только debug-сборка).
                    #[cfg(debug_assertions)]
                    if self.channels.is_empty()
                        && let Some(id) = std::env::var("AMADEUS_AUTOJOIN").ok().and_then(|v| v.parse::<i64>().ok())
                        && let Some(ch) = list.iter().find(|c| c.id == id)
                    {
                        self.voice.send(Cmd::Join(ch.clone()));
                    }
                    self.channels = list;
                    self.channels_error = None;
                }
                Msg::Channels(Err(e)) => {
                    if e.is_unauthorized() {
                        self.logout();
                    } else {
                        self.channels_error = Some(e.message);
                    }
                }
                Msg::Invite(res) => {
                    self.invite_busy = false;
                    self.invite = Some(res.map_err(|e: ApiError| e.message));
                }
            }
        }
        while let Ok(vk) = self.ptt_rx.try_recv() {
            if vk != 0x1B {
                self.settings.ptt_key = vk;
                self.ptt.set_key(vk);
                self.settings.save();
            }
        }
    }

    fn logout(&mut self) {
        self.voice.send(Cmd::Leave);
        let api = self.api.clone();
        self.rt.spawn(async move {
            let _ = api.logout().await;
        });
        self.settings.session_token = None;
        self.settings.save();
        self.set_api(self.api.with_token(None));
        self.me = None;
        self.channels.clear();
        self.settings_open = false;
        self.stop_mic_test();
        self.popup = None;
        self.invite = None;
        self.login.tab = Tab::Login;
        self.login.name = self.settings.last_name.clone();
        self.screen = Screen::Login;
    }

    fn submit_login(&mut self) {
        let f = &mut self.login;
        let name = f.name.trim().to_string();
        let pass = f.password.clone();
        let code = api::normalize_invite(&f.invite.concat());
        f.error = None;
        if name.chars().count() < proto::limits::NAME_MIN {
            f.error = Some("Введите ник.".into());
            return;
        }
        if f.tab == Tab::Invite {
            if code.len() != 12 {
                f.error = Some("Код приглашения — 12 знаков (XXXX-XXXX-XXXX).".into());
                return;
            }
            if pass.chars().count() < proto::limits::PASSWORD_MIN {
                f.error = Some(format!("Пароль — минимум {} символов.", proto::limits::PASSWORD_MIN));
                return;
            }
        } else if pass.is_empty() {
            f.error = Some("Введите пароль.".into());
            return;
        }
        f.busy = true;
        let api = self.api.with_token(None);
        let tab = f.tab;
        self.spawn(
            async move {
                if tab == Tab::Invite {
                    api.register(&api::format_invite(&code), &name, &pass).await
                } else {
                    api.login(&name, &pass).await
                }
            },
            Msg::Auth,
        );
    }

    fn apply_server(&mut self, raw: &str) {
        let mut url = raw.trim().trim_end_matches('/').to_string();
        if url.is_empty() {
            url = crate::config::DEFAULT_SERVER.to_string();
        }
        if !url.starts_with("http://") && !url.starts_with("https://") {
            url = format!("https://{url}");
        }
        self.settings.server_url = url.clone();
        self.settings.save();
        self.set_api(Api::new(&url, None));
    }

    // ------------------------------------------------------------ экран входа

    fn ui_boot(&mut self, ui: &mut Ui) {
        egui::CentralPanel::no_frame().frame(Frame::new().fill(t::GLASS)).show(ui, |ui| {
            let r = ui.max_rect();
            let p = ui.painter();
            t::logo(p, Rect::from_center_size(r.center() - vec2(0.0, 40.0), vec2(96.0, 96.0)), true);
            match &self.boot_error {
                None => {
                    p.text(r.center() + vec2(0.0, 40.0), Align2::CENTER_CENTER, "Подключаемся к серверу…", t::font(15.0), t::DUST);
                }
                Some(err) => {
                    let err = err.clone();
                    let area = Rect::from_center_size(r.center() + vec2(0.0, 80.0), vec2(420.0, 120.0));
                    ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new(format!("{} {err}", ph::WARNING_CIRCLE)).color(t::ERR_TEXT));
                            ui.horizontal(|ui| {
                                ui.add_space(120.0);
                                if ui.button("Повторить").clicked() {
                                    self.fetch_me();
                                }
                                if ui.button("Выйти").clicked() {
                                    self.logout();
                                }
                            });
                        });
                    });
                }
            }
        });
    }

    fn ui_login(&mut self, ui: &mut Ui) {
        egui::Panel::right("login-form")
            .exact_size(440.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(t::BAKELITE).inner_margin(Margin { left: 44, right: 44, top: 56, bottom: 56 }))
            .show(ui, |ui| {
                let r = ui.max_rect();
                ui.painter().vline(r.left() - 44.0, ui.clip_rect().y_range(), Stroke::new(1.0, t::BRASS));
                self.ui_login_form(ui);
            });
        egui::CentralPanel::no_frame().frame(Frame::new().fill(t::GLASS).inner_margin(Margin::symmetric(64, 56))).show(ui, |ui| {
            let r = ui.max_rect();
            let p = ui.painter().clone();
            // мягкое янтарное свечение за числом
            let c = pos2(r.left() + r.width() * 0.3, r.top() + r.height() * 0.62);
            t::radial_glow(&p, c, 380.0, Color32::from_rgba_unmultiplied(255, 140, 40, 26));
            t::wordmark(ui, 52.0, 26.0);
            let mid = r.center().y;
            t::nixie_text(&p, pos2(r.left(), mid + 10.0), Align2::LEFT_BOTTOM, "1.048596", 120.0);
            p.text(
                pos2(r.left(), mid + 30.0),
                Align2::LEFT_TOP,
                "Голосовой чат для своих.\nПопасть сюда можно только по приглашению.",
                t::font(15.0),
                t::DUST,
            );
            // адрес сервера
            let bottom = Rect::from_min_max(pos2(r.left(), r.bottom() - 34.0), r.max);
            ui.scope_builder(egui::UiBuilder::new().max_rect(bottom), |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    match self.login.server_edit.take() {
                        None => {
                            let host = self.settings.server_url.trim_start_matches("https://").trim_start_matches("http://").to_string();
                            ui.label(RichText::new(format!("{} {host} ·", ph::HARD_DRIVES)).color(t::DUST).size(13.0));
                            let link = ui.add(egui::Label::new(RichText::new("сменить сервер").size(13.0).underline()).sense(Sense::click()));
                            if link.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                self.login.server_edit = Some(self.settings.server_url.clone());
                            } 
                        }
                        Some(mut s) => {
                            ui.label(RichText::new(ph::HARD_DRIVES).color(t::DUST));
                            let r = ui.add(TextEdit::singleline(&mut s).desired_width(280.0).hint_text("https://api.example.ru"));
                            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                            if ui.button("OK").clicked() || enter {
                                self.apply_server(&s);
                            } else if ui.button("Отмена").clicked() {
                            } else {
                                self.login.server_edit = Some(s);
                            }
                        }
                    }
                });
            });
        });
    }

    fn ui_login_form(&mut self, ui: &mut Ui) {
        let w = ui.available_width();
        // вкладки
        let (tabs_rect, _) = ui.allocate_exact_size(vec2(w, 42.0), Sense::hover());
        ui.painter().rect_filled(tabs_rect, CornerRadius::same(10), t::GLASS);
        let half = (tabs_rect.width() - 8.0) / 2.0;
        for (i, (tab, label)) in [(Tab::Login, "Вход"), (Tab::Invite, "У меня приглашение")].into_iter().enumerate() {
            let rr = Rect::from_min_size(tabs_rect.min + vec2(4.0 + half * i as f32, 4.0), vec2(half, 34.0));
            let resp = ui.interact(rr, Id::new(("tab", i)), Sense::click());
            let on = self.login.tab == tab;
            if on {
                ui.painter().rect_filled(rr, CornerRadius::same(7), t::BAKELITE_2);
            }
            let color = if on || resp.hovered() { t::PAPER } else { t::DUST };
            ui.painter().text(rr.center(), Align2::CENTER_CENTER, label, t::semibold(14.0), color);
            if resp.clicked() && !on {
                self.login.tab = tab;
                self.login.error = None;
            }
        }
        ui.add_space(22.0);

        let mut submit = false;
        if self.login.tab == Tab::Invite {
            label(ui, "Код приглашения");
            self.invite_input(ui, w);
            ui.label(RichText::new("Регистр и дефисы не важны — можно вставить целиком.").size(12.5).color(t::DUST));
            ui.add_space(12.0);
        }
        label(ui, "Ник");
        let r = ui.add(field(&mut self.login.name, w).hint_text("например, okabe"));
        submit |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(12.0);
        label(ui, "Пароль");
        let r = ui.add(field(&mut self.login.password, w).password(true));
        submit |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(14.0);
        if let Some(err) = &self.login.error {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{}  {err}", ph::WARNING_CIRCLE)).size(13.0).color(t::ERR_TEXT));
            });
        }
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            let text = match (self.login.busy, self.login.tab) {
                (true, _) => "Подождите…",
                (false, Tab::Login) => "Войти",
                (false, Tab::Invite) => "Создать аккаунт",
            };
            if t::primary_button(ui, text, w, !self.login.busy).clicked() {
                submit = true;
            }
        });
        if submit && !self.login.busy {
            self.submit_login();
        }
    }

    /// Три сегмента кода «газоразрядными» цифрами; вставка целиком раскладывается по сегментам.
    fn invite_input(&mut self, ui: &mut Ui, w: f32) {
        let ids = [Id::new("inv0"), Id::new("inv1"), Id::new("inv2")];
        let dash_w = 14.0;
        let seg_w = (w - 2.0 * (dash_w + 16.0) - 4.0) / 3.0;
        let mut edited = None;
        ui.horizontal(|ui| {
            for i in 0..3 {
                let resp = ui.add(
                    TextEdit::singleline(&mut self.login.invite[i])
                        .id(ids[i])
                        .font(t::nixie(26.0))
                        .text_color(t::NIXIE)
                        .horizontal_align(Align::Center)
                        .desired_width(seg_w)
                        .margin(Margin::symmetric(10, 6))
                        .hint_text(RichText::new("····").font(t::nixie(26.0)).color(t::BRASS)),
                );
                if resp.changed() {
                    edited = Some(i);
                }
                if i < 2 {
                    ui.label(RichText::new("–").color(t::BRASS).size(20.0));
                }
            }
        });
        if let Some(i) = edited {
            let before = self.login.invite[i].len();
            let all = api::normalize_invite(&self.login.invite.concat());
            for (k, seg) in self.login.invite.iter_mut().enumerate() {
                *seg = all.chars().skip(k * 4).take(4).collect();
            }
            // заполнили сегмент (или вставили код целиком) — фокус дальше
            if before >= 4 {
                let target = (all.len() / 4).min(2);
                if target != i {
                    ui.memory_mut(|m| m.request_focus(ids[target]));
                }
            }
        }
    }

    // ------------------------------------------------------------ главный экран

    fn ui_main(&mut self, ui: &mut Ui) {
        if self.channels_at.elapsed() > CHANNELS_REFRESH {
            self.fetch_channels();
        }
        #[allow(unused_mut)]
        let mut vs = self.voice.snapshot();
        #[cfg(debug_assertions)]
        if std::env::var_os("AMADEUS_DEMO").is_some() {
            vs = demo_state(ui.input(|i| i.time));
        }
        #[cfg(debug_assertions)]
        self.debug_autostream(&vs);
        self.ui_sidebar(ui, &vs);
        self.ui_header(ui, &vs);
        self.ui_dock(ui, &vs);
        egui::CentralPanel::no_frame().frame(Frame::new().fill(t::GLASS).inner_margin(Margin::same(28))).show(ui, |ui| {
            self.ui_stage(ui, &vs);
        });
        self.ui_banner(ui, &vs);
        self.ui_volume_popup(ui);
        if self.settings_open {
            self.ui_settings(ui, &vs);
        }
        if self.picker.is_some() {
            self.ui_picker(ui, &vs);
        }
    }

    /// Отладка: AMADEUS_AUTOSTREAM=screen | <часть заголовка окна> [+ AMADEUS_STREAM_PRESET=720|1080|1080p60|native] —
    /// начать трансляцию сразу после входа в канал (один раз).
    #[cfg(debug_assertions)]
    fn debug_autostream(&mut self, vs: &VoiceState) {
        use std::sync::atomic::AtomicBool;
        static DONE: AtomicBool = AtomicBool::new(false);
        static PICKER: AtomicBool = AtomicBool::new(false);
        // AMADEUS_OPEN_PICKER=1 — открыть пикер сразу после входа (скриншоты).
        if vs.conn == Conn::Connected && std::env::var_os("AMADEUS_OPEN_PICKER").is_some() && !PICKER.swap(true, Ordering::Relaxed) {
            self.open_picker(vs);
        }
        let Ok(want) = std::env::var("AMADEUS_AUTOSTREAM") else { return };
        if vs.conn != Conn::Connected || DONE.swap(true, Ordering::Relaxed) {
            return;
        }
        let preset = match std::env::var("AMADEUS_STREAM_PRESET").as_deref() {
            Ok("720") => crate::stream::Preset::P720_30,
            Ok("1080p60") => crate::stream::Preset::P1080_60,
            Ok("1440") => crate::stream::Preset::P1440_60,
            Ok("native") => crate::stream::Preset::Native,
            _ => crate::stream::Preset::P1080_30,
        };
        let tx = self.voice.sender();
        std::thread::spawn(move || {
            let list = capture_win::list_sources(64, 36);
            let found = if want == "screen" {
                list.into_iter().find(|s| !s.id.is_window())
            } else {
                list.into_iter().find(|s| s.title.to_lowercase().contains(&want.to_lowercase()))
            };
            match found {
                Some(s) => {
                    let _ = tx.send(Cmd::StartStream { source: s.id, title: s.title, width: s.width, height: s.height, preset });
                }
                None => log::warn!("AMADEUS_AUTOSTREAM: источник «{want}» не найден"),
            }
            // AMADEUS_AUTOSTREAM_THEN=<часть заголовка> — через 15 с сменить источник на лету.
            if let Ok(next) = std::env::var("AMADEUS_AUTOSTREAM_THEN") {
                std::thread::sleep(Duration::from_secs(15));
                let list = capture_win::list_sources(64, 36);
                if let Some(s) = list.into_iter().find(|s| s.title.to_lowercase().contains(&next.to_lowercase())) {
                    let _ = tx.send(Cmd::StartStream { source: s.id, title: s.title, width: s.width, height: s.height, preset });
                }
            }
        });
    }

    fn open_picker(&mut self, vs: &VoiceState) {
        let current = vs.stream.as_ref().map(|s| s.source);
        let preset = vs.stream.as_ref().map(|s| s.preset).unwrap_or(self.settings.stream_preset);
        self.picker = Some(Picker::open(&self.ctx, preset, current));
    }

    fn ui_picker(&mut self, ui: &mut Ui, vs: &VoiceState) {
        if vs.channel.is_none() {
            self.picker = None;
            return;
        }
        let Some(p) = &mut self.picker else { return };
        match p.ui(ui.ctx()) {
            picker::Action::None => {}
            picker::Action::Cancel => self.picker = None,
            picker::Action::Start { source, title, width, height, preset } => {
                self.settings.stream_preset = preset;
                self.settings.save();
                self.voice.send(Cmd::StartStream { source, title, width, height, preset });
                self.picker = None;
            }
        }
    }

    fn ui_sidebar(&mut self, ui: &mut Ui, vs: &VoiceState) {
        egui::Panel::left("side")
            .exact_size(260.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(t::BAKELITE))
            .show(ui, |ui| {
                let full = ui.max_rect();
                ui.painter().vline(full.right(), full.y_range(), Stroke::new(1.0, t::BRASS_DIM));

                egui::Panel::top("side-top")
                    .show_separator_line(false)
                    .frame(Frame::new().inner_margin(Margin { left: 18, right: 18, top: 16, bottom: 12 }))
                    .show(ui, |ui| {
                        t::wordmark(ui, 30.0, 18.0);
                        let r = ui.max_rect();
                        ui.painter().hline(full.x_range(), r.bottom() + 12.0, Stroke::new(1.0, t::BRASS_DIM));
                    });
                egui::Panel::bottom("side-me")
                    .show_separator_line(false)
                    .frame(Frame::new().fill(t::ME_BG).inner_margin(Margin::symmetric(12, 12)))
                    .show(ui, |ui| self.ui_me(ui, vs));
                egui::CentralPanel::no_frame().show(ui, |ui| {
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.ui_channel_list(ui, vs));
                });
            });
    }

    fn ui_channel_list(&mut self, ui: &mut Ui, vs: &VoiceState) {
        ui.spacing_mut().item_spacing.y = 2.0;
        group_label(ui, "Голосовые каналы");
        let voice: Vec<Channel> = self.channels.iter().filter(|c| c.kind == ChannelKind::Voice).cloned().collect();
        if voice.is_empty() {
            let text = self.channels_error.clone().unwrap_or_else(|| "Загрузка…".into());
            ui.horizontal(|ui| {
                ui.add_space(18.0);
                ui.label(RichText::new(text).size(13.0).color(t::DUST));
            });
        }
        for ch in voice {
            let active = vs.channel.as_ref().is_some_and(|c| c.id == ch.id);
            let count = if active && vs.conn == Conn::Connected { Some(vs.peers.len()) } else { None };
            if channel_row(ui, ph::SPEAKER_HIGH, &ch.name, active, count, true).clicked() && !(active && vs.conn == Conn::Connected) {
                self.voice.send(Cmd::Join(ch.clone()));
            }
            if active {
                for p in &vs.peers {
                    member_row(ui, p);
                }
            }
        }
        let text: Vec<Channel> = self.channels.iter().filter(|c| c.kind == ChannelKind::Text).cloned().collect();
        if !text.is_empty() {
            ui.add_space(6.0);
            group_label(ui, "Текстовые каналы");
            for ch in text {
                channel_row(ui, ph::HASH, &ch.name, false, None, false).on_hover_text("Текстовые каналы появятся в следующих версиях");
            }
        }
    }

    fn ui_me(&mut self, ui: &mut Ui, vs: &VoiceState) {
        let name = self.me.as_ref().map(|u| u.name.clone()).unwrap_or_default();
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
            let glow = if vs.transmitting && vs.peers.first().is_some_and(|p| p.speaking) { 0.6 } else { 0.0 };
            t::avatar(ui.painter(), rect.center(), 17.0, &name, glow, false);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new(&name).font(t::semibold(14.0)));
                let sub = match (&vs.conn, &vs.channel) {
                    (Conn::Connected, Some(c)) => format!("в {}", c.name),
                    (Conn::Connecting, Some(_)) => "подключение…".into(),
                    (Conn::Reconnecting | Conn::Rejoining { .. }, _) => "нет связи…".into(),
                    _ => "не в канале".into(),
                };
                ui.label(RichText::new(sub).size(12.0).color(t::DUST));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                if t::icon_button(ui, ph::GEAR_SIX, false, false, "Настройки").clicked() {
                    self.open_settings();
                }
                let (icon, tip) = if vs.deafened { (ph::SPEAKER_SLASH, "Включить звук") } else { (ph::HEADPHONES, "Выключить звук") };
                if t::icon_button(ui, icon, vs.deafened, vs.deafened, tip).clicked() {
                    self.voice.send(Cmd::SetDeafened(!vs.deafened));
                }
                let mic_off = vs.self_muted || vs.deafened;
                let (icon, tip) = if mic_off { (ph::MICROPHONE_SLASH, "Включить микрофон") } else { (ph::MICROPHONE, "Выключить микрофон") };
                if t::icon_button(ui, icon, mic_off, mic_off, tip).clicked() {
                    self.voice.send(Cmd::SetSelfMuted(!mic_off));
                }
            });
        });
    }

    fn ui_header(&mut self, ui: &mut Ui, vs: &VoiceState) {
        egui::Panel::top("head")
            .exact_size(64.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(t::GLASS).inner_margin(Margin::symmetric(24, 0)))
            .show(ui, |ui| {
                let r = ui.max_rect();
                ui.painter().hline(r.x_range().expand(24.0), r.bottom(), Stroke::new(1.0, t::BRASS_DIM));
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    match &vs.channel {
                        Some(ch) => {
                            ui.label(RichText::new(ph::SPEAKER_HIGH).size(20.0).color(t::DUST));
                            ui.label(RichText::new(&ch.name).font(t::xbold(18.0)));
                            if vs.conn == Conn::Connected {
                                ui.label(
                                    RichText::new(t::plural(vs.peers.len(), "участник", "участника", "участников"))
                                        .size(13.5)
                                        .color(t::DUST),
                                );
                            }
                        }
                        None => {
                            ui.label(RichText::new("Не в канале").font(t::xbold(18.0)).color(t::DUST));
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(rtt) = vs.rtt {
                            ping_box(ui, rtt);
                        }
                        if let Some(s) = &vs.stream {
                            ui.add_space(4.0);
                            stream_pill(ui, s);
                        }
                    });
                });
            });
    }

    fn ui_dock(&mut self, ui: &mut Ui, vs: &VoiceState) {
        egui::Panel::bottom("dock")
            .exact_size(76.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(t::DOCK))
            .show(ui, |ui| {
                let r = ui.max_rect();
                ui.painter().hline(r.x_range(), r.top(), Stroke::new(1.0, t::BRASS_DIM));
                let in_room = vs.channel.is_some();
                let mic_off = vs.self_muted || vs.deafened;
                let (mic_icon, mic_style) = if mic_off {
                    (ph::MICROPHONE_SLASH, DockStyle::Off)
                } else if vs.transmitting {
                    (ph::MICROPHONE, DockStyle::Live)
                } else {
                    (ph::MICROPHONE, DockStyle::Normal)
                };
                let snd_style = if vs.deafened { DockStyle::Off } else { DockStyle::Normal };
                let snd_icon = if vs.deafened { ph::SPEAKER_SLASH } else { ph::HEADPHONES };
                let key = ptt::key_name(self.settings.ptt_key);
                // ширина ряда — чтобы отцентровать
                let can_stream = in_room && vs.conn == Conn::Connected;
                let streaming = vs.stream.is_some();
                let est = 140.0
                    + 110.0
                    + if vs.ptt_enabled { 150.0 + key.len() as f32 * 7.0 } else { 0.0 }
                    + if in_room { 170.0 } else { 0.0 }
                    + if streaming { 300.0 } else if can_stream { 120.0 } else { 0.0 };
                ui.horizontal_centered(|ui| {
                    ui.add_space(((r.width() - est) / 2.0).max(12.0));
                    ui.spacing_mut().item_spacing.x = 12.0;
                    if t::dock_button(ui, mic_icon, "Микрофон", mic_style).clicked() {
                        self.voice.send(Cmd::SetSelfMuted(!mic_off));
                    }
                    if t::dock_button(ui, snd_icon, "Звук", snd_style).clicked() {
                        self.voice.send(Cmd::SetDeafened(!vs.deafened));
                    }
                    if vs.ptt_enabled {
                        ui.add_space(4.0);
                        ui.label(RichText::new("Рация: держите").size(13.0).color(t::DUST));
                        t::keycap(ui, &key);
                        ui.add_space(4.0);
                    }
                    if streaming {
                        if t::dock_button(ui, ph::MONITOR_ARROW_UP, "Остановить", DockStyle::Live)
                            .on_hover_text("Остановить трансляцию")
                            .clicked()
                        {
                            self.voice.send(Cmd::StopStream);
                        }
                        if t::dock_button(ui, ph::SWAP, "Сменить", DockStyle::Normal).on_hover_text("Сменить окно или экран без остановки").clicked() {
                            self.open_picker(vs);
                        }
                    } else if can_stream
                        && t::dock_button(ui, ph::MONITOR_ARROW_UP, "Экран", DockStyle::Normal).on_hover_text("Показать окно или экран").clicked()
                    {
                        self.open_picker(vs);
                    }
                    if in_room && t::dock_button(ui, ph::PHONE_X, "Отключиться", DockStyle::Leave).clicked() {
                        self.voice.send(Cmd::Leave);
                    }
                });
            });
    }

    fn ui_stage(&mut self, ui: &mut Ui, vs: &VoiceState) {
        let r = ui.max_rect();
        match (&vs.conn, &vs.channel) {
            (Conn::Failed(msg), _) => {
                let msg = msg.clone();
                let area = Rect::from_center_size(r.center(), vec2(460.0, 140.0));
                ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(format!("{}  {msg}", ph::WARNING_CIRCLE)).color(t::ERR_TEXT));
                        ui.add_space(8.0);
                        if let Some(ch) = vs.channel.clone()
                            && ui.button("Повторить").clicked() {
                                self.voice.send(Cmd::Join(ch));
                            }
                    });
                });
            }
            (_, None) => {
                let p = ui.painter();
                t::logo(p, Rect::from_center_size(r.center() - vec2(0.0, 50.0), vec2(110.0, 110.0)), true);
                p.text(r.center() + vec2(0.0, 30.0), Align2::CENTER_TOP, "Выберите голосовой канал слева", t::semibold(16.0), t::DUST);
            }
            (Conn::Connecting, Some(_)) if vs.peers.is_empty() => {
                ui.painter().text(r.center(), Align2::CENTER_CENTER, "Подключаемся…", t::semibold(16.0), t::DUST);
            }
            _ => self.ui_tiles(ui, vs),
        }
    }

    fn ui_tiles(&mut self, ui: &mut Ui, vs: &VoiceState) {
        let r = ui.max_rect();
        let n = vs.peers.len().max(1);
        let cols = match n {
            1 => 1,
            2 => 2,
            3 | 5 | 6 | 9 => 3,
            4 => 2,
            _ => 4,
        };
        let rows = n.div_ceil(cols);
        let gap = 16.0;
        let tw = (r.width() - gap * (cols - 1) as f32) / cols as f32;
        let th = ((r.height() - gap * (rows - 1) as f32) / rows as f32).min(tw * 0.8);
        let total_h = th * rows as f32 + gap * (rows - 1) as f32;
        let top = r.top() + (r.height() - total_h) / 2.0;
        for (i, p) in vs.peers.iter().enumerate() {
            let (row, col) = (i / cols, i % cols);
            let in_row = if row == rows - 1 { n - row * cols } else { cols };
            let row_w = tw * in_row as f32 + gap * (in_row - 1) as f32;
            let left = r.left() + (r.width() - row_w) / 2.0;
            let rect = Rect::from_min_size(pos2(left + col as f32 * (tw + gap), top + row as f32 * (th + gap)), vec2(tw, th));
            let resp = ui.interact(rect, Id::new(("tile", &p.identity)), Sense::click());
            let glow = speak_glow(ui, p);
            tile(ui, rect, p, glow);
            if resp.clicked() && !p.is_local {
                self.popup = Some(VolumePopup {
                    identity: p.identity.clone(),
                    name: p.name.clone(),
                    pos: resp.interact_pointer_pos().unwrap_or(rect.center()),
                    opened_at: Instant::now(),
                });
            }
            if !p.is_local {
                resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            }
        }
    }

    fn ui_banner(&mut self, ui: &mut Ui, vs: &VoiceState) {
        if let Some(n) = &vs.stream_notice {
            egui::Area::new(Id::new("stream-notice"))
                .order(egui::Order::Foreground)
                .anchor(Align2::CENTER_BOTTOM, vec2(130.0, -96.0))
                .show(ui.ctx(), |ui| {
                    let r = Frame::new()
                        .fill(Color32::from_rgb(0x2E, 0x1A, 0x17))
                        .stroke(Stroke::new(1.0, Color32::from_rgb(0x6B, 0x33, 0x2C)))
                        .corner_radius(CornerRadius::same(255))
                        .inner_margin(Margin::symmetric(16, 8))
                        .show(ui, |ui| {
                            ui.label(RichText::new(format!("{}  {n}   {}", ph::MONITOR, ph::X)).color(t::ERR_TEXT).size(13.5));
                        })
                        .response
                        .interact(Sense::click());
                    if r.on_hover_text("Скрыть").clicked() {
                        self.voice.send(Cmd::DismissStreamNotice);
                    }
                });
        }
        let text = match vs.conn {
            Conn::Reconnecting => "Связь пропала — переподключаемся…".to_string(),
            Conn::Rejoining { attempt } => format!("Связь пропала — возвращаемся в канал (попытка {attempt})…"),
            _ => return,
        };
        let screen = ui.ctx().content_rect();
        egui::Area::new(Id::new("banner"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_TOP, vec2(130.0, 76.0))
            .show(ui.ctx(), |ui| {
                Frame::new()
                    .fill(Color32::from_rgb(0x2E, 0x22, 0x15))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(0x6B, 0x4A, 0x26)))
                    .corner_radius(CornerRadius::same(255))
                    .inner_margin(Margin::symmetric(16, 8))
                    .show(ui, |ui| {
                        ui.label(RichText::new(format!("{}  {text}", ph::ARROWS_CLOCKWISE)).color(t::FILAMENT).size(13.5));
                    });
            });
        let _ = screen;
    }

    fn ui_volume_popup(&mut self, ui: &mut Ui) {
        let Some(pop) = &self.popup else { return };
        let (identity, name, pos, opened) = (pop.identity.clone(), pop.name.clone(), pop.pos, pop.opened_at);
        let mut vol = self.settings.volume_of(&identity);
        let mut muted = self.settings.is_user_muted(&identity);
        let mut changed = false;
        let area = egui::Area::new(Id::new("vol-popup")).order(egui::Order::Foreground).fixed_pos(pos + vec2(8.0, 8.0)).show(ui.ctx(), |ui| {
            Frame::new()
                .fill(t::BAKELITE_2)
                .stroke(Stroke::new(1.0, t::BRASS))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::symmetric(16, 14))
                .shadow(ui.visuals().popup_shadow)
                .show(ui, |ui| {
                    ui.set_width(250.0);
                    ui.label(RichText::new(&name).font(t::bold(14.0)));
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Громкость").size(13.0).color(t::DUST));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(format!("{:.0}%", vol * 100.0)).size(13.0));
                        });
                    });
                    if t::slider(ui, &mut vol, 0.0..=2.0, 250.0).changed() {
                        changed = true;
                    }
                    ui.add_space(4.0);
                    let (icon, text) = if muted { (ph::SPEAKER_HIGH, "Включить для себя") } else { (ph::SPEAKER_SLASH, "Заглушить для себя") };
                    let r = ui.add(egui::Label::new(RichText::new(format!("{icon}  {text}")).size(13.5)).sense(Sense::click()));
                    if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        muted = !muted;
                        changed = true;
                    }
                    if (vol - 1.0).abs() > 0.01 {
                        let r = ui.add(egui::Label::new(RichText::new(format!("{}  Сбросить (100%)", ph::ARROW_COUNTER_CLOCKWISE)).size(13.5).color(t::DUST)).sense(Sense::click()));
                        if r.clicked() {
                            vol = 1.0;
                            changed = true;
                        }
                    }
                });
        });
        if changed {
            // «прилипание» к 100%
            if (vol - 1.0).abs() < 0.04 {
                vol = 1.0;
            }
            self.settings.user_volume.insert(identity.clone(), vol);
            self.settings.user_muted.insert(identity.clone(), muted);
            self.settings.save();
            self.voice.send(Cmd::SetUserVolume(identity.clone(), self.settings.effective_volume(&identity)));
        }
        let clicked_outside = ui.input(|i| i.pointer.any_pressed())
            && !area.response.contains_pointer()
            && opened.elapsed() > Duration::from_millis(150);
        if clicked_outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.popup = None;
        }
    }

    // ------------------------------------------------------------ настройки

    /// Держит проверку микрофона в актуальном состоянии, пока открыты настройки.
    fn sync_mic_test(&mut self, vs: &VoiceState) {
        let name_of = |id: &Option<String>, list: &[voice::Device]| {
            id.as_ref().and_then(|id| list.iter().find(|d| &d.id == id).map(|d| d.name.clone()))
        };
        let key = (
            name_of(&self.settings.input_device, &vs.inputs),
            name_of(&self.settings.output_device, &vs.outputs),
            self.settings.noise_suppression,
            self.settings.auto_gain,
            self.mic_playback,
        );
        if self.mic_test_key.as_ref() == Some(&key) {
            return;
        }
        self.mic_test = None; // сначала закрыть старые потоки
        self.mic_level.store(0, Ordering::Relaxed);
        match mictest::start(key.0.as_deref(), key.1.as_deref(), key.2, key.3, key.4, self.mic_level.clone()) {
            Ok(t) => {
                self.mic_test = Some(t);
                self.mic_test_error = None;
            }
            Err(e) => {
                log::warn!("проверка микрофона: {e}");
                self.mic_test_error = Some(e);
                self.mic_playback = false;
            }
        }
        self.mic_test_key = Some(key);
    }

    fn stop_mic_test(&mut self) {
        self.mic_test = None;
        self.mic_test_key = None;
        self.mic_playback = false;
        self.mic_level.store(0, Ordering::Relaxed);
    }

    fn open_settings(&mut self) {
        self.settings_open = true;
        self.invite = None;
        self.voice.send(Cmd::RefreshDevices);
    }

    fn ui_settings(&mut self, ui: &mut Ui, vs: &VoiceState) {
        self.sync_mic_test(vs);
        ui.ctx().request_repaint_after(Duration::from_millis(50)); // живой индикатор уровня
        let mut close = false;
        let modal = egui::Modal::new(Id::new("settings"))
            .backdrop_color(Color32::from_rgba_unmultiplied(10, 8, 6, 185))
            .frame(
                Frame::new()
                    .fill(t::BAKELITE)
                    .stroke(Stroke::new(1.0, t::BRASS))
                    .corner_radius(CornerRadius::same(14))
                    .inner_margin(Margin::symmetric(32, 28))
                    .shadow(ui.visuals().window_shadow),
            )
            .show(ui.ctx(), |ui| {
                ui.set_width(556.0);
                let w = 556.0;
                egui::ScrollArea::vertical().max_height(ui.ctx().content_rect().height() - 140.0).show(ui, |ui| {
                    ui.label(RichText::new("Голос и звук").font(t::xbold(20.0)));
                    ui.add_space(14.0);
                    if let Some(err) = &vs.audio_error {
                        ui.label(RichText::new(format!("{}  {err}", ph::WARNING_CIRCLE)).size(13.0).color(t::ERR_TEXT));
                    }

                    label(ui, "Микрофон");
                    let mut input = self.settings.input_device.clone();
                    if device_combo(ui, "in", &mut input, &vs.inputs, w) {
                        self.settings.input_device = input.clone();
                        self.settings.save();
                        self.voice.send(Cmd::SetInputDevice(input));
                    }
                    let level = if self.mic_test.is_some() {
                        Some(f32::from_bits(self.mic_level.load(Ordering::Relaxed)))
                    } else {
                        vs.peers.iter().find(|p| p.is_local).map(|p| p.level)
                    };
                    level_meter(ui, level, w);
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let (text, active) = if self.mic_playback {
                            (format!("{}  Остановить проверку", ph::STOP), true)
                        } else {
                            (format!("{}  Слышать себя", ph::HEADPHONES), false)
                        };
                        let btn = egui::Button::new(RichText::new(text).color(if active { t::ON_NIXIE } else { t::PAPER }))
                            .fill(if active { t::NIXIE } else { t::BAKELITE_2 });
                        if ui.add(btn).clicked() {
                            self.mic_playback = !self.mic_playback;
                        }
                        let hint = match &self.mic_test_error {
                            Some(e) => RichText::new(format!("Проверка недоступна: {e}")).color(t::ERR_TEXT),
                            None if self.mic_playback => RichText::new("Так вас слышат друзья. Наденьте наушники — на колонках будет заводка.").color(t::DUST),
                            None => RichText::new("Голос с шумоподавлением, как в звонке. Лучше в наушниках.").color(t::DUST),
                        };
                        ui.label(hint.size(12.5));
                    });
                    ui.add_space(12.0);

                    label(ui, "Вывод звука");
                    let mut output = self.settings.output_device.clone();
                    if device_combo(ui, "out", &mut output, &vs.outputs, w) {
                        self.settings.output_device = output.clone();
                        self.settings.save();
                        self.voice.send(Cmd::SetOutputDevice(output));
                    }
                    ui.add_space(10.0);

                    // рация
                    toggle_row(ui, w, |ui| {
                        ui.label("Режим рации — микрофон включён, пока держите клавишу");
                    }, |ui| {
                        let mut on = self.settings.ptt_enabled;
                        if t::switch(ui, &mut on).changed() {
                            self.settings.ptt_enabled = on;
                            self.settings.save();
                            self.ptt.set_enabled(on);
                            self.voice.send(Cmd::SetPttEnabled(on));
                        }
                        ui.add_space(10.0);
                        if self.ptt.capturing() {
                            if ui.button("Отмена").clicked() {
                                self.ptt.cancel_capture();
                            }
                            ui.label(RichText::new("нажмите клавишу…").color(t::NIXIE).size(13.0));
                        } else {
                            let r = t::keycap(ui, &ptt::key_name(self.settings.ptt_key)).interact(Sense::click());
                            if r.on_hover_text("Нажмите, чтобы назначить другую клавишу (можно кнопку мыши)").clicked() {
                                self.ptt.begin_capture();
                            }
                        }
                    });
                    let mut proc_changed = false;
                    for (text, val) in [
                        ("Эхоподавление", &mut self.settings.echo_cancellation),
                        ("Шумоподавление", &mut self.settings.noise_suppression),
                        ("Автоматическая громкость микрофона", &mut self.settings.auto_gain),
                    ] {
                        toggle_row(ui, w, |ui| {
                            ui.label(text);
                        }, |ui| {
                            if t::switch(ui, val).changed() {
                                proc_changed = true;
                            }
                        });
                    }
                    if proc_changed {
                        self.settings.save();
                        self.voice.send(Cmd::SetProcessing {
                            aec: self.settings.echo_cancellation,
                            ns: self.settings.noise_suppression,
                            agc: self.settings.auto_gain,
                        });
                    }
                    let mut sounds = self.settings.sounds;
                    toggle_row(ui, w, |ui| {
                        ui.label("Звуки входа и выхода");
                    }, |ui| {
                        if t::switch(ui, &mut sounds).changed() {
                            self.settings.sounds = sounds;
                            self.settings.save();
                            self.voice.send(Cmd::SetSounds(sounds));
                        }
                    });

                    // аккаунт
                    ui.add_space(18.0);
                    ui.label(RichText::new("Аккаунт").font(t::bold(15.0)));
                    ui.add_space(4.0);
                    let me = self.me.clone();
                    ui.horizontal(|ui| {
                        if let Some(u) = &me {
                            ui.label(RichText::new(format!("{}{}", u.name, if u.is_admin { " · администратор" } else { "" })).color(t::PAPER));
                        }
                        ui.label(RichText::new(format!("· {}", self.settings.server_url)).size(13.0).color(t::DUST));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button(format!("{} Выйти", ph::SIGN_OUT)).clicked() {
                                close = true;
                                self.logout();
                            }
                        });
                    });
                    if me.as_ref().is_some_and(|u| u.is_admin) {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let busy = self.invite_busy;
                            if ui.add_enabled(!busy, egui::Button::new(format!("{} Создать приглашение", ph::TICKET))).clicked() {
                                self.invite_busy = true;
                                let api = self.api.clone();
                                self.spawn(async move { api.create_invite().await }, Msg::Invite);
                            }
                            match &self.invite {
                                Some(Ok(inv)) => {
                                    let code = inv.code.clone();
                                    let (rect, _) = ui.allocate_exact_size(vec2(230.0, 34.0), Sense::hover());
                                    t::nixie_text(ui.painter(), rect.left_center() + vec2(6.0, 0.0), Align2::LEFT_CENTER, &code, 24.0);
                                    if ui.button(ph::COPY).on_hover_text("Скопировать").clicked() {
                                        ui.ctx().copy_text(code);
                                    }
                                }
                                Some(Err(e)) => {
                                    ui.label(RichText::new(e).size(13.0).color(t::ERR_TEXT));
                                }
                                None => {
                                    ui.label(RichText::new("1 вход, 7 дней").size(13.0).color(t::DUST));
                                }
                            }
                        });
                    }
                });
                ui.add_space(20.0);
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    if t::primary_button(ui, "Готово", 120.0, true).clicked() {
                        close = true;
                    }
                });
            });
        if close || modal.should_close() {
            self.ptt.cancel_capture();
            self.settings_open = false;
            self.stop_mic_test();
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.handle_msgs();
        match self.screen {
            Screen::Boot => self.ui_boot(ui),
            Screen::Login => self.ui_login(ui),
            Screen::Main => self.ui_main(ui),
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        t::GLASS.to_normalized_gamma_f32()
    }
}

/// Отладка вёрстки без сервера голоса: AMADEUS_DEMO=1 (только debug-сборка).
#[cfg(debug_assertions)]
fn demo_state(time: f64) -> VoiceState {
    let peer = |id: &str, local: bool, speaking: bool, muted: bool| Peer {
        identity: id.into(),
        name: id.into(),
        is_local: local,
        speaking,
        level: if speaking { (0.15 + 0.15 * (time * 7.0).sin().abs()) as f32 } else { 0.0 },
        mic_muted: muted,
        streaming: id == "kurisu",
    };
    VoiceState {
        conn: if std::env::var_os("AMADEUS_DEMO_RECONNECT").is_some() { Conn::Reconnecting } else { Conn::Connected },
        channel: Some(Channel { id: 1, name: "General".into(), kind: ChannelKind::Voice }),
        peers: vec![
            peer("tester", true, false, false),
            peer("mayuri", false, true, false),
            peer("kurisu", false, false, false),
            peer("daru", false, false, true),
        ],
        rtt: Some(0.037),
        transmitting: true,
        ..Default::default()
    }
}

// ------------------------------------------------------------ мелкие виджеты

fn label(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(13.0).color(t::DUST));
}

fn field(text: &mut String, w: f32) -> TextEdit<'_> {
    TextEdit::singleline(text).desired_width(w).margin(Margin::symmetric(13, 10)).font(t::font(15.0))
}

fn group_label(ui: &mut Ui, text: &str) {
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        ui.add_space(18.0);
        ui.label(RichText::new(text).font(t::semibold(12.5)).color(t::DUST));
    });
    ui.add_space(4.0);
}

fn channel_row(ui: &mut Ui, icon: &str, name: &str, active: bool, count: Option<usize>, enabled: bool) -> egui::Response {
    let w = ui.available_width();
    let (outer, resp) = ui.allocate_exact_size(vec2(w, 36.0), if enabled { Sense::click() } else { Sense::hover() });
    let rect = Rect::from_min_max(outer.min + vec2(10.0, 0.0), outer.max - vec2(10.0, 0.0));
    let p = ui.painter();
    if active {
        p.rect_filled(rect, CornerRadius::same(7), t::BAKELITE_2);
    } else if resp.hovered() && enabled {
        p.rect_filled(rect, CornerRadius::same(7), Color32::from_rgb(0x26, 0x1F, 0x19));
    }
    let color = if active || (resp.hovered() && enabled) { t::PAPER } else if enabled { t::DUST } else { t::DUST.gamma_multiply(0.6) };
    p.text(rect.left_center() + vec2(10.0, 0.0), Align2::LEFT_CENTER, icon, t::font(20.0), color);
    p.text(rect.left_center() + vec2(40.0, 0.0), Align2::LEFT_CENTER, name, t::semibold(15.0), color);
    if let Some(n) = count {
        p.text(rect.right_center() - vec2(10.0, 0.0), Align2::RIGHT_CENTER, n.to_string(), t::font(12.0), t::DUST);
    }
    if enabled { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp }
}

fn member_row(ui: &mut Ui, p: &Peer) {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover());
    let glow = speak_glow(ui, p);
    let painter = ui.painter();
    let c = pos2(rect.left() + 48.0 + 11.0, rect.center().y);
    t::avatar(painter, c, 11.0, &p.name, glow * 0.7, false);
    let color = if glow > 0.05 { t::PAPER } else { t::DUST };
    painter.text(pos2(c.x + 19.0, c.y), Align2::LEFT_CENTER, &p.name, t::font(13.5), color);
    let mut x = rect.right() - 22.0;
    if p.mic_muted {
        painter.text(pos2(x, c.y), Align2::RIGHT_CENTER, egui_phosphor::fill::MICROPHONE_SLASH, t::icon_fill(14.0), t::SIGNAL);
        x -= 20.0;
    }
    if p.streaming {
        painter.text(pos2(x, c.y), Align2::RIGHT_CENTER, egui_phosphor::fill::MONITOR_ARROW_UP, t::icon_fill(14.0), t::NIXIE);
    }
}

/// Плавное свечение говорящего (0..1) — «пульсирует в такт голосу».
fn speak_glow(ui: &Ui, p: &Peer) -> f32 {
    let target = if p.speaking && !p.mic_muted { (0.55 + p.level.clamp(0.0, 1.0) * 2.0).min(1.0) } else { 0.0 };
    ui.ctx().animate_value_with_time(Id::new(("glow", &p.identity)), target, 0.12)
}

fn tile(ui: &Ui, rect: Rect, p: &Peer, glow: f32) {
    let painter = ui.painter();
    let talk = glow > 0.05;
    painter.rect_filled(rect, CornerRadius::same(12), t::BAKELITE);
    if talk {
        let c = rect.center() - vec2(0.0, rect.height() * 0.05);
        t::radial_glow(painter, c, rect.width().min(rect.height()) * 0.7, t::nixie_a((32.0 * glow) as u8));
    }
    let stroke = if talk { Stroke::new(1.0, t::nixie_a((140.0 * glow) as u8 + 20)) } else { Stroke::new(1.0, t::BRASS_DIM) };
    painter.rect_stroke(rect, CornerRadius::same(12), stroke, StrokeKind::Inside);
    let r = (rect.height().min(rect.width()) * 0.22).clamp(24.0, 46.0);
    let ac = rect.center() - vec2(0.0, r * 0.35);
    t::avatar(painter, ac, r, &p.name, glow, p.mic_muted && !p.is_local);
    let name_pos = pos2(rect.center().x, ac.y + r + 18.0);
    let galley = painter.layout_no_wrap(p.name.clone(), t::bold(15.0), t::PAPER);
    let icon_w = if p.mic_muted { 24.0 } else { 0.0 };
    let start = name_pos.x - (galley.size().x + icon_w) / 2.0;
    painter.galley(pos2(start, name_pos.y - galley.size().y / 2.0), galley.clone(), t::PAPER);
    if p.mic_muted {
        painter.text(
            pos2(start + galley.size().x + 8.0, name_pos.y),
            Align2::LEFT_CENTER,
            egui_phosphor::fill::MICROPHONE_SLASH,
            t::icon_fill(16.0),
            t::SIGNAL,
        );
    }
    if p.streaming {
        live_badge(painter, pos2(rect.right() - 12.0, rect.top() + 12.0));
    }
    if p.is_local {
        let tag = painter.layout_no_wrap("это вы".into(), t::font(12.0), t::DUST);
        let tr = Rect::from_min_size(rect.min + vec2(12.0, 12.0), tag.size() + vec2(16.0, 6.0));
        painter.rect_filled(tr, CornerRadius::same(6), t::GLASS);
        painter.galley(tr.min + vec2(8.0, 3.0), tag, t::DUST);
    }
}

/// Плашка «В ЭФИРЕ» (правый верхний угол = `right_top`).
fn live_badge(painter: &egui::Painter, right_top: Pos2) {
    let galley = painter.layout_no_wrap("В ЭФИРЕ".into(), t::bold(11.0), t::ON_NIXIE);
    let size = galley.size() + vec2(24.0, 8.0);
    let r = Rect::from_min_size(pos2(right_top.x - size.x, right_top.y), size);
    painter.rect_filled(r.expand(3.0), CornerRadius::same(9), t::nixie_a(30));
    painter.rect_filled(r, CornerRadius::same(6), t::NIXIE);
    painter.circle_filled(pos2(r.left() + 9.0, r.center().y), 3.0, t::ON_NIXIE);
    painter.galley(pos2(r.left() + 16.0, r.center().y - galley.size().y / 2.0), galley, t::ON_NIXIE);
}

/// Статус своей трансляции в шапке: источник, качество, факт к/с и битрейт, энкодер.
fn stream_pill(ui: &mut Ui, s: &voice::StreamInfo) {
    let title: String = if s.title.chars().count() > 28 { s.title.chars().take(27).chain(['…']).collect() } else { s.title.clone() };
    let resp = Frame::new()
        .fill(t::nixie_a(18))
        .stroke(Stroke::new(1.0, t::nixie_a(110)))
        .corner_radius(CornerRadius::same(255))
        .inner_margin(Margin::symmetric(14, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                // right_to_left родитель — порядок обратный
                if s.minimized {
                    ui.label(RichText::new("окно свёрнуто").size(12.5).color(t::ERR_TEXT));
                } else if s.fps > 0.0 {
                    ui.label(RichText::new(format!("{:.0} к/с · {:.1} Мбит/с", s.fps, s.kbps / 1000.0)).size(12.5).color(t::DUST));
                }
                // Чем кодируется — видно сразу, без наведения.
                let (tag, color) = if s.backend.contains("видеокарта") { ("видеокарта", t::NIXIE) } else { ("процессор", t::DUST) };
                ui.label(RichText::new(tag).size(12.0).color(color));
                ui.label(RichText::new(&title).font(t::semibold(13.0)).color(t::FILAMENT));
                ui.label(RichText::new(egui_phosphor::fill::MONITOR_ARROW_UP).font(t::icon_fill(15.0)).color(t::NIXIE));
            });
        })
        .response;
    let size = s.size.map(|(w, h)| format!("{w}×{h}")).unwrap_or_else(|| "—".into());
    let enc = s.backend.clone();
    let lim = s.limitation.as_deref().map(|l| format!("
Ограничение качества: {l}")).unwrap_or_default();
    resp.on_hover_text(format!("В эфире: {}
Качество: {} · отправляется {size}
Энкодер: {enc}{lim}", s.title, s.preset.label()));
}

fn ping_box(ui: &mut Ui, rtt: f64) {
    Frame::new()
        .fill(Color32::from_rgb(0x0F, 0x0C, 0x0A))
        .stroke(Stroke::new(1.0, t::BRASS))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin { left: 12, right: 12, top: 2, bottom: 2 })
        .show(ui, |ui| {
            // Родитель — right_to_left (прижато к правому краю), поэтому порядок обратный: подпись, затем число.
            ui.horizontal(|ui| {
                let text = format!("{:.3}", rtt.min(9.999));
                let num_w = ui.painter().layout_no_wrap(text.clone(), t::nixie(28.0), t::NIXIE).size().x;
                ui.label(RichText::new("пинг, с").size(12.0).color(t::DUST));
                let (rect, _) = ui.allocate_exact_size(vec2(num_w + 4.0, 34.0), Sense::hover());
                t::nixie_text(ui.painter(), rect.left_center(), Align2::LEFT_CENTER, &text, 28.0);
            });
        })
        .response
        .on_hover_text(format!("Время до сервера и обратно: {:.0} мс", rtt * 1000.0));
}

/// Выпадающий список устройств. Возвращает true, если выбор изменился.
fn device_combo(ui: &mut Ui, salt: &str, selected: &mut Option<String>, devices: &[voice::Device], w: f32) -> bool {
    let current = match selected {
        None => "По умолчанию (как в Windows)".to_string(),
        Some(id) => devices.iter().find(|d| &d.id == id).map(|d| d.name.clone()).unwrap_or_else(|| "Устройство отключено".into()),
    };
    let before = selected.clone();
    egui::ComboBox::from_id_salt(salt).selected_text(current).width(w).height(320.0).show_ui(ui, |ui| {
        ui.selectable_value(selected, None, "По умолчанию (как в Windows)");
        for d in devices {
            ui.selectable_value(selected, Some(d.id.clone()), &d.name);
        }
    });
    *selected != before
}

fn level_meter(ui: &mut Ui, level: Option<f32>, w: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(w, 14.0), Sense::hover());
    let n = 20;
    let gap = 3.0;
    let seg = (w - gap * (n - 1) as f32) / n as f32;
    let lit = level.map(|l| ((l.clamp(0.0, 1.0)).sqrt() * n as f32).round() as usize).unwrap_or(0);
    let p = ui.painter();
    for i in 0..n {
        let r = Rect::from_min_size(rect.min + vec2(i as f32 * (seg + gap), 0.0), vec2(seg, 14.0));
        if i < lit {
            p.rect_filled(r.expand(1.5), CornerRadius::same(3), t::nixie_a(50));
            p.rect_filled(r, CornerRadius::same(2), t::NIXIE);
        } else {
            p.rect_filled(r, CornerRadius::same(2), Color32::from_rgb(0x2D, 0x25, 0x1E));
        }
    }
}

fn toggle_row(ui: &mut Ui, w: f32, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui)) {
    let top = ui.cursor().top();
    ui.painter().hline(ui.max_rect().left()..=ui.max_rect().left() + w, top, Stroke::new(1.0, t::BRASS_DIM));
    ui.add_space(4.0);
    ui.allocate_ui_with_layout(vec2(w, 36.0), Layout::left_to_right(Align::Center), |ui| {
        left(ui);
        ui.with_layout(Layout::right_to_left(Align::Center), right);
    });
    ui.add_space(4.0);
}
