//! Текстовый чат (фаза 7, вариант A): история каналов с подгрузкой, новые сообщения в реальном времени (SSE),
//! картинки — Ctrl+V, перетаскивание, кнопка. Всё сетевое — в tokio, результаты — в UI через канал.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, mpsc as std_mpsc};
use std::time::Duration;

use chrono::{Datelike, Local, TimeZone};
use eframe::egui::{
    self, Align, Align2, Color32, ColorImage, CornerRadius, Frame, Id, Key, KeyboardShortcut, Margin, Modifiers, Rect,
    RichText, Sense, Stroke, StrokeKind, TextEdit, TextureHandle, TextureOptions, Ui, pos2, vec2,
};
use egui_phosphor::regular as ph;
use proto::{Attachment, Channel, Message, SendMessageRequest, ServerEvent, limits};

use crate::api::{Api, ApiError, ApiResult};
use crate::theme as t;

/// Сообщения одного автора подряд в пределах этого времени — одной группой (без повторной шапки).
const GROUP_GAP_SECS: i64 = 5 * 60;
/// Картинки в ленте — не больше этого размера (логические точки).
const IMG_MAX: egui::Vec2 = egui::Vec2::new(420.0, 320.0);
/// Декодированная картинка в памяти — не больше этой стороны (пикселей).
const DECODE_MAX_SIDE: u32 = 2048;
/// Миниатюра вложения в поле ввода.
const THUMB: f32 = 88.0;
/// Подгружать старые сообщения, когда до верха осталось меньше.
const LOAD_OLDER_MARGIN: f32 = 400.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    Latest,
    Older,
    After,
}

/// Картинка, подготовленная к отправке (байты файла + миниатюра).
struct Prepared {
    bytes: Vec<u8>,
    thumb: ColorImage,
    size: [usize; 2],
}

enum Ev {
    History { channel: i64, mode: Mode, res: ApiResult<Vec<Message>> },
    Sent { channel: i64, local: u64, res: ApiResult<Message> },
    Server(ServerEvent),
    Disconnected { reason: String, fatal: bool },
    Image { id: i64, res: Result<ColorImage, String> },
    Attach { channel: i64, res: Vec<Result<Prepared, String>> },
    Notice(String),
}

pub struct PendingImage {
    local: u64,
    bytes: Arc<Vec<u8>>,
    size: [usize; 2],
    thumb: TextureHandle,
}

enum OutState {
    Sending,
    Failed(String),
}

/// Сообщение, которое ещё не подтвердил сервер.
struct Outgoing {
    local: u64,
    content: String,
    images: Vec<PendingImage>,
    state: OutState,
}

#[derive(Default)]
struct Log {
    /// По возрастанию id.
    msgs: Vec<Message>,
    loaded: bool,
    loading: bool,
    loading_older: bool,
    reached_start: bool,
    error: Option<String>,
    outbox: Vec<Outgoing>,
    scroll_to_bottom: bool,
    /// Высота ленты до того, как сверху добавились старые сообщения (чтобы не прыгала прокрутка).
    prepended_from: Option<f32>,
    last_content_h: f32,
    /// Сколько кадров лента уже показана (в первых кадрах прокрутка ещё не встала вниз — старое не грузим).
    shown_frames: u32,
}

#[derive(Default)]
struct Draft {
    text: String,
    images: Vec<PendingImage>,
    error: Option<String>,
}

enum Img {
    Loading,
    Ready(TextureHandle),
    Failed(String),
}

pub struct Chat {
    rt: tokio::runtime::Handle,
    ctx: egui::Context,
    api: Api,
    tx: std_mpsc::Sender<Ev>,
    rx: std_mpsc::Receiver<Ev>,
    logs: HashMap<i64, Log>,
    drafts: HashMap<i64, Draft>,
    images: HashMap<i64, Img>,
    stream: Option<tokio::task::JoinHandle<()>>,
    online: bool,
    stream_error: Option<String>,
    /// Сессия недействительна — поток не перезапускаем (приложение само выйдет на экран входа).
    unauthorized: bool,
    next_local: u64,
    /// Картинка, открытая на весь экран.
    lightbox: Option<i64>,
    /// Короткое сообщение внизу (скопировано / сохранено / ошибка).
    notice: Option<(String, f64)>,
    /// Ctrl+V был нажат на прошлом кадре (ловим фронт нажатия).
    paste_down: bool,
    /// Высота нижней панели с полем ввода, измеренная в прошлом кадре (панель egui сама не сжимается).
    composer_h: f32,
    /// Отладка: шаги AMADEUS_CHAT_SCRIPT и время последнего шага.
    #[cfg(debug_assertions)]
    script: (std::collections::VecDeque<String>, f64),
    #[cfg(debug_assertions)]
    scroll_top: bool,
}

impl Chat {
    pub fn new(rt: tokio::runtime::Handle, ctx: egui::Context, api: Api) -> Self {
        let (tx, rx) = std_mpsc::channel();
        Self {
            rt,
            ctx,
            api,
            tx,
            rx,
            logs: HashMap::new(),
            drafts: HashMap::new(),
            images: HashMap::new(),
            stream: None,
            online: false,
            stream_error: None,
            unauthorized: false,
            next_local: 1,
            lightbox: None,
            notice: None,
            paste_down: false,
            composer_h: 64.0,
            #[cfg(debug_assertions)]
            script: (
                std::env::var("AMADEUS_CHAT_SCRIPT").map(|s| s.split('|').map(str::to_string).collect()).unwrap_or_default(),
                0.0,
            ),
            #[cfg(debug_assertions)]
            scroll_top: false,
        }
    }

    /// Отладка без мыши и клавиатуры (debug): AMADEUS_CHAT_SCRIPT="wait|send:текст|paste|top|lightbox|close|send"
    /// — по шагу раз в 2 с: отправить, вставить из буфера, прокрутить ленту к началу, открыть/закрыть последнюю картинку.
    #[cfg(debug_assertions)]
    fn debug_script(&mut self, ui: &Ui, channel: i64) {
        let now = ui.input(|i| i.time);
        if self.script.0.is_empty() {
            return;
        }
        ui.ctx().request_repaint_after(Duration::from_millis(200));
        if now - self.script.1 < 2.0 {
            return;
        }
        self.script.1 = now;
        let step = self.script.0.pop_front().unwrap_or_default();
        log::info!("AMADEUS_CHAT_SCRIPT: {step}");
        match step.split_once(':').unwrap_or((step.as_str(), "")) {
            ("send", text) => {
                if !text.is_empty() {
                    self.drafts.entry(channel).or_default().text = text.to_string();
                }
                self.send(channel);
            }
            ("type", text) => self.drafts.entry(channel).or_default().text = text.to_string(),
            ("paste", _) => self.paste_from_clipboard(channel),
            ("attach", path) => self.attach_files(channel, vec![PathBuf::from(path)]),
            ("top", _) => self.scroll_top = true,
            ("lightbox", _) => {
                self.lightbox = self.logs.get(&channel).and_then(|l| l.msgs.iter().rev().find_map(|m| m.attachments.last().map(|a| a.id)))
            }
            ("close", _) => self.lightbox = None,
            _ => {}
        }
    }

