//! Друзья: заявки по нику, список с онлайн-статусом (окно «Друзья» из колонки серверов).
//! Данные — `GET /friends`; события `friends_changed` (перечитать) и `presence` (в сети / нет) приходят в потоке `/events`.

use std::sync::mpsc as std_mpsc;

use eframe::egui::{self, Align, Align2, Layout, RichText, Sense, Ui, pos2, vec2};
use proto::{FriendsList, PublicUser, ServerEvent};

use crate::api::Api;
use crate::kit::{self, ButtonKind};
use crate::people::People;
use crate::theme as t;

enum Ev {
    List(Result<FriendsList, String>),
    /// Ответ на действие: новый список или ошибка (текст), `added` — это была заявка по нику.
    Done { res: Result<Option<FriendsList>, String>, added: Option<String> },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Online,
    All,
    Requests,
}

pub struct Friends {
    rt: tokio::runtime::Handle,
    ctx: egui::Context,
    api: Api,
    tx: std_mpsc::Sender<Ev>,
    rx: std_mpsc::Receiver<Ev>,
    list: FriendsList,
    loaded: bool,
    /// Список не загрузился (например, сервер ещё без друзей — 404).
    load_error: Option<String>,
    pub open: bool,
    tab: Tab,
    add_name: String,
    /// Итог последнего действия: Ok — подсказка («заявка отправлена»), Err — ошибка.
    status: Option<Result<String, String>>,
    busy: bool,
    /// Кого спросили «удалить из друзей?».
    confirm_remove: Option<i64>,
}

impl Friends {
    pub fn new(rt: tokio::runtime::Handle, ctx: egui::Context, api: Api) -> Self {
        let (tx, rx) = std_mpsc::channel();
        Self {
            rt,
            ctx,
            api,
            tx,
            rx,
            list: FriendsList::default(),
            loaded: false,
            load_error: None,
            open: false,
            tab: Tab::Online,
            add_name: String::new(),
            status: None,
            busy: false,
            confirm_remove: None,
        }
    }

    /// Новая сессия / сервер: всё забыть.
    pub fn set_api(&mut self, api: Api) {
        self.api = api;
        let (tx, rx) = std_mpsc::channel();
        self.tx = tx;
        self.rx = rx;
        self.list = FriendsList::default();
        self.loaded = false;
        self.load_error = None;
        self.open = false;
        self.status = None;
        self.busy = false;
    }

