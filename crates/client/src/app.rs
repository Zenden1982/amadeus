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
use crate::chat::Chat;
use crate::config::Settings;
use crate::desktop::{self, Window};
use crate::tray::Tray;
use crate::update::{self, Updater};
use crate::mictest::{self, MicTest};
use crate::picker::{self, Picker};
use crate::ptt::{self, Ptt};
use crate::theme::{self as t, DockStyle};
use crate::video::VideoHub;
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

/// Что в центре окна: голосовой канал (сцена) или текстовый канал (чат). Голос при этом не прерывается.
#[derive(PartialEq, Clone, Copy, Debug)]
enum View {
    Voice,
    Text(i64),
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
    /// Кадры просматриваемых трансляций.
    video: VideoHub,
    /// Трансляция в «большом виде» (identity); `None` — сетка.
    focus: Option<String>,
    /// Полноэкранный просмотр трансляции `focus`.
    fullscreen: bool,
    /// Что последним сообщили движку о видимости/размере трансляций.
    views_sent: HashMap<String, (bool, u32, u32)>,
    /// Последнее движение мыши (в полном экране панель и курсор прячутся).
    pointer_moved_at: Instant,
    /// Какие трансляции нарисованы в текущем кадре и какого размера (физ. пиксели).
    views_frame: HashMap<String, (u32, u32)>,
    /// «Смотреть» нажато, движок ещё не подтвердил (фокус не сбрасывать).
    watch_pending: Option<(String, Instant)>,
    /// Текстовый чат.
    chat: Chat,
    view: View,
    /// Окно (HWND) — прятать в трей и показывать.
    window: Window,
    /// Значок в трее; `None` — не создался, тогда «закрыть» закрывает.
    tray: Option<Tray>,
    updater: Updater,
    /// Окно «Доступно обновление».
    update_open: bool,
    /// Выходим по-настоящему (меню трея, перезапуск после обновления): «закрыть» не прятать в трей.
    quitting: bool,
    /// Не удалось прописать автозапуск.
    autostart_error: Option<String>,
    /// Запуск из автозагрузки: до этого момента держать окно спрятанным (eframe показывает его после первого кадра).
    hide_until: Option<Instant>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, rt: tokio::runtime::Runtime) -> Self {
        t::install(&cc.egui_ctx);
        crate::frame::style_native_window(cc);
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
            stream_volumes: settings.stream_volume.clone(),
        };
        let video = VideoHub::new(&ctx);
        let chat = Chat::new(rt.handle().clone(), ctx.clone(), api.clone());
        let updater = Updater::new(rt.handle().clone(), ctx.clone(), api.clone());
        let window = Window::from_cc(cc);
        desktop::mark_window(window);
        let tray = Tray::new(window, ctx.clone());
        if tray.is_none() {
            desktop::show(window); // без трея окно не прячем (даже при запуске из автозагрузки)
        }
        let autostart_error = if desktop::is_installed() { desktop::set_autostart(settings.autostart).err() } else { None };
        let voice = voice::spawn(rt.handle(), api.clone(), prefs, video.clone(), repaint.clone());
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
            video,
            focus: None,
            fullscreen: false,
            views_sent: HashMap::new(),
            pointer_moved_at: Instant::now(),
            views_frame: HashMap::new(),
            watch_pending: None,
            chat,
            view: View::Voice,
            window,
            tray,
            updater,
            update_open: false,
            quitting: false,
            autostart_error,
            hide_until: None,
        };
        if app.tray.is_some() && std::env::args().any(|a| a == desktop::ARG_TRAY) {
            // eframe показывает окно после первого кадра; свёрнутое — покажется только в панели задач,
            // а через пару кадров уйдёт в трей (на экране ничего не мелькнёт)
            desktop::minimize_inactive(window);
            app.hide_until = Some(Instant::now() + Duration::from_secs(3));
        }
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
        self.chat.set_api(api.clone());
        self.updater.set_api(api.clone());
        self.view = View::Voice;
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
                    // Отладка: AMADEUS_OPEN_TEXT=<id канала> — сразу открыть текстовый канал.
                    #[cfg(debug_assertions)]
                    if self.channels.is_empty()
                        && let Some(id) = std::env::var("AMADEUS_OPEN_TEXT").ok().and_then(|v| v.parse::<i64>().ok())
                    {
                        self.view = View::Text(id);
                    }
                    if let View::Text(id) = self.view
                        && !list.iter().any(|c| c.id == id && c.kind == ChannelKind::Text)
                    {
                        self.view = View::Voice;
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
            let watching = std::mem::take(&mut vs.watching);
            vs = demo_state(ui.input(|i| i.time));
            vs.watching = watching;
            self.demo_frames(ui, &vs);
        }
        #[cfg(debug_assertions)]
        self.debug_autostream(&vs);
        #[cfg(debug_assertions)]
        self.debug_autowatch(ui, &vs);
        self.chat.ensure_stream();
        self.sync_focus(ui, &vs);
        if self.fullscreen {
            self.ui_fullscreen(ui, &vs);
            self.report_views(&vs);
            return;
        }
        self.ui_sidebar(ui, &vs);
        self.ui_header(ui, &vs);
        self.ui_dock(ui, &vs);
        let text_channel = match self.view {
            View::Text(id) => self.channels.iter().find(|c| c.id == id).cloned(),
            View::Voice => None,
        };
        match text_channel {
            Some(ch) => {
                let modal = self.settings_open || self.picker.is_some() || self.popup.is_some() || self.update_open;
                egui::CentralPanel::no_frame().frame(Frame::new().fill(t::GLASS)).show(ui, |ui| {
                    self.chat.ui(ui, &ch, modal);
                });
            }
            None => {
                egui::CentralPanel::no_frame().frame(Frame::new().fill(t::GLASS).inner_margin(Margin::same(28))).show(ui, |ui| {
                    self.ui_stage(ui, &vs);
                });
            }
        }
        self.ui_banner(ui, &vs);
        self.ui_volume_popup(ui, &vs);
        if self.settings_open {
            self.ui_settings(ui, &vs);
        }
        if self.picker.is_some() {
            self.ui_picker(ui, &vs);
        }
        self.report_views(&vs);
    }

    /// Отладка: AMADEUS_AUTOWATCH=1 — сразу смотреть каждую трансляцию в канале
    /// (+ AMADEUS_AUTOFULLSCREEN=1 — развернуть на весь экран, когда придёт первый кадр).
    #[cfg(debug_assertions)]
    fn debug_autowatch(&mut self, ui: &Ui, vs: &VoiceState) {
        use std::sync::Mutex;
        static ASKED: Mutex<Vec<String>> = Mutex::new(Vec::new());
        static FULL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if std::env::var_os("AMADEUS_AUTOWATCH").is_none() || vs.conn != Conn::Connected {
            return;
        }
        let mut asked = ASKED.lock().unwrap();
        for p in vs.peers.iter().filter(|p| p.streaming && !p.is_local) {
            if !asked.contains(&p.identity) {
                asked.push(p.identity.clone());
                self.watch(&p.identity);
            }
        }
        if std::env::var_os("AMADEUS_AUTOFULLSCREEN").is_some()
            && let Some(f) = self.focus.clone()
            && self.video.frame(&f).is_some()
            && !FULL.swap(true, Ordering::Relaxed)
        {
            log::info!("AMADEUS_AUTOFULLSCREEN: полный экран");
            self.set_fullscreen(ui.ctx(), true);
        }
    }

    /// Демо-кадры для просматриваемых «трансляций» (AMADEUS_DEMO), ~10 к/с.
    #[cfg(debug_assertions)]
    fn demo_frames(&mut self, ui: &Ui, vs: &VoiceState) {
        use std::sync::Mutex;
        static AT: Mutex<Option<Instant>> = Mutex::new(None);
        let mut at = AT.lock().unwrap();
        if at.is_some_and(|t| t.elapsed() < Duration::from_millis(100)) {
            return;
        }
        *at = Some(Instant::now());
        let time = ui.input(|i| i.time) as f32;
        for id in &vs.watching {
            self.video.put_demo(id, time);
        }
        ui.ctx().request_repaint_after(Duration::from_millis(100));
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
        // AMADEUS_STREAM_AUDIO=0 — без звука.
        let audio = std::env::var("AMADEUS_STREAM_AUDIO").map(|v| v != "0").unwrap_or(self.settings.stream_audio);
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
                    let _ = tx.send(Cmd::StartStream { source: s.id, title: s.title, width: s.width, height: s.height, preset, audio });
                }
                None => log::warn!("AMADEUS_AUTOSTREAM: источник «{want}» не найден"),
            }
            // AMADEUS_AUTOSTREAM_THEN=<часть заголовка> — через 15 с сменить источник на лету.
            if let Ok(next) = std::env::var("AMADEUS_AUTOSTREAM_THEN") {
                std::thread::sleep(Duration::from_secs(15));
                let list = capture_win::list_sources(64, 36);
                if let Some(s) = list.into_iter().find(|s| s.title.to_lowercase().contains(&next.to_lowercase())) {
                    let _ = tx.send(Cmd::StartStream { source: s.id, title: s.title, width: s.width, height: s.height, preset, audio });
                }
            }
        });
    }

    fn open_picker(&mut self, vs: &VoiceState) {
        let current = vs.stream.as_ref().map(|s| s.source);
        let preset = vs.stream.as_ref().map(|s| s.preset).unwrap_or(self.settings.stream_preset);
        let audio = vs.stream.as_ref().map(|s| s.audio != voice::AudioStatus::Off).unwrap_or(self.settings.stream_audio);
        self.picker = Some(Picker::open(&self.ctx, preset, audio, current));
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
            picker::Action::Start { source, title, width, height, preset, audio } => {
                self.settings.stream_preset = preset;
                self.settings.stream_audio = audio;
                self.settings.save();
                self.voice.send(Cmd::StartStream { source, title, width, height, preset, audio });
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
            if channel_row(ui, ph::SPEAKER_HIGH, &ch.name, active, count, true).clicked() {
                // уже в этом канале — просто вернуться к сцене (из чата)
                if !(active && vs.conn == Conn::Connected) {
                    self.voice.send(Cmd::Join(ch.clone()));
                }
                self.view = View::Voice;
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
                let open = self.view == View::Text(ch.id);
                if channel_row(ui, ph::HASH, &ch.name, open, None, true).clicked() {
                    self.view = View::Text(ch.id);
                }
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
                let text_ch = match self.view {
                    View::Text(id) => self.channels.iter().find(|c| c.id == id).map(|c| c.name.clone()),
                    View::Voice => None,
                };
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    if let Some(name) = text_ch {
                        ui.label(RichText::new(ph::HASH).size(20.0).color(t::DUST));
                        ui.label(RichText::new(name).font(t::xbold(18.0)));
                    } else {
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
            _ => self.ui_room(ui, vs),
        }
    }

    /// Комната: сетка (участники + трансляции) или «большой вид» трансляции с лентой остальных.
    fn ui_room(&mut self, ui: &mut Ui, vs: &VoiceState) {
        let r = ui.max_rect();
        // Трансляции — первыми (как в Discord), затем участники.
        let mut items: Vec<Item> = vs.peers.iter().filter(|p| p.streaming).map(Item::Stream).collect();
        items.extend(vs.peers.iter().map(Item::Person));
        let focused = self.focus.as_ref().and_then(|f| items.iter().position(|i| matches!(i, Item::Stream(p) if &p.identity == f)));
        let Some(fi) = focused else {
            let aspect = if items.iter().any(|i| matches!(i, Item::Stream(_))) { 9.0 / 16.0 } else { 0.8 };
            for (item, rect) in items.iter().zip(grid_rects(r, items.len(), aspect, 16.0)) {
                self.ui_item(ui, rect, *item, vs, false);
            }
            return;
        };
        let big_item = items.remove(fi);
        let gap = 14.0;
        let strip_h = if items.is_empty() { 0.0 } else { (r.height() * 0.2).clamp(120.0, 160.0) };
        let big = Rect::from_min_max(r.min, pos2(r.right(), r.bottom() - if strip_h > 0.0 { strip_h + gap } else { 0.0 }));
        self.ui_item(ui, big, big_item, vs, true);
        if strip_h > 0.0 {
            // 16:9, а если не помещаются все — уже (но не меньше 130 px), остальные за краем.
            let n = items.len() as f32;
            let tw = (strip_h * 16.0 / 9.0).min((r.width() - gap * (n - 1.0)) / n).max(130.0);
            let fit = (((r.width() + gap) / (tw + gap)).floor() as usize).max(1).min(items.len());
            let row_w = tw * fit as f32 + gap * (fit - 1) as f32;
            let left = r.center().x - row_w / 2.0;
            for (i, item) in items.iter().take(fit).enumerate() {
                let rect = Rect::from_min_size(pos2(left + i as f32 * (tw + gap), r.bottom() - strip_h), vec2(tw, strip_h));
                self.ui_item(ui, rect, *item, vs, false);
            }
        }
    }

    fn ui_item(&mut self, ui: &mut Ui, rect: Rect, item: Item, vs: &VoiceState, big: bool) {
        match item {
            Item::Person(p) => {
                let resp = ui.interact(rect, Id::new(("tile", &p.identity)), Sense::click());
                let glow = speak_glow(ui, p);
                tile(ui, rect, p, glow);
                if resp.clicked() && !p.is_local {
                    self.open_popup(p, resp.interact_pointer_pos().unwrap_or(rect.center()));
                }
                if !p.is_local {
                    resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                }
            }
            Item::Stream(p) => self.stream_tile(ui, rect, p, vs, big),
        }
    }

    fn open_popup(&mut self, p: &Peer, pos: Pos2) {
        self.popup = Some(VolumePopup { identity: p.identity.clone(), name: p.name.clone(), pos, opened_at: Instant::now() });
    }

    /// Плитка трансляции: «Смотреть» → видео (вписано с полями) → по клику большой вид, двойной клик — полный экран.
    fn stream_tile(&mut self, ui: &mut Ui, rect: Rect, p: &Peer, vs: &VoiceState, big: bool) {
        let watching = vs.watching.contains(&p.identity);
        let resp = ui.interact(rect, Id::new(("stream", &p.identity)), Sense::click());
        let hovered = ui.rect_contains_pointer(rect);
        let painter = ui.painter().clone();
        let radius = CornerRadius::same(12);
        painter.rect_filled(rect, radius, VIDEO_BG);
        let small = rect.height() < 200.0;

        if p.is_local {
            // Своё видео не декодируем (лишняя нагрузка) — только подпись.
            let title = vs.stream.as_ref().map(|s| s.title.clone()).unwrap_or_default();
            let c = rect.center();
            painter.text(c - vec2(0.0, if small { 14.0 } else { 26.0 }), Align2::CENTER_CENTER, ph::MONITOR_ARROW_UP, t::font(if small { 26.0 } else { 40.0 }), t::NIXIE);
            painter.text(c + vec2(0.0, if small { 12.0 } else { 14.0 }), Align2::CENTER_CENTER, "Вы транслируете", t::bold(if small { 13.0 } else { 15.0 }), t::PAPER);
            if !small {
                painter.text(c + vec2(0.0, 36.0), Align2::CENTER_CENTER, ellipsize(&title, 40), t::font(13.0), t::DUST);
            }
        } else if watching {
            match self.video.frame(&p.identity) {
                Some(f) => {
                    let vr = fit_rect(rect, f.size);
                    egui::Image::new(egui::load::SizedTexture::new(f.texture, vr.size())).corner_radius(radius).paint_at(ui, vr);
                    self.mark_view(&p.identity, vr, ui.ctx().pixels_per_point());
                    if f.stale {
                        chip(&painter, rect.center(), Align2::CENTER_CENTER, &format!("{}  Нет новых кадров — стример свернул окно или пропала связь", ph::HOURGLASS), t::DUST);
                    }
                    if big && hovered && f.fps > 0.0 {
                        chip(&painter, rect.left_top() + vec2(12.0, 12.0), Align2::LEFT_TOP, &format!("{}×{} · {:.0} к/с", f.size[0], f.size[1], f.fps), t::DUST);
                    }
                }
                None => {
                    self.mark_view(&p.identity, rect, ui.ctx().pixels_per_point());
                    egui::Spinner::new().size(30.0).color(t::NIXIE).paint_at(ui, Rect::from_center_size(rect.center() - vec2(0.0, 12.0), vec2(30.0, 30.0)));
                    painter.text(rect.center() + vec2(0.0, 22.0), Align2::CENTER_CENTER, "Подключаемся к трансляции…", t::font(13.0), t::DUST);
                    ui.ctx().request_repaint_after(Duration::from_millis(50));
                }
            }
        } else {
            // Не смотрим: аватар стримера и «Смотреть» (канал не тратится, пока не нажали).
            t::radial_glow(&painter, rect.center(), rect.width().min(rect.height()) * 0.6, t::nixie_a(if hovered { 22 } else { 12 }));
            // Совсем маленькая плитка (лента, много трансляций) — только кнопка, без аватара.
            let tiny = rect.height() < 140.0;
            let ar = (rect.height() * 0.16).clamp(16.0, 34.0);
            let ac = rect.center() - vec2(0.0, ar + if small { 4.0 } else { 14.0 });
            if !tiny {
                t::avatar(&painter, ac, ar, &p.name, 0.0, false);
            }
            if !small {
                painter.text(ac + vec2(0.0, ar + 18.0), Align2::CENTER_CENTER, format!("{} транслирует экран", p.name), t::semibold(14.0), t::PAPER);
            }
            let bw = if small { 116.0 } else { 150.0 };
            let by = if tiny { rect.center().y - 4.0 } else { ac.y + ar + if small { 22.0 } else { 52.0 } };
            let br = Rect::from_center_size(pos2(rect.center().x, by), vec2(bw, if small { 30.0 } else { 38.0 }));
            painter.rect_filled(br, CornerRadius::same(255), if hovered { t::FILAMENT } else { t::NIXIE });
            painter.text(br.center(), Align2::CENTER_CENTER, format!("{}  Смотреть", ph::PLAY), t::bold(if small { 13.0 } else { 14.5 }), t::ON_NIXIE);
        }

        let stroke = if big || (watching && hovered) {
            Stroke::new(1.0, t::nixie_a(120))
        } else if hovered {
            Stroke::new(1.0, t::BRASS)
        } else {
            Stroke::new(1.0, t::BRASS_DIM)
        };
        painter.rect_stroke(rect, radius, stroke, StrokeKind::Inside);
        live_badge(&painter, pos2(rect.right() - 12.0, rect.top() + 12.0));
        chip(&painter, rect.left_bottom() + vec2(12.0, -12.0), Align2::LEFT_BOTTOM, &format!("{}  {}", ph::MONITOR, p.name), t::PAPER);

        if p.is_local {
            return;
        }
        // Кнопки поверх видео (появляются при наведении).
        let mut act = None;
        if watching && hovered {
            let mut c = pos2(rect.right() - 30.0, rect.bottom() - 30.0);
            if overlay_button(ui, c, ph::X, "Перестать смотреть", Id::new(("s-stop", &p.identity))).clicked() {
                act = Some(StreamAct::Unwatch);
            }
            c.x -= 44.0;
            if overlay_button(ui, c, ph::CORNERS_OUT, "Полный экран (F11)", Id::new(("s-full", &p.identity))).clicked() {
                act = Some(StreamAct::Fullscreen);
            }
            if big {
                c.x -= 44.0;
                if overlay_button(ui, c, ph::SQUARES_FOUR, "Все участники (Esc)", Id::new(("s-grid", &p.identity))).clicked() {
                    act = Some(StreamAct::Grid);
                }
            }
        }
        if act.is_none() {
            if resp.double_clicked() && watching {
                act = Some(StreamAct::Fullscreen);
            } else if resp.clicked() {
                act = Some(if watching { StreamAct::Focus } else { StreamAct::Watch });
            } else if resp.secondary_clicked() {
                self.open_popup(p, resp.interact_pointer_pos().unwrap_or(rect.center()));
            }
        }
        resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        match act {
            Some(StreamAct::Watch) => {
                // Как в Discord: «Смотреть» сразу открывает трансляцию крупно.
                self.watch(&p.identity);
            }
            Some(StreamAct::Focus) => self.focus = Some(p.identity.clone()),
            Some(StreamAct::Fullscreen) => {
                self.focus = Some(p.identity.clone());
                self.set_fullscreen(ui.ctx(), true);
            }
            Some(StreamAct::Grid) => self.focus = None,
            Some(StreamAct::Unwatch) => self.unwatch(ui.ctx(), &p.identity),
            None => {}
        }
    }

    fn watch(&mut self, identity: &str) {
        self.voice.send(Cmd::Watch { identity: identity.to_string(), on: true });
        self.watch_pending = Some((identity.to_string(), Instant::now()));
        self.focus = Some(identity.to_string());
    }

    fn unwatch(&mut self, ctx: &egui::Context, identity: &str) {
        self.voice.send(Cmd::Watch { identity: identity.to_string(), on: false });
        if self.focus.as_deref() == Some(identity) {
            self.focus = None;
            self.set_fullscreen(ctx, false);
        }
    }

    fn set_fullscreen(&mut self, ctx: &egui::Context, on: bool) {
        if self.fullscreen != on {
            self.fullscreen = on;
            self.pointer_moved_at = Instant::now();
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
        }
    }

    /// Трансляция нарисована в `rect` в этом кадре (для отчёта движку о видимости и размере).
    fn mark_view(&mut self, identity: &str, rect: Rect, ppp: f32) {
        self.views_frame.insert(identity.to_string(), ((rect.width() * ppp) as u32, (rect.height() * ppp) as u32));
    }

    /// Сообщает движку, какие трансляции видны и какого они размера; невидимые SFU ставит на паузу.
    fn report_views(&mut self, vs: &VoiceState) {
        let seen = std::mem::take(&mut self.views_frame);
        for id in &vs.watching {
            let (visible, w, h) = match seen.get(id) {
                Some(&(w, h)) => (true, w, h),
                None => (false, 0, 0),
            };
            let changed = |a: u32, b: u32| (a as f32 - b as f32).abs() > 0.2 * a.max(b).max(1) as f32;
            let send = match self.views_sent.get(id) {
                None => true,
                Some(&(v, pw, ph)) => v != visible || (visible && (changed(pw, w) || changed(ph, h))),
            };
            if send {
                self.views_sent.insert(id.clone(), (visible, w, h));
                self.voice.send(Cmd::StreamView { identity: id.clone(), visible, width: w, height: h });
            }
        }
        self.views_sent.retain(|id, _| vs.watching.contains(id));
    }

    /// Фокус/полный экран: сброс, если трансляция закончилась; клавиши F11 и Esc.
    fn sync_focus(&mut self, ui: &Ui, vs: &VoiceState) {
        let ctx = ui.ctx().clone();
        if self.watch_pending.as_ref().is_some_and(|(id, at)| vs.watching.contains(id) || at.elapsed() > Duration::from_secs(2)) {
            self.watch_pending = None;
        }
        let pending = self.watch_pending.as_ref().map(|(id, _)| id.clone());
        let alive = |id: &str| {
            (vs.watching.iter().any(|w| w == id) || pending.as_deref() == Some(id)) && vs.peers.iter().any(|p| p.identity == id && p.streaming)
        };
        if let Some(f) = &self.focus
            && !alive(f)
        {
            self.focus = None;
        }
        if self.fullscreen && self.focus.is_none() {
            self.set_fullscreen(&ctx, false);
        }
        let modal_open =
            self.settings_open || self.picker.is_some() || self.popup.is_some() || self.update_open || self.view != View::Voice;
        let (f11, esc) = ui.input(|i| (i.key_pressed(egui::Key::F11), i.key_pressed(egui::Key::Escape)));
        if f11 && !modal_open {
            if self.fullscreen {
                self.set_fullscreen(&ctx, false);
            } else {
                if self.focus.is_none() {
                    // Смотрим ровно одну трансляцию — F11 сразу разворачивает её.
                    let watched: Vec<&String> = vs.watching.iter().filter(|w| alive(w)).collect();
                    if let [one] = watched.as_slice() {
                        self.focus = Some((*one).clone());
                    }
                }
                if self.focus.is_some() {
                    self.set_fullscreen(&ctx, true);
                }
            }
        }
        if esc && !modal_open {
            if self.fullscreen {
                self.set_fullscreen(&ctx, false);
            } else if self.focus.is_some() {
                self.focus = None;
            }
        }
    }

    /// Полноэкранный просмотр: только видео; панель и курсор прячутся, пока мышь не двигается.
    fn ui_fullscreen(&mut self, ui: &mut Ui, vs: &VoiceState) {
        let Some(id) = self.focus.clone() else { return };
        let Some(p) = vs.peers.iter().find(|p| p.identity == id) else { return };
        if ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO || i.pointer.any_down()) {
            self.pointer_moved_at = Instant::now();
        }
        let idle = self.pointer_moved_at.elapsed() > Duration::from_millis(2500);
        egui::CentralPanel::no_frame().frame(Frame::new().fill(Color32::BLACK)).show(ui, |ui| {
            let r = ui.max_rect();
            let resp = ui.interact(r, Id::new("fs-video"), Sense::click());
            match self.video.frame(&id) {
                Some(f) => {
                    let vr = fit_rect(r, f.size);
                    egui::Image::new(egui::load::SizedTexture::new(f.texture, vr.size())).paint_at(ui, vr);
                    self.mark_view(&id, vr, ui.ctx().pixels_per_point());
                    if !idle {
                        chip(ui.painter(), r.left_top() + vec2(20.0, 20.0), Align2::LEFT_TOP, &format!("{}  {} · {}×{} · {:.0} к/с", ph::MONITOR, p.name, f.size[0], f.size[1], f.fps), t::PAPER);
                    }
                }
                None => {
                    self.mark_view(&id, r, ui.ctx().pixels_per_point());
                    egui::Spinner::new().size(36.0).color(t::NIXIE).paint_at(ui, Rect::from_center_size(r.center(), vec2(36.0, 36.0)));
                }
            }
            if idle {
                ui.ctx().set_cursor_icon(egui::CursorIcon::None);
                // Проверить ещё раз, когда пора прятать/показывать.
                ui.ctx().request_repaint_after(Duration::from_millis(250));
                if resp.double_clicked() {
                    self.set_fullscreen(ui.ctx(), false);
                }
                return;
            }
            ui.ctx().request_repaint_after(Duration::from_millis(250));
            chip(ui.painter(), pos2(r.center().x, r.top() + 20.0), Align2::CENTER_TOP, "Esc или двойной клик — выйти из полного экрана", t::DUST);
            let mut c = pos2(r.right() - 36.0, r.bottom() - 36.0);
            if overlay_button(ui, c, ph::X, "Перестать смотреть", Id::new("fs-stop")).clicked() {
                self.unwatch(ui.ctx(), &id);
                return;
            }
            c.x -= 44.0;
            if overlay_button(ui, c, ph::CORNERS_IN, "Выйти из полного экрана (Esc)", Id::new("fs-exit")).clicked() || resp.double_clicked() {
                self.set_fullscreen(ui.ctx(), false);
            }
        });
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

    fn ui_volume_popup(&mut self, ui: &mut Ui, vs: &VoiceState) {
        let Some(pop) = &self.popup else { return };
        let (identity, name, pos, opened) = (pop.identity.clone(), pop.name.clone(), pop.pos, pop.opened_at);
        let mut vol = self.settings.volume_of(&identity);
        let mut muted = self.settings.is_user_muted(&identity);
        let mut changed = false;
        let streaming = vs.peers.iter().any(|p| p.identity == identity && p.streaming);
        let mut svol = self.settings.stream_volume_of(&identity);
        let mut s_changed = false;
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
                    // Звук трансляции — отдельно от голоса (как в Discord).
                    if streaming {
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{}  Звук трансляции", ph::MONITOR_PLAY)).size(13.0).color(t::DUST));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(format!("{:.0}%", svol * 100.0)).size(13.0));
                            });
                        });
                        if t::slider(ui, &mut svol, 0.0..=2.0, 250.0).changed() {
                            s_changed = true;
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
        if s_changed {
            if (svol - 1.0).abs() < 0.04 {
                svol = 1.0;
            }
            self.settings.stream_volume.insert(identity.clone(), svol);
            self.settings.save();
            self.voice.send(Cmd::SetStreamVolume(identity.clone(), svol));
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

    // ------------------------------------------------------------ трей, выход, обновление

    /// «Закрыть» (лампа, Alt+F4) — спрятать в трей; «Выйти» из трея и готовое обновление — выйти по-настоящему.
    fn handle_close(&mut self, ctx: &egui::Context) {
        if self.tray.as_ref().is_some_and(|t| t.quit_requested()) && !self.quitting {
            self.quit(ctx);
        }
        if self.updater.ready && !self.quitting {
            match self.updater.restart() {
                Ok(()) => self.quit(ctx),
                Err(e) => {
                    self.updater.ready = false;
                    self.updater.error = Some(e);
                }
            }
        }
        let close = ctx.input(|i| i.viewport().close_requested());
        if close && !self.quitting && self.settings.close_to_tray && self.tray.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.fullscreen {
                self.set_fullscreen(ctx, false);
            }
            desktop::hide(self.window);
            log::info!("окно спрятано в трей");
        }
    }

    /// Отладка (только debug): AMADEUS_AUTO_UPDATE=1 — сразу ставить найденное обновление (кроме копии, только что
    /// перезапущенной после обновления); AMADEUS_TRAY_QUIT_IN=<с> — «Выйти» из трея через N секунд (из другого потока).
    #[cfg(debug_assertions)]
    fn debug_update_hooks(&mut self) {
        let after_update = std::env::args().any(|a| a == desktop::ARG_AFTER_UPDATE);
        if std::env::var_os("AMADEUS_AUTO_UPDATE").is_some() && !after_update && self.updater.available.is_some() && !self.update_open {
            log::info!("отладка: ставлю обновление");
            self.update_open = true;
            self.updater.install();
        }
        static ARMED: std::sync::Once = std::sync::Once::new();
        if let (Some(secs), Some(tray)) = (std::env::var("AMADEUS_TRAY_QUIT_IN").ok().and_then(|s| s.parse::<u64>().ok()), &self.tray) {
            ARMED.call_once(|| tray.debug_quit_in(Duration::from_secs(secs), self.ctx.clone()));
        }
    }

    fn quit(&mut self, ctx: &egui::Context) {
        log::info!("выход");
        self.quitting = true;
        self.voice.send(Cmd::Leave);
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Окно «Доступно обновление».
    fn ui_update(&mut self, ui: &mut Ui) {
        let Some(info) = self.updater.available.clone() else {
            self.update_open = false;
            return;
        };
        let in_call = self.voice.snapshot().channel.is_some();
        let mut close = false;
        let modal = egui::Modal::new(Id::new("update"))
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
                ui.set_width(440.0);
                ui.label(RichText::new(format!("Доступна версия {}", info.version)).font(t::xbold(20.0)));
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!("Сейчас у вас {} · загрузка {:.0} МБ", update::CURRENT, info.size as f64 / 1_048_576.0))
                        .size(13.0)
                        .color(t::DUST),
                );
                if !info.notes.trim().is_empty() {
                    ui.add_space(12.0);
                    ui.label(RichText::new(info.notes.trim()).color(t::PAPER));
                }
                if in_call {
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(format!("{}  Вы в голосовом канале — после перезапуска зайдите снова.", ph::INFO))
                            .size(13.0)
                            .color(t::DUST),
                    );
                }
                if self.updater.installing {
                    ui.add_space(14.0);
                    ui.add(
                        egui::ProgressBar::new(self.updater.progress())
                            .fill(t::NIXIE)
                            .desired_height(8.0)
                            .corner_radius(CornerRadius::same(4)),
                    );
                    ui.label(RichText::new("Скачиваю…").size(13.0).color(t::DUST));
                }
                if let Some(err) = &self.updater.error {
                    ui.add_space(10.0);
                    ui.label(RichText::new(format!("{}  Не получилось: {err}", ph::WARNING_CIRCLE)).size(13.0).color(t::ERR_TEXT));
                    if ui.link("Скачать установщик в браузере").clicked() {
                        desktop::open_url(&self.updater.installer_url());
                    }
                }
                ui.add_space(20.0);
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    let busy = self.updater.installing || self.updater.ready;
                    if t::primary_button(ui, "Обновить и перезапустить", 230.0, !busy).clicked() {
                        self.updater.install();
                    }
                    if ui.add_enabled(!busy, egui::Button::new("Позже")).clicked() {
                        close = true;
                    }
                });
            });
        if close || (modal.should_close() && !self.updater.installing) {
            self.update_open = false;
        }
    }

    /// Раздел настроек «Приложение»: автозапуск, трей, версия и обновления.
    fn ui_settings_app(&mut self, ui: &mut Ui, w: f32) {
        ui.add_space(18.0);
        ui.label(RichText::new("Приложение").font(t::bold(15.0)));
        ui.add_space(4.0);
        let installed = desktop::is_installed();
        let mut autostart = self.settings.autostart;
        toggle_row(ui, w, |ui| {
            let r = ui.add_enabled(installed, egui::Label::new("Запускать вместе с Windows (свёрнутым в трей)"));
            if !installed {
                r.on_disabled_hover_text("Только у программы, поставленной установщиком");
            }
        }, |ui| {
            ui.add_enabled_ui(installed, |ui| {
                if t::switch(ui, &mut autostart).changed() {
                    self.settings.autostart = autostart;
                    self.settings.save();
                    self.autostart_error = desktop::set_autostart(autostart).err();
                }
            });
        });
        if let Some(e) = &self.autostart_error {
            ui.label(RichText::new(format!("Автозапуск: {e}")).size(12.5).color(t::ERR_TEXT));
        }
        let mut to_tray = self.settings.close_to_tray;
        toggle_row(ui, w, |ui| {
            ui.label("«Закрыть» сворачивает в трей (выйти — через значок в трее)");
        }, |ui| {
            if t::switch(ui, &mut to_tray).changed() {
                self.settings.close_to_tray = to_tray;
                self.settings.save();
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("Версия {}", update::CURRENT)).color(t::PAPER));
            let u = &self.updater;
            let status = if u.checking {
                RichText::new("проверяю…").color(t::DUST)
            } else if let Some(i) = &u.available {
                RichText::new(format!("доступна {}", i.version)).color(t::NIXIE)
            } else if let Some(e) = &u.error {
                RichText::new(e.clone()).color(t::ERR_TEXT)
            } else if u.checked_once {
                RichText::new("последняя").color(t::DUST)
            } else {
                RichText::new("")
            };
            ui.label(status.size(13.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.updater.available.is_some() {
                    if ui.button(format!("{} Обновить", ph::ARROW_CIRCLE_UP)).clicked() {
                        self.update_open = true;
                        self.settings_open = false;
                    }
                } else if ui.add_enabled(!self.updater.checking, egui::Button::new("Проверить обновления")).clicked() {
                    self.updater.check();
                }
            });
        });
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

                    self.ui_settings_app(ui, w);

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
    /// Вызывается и когда окно спрятано/свёрнуто (тогда `ui` не вызывается): трей, выход, обновления.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.updater.poll();
        #[cfg(debug_assertions)]
        self.debug_update_hooks();
        self.handle_close(ctx);
        if let Some(until) = self.hide_until {
            if desktop::is_visible(self.window) {
                desktop::hide(self.window);
                log::info!("запуск из автозагрузки — окно в трее");
            }
            if Instant::now() < until {
                ctx.request_repaint_after(Duration::from_millis(50));
            } else {
                self.hide_until = None;
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.handle_msgs();
        self.chat.poll();
        if self.screen == Screen::Main && self.chat.unauthorized() {
            self.logout();
        }
        // В полноэкранном просмотре трансляции рамки нет.
        if !self.fullscreen {
            let badge = self.updater.available.as_ref().map(|i| format!("Обновление {}", i.version));
            if crate::frame::title_bar(ui, badge.as_deref()) {
                self.update_open = true;
            }
        }
        match self.screen {
            Screen::Boot => self.ui_boot(ui),
            Screen::Login => self.ui_login(ui),
            Screen::Main => self.ui_main(ui),
        }
        if self.update_open {
            self.ui_update(ui);
        }
        if !self.fullscreen {
            crate::frame::resize_edges(ui.ctx());
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
        streaming: id == "kurisu" || (id == "daru" && std::env::var_os("AMADEUS_DEMO_STREAMS").is_some()),
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

/// Фон под видео (поля при вписывании).
const VIDEO_BG: Color32 = Color32::from_rgb(0x0C, 0x0A, 0x08);

/// Элемент сцены комнаты.
#[derive(Clone, Copy)]
enum Item<'a> {
    Person(&'a Peer),
    Stream(&'a Peer),
}

enum StreamAct {
    Watch,
    Focus,
    Fullscreen,
    Grid,
    Unwatch,
}

/// Раскладка n плиток сеткой по центру `r`; высота плитки не больше `aspect` × ширина.
fn grid_rects(r: Rect, n: usize, aspect: f32, gap: f32) -> Vec<Rect> {
    let n = n.max(1);
    let cols = match n {
        1 => 1,
        2 => 2,
        3 | 5 | 6 | 9 => 3,
        4 => 2,
        _ => 4,
    };
    let rows = n.div_ceil(cols);
    let tw = (r.width() - gap * (cols - 1) as f32) / cols as f32;
    let th = ((r.height() - gap * (rows - 1) as f32) / rows as f32).min(tw * aspect);
    let total_h = th * rows as f32 + gap * (rows - 1) as f32;
    let top = r.top() + (r.height() - total_h) / 2.0;
    (0..n)
        .map(|i| {
            let (row, col) = (i / cols, i % cols);
            let in_row = if row == rows - 1 { n - row * cols } else { cols };
            let row_w = tw * in_row as f32 + gap * (in_row - 1) as f32;
            let left = r.left() + (r.width() - row_w) / 2.0;
            Rect::from_min_size(pos2(left + col as f32 * (tw + gap), top + row as f32 * (th + gap)), vec2(tw, th))
        })
        .collect()
}

/// Вписать видео `size` в `outer` с сохранением пропорций (поля по краям).
fn fit_rect(outer: Rect, size: [usize; 2]) -> Rect {
    let (w, h) = (size[0].max(1) as f32, size[1].max(1) as f32);
    let s = (outer.width() / w).min(outer.height() / h);
    Rect::from_center_size(outer.center(), vec2(w * s, h * s))
}

fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() > max { s.chars().take(max - 1).chain(['…']).collect() } else { s.to_string() }
}

/// Полупрозрачная подпись поверх видео.
fn chip(painter: &egui::Painter, pos: Pos2, anchor: Align2, text: &str, color: Color32) {
    let galley = painter.layout_no_wrap(text.to_string(), t::semibold(13.0), color);
    let size = galley.size() + vec2(18.0, 10.0);
    let r = anchor.anchor_size(pos, size);
    painter.rect_filled(r, CornerRadius::same(7), Color32::from_rgba_unmultiplied(8, 6, 4, 190));
    painter.galley(r.min + vec2(9.0, 5.0), galley, color);
}

/// Круглая кнопка поверх видео.
fn overlay_button(ui: &Ui, center: Pos2, icon: &str, tip: &str, id: Id) -> egui::Response {
    let rect = Rect::from_center_size(center, vec2(36.0, 36.0));
    let resp = ui.interact(rect, id, Sense::click());
    let hot = resp.hovered();
    let p = ui.painter();
    p.circle_filled(center, 18.0, Color32::from_rgba_unmultiplied(8, 6, 4, if hot { 235 } else { 170 }));
    p.circle_stroke(center, 18.0, Stroke::new(1.0, if hot { t::nixie_a(160) } else { t::BRASS }));
    p.text(center, Align2::CENTER_CENTER, icon, t::font(18.0), if hot { t::FILAMENT } else { t::PAPER });
    resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand)
}

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
                match &s.audio {
                    voice::AudioStatus::On(_) => {
                        ui.label(RichText::new(egui_phosphor::regular::SPEAKER_HIGH).font(t::font(14.0)).color(t::NIXIE));
                    }
                    voice::AudioStatus::Failed(_) => {
                        ui.label(RichText::new(egui_phosphor::regular::SPEAKER_X).font(t::font(14.0)).color(t::ERR_TEXT));
                    }
                    voice::AudioStatus::Off => {}
                }
                ui.label(RichText::new(&title).font(t::semibold(13.0)).color(t::FILAMENT));
                ui.label(RichText::new(egui_phosphor::fill::MONITOR_ARROW_UP).font(t::icon_fill(15.0)).color(t::NIXIE));
            });
        })
        .response;
    let size = s.size.map(|(w, h)| format!("{w}×{h}")).unwrap_or_else(|| "—".into());
    let enc = s.backend.clone();
    let lim = s.limitation.as_deref().map(|l| format!("
Ограничение качества: {l}")).unwrap_or_default();
    let audio = match &s.audio {
        voice::AudioStatus::Off => "выключен".to_string(),
        voice::AudioStatus::On(what) => what.to_string(),
        voice::AudioStatus::Failed(e) => format!("не захватывается — {e}"),
    };
    resp.on_hover_text(format!("В эфире: {}
Качество: {} · отправляется {size}
Энкодер: {enc}{lim}
Звук: {audio}", s.title, s.preset.label()));
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