    /// Новый сервер/сессия: всё забываем, старые задачи пишут в закрытый канал.
    pub fn set_api(&mut self, api: Api) {
        self.api = api;
        self.reset();
    }

    pub fn reset(&mut self) {
        if let Some(h) = self.stream.take() {
            h.abort();
        }
        let (tx, rx) = std_mpsc::channel();
        self.tx = tx;
        self.rx = rx;
        self.logs.clear();
        self.drafts.clear();
        self.images.clear();
        self.online = false;
        self.stream_error = None;
        self.unauthorized = false;
        self.lightbox = None;
        self.notice = None;
    }

    /// Поток событий: запустить, если ещё не идёт (вызывается каждый кадр главного экрана).
    pub fn ensure_stream(&mut self) {
        if self.stream.is_some() || self.unauthorized || !self.api.has_token() {
            return;
        }
        let (api, tx, ctx) = (self.api.clone(), self.tx.clone(), self.ctx.clone());
        self.stream = Some(self.rt.spawn(run_stream(api, tx, ctx)));
    }

    /// Сессия истекла (поток получил 401) — пусть приложение выйдет на экран входа.
    pub fn unauthorized(&self) -> bool {
        self.unauthorized
    }

    pub fn lightbox_open(&self) -> bool {
        self.lightbox.is_some()
    }