    fn spawn(&self, fut: impl std::future::Future<Output = Ev> + Send + 'static) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        self.rt.spawn(async move {
            let _ = tx.send(fut.await);
            ctx.request_repaint();
        });
    }

    /// Перечитать список (вход, переподключение потока, `friends_changed`).
    pub fn fetch(&self) {
        let api = self.api.clone();
        self.spawn(async move { Ev::List(api.friends().await.map_err(|e| e.message)) });
    }

    /// Событие потока.
    pub fn on_event(&mut self, e: &ServerEvent) {
        match e {
            ServerEvent::Hello | ServerEvent::Resync | ServerEvent::FriendsChanged => self.fetch(),
            ServerEvent::Presence { user_id, online } => {
                if let Some(f) = self.list.friends.iter_mut().find(|f| f.user.id == *user_id) {
                    f.online = *online;
                }
            }
            ServerEvent::UserUpdated { user } => {
                let fix = |u: &mut PublicUser| {
                    if u.id == user.id {
                        *u = user.clone();
                    }
                };
                self.list.friends.iter_mut().for_each(|f| fix(&mut f.user));
                self.list.incoming.iter_mut().chain(self.list.outgoing.iter_mut()).for_each(|r| fix(&mut r.user));
            }
            _ => {}
        }
    }

    /// Разобрать ответы. Возвращает true, если входящих заявок стало больше (звук).
    pub fn poll(&mut self, people: &mut People) -> bool {
        let before: Vec<i64> = self.list.incoming.iter().map(|r| r.user.id).collect();
        let was_loaded = self.loaded;
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Ev::List(Ok(l)) => {
                    self.load_error = None;
                    self.set_list(l, people);
                }
                Ev::List(Err(e)) => {
                    log::warn!("друзья: {e}");
                    self.load_error = Some(e);
                }
                Ev::Done { res, added } => {
                    self.busy = false;
                    match res {
                        Ok(l) => {
                            if let Some(l) = l {
                                self.set_list(l, people);
                            }
                            if let Some(name) = added {
                                let friend = self.list.friends.iter().any(|f| f.user.name.eq_ignore_ascii_case(&name));
                                self.status = Some(Ok(if friend {
                                    format!("{name} теперь в друзьях")
                                } else {
                                    format!("Заявка для {name} отправлена")
                                }));
                                self.add_name.clear();
                            }
                        }
                        Err(e) => self.status = Some(Err(e)),
                    }
                }
            }
        }
        // при первой загрузке старые заявки — не повод для звука
        was_loaded && self.list.incoming.iter().any(|r| !before.contains(&r.user.id))
    }

    fn set_list(&mut self, l: FriendsList, people: &mut People) {
        for u in l.friends.iter().map(|f| &f.user).chain(l.incoming.iter().map(|r| &r.user)).chain(l.outgoing.iter().map(|r| &r.user)) {
            people.learn(u.clone());
        }
        self.list = l;
        self.loaded = true;
    }

    /// Входящих заявок (бейдж на кнопке).
    pub fn incoming(&self) -> usize {
        self.list.incoming.len()
    }

    /// Друзья есть, и никого нет в сети — кот в заголовке спит.
    pub fn all_offline(&self) -> bool {
        !self.list.friends.is_empty() && self.list.friends.iter().all(|f| !f.online)
    }

    fn add(&mut self) {
        let name = self.add_name.trim().to_string();
        if name.is_empty() || self.busy {
            return;
        }
        self.busy = true;
        self.status = None;
        let api = self.api.clone();
        self.spawn(async move {
            let res = api.add_friend(&name).await.map(Some).map_err(|e| match e.code.as_str() {
                "not_found" => format!("Нет пользователя «{name}» на этом сервере"),
                "bad_request" => format!("{name} уже в друзьях"),
                _ => e.message,
            });
            Ev::Done { res, added: Some(name) }
        });
    }

    fn act(&mut self, accept: bool, id: i64) {
        self.busy = true;
        self.status = None;
        let api = self.api.clone();
        self.spawn(async move {
            let res = if accept {
                api.accept_friend(id).await.map(Some)
            } else {
                api.remove_friend(id).await.map(|_| None)
            };
            Ev::Done { res: res.map_err(|e| e.message), added: None }
        });
        if !accept {
            // сразу убрать из списка — сервер всё равно пришлёт friends_changed
            self.list.friends.retain(|f| f.user.id != id);
            self.list.incoming.retain(|r| r.user.id != id);
            self.list.outgoing.retain(|r| r.user.id != id);
        }
    }

    /// Отладка (скриншоты): открыть окно сразу, `requests` / `all` — на этой вкладке.
    #[cfg(debug_assertions)]
    pub fn debug_open(&mut self, tab: &str) {
        self.open = true;
        self.tab = match tab {
            "requests" => Tab::Requests,
            "all" => Tab::All,
            _ => Tab::Online,
        };
    }

    /// Окно «Друзья».
    pub fn ui(&mut self, ctx: &egui::Context, people: &People) {
        if !self.open {
            return;
        }
        let res = kit::modal(ctx, "friends", "Друзья", 480.0, |ui| self.body(ui, people));
        if res == kit::ModalResult::Closed {
            self.open = false;
            self.confirm_remove = None;
            self.status = None;
        }
    }

    fn body(&mut self, ui: &mut Ui, people: &People) {
        let p = t::p();
        // добавить по нику
        ui.label(RichText::new("Добавить по нику").font(t::semibold(13.0)).color(p.text_muted));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let r = ui.add_enabled(!self.busy, kit::text_field(&mut self.add_name, 340.0).hint_text("логин друга на этом сервере").char_limit(proto::limits::NAME_MAX));
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let ok = !self.add_name.trim().is_empty() && !self.busy;
            if kit::button_ex(ui, ButtonKind::Primary, "Отправить", None, ok).clicked() || (enter && ok) {
                self.add();
            }
        });
        match &self.status {
            Some(Ok(s)) => {
                ui.label(RichText::new(s).size(13.0).color(p.accent_text));
            }
            Some(Err(e)) => {
                ui.label(RichText::new(format!("{}  {e}", egui_phosphor::regular::WARNING_CIRCLE)).size(13.0).color(p.danger_text));
            }
            None => {}
        }
        ui.add_space(14.0);

        let online = self.list.friends.iter().filter(|f| f.online).count();
        let req = self.list.incoming.len() + self.list.outgoing.len();
        let l_online = format!("В сети · {online}");
        let l_all = format!("Все · {}", self.list.friends.len());
        let l_req = if self.list.incoming.is_empty() { format!("Заявки · {req}") } else { format!("Заявки · {}", t::plural(self.list.incoming.len(), "новая", "новые", "новых")) };
        kit::segmented(ui, &mut self.tab, &[(Tab::Online, &l_online), (Tab::All, &l_all), (Tab::Requests, &l_req)]);
        ui.add_space(10.0);

        // постоянная высота списка — окно не «прыгает» между вкладками
        let h = (ui.ctx().content_rect().height() - 340.0).clamp(200.0, 380.0);
        egui::ScrollArea::vertical().max_height(h).min_scrolled_height(h).auto_shrink([false, false]).show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !self.loaded {
                match &self.load_error {
                    Some(e) => {
                        let sub = if e == "Не найдено." { "Сервер ещё не умеет друзей — нужно обновить бэкенд".to_string() } else { e.clone() };
                        if kit::cat_state(ui, kit::Cat::Offline, "Друзья недоступны", &sub, Some("Повторить")) {
                            self.load_error = None;
                            self.fetch();
                        }
                    }
                    None => {
                        kit::cat_state(ui, kit::Cat::Loading, "Загружаем…", "Ещё секунду", None);
                    }
                }
                return;
            }
            match self.tab {
                Tab::Online | Tab::All => {
                    let only_online = self.tab == Tab::Online;
                    let list: Vec<_> = self.list.friends.iter().filter(|f| !only_online || f.online).cloned().collect();
                    if list.is_empty() {
                        ui.add_space(8.0);
                        if self.list.friends.is_empty() {
                            kit::cat_state(ui, kit::Cat::Peek, "Друзей пока нет", "Добавь кого-нибудь по нику", None);
                        } else {
                            kit::cat_state(ui, kit::Cat::Sleep, "Все спят", "Никого из друзей нет в сети", None);
                        }
                    }
                    // онлайн — выше
                    let mut list = list;
                    list.sort_by_key(|f| !f.online);
                    for f in list {
                        self.friend_row(ui, people, &f.user, Some(f.online));
                    }
                }
                Tab::Requests => {
                    if self.list.incoming.is_empty() && self.list.outgoing.is_empty() {
                        ui.add_space(8.0);
                        kit::cat_state(ui, kit::Cat::Sleep, "Заявок нет", "Новые появятся здесь", None);
                    }
                    if !self.list.incoming.is_empty() {
                        kit::group_label(ui, "Входящие");
                        for r in self.list.incoming.clone() {
                            self.request_row(ui, people, &r.user, true);
                        }
                    }
                    if !self.list.outgoing.is_empty() {
                        ui.add_space(6.0);
                        kit::group_label(ui, "Отправленные");
                        for r in self.list.outgoing.clone() {
                            self.request_row(ui, people, &r.user, false);
                        }
                    }
                }
            }
        });
    }

    /// Строка: аватар, имя и логин; `online` — точка статуса (для друзей).
    fn person(ui: &mut Ui, people: &People, u: &PublicUser, online: Option<bool>, sub: &str) {
        let (rect, _) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::hover());
        let p = t::p();
        people.paint(ui.painter(), rect.center(), 18.0, u, 0.0, online == Some(false));
        if let Some(on) = online {
            let c = rect.center() + vec2(13.0, 13.0);
            ui.painter().circle_filled(c, 6.5, p.surface_2);
            ui.painter().circle_filled(c, 4.5, if on { p.status_online } else { p.status_offline });
        }
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(2.0);
            ui.label(RichText::new(u.shown_name()).font(t::semibold(14.5)).color(p.text));
            let login = if u.display_name.as_deref().is_some_and(|d| !d.is_empty()) { format!("{} · {sub}", u.name) } else { sub.to_string() };
            ui.label(RichText::new(login).size(12.5).color(p.text_muted));
        });
    }

    fn friend_row(&mut self, ui: &mut Ui, people: &People, u: &PublicUser, online: Option<bool>) {
        ui.horizontal(|ui| {
            ui.set_min_height(48.0);
            let sub = if online == Some(true) { "в сети" } else { "не в сети" };
            Self::person(ui, people, u, online, sub);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.confirm_remove == Some(u.id) {
                    if kit::button(ui, ButtonKind::Ghost, "Нет", None).clicked() {
                        self.confirm_remove = None;
                    }
                    if kit::button(ui, ButtonKind::Danger, "Удалить", None).clicked() {
                        self.confirm_remove = None;
                        self.act(false, u.id);
                    }
                } else if kit::icon_button(ui, egui_phosphor::regular::USER_MINUS, kit::IconState::Normal, "Удалить из друзей").clicked() {
                    self.confirm_remove = Some(u.id);
                }
            });
        });
    }

    fn request_row(&mut self, ui: &mut Ui, people: &People, u: &PublicUser, incoming: bool) {
        ui.horizontal(|ui| {
            ui.set_min_height(48.0);
            Self::person(ui, people, u, None, if incoming { "хочет дружить" } else { "ждём ответа" });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let busy = self.busy;
                if incoming {
                    if kit::icon_button(ui, egui_phosphor::regular::X, kit::IconState::Normal, "Отклонить").clicked() && !busy {
                        self.act(false, u.id);
                    }
                    if kit::button_ex(ui, ButtonKind::Primary, "Принять", None, !busy).clicked() {
                        self.act(true, u.id);
                    }
                } else if kit::icon_button(ui, egui_phosphor::regular::X, kit::IconState::Normal, "Отменить заявку").clicked() && !busy {
                    self.act(false, u.id);
                }
            });
        });
    }
}

/// Кнопка «Друзья» в колонке серверов (значок людей, бейдж — входящие заявки). true — нажата.
pub fn rail_button(ui: &mut Ui, incoming: usize, open: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::click());
    let k = ui.ctx().animate_bool_with_time(resp.id, open || resp.hovered(), kit::HOVER_S * 1.5);
    let p = t::p();
    let r = egui::lerp(22.0..=14.0, k) as u8;
    let bg = if open { p.accent_strong } else { p.surface_3.lerp_to_gamma(p.row_active, k) };
    ui.painter().rect_filled(rect, egui::CornerRadius::same(r), bg);
    let fg = if open { p.on_accent } else { p.accent };
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, egui_phosphor::fill::USERS, t::icon_fill(21.0), fg);
    let resp = resp.on_hover_text("Друзья").on_hover_cursor(egui::CursorIcon::PointingHand);
    if incoming > 0 {
        let c = pos2(resp.rect.right() - 4.0, resp.rect.top() + 4.0);
        let text = if incoming > 9 { "9+".to_string() } else { incoming.to_string() };
        ui.painter().circle_filled(c, 9.5, p.rail);
        ui.painter().circle_filled(c, 8.0, p.accent_strong);
        ui.painter().text(c, Align2::CENTER_CENTER, text, t::xbold(10.5), p.on_accent);
    }
    resp.clicked()
}