    fn spawn<T: Send + 'static>(
        &self,
        fut: impl std::future::Future<Output = T> + Send + 'static,
        wrap: impl FnOnce(T) -> Ev + Send + 'static,
    ) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let r = fut.await;
            let _ = tx.send(wrap(r));
            ctx.request_repaint();
        });
    }

    fn fetch(&mut self, channel: i64, mode: Mode) {
        let log = self.logs.entry(channel).or_default();
        let (before, after) = match mode {
            Mode::Latest => (None, None),
            Mode::Older => (log.msgs.first().map(|m| m.id), None),
            Mode::After => (None, log.msgs.last().map(|m| m.id)),
        };
        if mode == Mode::Older {
            log.loading_older = true;
        } else {
            log.loading = true;
        }
        let api = self.api.clone();
        self.spawn(async move { api.messages(channel, before, after).await }, move |res| Ev::History { channel, mode, res });
    }

    /// Канал открыт в UI: загрузить историю, если её ещё нет.
    fn open(&mut self, channel: i64) {
        let log = self.logs.entry(channel).or_default();
        if !log.loaded && !log.loading && log.error.is_none() {
            self.fetch(channel, Mode::Latest);
        }
    }

    // ------------------------------------------------------------ события

    pub fn poll(&mut self) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Ev::History { channel, mode, res } => self.on_history(channel, mode, res),
                Ev::Sent { channel, local, res } => {
                    let log = self.logs.entry(channel).or_default();
                    match res {
                        Ok(msg) => {
                            log.outbox.retain(|o| o.local != local);
                            merge(&mut log.msgs, vec![msg]);
                            log.scroll_to_bottom = true;
                        }
                        Err(e) => {
                            if let Some(o) = log.outbox.iter_mut().find(|o| o.local == local) {
                                o.state = OutState::Failed(err_text(&e));
                            }
                        }
                    }
                }
                Ev::Server(e) => self.on_server(e),
                Ev::Disconnected { reason, fatal } => {
                    self.online = false;
                    self.stream_error = Some(reason);
                    if fatal {
                        self.unauthorized = true;
                        self.stream = None;
                    }
                }
                Ev::Image { id, res } => {
                    let img = match res {
                        Ok(ci) => Img::Ready(self.ctx.load_texture(format!("att-{id}"), ci, TextureOptions::LINEAR)),
                        Err(e) => Img::Failed(e),
                    };
                    self.images.insert(id, img);
                }
                Ev::Attach { channel, res } => {
                    let mut errors = Vec::new();
                    let mut added = Vec::new();
                    for r in res {
                        match r {
                            Ok(p) => {
                                let local = self.next_local;
                                self.next_local += 1;
                                let thumb = self.ctx.load_texture(format!("thumb-{local}"), p.thumb, TextureOptions::LINEAR);
                                added.push(PendingImage { local, bytes: Arc::new(p.bytes), size: p.size, thumb });
                            }
                            Err(e) => errors.push(e),
                        }
                    }
                    let d = self.drafts.entry(channel).or_default();
                    d.error = None;
                    for img in added {
                        if d.images.len() >= limits::ATTACHMENTS_MAX {
                            errors.push(format!("Не больше {} картинок в одном сообщении.", limits::ATTACHMENTS_MAX));
                            break;
                        }
                        d.images.push(img);
                    }
                    if !errors.is_empty() {
                        errors.dedup();
                        d.error = Some(errors.join(" "));
                    }
                }
                Ev::Notice(text) => self.notice = Some((text, self.ctx.input(|i| i.time))),
            }
        }
    }

    fn on_history(&mut self, channel: i64, mode: Mode, res: ApiResult<Vec<Message>>) {
        let full_page = |v: &Vec<Message>| v.len() >= limits::HISTORY_PAGE as usize;
        let log = self.logs.entry(channel).or_default();
        match mode {
            Mode::Older => log.loading_older = false,
            _ => log.loading = false,
        }
        let list = match res {
            Ok(l) => l,
            Err(e) => {
                log.error = Some(err_text(&e));
                return;
            }
        };
        log.error = None;
        let more = full_page(&list);
        match mode {
            Mode::Latest => {
                log.loaded = true;
                log.reached_start = !more;
                merge(&mut log.msgs, list);
                log.scroll_to_bottom = true;
            }
            Mode::Older => {
                log.reached_start = !more;
                if !list.is_empty() {
                    log.prepended_from = Some(log.last_content_h);
                }
                merge(&mut log.msgs, list);
            }
            Mode::After => {
                merge(&mut log.msgs, list);
                if more {
                    // пропущено больше страницы — дочитать
                    self.fetch(channel, Mode::After);
                }
            }
        }
    }

    fn on_server(&mut self, e: ServerEvent) {
        match e {
            ServerEvent::Hello | ServerEvent::Resync => {
                self.online = true;
                self.stream_error = None;
                // дочитать то, что могло прийти, пока потока не было
                let loaded: Vec<(i64, bool)> =
                    self.logs.iter().filter(|(_, l)| l.loaded).map(|(id, l)| (*id, l.msgs.is_empty())).collect();
                for (id, empty) in loaded {
                    self.fetch(id, if empty { Mode::Latest } else { Mode::After });
                }
            }
            ServerEvent::MessageCreated { message } => {
                if let Some(log) = self.logs.get_mut(&message.channel_id)
                    && log.loaded
                {
                    merge(&mut log.msgs, vec![message]);
                }
            }
        }
    }

    // ------------------------------------------------------------ отправка и вложения

    fn send(&mut self, channel: i64) {
        let d = self.drafts.entry(channel).or_default();
        let content = d.text.trim().to_string();
        if content.is_empty() && d.images.is_empty() {
            return;
        }
        if content.chars().count() > limits::MESSAGE_MAX {
            d.error = Some(format!("Сообщение длиннее {} символов.", limits::MESSAGE_MAX));
            return;
        }
        let out = Outgoing {
            local: self.next_local,
            content,
            images: std::mem::take(&mut d.images),
            state: OutState::Sending,
        };
        self.next_local += 1;
        d.text.clear();
        d.error = None;
        self.spawn_send(channel, &out);
        let log = self.logs.entry(channel).or_default();
        log.outbox.push(out);
        log.scroll_to_bottom = true;
    }

    fn spawn_send(&self, channel: i64, out: &Outgoing) {
        let api = self.api.clone();
        let content = out.content.clone();
        let files: Vec<Arc<Vec<u8>>> = out.images.iter().map(|i| i.bytes.clone()).collect();
        let local = out.local;
        self.spawn(
            async move {
                let mut attachments = Vec::with_capacity(files.len());
                for f in files {
                    attachments.push(api.upload((*f).clone()).await?.id);
                }
                api.send_message(channel, &SendMessageRequest { content, attachments }).await
            },
            move |res| Ev::Sent { channel, local, res },
        );
    }

    fn retry(&mut self, channel: i64, local: u64) {
        let Some(log) = self.logs.get_mut(&channel) else { return };
        let Some(i) = log.outbox.iter().position(|o| o.local == local) else { return };
        log.outbox[i].state = OutState::Sending;
        let out = log.outbox.remove(i);
        self.spawn_send(channel, &out);
        self.logs.entry(channel).or_default().outbox.insert(i, out);
    }

    fn attach_bytes(&self, channel: i64, sources: Vec<Source>) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        self.rt.spawn_blocking(move || {
            let res: Vec<Result<Prepared, String>> = sources.into_iter().filter_map(|s| s.load().transpose()).map(|r| r.and_then(prepare)).collect();
            if !res.is_empty() {
                let _ = tx.send(Ev::Attach { channel, res });
                ctx.request_repaint();
            }
        });
    }

    /// Ctrl+V без текста в буфере: картинка или скопированные в проводнике файлы.
    fn paste_from_clipboard(&self, channel: i64) {
        self.attach_bytes(channel, vec![Source::Clipboard]);
    }

    pub fn attach_files(&self, channel: i64, paths: Vec<PathBuf>) {
        if !paths.is_empty() {
            self.attach_bytes(channel, paths.into_iter().map(Source::File).collect());
        }
    }

    fn pick_files(&self, channel: i64) {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        self.rt.spawn_blocking(move || {
            let Some(paths) = rfd::FileDialog::new()
                .set_title("Прикрепить картинки")
                .add_filter("Картинки", &["png", "jpg", "jpeg", "gif", "webp", "bmp"])
                .pick_files()
            else {
                return;
            };
            let res: Vec<Result<Prepared, String>> =
                paths.into_iter().filter_map(|p| Source::File(p).load().transpose()).map(|r| r.and_then(prepare)).collect();
            let _ = tx.send(Ev::Attach { channel, res });
            ctx.request_repaint();
        });
    }

    fn request_image(&mut self, id: i64) {
        if self.images.contains_key(&id) {
            return;
        }
        self.images.insert(id, Img::Loading);
        let api = self.api.clone();
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let res = match api.attachment(id).await {
                Ok(bytes) => tokio::task::spawn_blocking(move || decode_for_display(&bytes))
                    .await
                    .unwrap_or_else(|e| Err(format!("{e}"))),
                Err(e) => Err(e.message),
            };
            let _ = tx.send(Ev::Image { id, res });
            ctx.request_repaint();
        });
    }

    /// Скопировать картинку в буфер обмена (оригинал с сервера).
    fn copy_image(&self, id: i64) {
        let api = self.api.clone();
        self.spawn(
            async move {
                let bytes = api.attachment(id).await.map_err(|e| e.message)?;
                tokio::task::spawn_blocking(move || {
                    let img = image::load_from_memory(&bytes).map_err(|e| format!("{e}"))?.to_rgba8();
                    let (w, h) = img.dimensions();
                    arboard::Clipboard::new()
                        .and_then(|mut c| {
                            c.set_image(arboard::ImageData {
                                width: w as usize,
                                height: h as usize,
                                bytes: std::borrow::Cow::Owned(img.into_raw()),
                            })
                        })
                        .map_err(|e| format!("{e}"))
                })
                .await
                .map_err(|e| format!("{e}"))?
            },
            |r: Result<(), String>| {
                Ev::Notice(match r {
                    Ok(()) => "Картинка скопирована".into(),
                    Err(e) => format!("Не удалось скопировать: {e}"),
                })
            },
        );
    }

    fn save_image(&self, att: &Attachment) {
        let api = self.api.clone();
        let (id, ext) = (att.id, ext_of(&att.mime));
        self.spawn(
            async move {
                let bytes = api.attachment(id).await.map_err(|e| e.message)?;
                tokio::task::spawn_blocking(move || {
                    let Some(path) = rfd::FileDialog::new()
                        .set_title("Сохранить картинку")
                        .set_file_name(format!("amadeus-{id}.{ext}"))
                        .add_filter("Картинка", &[ext])
                        .save_file()
                    else {
                        return Ok(None);
                    };
                    std::fs::write(&path, bytes).map_err(|e| format!("{e}"))?;
                    Ok(Some(path))
                })
                .await
                .map_err(|e| format!("{e}"))?
            },
            |r: Result<Option<PathBuf>, String>| match r {
                Ok(Some(p)) => Ev::Notice(format!("Сохранено: {}", p.display())),
                Ok(None) => Ev::Notice(String::new()),
                Err(e) => Ev::Notice(format!("Не удалось сохранить: {e}")),
            },
        );
    }

    // ------------------------------------------------------------ UI

    /// Центр окна для текстового канала: лента + поле ввода. `modal` — поверх открыто окно (не ловим Ctrl+V).
    pub fn ui(&mut self, ui: &mut Ui, ch: &Channel, modal: bool) {
        self.open(ch.id);
        let composer_id = Id::new(("composer", ch.id));
        #[cfg(debug_assertions)]
        self.debug_script(ui, ch.id);
        self.handle_paste_and_drop(ui, ch.id, composer_id, modal);

        let panel = egui::Panel::bottom(Id::new(("chat-bottom", ch.id)))
            .resizable(false)
            .exact_size(self.composer_h)
            .show_separator_line(false)
            .frame(Frame::new().inner_margin(Margin { left: 24, right: 24, top: 6, bottom: 18 }))
            .show(ui, |ui| {
                let top = ui.cursor().top();
                self.ui_composer(ui, ch, composer_id);
                ui.min_rect().bottom() - top
            });
        let need = (panel.inner + 24.0).ceil();
        if (need - self.composer_h).abs() > 0.5 {
            self.composer_h = need;
            ui.ctx().request_discard("chat: composer height changed");
        }
        egui::CentralPanel::no_frame().show(ui, |ui| self.ui_feed(ui, ch));

        self.ui_drop_overlay(ui);
        self.ui_lightbox(ui);
        self.ui_notice(ui);
    }

    fn handle_paste_and_drop(&mut self, ui: &Ui, channel: i64, composer_id: Id, modal: bool) {
        let (dropped, text_paste, focused_window) = ui.input(|i| {
            (
                i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect::<Vec<_>>(),
                i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))),
                i.focused,
            )
        });
        if !modal && !dropped.is_empty() {
            self.attach_files(channel, dropped);
            ui.memory_mut(|m| m.request_focus(composer_id));
        }
        // egui-winit отдаёт Ctrl+V только если в буфере есть текст. Картинку ловим сами: фронт нажатия Ctrl+V.
        let down = focused_window && ctrl_v_down();
        let edge = down && !self.paste_down;
        self.paste_down = down;
        let focus = ui.memory(|m| m.focused());
        let ours = focus.is_none() || focus == Some(composer_id);
        if edge && !text_paste && !modal && ours && self.lightbox.is_none() {
            self.paste_from_clipboard(channel);
            ui.memory_mut(|m| m.request_focus(composer_id));
        }
    }

    fn ui_feed(&mut self, ui: &mut Ui, ch: &Channel) {
        let me_online = self.online;
        let stream_error = self.stream_error.clone();
        let log = self.logs.entry(ch.id).or_default();
        if !log.loaded {
            let r = ui.max_rect();
            match &log.error {
                Some(e) => {
                    let e = e.clone();
                    let area = Rect::from_center_size(r.center(), vec2(460.0, 110.0));
                    let mut retry = false;
                    ui.scope_builder(egui::UiBuilder::new().max_rect(area), |ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new(format!("{}  {e}", ph::WARNING_CIRCLE)).color(t::ERR_TEXT));
                            ui.add_space(8.0);
                            retry = ui.button("Повторить").clicked();
                        });
                    });
                    if retry {
                        log.error = None;
                        self.fetch(ch.id, Mode::Latest);
                    }
                }
                None => {
                    egui::Spinner::new().size(28.0).color(t::NIXIE).paint_at(ui, Rect::from_center_size(r.center(), vec2(28.0, 28.0)));
                }
            }
            return;
        }
        if !me_online && let Some(err) = stream_error {
            let r = ui.max_rect();
            let p = ui.painter();
            let text = format!("{}  Нет связи с сервером — переподключаемся… ({})", ph::WIFI_SLASH, ellipsize(&err, 60));
            let g = p.layout_no_wrap(text, t::font(12.5), t::ERR_TEXT);
            let rr = Rect::from_center_size(pos2(r.center().x, r.top() + 16.0), g.size() + vec2(20.0, 8.0));
            p.rect_filled(rr, CornerRadius::same(10), Color32::from_rgba_unmultiplied(229, 83, 75, 30));
            p.galley(rr.center() - g.size() / 2.0, g, t::ERR_TEXT);
            ui.add_space(30.0);
        }

        let scroll_to_bottom = std::mem::take(&mut log.scroll_to_bottom);
        if scroll_to_bottom {
            log.shown_frames = 0;
        }
        log.shown_frames = log.shown_frames.saturating_add(1);
        let reached_start = log.reached_start;
        let loading_older = log.loading_older;
        let msgs = std::mem::take(&mut log.msgs);
        let outbox = std::mem::take(&mut log.outbox);
        let mut act = FeedAct::None;
        let out = egui::ScrollArea::vertical()
            .id_salt(("chat-feed", ch.id))
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add_space(12.0);
                if reached_start {
                    channel_intro(ui, &ch.name);
                } else {
                    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
                    if loading_older {
                        egui::Spinner::new().size(20.0).color(t::NIXIE).paint_at(ui, Rect::from_center_size(r.center(), vec2(20.0, 20.0)));
                    }
                }
                let mut prev: Option<&Message> = None;
                for m in &msgs {
                    let new_day = prev.is_none_or(|p| day_of(p.created_at) != day_of(m.created_at));
                    if new_day {
                        day_separator(ui, m.created_at);
                    }
                    let grouped = !new_day
                        && prev.is_some_and(|p| p.author.id == m.author.id && m.created_at - p.created_at < GROUP_GAP_SECS);
                    if let Some(a) = self.message_row(ui, m, grouped) {
                        act = a;
                    }
                    prev = Some(m);
                }
                for o in &outbox {
                    if let Some(a) = outgoing_row(ui, o) {
                        act = a;
                    }
                }
                ui.add_space(8.0);
            });
        let log = self.logs.entry(ch.id).or_default();
        log.msgs = msgs;
        log.outbox = outbox;
        // старые сообщения добавились сверху — сдвигаем прокрутку на их высоту (кадр перерисовывается без скачка)
        if let Some(prev_h) = log.prepended_from.take() {
            let delta = out.content_size.y - prev_h;
            if delta > 0.0 {
                let mut st = out.state;
                st.offset.y += delta;
                st.store(ui.ctx(), out.id);
                ui.ctx().request_discard("chat: older messages prepended");
            }
        }
        log.last_content_h = out.content_size.y;
        #[cfg(debug_assertions)]
        if std::mem::take(&mut self.scroll_top) {
            let mut st = out.state;
            st.offset.y = 0.0;
            st.store(ui.ctx(), out.id);
            ui.ctx().request_repaint();
        }
        if scroll_to_bottom {
            let bottom = (out.content_size.y - out.inner_rect.height()).max(0.0);
            if (out.state.offset.y - bottom).abs() > 0.5 {
                let mut st = out.state;
                st.offset.y = bottom;
                st.store(ui.ctx(), out.id);
                ui.ctx().request_discard("chat: scroll to bottom");
            }
        }
        let near_top = out.state.offset.y < LOAD_OLDER_MARGIN;
        let settled = log.shown_frames > 3 && out.content_size.y > out.inner_rect.height();
        if settled && near_top && !log.reached_start && !log.loading_older && log.error.is_none() && !log.msgs.is_empty() {
            self.fetch(ch.id, Mode::Older);
        }
        match act {
            FeedAct::None => {}
            FeedAct::Retry(local) => self.retry(ch.id, local),
            FeedAct::Discard(local) => self.logs.entry(ch.id).or_default().outbox.retain(|o| o.local != local),
            FeedAct::Open(id) => self.lightbox = Some(id),
            FeedAct::Copy(id) => self.copy_image(id),
            FeedAct::Save(att) => self.save_image(&att),
            FeedAct::ReloadImage(id) => {
                self.images.remove(&id);
            }
        }
    }

    /// Одна строка ленты: шапка группы (аватар, имя, время) или продолжение, текст и картинки.
    fn message_row(&mut self, ui: &mut Ui, m: &Message, grouped: bool) -> Option<FeedAct> {
        let mut act = None;
        let gutter = 56.0;
        let top_pad = if grouped { 3.0 } else { 16.0 };
        ui.add_space(top_pad);
        let start = ui.cursor().min;
        let full_w = ui.available_width();
        // фон при наведении — рисуем под строкой после раскладки
        let bg_idx = ui.painter().add(egui::Shape::Noop);
        let inner = ui.horizontal_top(|ui| {
            ui.add_space(gutter);
            ui.vertical(|ui| {
                ui.set_width(full_w - gutter - 24.0);
                ui.spacing_mut().item_spacing.y = 3.0;
                if !grouped {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.label(RichText::new(&m.author.name).font(t::semibold(15.0)).color(t::avatar_color(&m.author.name)));
                        ui.label(RichText::new(time_label(m.created_at, true)).size(12.0).color(t::DUST));
                    });
                }
                if !m.content.is_empty() {
                    ui.add(egui::Label::new(RichText::new(&m.content).size(15.0).color(t::PAPER)).wrap().selectable(true));
                }
                for a in &m.attachments {
                    if let Some(x) = self.attachment_view(ui, a) {
                        act = Some(x);
                    }
                }
            });
        });
        let row = Rect::from_min_max(pos2(start.x, start.y - top_pad / 2.0), pos2(start.x + full_w, inner.response.rect.bottom() + 2.0));
        let hovered = ui.rect_contains_pointer(row);
        if hovered {
            ui.painter().set(bg_idx, egui::Shape::rect_filled(row, CornerRadius::same(6), Color32::from_rgba_unmultiplied(0x2A, 0x22, 0x1C, 150)));
        }
        let p = ui.painter();
        if !grouped {
            t::avatar(p, pos2(start.x + 30.0, start.y + 19.0), 18.0, &m.author.name, 0.0, false);
        } else if hovered {
            p.text(pos2(start.x + 30.0, start.y + 11.0), Align2::CENTER_CENTER, time_label(m.created_at, false), t::font(11.0), t::DUST);
        }
        act
    }

    fn attachment_view(&mut self, ui: &mut Ui, a: &Attachment) -> Option<FeedAct> {
        self.request_image(a.id);
        let src = vec2(a.width.max(1) as f32, a.height.max(1) as f32);
        let max = vec2(IMG_MAX.x.min(ui.available_width()), IMG_MAX.y);
        let scale = (max.x / src.x).min(max.y / src.y).min(1.0);
        let size = (src * scale).max(vec2(48.0, 48.0));
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
        let p = ui.painter();
        let mut act = None;
        match self.images.get(&a.id) {
            Some(Img::Ready(tex)) => {
                egui::Image::new(tex).corner_radius(CornerRadius::same(8)).paint_at(ui, rect);
                if resp.hovered() {
                    p.rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, t::BRASS), StrokeKind::Inside);
                }
                if resp.clicked() {
                    act = Some(FeedAct::Open(a.id));
                }
            }
            Some(Img::Failed(e)) => {
                p.rect(rect, CornerRadius::same(8), t::BAKELITE, Stroke::new(1.0, t::BRASS_DIM), StrokeKind::Inside);
                p.text(rect.center(), Align2::CENTER_CENTER, format!("{}  не загрузилась", ph::IMAGE_BROKEN), t::font(13.0), t::ERR_TEXT);
                let tip = format!("{e}\nНажмите, чтобы повторить");
                if resp.clicked() {
                    act = Some(FeedAct::ReloadImage(a.id));
                }
                resp.clone().on_hover_text(tip);
            }
            _ => {
                p.rect(rect, CornerRadius::same(8), t::BAKELITE, Stroke::new(1.0, t::BRASS_DIM), StrokeKind::Inside);
                egui::Spinner::new().size(20.0).color(t::DUST).paint_at(ui, Rect::from_center_size(rect.center(), vec2(20.0, 20.0)));
            }
        }
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        resp.context_menu(|ui| {
            if ui.button(format!("{}  Открыть", ph::ARROWS_OUT)).clicked() {
                act = Some(FeedAct::Open(a.id));
                ui.close();
            }
            if ui.button(format!("{}  Скопировать", ph::COPY)).clicked() {
                act = Some(FeedAct::Copy(a.id));
                ui.close();
            }
            if ui.button(format!("{}  Сохранить как…", ph::DOWNLOAD_SIMPLE)).clicked() {
                act = Some(FeedAct::Save(a.clone()));
                ui.close();
            }
        });
        act
    }

    fn ui_composer(&mut self, ui: &mut Ui, ch: &Channel, id: Id) {
        let mut send = false;
        let mut pick = false;
        let mut remove = None;
        let d = self.drafts.entry(ch.id).or_default();
        if let Some(e) = &d.error {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(format!("{}  {e}", ph::WARNING_CIRCLE)).size(13.0).color(t::ERR_TEXT));
            });
        }
        Frame::new()
            .fill(t::BAKELITE_2)
            .stroke(Stroke::new(1.0, if ui.memory(|m| m.has_focus(id)) { t::BRASS } else { t::BRASS_DIM }))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(Margin { left: 6, right: 6, top: 3, bottom: 3 })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if !d.images.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(6.0);
                        ui.spacing_mut().item_spacing.x = 10.0;
                        for img in &d.images {
                            let (r, _) = ui.allocate_exact_size(vec2(THUMB, THUMB), Sense::hover());
                            let tsz = img.thumb.size_vec2();
                            let fit = Rect::from_center_size(r.center(), tsz * (THUMB / tsz.x).min(THUMB / tsz.y));
                            ui.painter().rect_filled(r, CornerRadius::same(8), t::GLASS);
                            egui::Image::new(&img.thumb).corner_radius(CornerRadius::same(6)).paint_at(ui, fit);
                            let x = Rect::from_center_size(r.right_top() + vec2(-11.0, 11.0), vec2(20.0, 20.0));
                            let xr = ui.interact(x, Id::new(("thumb-x", img.local)), Sense::click());
                            ui.painter().circle_filled(x.center(), 10.0, if xr.hovered() { t::SIGNAL } else { Color32::from_black_alpha(190) });
                            ui.painter().text(x.center(), Align2::CENTER_CENTER, ph::X, t::font(12.0), Color32::WHITE);
                            if xr.on_hover_text("Убрать").clicked() {
                                remove = Some(img.local);
                            }
                            let r2 = ui.interact(r, Id::new(("thumb", img.local)), Sense::hover());
                            r2.on_hover_text(format!("{}×{} · {}", img.size[0], img.size[1], human_size(img.bytes.len())));
                        }
                    });
                    ui.add_space(4.0);
                }
                ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if t::icon_button(ui, ph::PLUS_CIRCLE, false, false, "Прикрепить картинку (или Ctrl+V, перетаскивание)").clicked() {
                        pick = true;
                    }
                    let can_send = !d.text.trim().is_empty() || !d.images.is_empty();
                    let w = ui.available_width() - 40.0;
                    // Enter — отправить, Shift+Enter — новая строка
                    let enter = ui.memory(|m| m.has_focus(id))
                        && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift && !i.modifiers.ctrl && !i.modifiers.alt);
                    let edit = TextEdit::multiline(&mut d.text)
                        .id(id)
                        .frame(egui::Frame::NONE)
                        .desired_rows(1)
                        .desired_width(w)
                        .font(t::font(15.0))
                        .margin(Margin::symmetric(4, 6))
                        .return_key(KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter))
                        .hint_text(RichText::new(format!("Написать в #{}", ch.name)).color(t::DUST));
                    egui::ScrollArea::vertical().id_salt(("composer-scroll", ch.id)).max_height(200.0).min_scrolled_height(0.0).max_width(w).show(ui, |ui| {
                        ui.add(edit);
                    });
                    if enter {
                        send = true;
                    }
                    ui.add_enabled_ui(can_send, |ui| {
                        if t::icon_button(ui, ph::PAPER_PLANE_RIGHT, false, can_send, "Отправить (Enter)").clicked() {
                            send = true;
                        }
                    });
                });
                let n = d.text.chars().count();
                if n > limits::MESSAGE_MAX - 300 {
                    let color = if n > limits::MESSAGE_MAX { t::SIGNAL } else { t::DUST };
                    ui.with_layout(egui::Layout::right_to_left(Align::Min), |ui| {
                        ui.label(RichText::new(format!("{n} / {}", limits::MESSAGE_MAX)).size(12.0).color(color));
                    });
                }
            });
        if let Some(local) = remove {
            d.images.retain(|i| i.local != local);
            d.error = None;
        }
        if pick {
            self.pick_files(ch.id);
        }
        if send {
            self.send(ch.id);
            ui.memory_mut(|m| m.request_focus(id));
        }
        // Открыли канал — сразу можно печатать.
        if ui.memory(|m| m.focused().is_none()) && !self.lightbox_open() {
            ui.memory_mut(|m| m.request_focus(id));
        }
    }

    fn ui_drop_overlay(&self, ui: &Ui) {
        let hovering = ui.input(|i| !i.raw.hovered_files.is_empty());
        if !hovering {
            return;
        }
        let r = ui.max_rect();
        let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("drop-overlay")));
        p.rect_filled(r, CornerRadius::ZERO, Color32::from_black_alpha(170));
        let card = Rect::from_center_size(r.center(), vec2(360.0, 150.0));
        p.rect(card, CornerRadius::same(14), t::BAKELITE_2, Stroke::new(2.0, t::NIXIE), StrokeKind::Inside);
        p.text(card.center() - vec2(0.0, 22.0), Align2::CENTER_CENTER, ph::IMAGE, t::font(36.0), t::NIXIE);
        p.text(card.center() + vec2(0.0, 26.0), Align2::CENTER_CENTER, "Отпустите, чтобы прикрепить", t::semibold(16.0), t::PAPER);
    }

    fn ui_lightbox(&mut self, ui: &Ui) {
        let Some(id) = self.lightbox else { return };
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.lightbox = None;
            return;
        }
        let screen = ctx.content_rect();
        let mut close = false;
        egui::Area::new(Id::new("lightbox")).order(egui::Order::Foreground).fixed_pos(screen.min).show(&ctx, |ui| {
            let (r, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(r, CornerRadius::ZERO, Color32::from_black_alpha(215));
            match self.images.get(&id) {
                Some(Img::Ready(tex)) => {
                    let area = r.shrink2(vec2(48.0, 56.0));
                    let ts = tex.size_vec2();
                    let s = (area.width() / ts.x).min(area.height() / ts.y).min(1.0);
                    let ir = Rect::from_center_size(area.center(), ts * s);
                    egui::Image::new(tex).corner_radius(CornerRadius::same(6)).paint_at(ui, ir);
                }
                _ => {
                    egui::Spinner::new().size(32.0).color(t::NIXIE).paint_at(ui, Rect::from_center_size(r.center(), vec2(32.0, 32.0)));
                }
            }
            ui.painter().text(pos2(r.center().x, r.bottom() - 24.0), Align2::CENTER_CENTER, "Esc или клик — закрыть · ПКМ по картинке в чате — скопировать / сохранить", t::font(12.5), t::DUST);
            if resp.clicked() {
                close = true;
            }
        });
        if close {
            self.lightbox = None;
        }
    }

    fn ui_notice(&mut self, ui: &Ui) {
        let Some((text, at)) = &self.notice else { return };
        let now = ui.input(|i| i.time);
        if text.is_empty() || now - at > 3.0 {
            self.notice = None;
            return;
        }
        ui.ctx().request_repaint_after(Duration::from_millis(300));
        let r = ui.max_rect();
        let p = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("chat-notice")));
        let g = p.layout_no_wrap(text.clone(), t::font(13.5), t::PAPER);
        let rr = Rect::from_center_size(pos2(r.center().x, r.bottom() - 110.0), g.size() + vec2(28.0, 14.0));
        p.rect(rr, CornerRadius::same(10), t::BAKELITE_2, Stroke::new(1.0, t::BRASS), StrokeKind::Inside);
        p.galley(rr.center() - g.size() / 2.0, g, t::PAPER);
    }
}

enum FeedAct {
    None,
    Retry(u64),
    Discard(u64),
    Open(i64),
    Copy(i64),
    Save(Attachment),
    ReloadImage(i64),
}

fn outgoing_row(ui: &mut Ui, o: &Outgoing) -> Option<FeedAct> {
    let mut act = None;
    ui.add_space(4.0);
    ui.horizontal_top(|ui| {
        ui.add_space(56.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            if !o.content.is_empty() {
                ui.add(egui::Label::new(RichText::new(&o.content).size(15.0).color(t::DUST)).wrap());
            }
            if !o.images.is_empty() {
                ui.horizontal(|ui| {
                    for img in &o.images {
                        let tsz = img.thumb.size_vec2();
                        let s = (THUMB / tsz.x).min(THUMB / tsz.y);
                        ui.add(egui::Image::new(&img.thumb).fit_to_exact_size(tsz * s).corner_radius(CornerRadius::same(6)).tint(Color32::from_white_alpha(150)));
                    }
                });
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                match &o.state {
                    OutState::Sending => {
                        ui.add(egui::Spinner::new().size(12.0).color(t::DUST));
                        ui.label(RichText::new(if o.images.is_empty() { "отправка…" } else { "загрузка картинок…" }).size(12.0).color(t::DUST));
                    }
                    OutState::Failed(e) => {
                        ui.label(RichText::new(format!("{}  Не отправлено: {e}", ph::WARNING_CIRCLE)).size(12.5).color(t::ERR_TEXT));
                        if link(ui, "Повторить").clicked() {
                            act = Some(FeedAct::Retry(o.local));
                        }
                        if link(ui, "Удалить").clicked() {
                            act = Some(FeedAct::Discard(o.local));
                        }
                    }
                }
            });
        });
    });
    act
}

fn link(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(egui::Label::new(RichText::new(text).size(12.5).underline().color(t::PAPER)).sense(Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn channel_intro(ui: &mut Ui, name: &str) {
    ui.add_space(24.0);
    ui.horizontal(|ui| {
        ui.add_space(16.0);
        ui.vertical(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
            ui.painter().circle_filled(r.center(), 32.0, t::BAKELITE_2);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, ph::HASH, t::font(34.0), t::PAPER);
            ui.add_space(6.0);
            ui.label(RichText::new(format!("Добро пожаловать в #{name}")).font(t::xbold(24.0)));
            ui.label(RichText::new(format!("Это начало канала #{name}.")).size(14.0).color(t::DUST));
        });
    });
    ui.add_space(12.0);
}

fn day_separator(ui: &mut Ui, ts: i64) {
    ui.add_space(14.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    let p = ui.painter();
    let label = day_label(ts);
    let g = p.layout_no_wrap(label, t::semibold(12.0), t::DUST);
    let c = r.center();
    let half = g.size().x / 2.0 + 10.0;
    p.hline(r.left() + 16.0..=c.x - half, c.y, Stroke::new(1.0, t::BRASS_DIM));
    p.hline(c.x + half..=r.right() - 16.0, c.y, Stroke::new(1.0, t::BRASS_DIM));
    p.galley(c - g.size() / 2.0, g, t::DUST);
}

// ------------------------------------------------------------ поток событий (SSE)

async fn run_stream(api: Api, tx: std_mpsc::Sender<Ev>, ctx: egui::Context) {
    let mut backoff = Duration::from_secs(1);
    loop {
        let send = |ev: Ev| {
            let ok = tx.send(ev).is_ok();
            ctx.request_repaint();
            ok
        };
        match api.events().await {
            Ok(mut resp) => {
                let mut buf: Vec<u8> = Vec::new();
                let reason = loop {
                    match resp.chunk().await {
                        Ok(Some(b)) => {
                            buf.extend_from_slice(&b);
                            while let Some(end) = block_end(&buf) {
                                let block: Vec<u8> = buf.drain(..end).collect();
                                if let Some(ev) = parse_block(&block) {
                                    backoff = Duration::from_secs(1);
                                    if !send(Ev::Server(ev)) {
                                        return;
                                    }
                                }
                            }
                            if buf.len() > 4 * 1024 * 1024 {
                                break "слишком большое событие".to_string();
                            }
                        }
                        Ok(None) => break "сервер закрыл соединение".to_string(),
                        Err(e) => break if e.is_timeout() { "сервер не отвечает".to_string() } else { format!("{e}") },
                    }
                };
                log::info!("поток событий прервался: {reason}");
                if !send(Ev::Disconnected { reason, fatal: false }) {
                    return;
                }
            }
            Err(e) => {
                let fatal = e.is_unauthorized();
                log::info!("поток событий: {}", e.message);
                if !send(Ev::Disconnected { reason: e.message, fatal }) || fatal {
                    return;
                }
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(Duration::from_secs(30));
    }
}

/// Конец первого блока SSE (после пустой строки), с учётом \r\n.
fn block_end(buf: &[u8]) -> Option<usize> {
    let lf = buf.windows(2).position(|w| w == b"\n\n").map(|i| i + 2);
    let crlf = buf.windows(4).position(|w| w == b"\r\n\r\n").map(|i| i + 4);
    match (lf, crlf) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// Блок SSE → событие. Комментарии (пинги) и незнакомые типы — None.
fn parse_block(block: &[u8]) -> Option<ServerEvent> {
    let text = String::from_utf8_lossy(block);
    let data: Vec<&str> = text
        .lines()
        .filter_map(|l| l.strip_prefix("data:"))
        .map(|d| d.strip_prefix(' ').unwrap_or(d))
        .collect();
    if data.is_empty() {
        return None;
    }
    match serde_json::from_str(&data.join("\n")) {
        Ok(e) => Some(e),
        Err(e) => {
            log::debug!("событие пропущено ({e})");
            None
        }
    }
}

// ------------------------------------------------------------ картинки

enum Source {
    File(PathBuf),
    Clipboard,
}

impl Source {
    /// Байты картинки. `Ok(None)` — в буфере обмена нет картинки (молча ничего не делаем).
    fn load(self) -> Result<Option<Vec<u8>>, String> {
        match self {
            Source::File(p) => {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let meta = std::fs::metadata(&p).map_err(|e| format!("{name}: {e}"))?;
                if meta.is_dir() {
                    return Err(format!("{name}: это папка."));
                }
                // заведомо огромное не читаем целиком (картинку до ~64 МБ ещё можно пережать)
                if meta.len() > 64 * 1024 * 1024 {
                    return Err(format!("{name}: файл слишком большой."));
                }
                std::fs::read(&p).map(Some).map_err(|e| format!("{name}: {e}"))
            }
            Source::Clipboard => {
                let mut cb = arboard::Clipboard::new().map_err(|e| format!("Буфер обмена: {e}"))?;
                if let Ok(img) = cb.get_image() {
                    let rgba = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned())
                        .ok_or("Буфер обмена: странная картинка")?;
                    return encode(&image::DynamicImage::ImageRgba8(rgba), image::ImageFormat::Png).map(Some);
                }
                // скопированный в проводнике файл
                if let Ok(files) = cb.get().file_list()
                    && let Some(f) = files.into_iter().next()
                {
                    return Source::File(f).load();
                }
                Ok(None)
            }
        }
    }
}

fn encode(img: &image::DynamicImage, fmt: image::ImageFormat) -> Result<Vec<u8>, String> {
    let mut out = std::io::Cursor::new(Vec::new());
    match fmt {
        image::ImageFormat::Jpeg => {
            let rgb = image::DynamicImage::ImageRgb8(img.to_rgb8());
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 88);
            rgb.write_with_encoder(enc).map_err(|e| format!("{e}"))?;
        }
        _ => img.write_to(&mut out, fmt).map_err(|e| format!("{e}"))?,
    }
    Ok(out.into_inner())
}

/// Проверить и подготовить картинку к отправке: формат, размер (большое — пережать в JPEG), миниатюра.
fn prepare(bytes: Vec<u8>) -> Result<Prepared, String> {
    let fmt = image::guess_format(&bytes).map_err(|_| "Можно прикрепить только картинку (PNG, JPEG, GIF, WebP).".to_string())?;
    let img = image::load_from_memory_with_format(&bytes, fmt).map_err(|e| format!("Картинка не читается: {e}"))?;
    let sendable = matches!(fmt, image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::Gif | image::ImageFormat::WebP);
    let side = limits::IMAGE_MAX_SIDE;
    let mut work = img;
    let mut bytes = bytes;
    if work.width() > side || work.height() > side {
        work = work.resize(side, side, image::imageops::FilterType::Triangle);
        bytes = encode(&work, image::ImageFormat::Png)?;
    } else if !sendable {
        // BMP и прочее — в PNG
        bytes = encode(&work, image::ImageFormat::Png)?;
    }
    if bytes.len() > limits::IMAGE_MAX_BYTES {
        // большой скриншот: JPEG, при необходимости — меньше по размеру
        let mut s = side.min(work.width().max(work.height()));
        loop {
            let scaled = if s < work.width().max(work.height()) { work.resize(s, s, image::imageops::FilterType::Triangle) } else { work.clone() };
            bytes = encode(&scaled, image::ImageFormat::Jpeg)?;
            if bytes.len() <= limits::IMAGE_MAX_BYTES {
                work = scaled;
                break;
            }
            if s <= 1024 {
                return Err(format!("Картинка больше {} МБ.", limits::IMAGE_MAX_BYTES / (1024 * 1024)));
            }
            s = s * 3 / 4;
        }
    }
    let size = [work.width() as usize, work.height() as usize];
    let th = work.thumbnail(160, 160).to_rgba8();
    let thumb = ColorImage::from_rgba_unmultiplied([th.width() as usize, th.height() as usize], th.as_raw());
    Ok(Prepared { bytes, thumb, size })
}

/// Для ленты: декодировать (у GIF — первый кадр) и уменьшить до DECODE_MAX_SIDE.
fn decode_for_display(bytes: &[u8]) -> Result<ColorImage, String> {
    let mut img = image::load_from_memory(bytes).map_err(|e| format!("Картинка не читается: {e}"))?;
    if img.width() > DECODE_MAX_SIDE || img.height() > DECODE_MAX_SIDE {
        img = img.resize(DECODE_MAX_SIDE, DECODE_MAX_SIDE, image::imageops::FilterType::Triangle);
    }
    let rgba = img.to_rgba8();
    Ok(ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
}

fn ext_of(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "png",
    }
}

#[cfg(windows)]
fn ctrl_v_down() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL};
    // GetKeyState — состояние на момент обрабатываемого сообщения окна (тот же поток, что и UI)
    let down = |vk: i32| unsafe { GetKeyState(vk) } as u16 & 0x8000 != 0;
    down(VK_CONTROL as i32) && down(0x56)
}

#[cfg(not(windows))]
fn ctrl_v_down() -> bool {
    false
}

// ------------------------------------------------------------ мелочи

/// Вставить/обновить сообщения, сохраняя порядок по id.
fn merge(list: &mut Vec<Message>, new: Vec<Message>) {
    for m in new {
        match list.binary_search_by_key(&m.id, |x| x.id) {
            Ok(i) => list[i] = m,
            Err(i) => list.insert(i, m),
        }
    }
}

fn err_text(e: &ApiError) -> String {
    if e.code == "validation" && !e.detail.is_empty() {
        let mut c = e.detail.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    } else {
        e.message.clone()
    }
}

fn local(ts: i64) -> chrono::DateTime<Local> {
    Local.timestamp_opt(ts, 0).single().unwrap_or_else(Local::now)
}

fn day_of(ts: i64) -> chrono::NaiveDate {
    local(ts).date_naive()
}

const MONTHS: [&str; 12] =
    ["января", "февраля", "марта", "апреля", "мая", "июня", "июля", "августа", "сентября", "октября", "ноября", "декабря"];

fn day_label(ts: i64) -> String {
    let d = day_of(ts);
    let today = Local::now().date_naive();
    if d == today {
        "Сегодня".into()
    } else if Some(d) == today.pred_opt() {
        "Вчера".into()
    } else if d.year() == today.year() {
        format!("{} {}", d.day(), MONTHS[d.month0() as usize])
    } else {
        format!("{} {} {}", d.day(), MONTHS[d.month0() as usize], d.year())
    }
}

/// «14:05» (с днём, если не сегодня и `with_day`).
fn time_label(ts: i64, with_day: bool) -> String {
    let dt = local(ts);
    let hm = dt.format("%H:%M").to_string();
    if !with_day || dt.date_naive() == Local::now().date_naive() {
        hm
    } else if Some(dt.date_naive()) == Local::now().date_naive().pred_opt() {
        format!("вчера в {hm}")
    } else {
        format!("{}.{:02}.{} {hm}", dt.day(), dt.month(), dt.year())
    }
}

fn human_size(n: usize) -> String {
    if n >= 1024 * 1024 {
        format!("{:.1} МБ", n as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} КБ", n.div_ceil(1024))
    }
}

fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { s.chars().take(max - 1).collect::<String>() + "…" }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: i64) -> Message {
        Message {
            id,
            channel_id: 1,
            author: proto::Author { id: 1, name: "a".into() },
            content: format!("m{id}"),
            created_at: 0,
            edited_at: None,
            reply_to: None,
            attachments: vec![],
        }
    }

    #[test]
    fn merge_keeps_order_and_dedups() {
        let mut v = vec![msg(2), msg(5)];
        merge(&mut v, vec![msg(7), msg(1), msg(5), msg(3)]);
        assert_eq!(v.iter().map(|m| m.id).collect::<Vec<_>>(), [1, 2, 3, 5, 7]);
    }

    #[test]
    fn sse_parsing() {
        let mut buf = b": ping\n\ndata: {\"type\":\"hello\"}\n\ndata: {\"type\":\"mess".to_vec();
        let mut got = vec![];
        while let Some(end) = block_end(&buf) {
            let b: Vec<u8> = buf.drain(..end).collect();
            got.push(parse_block(&b));
        }
        assert_eq!(got, vec![None, Some(ServerEvent::Hello)]);
        assert!(block_end(&buf).is_none(), "неполный блок ждёт продолжения");
        buf.extend_from_slice(b"age_created\",\"message\":{\"id\":1,\"channel_id\":2,\"author\":{\"id\":3,\"name\":\"\xd1\x8f\"},\"content\":\"x\",\"created_at\":5}}\r\n\r\n");
        let end = block_end(&buf).unwrap();
        match parse_block(&buf[..end]) {
            Some(ServerEvent::MessageCreated { message }) => assert_eq!(message.author.name, "я"),
            e => panic!("{e:?}"),
        }
        assert_eq!(parse_block(b"event: x\ndata: {\"type\":\"unknown_future\"}\n\n"), None);
    }

    #[test]
    fn prepare_png_and_reject_garbage() {
        let img = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(300, 200, image::Rgba([255, 0, 0, 255])));
        let png = encode(&img, image::ImageFormat::Png).unwrap();
        let p = prepare(png.clone()).unwrap();
        assert_eq!(p.size, [300, 200]);
        assert_eq!(p.bytes, png, "маленький PNG уходит как есть");
        assert!(p.thumb.size[0] <= 160 && p.thumb.size[1] <= 160);
        assert!(prepare(b"not an image".to_vec()).is_err());
        // BMP → PNG
        let bmp = encode(&img, image::ImageFormat::Bmp);
        if let Ok(bmp) = bmp {
            let p = prepare(bmp).unwrap();
            assert!(p.bytes.starts_with(b"\x89PNG"));
        }
    }

    #[test]
    fn labels() {
        assert_eq!(human_size(1500), "2 КБ");
        assert_eq!(human_size(3 * 1024 * 1024), "3.0 МБ");
        let now = Local::now().timestamp();
        assert_eq!(day_label(now), "Сегодня");
        assert_eq!(day_label(now - 86_400), "Вчера");
    }
}
