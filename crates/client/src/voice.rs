//! Голосовой движок: LiveKit-комната + платформенный ADM (WASAPI, AEC/NS/AGC из WebRTC).
//!
//! Работает в tokio-задаче. UI шлёт команды (`Cmd`) и читает общий снимок состояния
//! (`VoiceState`) — UI-поток никогда не ждёт сеть или звук.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use capture_win::{Event as CapEvent, SourceId};
use livekit::options::{AudioEncoding, TrackPublishOptions};
use livekit::webrtc::audio_source::native::NativeAudioSource;
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::prelude::*;
use livekit::webrtc::stats::RtcStats;
use livekit::{AudioProcessingOptions, PlatformAudio, PlayoutDeviceId, RecordingDeviceId};
use proto::Channel;
use tokio::sync::mpsc;

use crate::api::Api;
use crate::sounds::{self, Sound};
use crate::stream::{self, CaptureEvent, Preset};
use crate::video::VideoHub;

/// Задержка отпускания рации: хвост слова не обрезается.
const PTT_RELEASE_DELAY: Duration = Duration::from_millis(180);
const TICK: Duration = Duration::from_millis(80);
const STATS_EVERY: Duration = Duration::from_secs(2);
/// Паузы между попытками вернуться в канал после разрыва.
const REJOIN_BACKOFF: [u64; 6] = [1, 2, 3, 5, 10, 15];

#[derive(Debug, Clone)]
pub enum Cmd {
    Join(Channel),
    Leave,
    SetSelfMuted(bool),
    SetDeafened(bool),
    SetPttEnabled(bool),
    PttHeld(bool),
    /// identity → громкость (1.0 = 100%, 0 = заглушён).
    SetUserVolume(String, f32),
    /// identity → громкость звука его трансляции (1.0 = 100%).
    SetStreamVolume(String, f32),
    SetInputDevice(Option<String>),
    SetOutputDevice(Option<String>),
    SetProcessing { aec: bool, ns: bool, agc: bool },
    SetSounds(bool),
    RefreshDevices,
    /// Новый API-клиент (после входа/смены сервера).
    SetApi(Api),
    /// Начать трансляцию или сменить источник на лету (тот же трек, без переподключения).
    /// `audio` — передавать звук окна/системы (фаза 5).
    StartStream { source: SourceId, title: String, width: u32, height: u32, preset: Preset, audio: bool },
    StopStream,
    DismissStreamNotice,
    /// Смотреть / перестать смотреть трансляцию участника (подписка на её видео и звук).
    Watch { identity: String, on: bool },
    /// Где видна трансляция: размер на экране (физические пиксели) для SFU; невидимая — на паузе.
    StreamView { identity: String, visible: bool, width: u32, height: u32 },
    /// Отладочный оверлей F3 открыт/закрыт: открыт — метрики раз в 1 с в `VoiceState.metrics`.
    Metrics(bool),
    /// Вся сырая статистика WebRTC — в лог (один раз, на следующем опросе).
    DumpStats,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Conn {
    Idle,
    Connecting,
    Connected,
    /// SDK сам восстанавливает соединение (ICE/сигналинг).
    Reconnecting,
    /// Комната потеряна, заходим заново с новым токеном.
    Rejoining { attempt: usize },
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct Peer {
    pub identity: String,
    pub name: String,
    pub is_local: bool,
    pub speaking: bool,
    pub level: f32,
    pub mic_muted: bool,
    /// Транслирует экран/окно.
    pub streaming: bool,
}

#[derive(Debug, Clone)]
pub struct Device {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct VoiceState {
    pub conn: Conn,
    pub channel: Option<Channel>,
    pub peers: Vec<Peer>,
    /// RTT до LiveKit, секунды.
    pub rtt: Option<f64>,
    pub self_muted: bool,
    pub deafened: bool,
    pub ptt_enabled: bool,
    pub ptt_held: bool,
    /// Микрофон реально передаётся.
    pub transmitting: bool,
    pub inputs: Vec<Device>,
    pub outputs: Vec<Device>,
    pub audio_error: Option<String>,
    pub stream: Option<StreamInfo>,
    /// Почему трансляция остановилась сама (окно закрыто, ошибка).
    pub stream_notice: Option<String>,
    /// Чьи трансляции мы смотрим (identity, по алфавиту).
    pub watching: Vec<String>,
    /// Метрики для оверлея F3 (только пока он открыт и мы в канале).
    pub metrics: Option<crate::metrics::Metrics>,
}

#[derive(Debug, Clone)]
pub struct StreamInfo {
    pub source: SourceId,
    pub title: String,
    pub preset: Preset,
    pub minimized: bool,
    /// Размер отправляемого видео.
    pub size: Option<(u32, u32)>,
    /// Чем кодируется: «NVIDIA H.264 Encoder MFT (видеокарта)» / «OpenH264 (процессор)».
    pub backend: String,
    /// Из статистики WebRTC: реализация энкодера, к/с, кбит/с, ограничение качества.
    pub encoder: Option<String>,
    pub fps: f64,
    pub kbps: f64,
    pub limitation: Option<String>,
    pub audio: AudioStatus,
}

/// Звук трансляции (для интерфейса).
#[derive(Debug, Clone, PartialEq)]
pub enum AudioStatus {
    Off,
    /// Захватывается: «звук приложения» / «весь звук, кроме Amadeus».
    On(&'static str),
    Failed(String),
}

impl Default for VoiceState {
    fn default() -> Self {
        Self {
            conn: Conn::Idle,
            channel: None,
            peers: Vec::new(),
            rtt: None,
            self_muted: false,
            deafened: false,
            ptt_enabled: false,
            ptt_held: false,
            transmitting: false,
            inputs: Vec::new(),
            outputs: Vec::new(),
            audio_error: None,
            stream: None,
            stream_notice: None,
            watching: Vec::new(),
            metrics: None,
        }
    }
}

pub type Shared = Arc<Mutex<VoiceState>>;

#[derive(Clone)]
pub struct VoiceHandle {
    tx: mpsc::UnboundedSender<Cmd>,
    pub state: Shared,
}

impl VoiceHandle {
    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }

    pub fn sender(&self) -> mpsc::UnboundedSender<Cmd> {
        self.tx.clone()
    }

    pub fn snapshot(&self) -> VoiceState {
        self.state.lock().unwrap().clone()
    }
}

/// Начальные параметры звука (из настроек).
#[derive(Debug, Clone)]
pub struct AudioPrefs {
    pub input: Option<String>,
    pub output: Option<String>,
    pub aec: bool,
    pub ns: bool,
    pub agc: bool,
    pub ptt_enabled: bool,
    pub sounds: bool,
    pub volumes: HashMap<String, f32>,
    /// Громкость звука трансляций по identity.
    pub stream_volumes: HashMap<String, f32>,
}

pub fn spawn(
    rt: &tokio::runtime::Handle,
    api: Api,
    prefs: AudioPrefs,
    video: VideoHub,
    repaint: Arc<dyn Fn() + Send + Sync>,
) -> VoiceHandle {
    let (tx, rx) = mpsc::unbounded_channel();
    let (cap_tx, cap_rx) = mpsc::unbounded_channel();
    let state: Shared = Arc::new(Mutex::new(VoiceState { ptt_enabled: prefs.ptt_enabled, ..Default::default() }));
    let engine = Engine {
        api,
        state: state.clone(),
        repaint,
        audio: None,
        room: None,
        mic: None,
        prefs,
        self_muted: false,
        deafened: false,
        ptt_held: false,
        ptt_release_at: None,
        rejoin: None,
        last_stats: Instant::now() - STATS_EVERY,
        last_peers: HashMap::new(),
        stream: None,
        cap_tx,
        generation: 0,
        last_video_bytes: None,
        stats_n: 0,
        video,
        watching: HashSet::new(),
        receivers: HashMap::new(),
        views: HashMap::new(),
        overlay: false,
        counters: Default::default(),
        metrics_at: None,
        dump_stats: false,
        force_relay: std::env::var("AMADEUS_FORCE_RELAY").is_ok_and(|v| v == "1"),
    };
    rt.spawn(engine.run(rx, cap_rx));
    VoiceHandle { tx, state }
}

struct Rejoin {
    channel: Channel,
    attempt: usize,
    at: Instant,
}

struct Engine {
    api: Api,
    state: Shared,
    repaint: Arc<dyn Fn() + Send + Sync>,
    audio: Option<PlatformAudio>,
    room: Option<(Room, mpsc::UnboundedReceiver<RoomEvent>)>,
    mic: Option<LocalTrackPublication>,
    prefs: AudioPrefs,
    self_muted: bool,
    deafened: bool,
    ptt_held: bool,
    ptt_release_at: Option<Instant>,
    rejoin: Option<Rejoin>,
    last_stats: Instant,
    /// identity → имя: для звуков входа/выхода и сравнения состава.
    last_peers: HashMap<String, String>,
    stream: Option<Streaming>,
    cap_tx: mpsc::UnboundedSender<CaptureEvent>,
    /// Поколение захвата: растёт при каждом старте/смене источника.
    generation: u64,
    last_video_bytes: Option<(u64, Instant)>,
    stats_n: u64,
    /// Текстуры просматриваемых трансляций (общие с UI).
    video: VideoHub,
    /// Чьи трансляции смотрим; переживает переподключение, сбрасывается при выходе/смене канала.
    watching: HashSet<String>,
    /// Задачи приёма кадров по identity.
    receivers: HashMap<String, tokio::task::JoinHandle<()>>,
    /// Последний вид из UI: (видна, ширина, высота) — повторно применяется после переподписки.
    views: HashMap<String, (bool, u32, u32)>,
    /// Оверлей F3 открыт.
    overlay: bool,
    counters: crate::metrics::Counters,
    /// Время прошлого опроса для метрик (интервал для скоростей).
    metrics_at: Option<Instant>,
    dump_stats: bool,
    /// `AMADEUS_FORCE_RELAY=1`: только через TURN (проверка запасного пути).
    force_relay: bool,
}

/// Идущая трансляция. Захват и источник живут дольше публикации: при переподключении
/// трек публикуется заново в новую комнату из того же источника.
struct Streaming {
    capture: Option<stream::AnyCapture>,
    source: NativeVideoSource,
    preset: Preset,
    /// Аппаратный путь (pre-encoded H.264).
    hw: bool,
    publication: Option<LocalTrackPublication>,
    /// Звук окна/системы (фаза 5); None — не передаётся.
    audio: Option<StreamSound>,
}

/// Звук трансляции: источник живёт дольше публикации (переподключение), захват меняется со сменой окна.
struct StreamSound {
    capture: Option<capture_win::AppAudio>,
    source: NativeAudioSource,
    publication: Option<LocalTrackPublication>,
}

/// Понятный текст ошибки входа в канал (подробности — в логе).
fn humanize_join_error(e: &anyhow::Error) -> String {
    if let Some(api) = e.downcast_ref::<crate::api::ApiError>() {
        return format!("Не удалось войти в канал: {}", api.message);
    }
    let raw = format!("{e:#}");
    let low = raw.to_lowercase();
    let what = if low.contains("401") || low.contains("invalid api key") || low.contains("unauthorized") {
        "сервер голоса отклонил доступ (ключи LiveKit на бэкенде не совпадают с сервером голоса)"
    } else if low.contains("timeout") || low.contains("timed out") {
        "сервер голоса не отвечает — проверьте интернет"
    } else if low.contains("dns") || low.contains("resolve") || low.contains("connect") {
        "нет связи с сервером голоса"
    } else {
        "ошибка подключения к серверу голоса"
    };
    format!("Не удалось войти в канал: {what}.")
}

async fn next_event(room: &mut Option<(Room, mpsc::UnboundedReceiver<RoomEvent>)>) -> Option<RoomEvent> {
    match room {
        Some((_, rx)) => rx.recv().await,
        None => std::future::pending().await,
    }
}

impl Engine {
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Cmd>, mut cap_rx: mpsc::UnboundedReceiver<CaptureEvent>) {
        self.init_audio();
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                cmd = rx.recv() => match cmd {
                    Some(cmd) => self.on_cmd(cmd).await,
                    None => break,
                },
                ev = next_event(&mut self.room) => match ev {
                    Some(ev) => self.on_room_event(ev).await,
                    None => {
                        // Канал событий закрыт — комната уничтожена.
                        self.drop_room();
                        self.lost_connection("соединение закрыто");
                    }
                },
                Some(ev) = cap_rx.recv() => self.on_capture_event(ev).await,
                _ = tick.tick() => self.on_tick().await,
            }
        }
        self.stop_stream().await;
        self.leave().await;
    }

    fn set(&self, f: impl FnOnce(&mut VoiceState)) {
        f(&mut self.state.lock().unwrap());
        (self.repaint)();
    }

    // ---------------------------------------------------------------- звук

    fn init_audio(&mut self) {
        match PlatformAudio::new() {
            Ok(audio) => {
                self.audio = Some(audio);
                self.apply_processing();
                // None = системное по умолчанию: ADM и так стартует на нём, переключать не нужно.
                if let Some(inp) = self.prefs.input.clone() {
                    self.apply_input(Some(inp));
                }
                if let Some(out) = self.prefs.output.clone() {
                    self.apply_output(Some(out));
                }
                self.refresh_devices();
            }
            Err(e) => {
                log::error!("PlatformAudio: {e}");
                self.set(|s| s.audio_error = Some(format!("Звуковые устройства недоступны: {e}")));
            }
        }
    }

    fn refresh_devices(&self) {
        let Some(audio) = &self.audio else { return };
        let inputs: Vec<Device> =
            audio.recording_devices().map(|d| Device { id: d.id.as_str().to_string(), name: d.name }).collect();
        let outputs: Vec<Device> =
            audio.playout_devices().map(|d| Device { id: d.id.as_str().to_string(), name: d.name }).collect();
        self.set(|s| {
            s.inputs = inputs;
            s.outputs = outputs;
        });
    }

    fn apply_processing(&self) {
        let Some(audio) = &self.audio else { return };
        let opts = AudioProcessingOptions {
            echo_cancellation: self.prefs.aec,
            noise_suppression: self.prefs.ns,
            auto_gain_control: self.prefs.agc,
            ..Default::default()
        };
        if let Err(e) = audio.configure_audio_processing(opts) {
            log::warn!("настройка AEC/NS/AGC: {e}");
        }
    }

    fn apply_input(&mut self, id: Option<String>) {
        self.prefs.input = id.clone();
        let Some(audio) = &self.audio else { return };
        let res = match &id {
            // switch_* работает и во время записи (горячая смена устройства).
            Some(guid) => audio.switch_recording_device(&RecordingDeviceId::from_unchecked_guid(guid)),
            None => audio.switch_recording_device_by_index(0),
        };
        if let Err(e) = res {
            log::warn!("микрофон {id:?}: {e}");
        }
    }

    fn apply_output(&mut self, id: Option<String>) {
        self.prefs.output = id.clone();
        let Some(audio) = &self.audio else { return };
        let res = match &id {
            Some(guid) => audio.switch_playout_device(&PlayoutDeviceId::from_unchecked_guid(guid)),
            None => audio.switch_playout_device_by_index(0),
        };
        if let Err(e) = res {
            log::warn!("вывод {id:?}: {e}");
        }
    }

    // ---------------------------------------------------------------- команды

    async fn on_cmd(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Join(ch) => {
                self.rejoin = None;
                self.stop_stream().await;
                self.leave().await;
                self.forget_watching();
                self.join(ch, 0).await;
            }
            Cmd::Leave => {
                self.rejoin = None;
                let was_in = self.room.is_some();
                self.stop_stream().await;
                self.leave().await;
                self.forget_watching();
                if was_in {
                    self.play(Sound::LeaveSelf);
                }
                self.set(|s| {
                    s.conn = Conn::Idle;
                    s.channel = None;
                    s.peers.clear();
                    s.rtt = None;
                });
            }
            Cmd::SetSelfMuted(m) => {
                self.self_muted = m;
                if !m && self.deafened {
                    // Включить микрофон в «наушниках выкл» = снять и глушение звука (как в Discord).
                    self.deafened = false;
                    self.apply_volumes();
                }
                self.apply_mic();
            }
            Cmd::SetDeafened(d) => {
                self.deafened = d;
                self.apply_volumes();
                self.apply_mic();
            }
            Cmd::SetPttEnabled(on) => {
                self.prefs.ptt_enabled = on;
                self.ptt_held = false;
                self.ptt_release_at = None;
                self.set(|s| s.ptt_enabled = on);
                self.apply_mic();
            }
            Cmd::PttHeld(held) => {
                if held {
                    self.ptt_release_at = None;
                    if !self.ptt_held {
                        self.ptt_held = true;
                        self.apply_mic();
                    }
                } else if self.ptt_held {
                    self.ptt_release_at = Some(Instant::now() + PTT_RELEASE_DELAY);
                }
            }
            Cmd::SetUserVolume(id, v) => {
                self.prefs.volumes.insert(id.clone(), v);
                self.apply_volume_for(&id);
            }
            Cmd::SetStreamVolume(id, v) => {
                self.prefs.stream_volumes.insert(id.clone(), v);
                self.apply_volume_for(&id);
            }
            Cmd::SetInputDevice(id) => self.apply_input(id),
            Cmd::SetOutputDevice(id) => self.apply_output(id),
            Cmd::SetProcessing { aec, ns, agc } => {
                self.prefs.aec = aec;
                self.prefs.ns = ns;
                self.prefs.agc = agc;
                self.apply_processing();
            }
            Cmd::SetSounds(on) => self.prefs.sounds = on,
            Cmd::RefreshDevices => self.refresh_devices(),
            Cmd::SetApi(api) => self.api = api,
            Cmd::StartStream { source, title, width, height, preset, audio } => {
                self.start_stream(source, title, width, height, preset, audio).await
            }
            Cmd::StopStream => self.stop_stream().await,
            Cmd::DismissStreamNotice => self.set(|s| s.stream_notice = None),
            Cmd::Metrics(on) => {
                self.overlay = on;
                self.counters = Default::default();
                self.metrics_at = None;
                // Сразу опросить на следующем тике.
                self.last_stats = Instant::now() - STATS_EVERY;
                if !on {
                    self.set(|s| s.metrics = None);
                }
            }
            Cmd::DumpStats => self.dump_stats = true,
            Cmd::Watch { identity, on } => self.set_watching(&identity, on),
            Cmd::StreamView { identity, visible, width, height } => {
                self.views.insert(identity.clone(), (visible, width, height));
                self.apply_view(&identity);
            }
        }
    }

    fn play(&self, s: Sound) {
        if self.prefs.sounds {
            sounds::play(s);
        }
    }

    // ---------------------------------------------------------------- комната

    async fn join(&mut self, ch: Channel, attempt: usize) {
        self.set(|s| {
            s.conn = if attempt == 0 { Conn::Connecting } else { Conn::Rejoining { attempt } };
            s.channel = Some(ch.clone());
            s.peers.clear();
            s.rtt = None;
        });
        match self.try_join(&ch).await {
            Ok(()) => {
                self.rejoin = None;
                if attempt == 0 {
                    self.play(Sound::JoinSelf);
                }
                self.set(|s| s.conn = Conn::Connected);
                self.refresh_peers(false);
            }
            Err(e) => {
                log::warn!("вход в канал {} (попытка {attempt}): {e:#}", ch.id);
                self.drop_room();
                let unauthorized = e.downcast_ref::<crate::api::ApiError>().is_some_and(|a| a.is_unauthorized());
                if attempt == 0 || unauthorized {
                    let msg = humanize_join_error(&e);
                    self.set(|s| {
                        s.conn = Conn::Failed(msg);
                        s.peers.clear();
                    });
                } else {
                    self.schedule_rejoin(ch, attempt);
                }
            }
        }
    }

    async fn try_join(&mut self, ch: &Channel) -> anyhow::Result<()> {
        let join = self.api.join(ch.id).await?;
        let mut opts = RoomOptions::default();
        // Подписки — вручную: голос всегда, трансляции (видео + звук) — только по «Смотреть» (канал зрителя не тратится).
        opts.auto_subscribe = false;
        opts.adaptive_stream = true;
        opts.dynacast = true;
        if self.force_relay {
            log::warn!("AMADEUS_FORCE_RELAY=1: подключение только через TURN");
            opts.rtc_config.ice_transport_type = livekit::webrtc::prelude::IceTransportsType::Relay;
        }
        let (room, events) = Room::connect(&join.livekit_url, &join.livekit_token, opts).await?;

        if self.audio.is_none() {
            self.init_audio();
        }
        let mut mic = None;
        if let Some(audio) = &self.audio {
            let track = LocalAudioTrack::create_audio_track("microphone", audio.rtc_source());
            let publish = TrackPublishOptions {
                source: TrackSource::Microphone,
                // 128 кбит/с: максимум для голоса (Opus, фуллбэнд 48 кГц) — чище и «ярче» 64; трафик всё равно мал.
                // DTX выключен: на паузах он даёт «рацию»/комфортный шум и режет тихие начала слов.
                audio_encoding: Some(AudioEncoding { max_bitrate: 128_000 }),
                dtx: false,
                red: true,
                ..Default::default()
            };
            match room.local_participant().publish_track(LocalTrack::Audio(track), publish).await {
                Ok(p) => mic = Some(p),
                Err(e) => {
                    log::error!("публикация микрофона: {e}");
                    self.set(|s| s.audio_error = Some(format!("Микрофон не опубликован: {e}")));
                }
            }
        }
        // Переподключение во время трансляции: тот же источник — в новую комнату.
        if let Some(st) = &mut self.stream {
            match stream::publish(&room, &st.source, st.preset, st.hw).await {
                Ok(p) => st.publication = Some(p),
                Err(e) => log::error!("повторная публикация трансляции: {e}"),
            }
            if let Some(a) = &mut st.audio {
                match stream::publish_audio(&room, &a.source).await {
                    Ok(p) => a.publication = Some(p),
                    Err(e) => log::error!("повторная публикация звука трансляции: {e}"),
                }
            }
        }
        self.room = Some((room, events));
        self.sync_subscriptions();
        self.mic = mic;
        self.last_peers.clear();
        self.apply_mic();
        self.apply_volumes();
        Ok(())
    }

    fn drop_room(&mut self) {
        self.stop_receivers();
        self.mic = None;
        if let Some(st) = &mut self.stream {
            st.publication = None;
            if let Some(a) = &mut st.audio {
                a.publication = None;
            }
        }
        self.room = None;
    }

    async fn leave(&mut self) {
        log::info!("выход: начало (комната: {})", self.room.is_some());
        self.stop_receivers();
        log::info!("выход: приёмники видео остановлены");
        self.mic = None;
        log::info!("выход: микрофонный трек отпущен");
        if let Some(st) = &mut self.stream {
            st.publication = None;
            if let Some(a) = &mut st.audio {
                a.publication = None;
            }
        }
        if let Some((room, _)) = self.room.take()
            && let Err(e) = room.close().await {
                log::warn!("закрытие комнаты: {e}");
            }
        log::info!("выход: комната закрыта");
        self.last_peers.clear();
        self.counters = Default::default();
        self.metrics_at = None;
        self.set(|s| s.metrics = None);
    }

    fn lost_connection(&mut self, why: &str) {
        let ch = self.state.lock().unwrap().channel.clone();
        log::warn!("связь с комнатой потеряна: {why}");
        match ch {
            Some(ch) => self.schedule_rejoin(ch, 0),
            None => self.set(|s| s.conn = Conn::Idle),
        }
    }

    fn schedule_rejoin(&mut self, channel: Channel, prev_attempt: usize) {
        let attempt = prev_attempt + 1;
        let wait = REJOIN_BACKOFF[(attempt - 1).min(REJOIN_BACKOFF.len() - 1)];
        self.rejoin = Some(Rejoin { channel, attempt, at: Instant::now() + Duration::from_secs(wait) });
        self.set(|s| {
            s.conn = Conn::Rejoining { attempt };
            s.rtt = None;
        });
    }

    async fn on_room_event(&mut self, ev: RoomEvent) {
        match ev {
            RoomEvent::ParticipantConnected(p) => {
                log::info!("+ {} ({})", p.name(), p.identity());
                self.play(Sound::JoinOther);
                self.refresh_peers(false);
            }
            RoomEvent::ParticipantDisconnected(p) => {
                log::info!("- {} ({})", p.name(), p.identity());
                self.set_watching(p.identity().as_str(), false);
                self.play(Sound::LeaveOther);
                self.refresh_peers(false);
            }
            RoomEvent::TrackSubscribed { track, publication, participant } => {
                match track {
                    RemoteTrack::Audio(a) => {
                        let v = self.volume_for(participant.identity().as_str(), publication.source());
                        log::info!("подписка на аудио {} {:?} (громкость {v})", participant.identity(), publication.source());
                        a.rtc_track().set_volume(v as f64);
                    }
                    RemoteTrack::Video(v) => {
                        let id = participant.identity().as_str().to_string();
                        if publication.source() == TrackSource::Screenshare && self.watching.contains(&id) {
                            log::info!("смотрим трансляцию {id}");
                            if let Some(old) = self.receivers.insert(id.clone(), self.video.spawn_receiver(id.clone(), v)) {
                                old.abort();
                            }
                            self.apply_view(&id);
                        } else {
                            // Пришло то, что уже не нужно (успели нажать «Перестать смотреть»).
                            publication.set_subscribed(false);
                        }
                    }
                }
                self.refresh_peers(false);
            }
            RoomEvent::TrackPublished { publication, participant } => {
                log::info!("{} опубликовал {:?}", participant.identity(), publication.source());
                self.sync_subscription(participant.identity().as_str(), &publication);
                self.refresh_peers(false);
            }
            RoomEvent::TrackUnpublished { publication, participant } => {
                log::info!("{} снял {:?}", participant.identity(), publication.source());
                if publication.source() == TrackSource::Screenshare {
                    // Трансляция закончилась — перестаём «смотреть» (новую стример покажет заново, как в Discord).
                    self.set_watching(participant.identity().as_str(), false);
                }
                self.refresh_peers(false);
            }
            RoomEvent::TrackUnsubscribed { publication, participant, .. } => {
                if publication.source() == TrackSource::Screenshare {
                    self.stop_receiver(participant.identity().as_str());
                }
                self.refresh_peers(false);
            }
            RoomEvent::TrackMuted { .. }
            | RoomEvent::TrackUnmuted { .. }
            | RoomEvent::ActiveSpeakersChanged { .. }
            | RoomEvent::ParticipantNameChanged { .. } => self.refresh_peers(false),
            RoomEvent::Reconnecting => self.set(|s| s.conn = Conn::Reconnecting),
            RoomEvent::Reconnected => {
                self.set(|s| s.conn = Conn::Connected);
                self.apply_mic();
                self.apply_volumes();
                self.refresh_peers(false);
            }
            RoomEvent::Disconnected { reason } => {
                log::warn!("комната: Disconnected {reason:?}");
                self.drop_room();
                if reason == livekit::DisconnectReason::ClientInitiated {
                    self.stop_stream().await;
                    self.set(|s| s.conn = Conn::Idle);
                } else if reason == livekit::DisconnectReason::DuplicateIdentity {
                    self.stop_stream().await;
                    self.set(|s| {
                        s.conn = Conn::Failed("Вы зашли в канал с другого устройства.".into());
                        s.peers.clear();
                    });
                } else {
                    self.lost_connection(&format!("{reason:?}"));
                }
            }
            _ => {}
        }
    }

    async fn on_tick(&mut self) {
        if let Some(at) = self.ptt_release_at
            && Instant::now() >= at {
                self.ptt_release_at = None;
                self.ptt_held = false;
                self.apply_mic();
            }
        if let Some(rj) = &self.rejoin
            && Instant::now() >= rj.at {
                let (ch, attempt) = (rj.channel.clone(), rj.attempt);
                self.rejoin = None;
                self.leave().await;
                self.join(ch, attempt).await;
            }
        if self.room.is_some() {
            self.refresh_peers(true);
            let every = if self.overlay { Duration::from_secs(1) } else { STATS_EVERY };
            if self.last_stats.elapsed() >= every {
                self.last_stats = Instant::now();
                self.update_rtt().await;
            }
        }
    }

    async fn update_rtt(&mut self) {
        let Some((room, _)) = &self.room else { return };
        let Ok(stats) = room.get_stats().await else { return };
        self.stats_n += 1;
        if self.dump_stats {
            self.dump_stats = false;
            log::info!("статистика WebRTC (publisher): {:#?}", stats.publisher_stats);
            log::info!("статистика WebRTC (subscriber): {:#?}", stats.subscriber_stats);
        }
        if self.overlay {
            let now = Instant::now();
            let dt = self.metrics_at.map_or(1.0, |t| now.duration_since(t).as_secs_f64());
            self.metrics_at = Some(now);
            let all: Vec<RtcStats> = stats.publisher_stats.iter().chain(stats.subscriber_stats.iter()).cloned().collect();
            let labels = self.track_labels();
            let mut m = crate::metrics::collect(&all, &labels, &mut self.counters, dt, self.force_relay);
            self.set(|s| {
                // Pre-encoded: libwebrtc видит только «passthrough» — подставляем настоящий энкодер.
                if let Some(info) = &s.stream {
                    for r in m.rows.iter_mut().filter(|r| r.dir == crate::metrics::Dir::Out && r.kind == "video") {
                        r.detail = r.detail.replacen("LiveKit pre-encoded passthrough", &info.backend, 1);
                    }
                }
                s.metrics = Some(m);
            });
        }
        self.update_video_stats(&stats.publisher_stats);
        // Одна PeerConnection на всё (LiveKit 1.13) — входящие тоже в publisher_stats.
        self.log_inbound_video(&stats.subscriber_stats);
        self.log_inbound_video(&stats.publisher_stats);
        self.log_audio(&stats.publisher_stats);
        let rtt = stats
            .publisher_stats
            .iter()
            .chain(stats.subscriber_stats.iter())
            .filter_map(|s| match s {
                RtcStats::CandidatePair(cp) if cp.candidate_pair.nominated && cp.candidate_pair.current_round_trip_time > 0.0 => {
                    Some(cp.candidate_pair.current_round_trip_time)
                }
                _ => None,
            })
            .reduce(f64::min);
        if rtt.is_some() {
            log::debug!("rtt {rtt:?}");
            self.set(|s| s.rtt = rtt);
        }
    }

    // ---------------------------------------------------------------- трансляция

    async fn start_stream(&mut self, source: SourceId, title: String, width: u32, height: u32, preset: Preset, audio: bool) {
        if self.room.is_none() {
            return;
        }
        self.set(|s| s.stream_notice = None);
        // Тот же пресет — меняем только источник: трек и публикация остаются (зрители не переподключаются).
        if let Some(mut st) = self.stream.take() {
            if st.preset == preset && st.publication.is_some() {
                if let Some(c) = st.capture.take() {
                    tokio::task::block_in_place(|| c.stop());
                }
                self.generation += 1;
                self.last_video_bytes = None;
                let r = tokio::task::block_in_place(|| stream::restart(source, preset, &st.source, st.hw, self.generation, &self.cap_tx));
                match r {
                    Ok(c) => {
                        st.capture = Some(c);
                        self.stream = Some(st);
                        log::info!("трансляция: источник сменён на {source:?}");
                        self.set(|s| {
                            if let Some(info) = &mut s.stream {
                                info.source = source;
                                info.title = title;
                                info.minimized = false;
                                info.size = None;
                            }
                        });
                        self.set_stream_audio(source, audio).await;
                        return;
                    }
                    Err(e) => {
                        // Например, аппаратный энкодер не поднялся на новом источнике — начинаем заново.
                        log::warn!("смена источника на лету: {e} — перезапуск трансляции");
                    }
                }
            }
            self.stream = Some(st);
            self.stop_stream().await;
        }

        self.generation += 1;
        self.last_video_bytes = None;
        let started = match tokio::task::block_in_place(|| stream::start(source, width, height, preset, self.generation, &self.cap_tx)) {
            Ok(s) => s,
            Err(e) => {
                log::error!("трансляция: {e}");
                self.set(|s| s.stream_notice = Some(e));
                return;
            }
        };
        let Some((room, _)) = &self.room else { return };
        let publication = match stream::publish(room, &started.video, preset, started.hw).await {
            Ok(p) => p,
            Err(e) => {
                log::error!("публикация трансляции: {e}");
                tokio::task::block_in_place(|| started.capture.stop());
                self.set(|s| s.stream_notice = Some(format!("Не удалось начать трансляцию: {e}")));
                return;
            }
        };
        log::info!("трансляция: {source:?} «{title}», {preset:?}, энкодер {}", started.encoder);
        let backend = started.encoder.clone();
        self.stream = Some(Streaming {
            capture: Some(started.capture),
            source: started.video,
            preset,
            hw: started.hw,
            publication: Some(publication),
            audio: None,
        });
        self.set(|s| {
            s.stream = Some(StreamInfo {
                source,
                title,
                preset,
                minimized: false,
                size: None,
                backend,
                encoder: None,
                fps: 0.0,
                kbps: 0.0,
                limitation: None,
                audio: AudioStatus::Off,
            })
        });
        self.set_stream_audio(source, audio).await;
        self.refresh_peers(false);
    }

    /// Включает/выключает/перенацеливает звук трансляции. Ошибка звука трансляцию не останавливает.
    async fn set_stream_audio(&mut self, source: SourceId, want: bool) {
        let Some(st) = &mut self.stream else { return };
        let room = self.room.as_ref().map(|(r, _)| r);
        if let Some(c) = st.audio.as_mut().and_then(|a| a.capture.take()) {
            tokio::task::block_in_place(|| c.stop());
        }
        if !want {
            if let Some(a) = st.audio.take()
                && let (Some(p), Some(room)) = (a.publication, room)
                && let Err(e) = room.local_participant().unpublish_track(&p.sid()).await
            {
                log::warn!("снятие звука трансляции: {e}");
            }
            self.set(|s| {
                if let Some(i) = &mut s.stream {
                    i.audio = AudioStatus::Off;
                }
            });
            return;
        }
        let a = st.audio.get_or_insert_with(|| StreamSound { capture: None, source: stream::new_audio_source(), publication: None });
        let status = match tokio::task::block_in_place(|| stream::start_audio(source, &a.source)) {
            Ok(c) => {
                a.capture = Some(c);
                if a.publication.is_none()
                    && let Some(room) = room
                {
                    match stream::publish_audio(room, &a.source).await {
                        Ok(p) => a.publication = Some(p),
                        Err(e) => log::error!("публикация звука трансляции: {e}"),
                    }
                }
                AudioStatus::On(if source.is_window() { "звук приложения" } else { "весь звук, кроме Amadeus" })
            }
            Err(e) => {
                log::error!("звук трансляции: {e}");
                AudioStatus::Failed(e)
            }
        };
        self.set(|s| {
            if let Some(i) = &mut s.stream {
                i.audio = status;
            }
        });
    }

    async fn stop_stream(&mut self) {
        let Some(mut st) = self.stream.take() else { return };
        if let Some(c) = st.capture.take() {
            tokio::task::block_in_place(|| c.stop());
        }
        if let Some(mut a) = st.audio.take() {
            if let Some(c) = a.capture.take() {
                tokio::task::block_in_place(|| c.stop());
            }
            if let (Some(p), Some((room, _))) = (a.publication.take(), &self.room)
                && let Err(e) = room.local_participant().unpublish_track(&p.sid()).await
            {
                log::warn!("снятие звука трансляции: {e}");
            }
        }
        if let (Some(p), Some((room, _))) = (st.publication.take(), &self.room)
            && let Err(e) = room.local_participant().unpublish_track(&p.sid()).await {
                log::warn!("снятие трансляции: {e}");
            }
        self.generation += 1; // события остановленного захвата больше не нужны
        self.last_video_bytes = None;
        log::info!("трансляция остановлена");
        self.set(|s| s.stream = None);
        self.refresh_peers(false);
    }

    async fn on_capture_event(&mut self, ev: CaptureEvent) {
        if ev.generation != self.generation || self.stream.is_none() {
            return;
        }
        match ev.event {
            CapEvent::Started { width, height } | CapEvent::Resized { width, height } => self.set(|s| {
                if let Some(i) = &mut s.stream {
                    i.size = Some((width, height));
                }
            }),
            CapEvent::Minimized(m) => self.set(|s| {
                if let Some(i) = &mut s.stream {
                    i.minimized = m;
                }
            }),
            CapEvent::Closed => {
                self.stop_stream().await;
                self.set(|s| s.stream_notice = Some("Окно закрыто — трансляция остановлена.".into()));
            }
            CapEvent::Error(e) => {
                self.stop_stream().await;
                self.set(|s| s.stream_notice = Some(format!("Трансляция остановлена: {e}")));
            }
        }
    }

    /// Аудиопотоки в лог (раз в 5 опросов): кодек/каналы/fmtp, байты, потери и «заплатки» у приёмника.
    fn log_audio(&self, stats: &[RtcStats]) {
        if !self.stats_n.is_multiple_of(5) {
            return;
        }
        let codec = |id: &str| {
            stats.iter().find_map(|s| match s {
                RtcStats::Codec(c) if c.rtc.id == id => Some(format!("{}/{}ch [{}]", c.codec.mime_type, c.codec.channels, c.codec.sdp_fmtp_line)),
                _ => None,
            })
        };
        for s in stats {
            match s {
                RtcStats::OutboundRtp(o) if o.stream.kind == "audio" => log::info!(
                    "исходящее аудио ssrc {}: {:?}, отправлено {} пакетов / {} КБ",
                    o.stream.ssrc,
                    codec(&o.stream.codec_id),
                    o.sent.packets_sent,
                    o.sent.bytes_sent / 1024
                ),
                RtcStats::MediaSource(m) if m.source.kind == "audio" => {
                    log::info!("источник аудио {}: уровень {:.3}, энергия {:.2}", m.source.track_identifier, m.audio.audio_level, m.audio.total_audio_energy)
                }
                RtcStats::InboundRtp(i) if i.stream.kind == "audio" => log::info!(
                    "входящее аудио ssrc {}: {:?}, принято {} пакетов / {} КБ, потеряно {}, сэмплов {} (заплаток {}), уровень {:.3}",
                    i.stream.ssrc,
                    codec(&i.stream.codec_id),
                    i.received.packets_received,
                    i.inbound.bytes_received / 1024,
                    i.received.packets_lost,
                    i.inbound.total_samples_received,
                    i.inbound.concealed_samples,
                    i.inbound.audio_level
                ),
                _ => {}
            }
        }
    }

    /// Входящее видео трансляций — в лог (раз в 5 опросов).
    fn log_inbound_video(&self, stats: &[RtcStats]) {
        if !self.stats_n.is_multiple_of(5) {
            return;
        }
        log::debug!("subscriber stats: {} записей, inbound: {}", stats.len(), stats.iter().filter(|s| matches!(s, RtcStats::InboundRtp(_))).count());
        for s in stats {
            if let RtcStats::InboundRtp(i) = s
                && i.stream.kind == "video" {
                    log::info!(
                        "входящее видео: декодер {:?}, {}×{} {:.0} к/с, кадров {} (ключевых {}, выброшено {}), потеряно пакетов {} из {}, NACK {}, PLI {}, заморозок {}",
                        i.inbound.decoder_implementation,
                        i.inbound.frame_width,
                        i.inbound.frame_height,
                        i.inbound.frames_per_second,
                        i.inbound.frames_decoded,
                        i.inbound.key_frames_decoded,
                        i.inbound.frames_dropped,
                        i.received.packets_lost,
                        i.received.packets_received,
                        i.inbound.nack_count,
                        i.inbound.pli_count,
                        i.inbound.freeze_count
                    );
                }
        }
    }

    /// Статистика исходящего видео (раз в STATS_EVERY).
    fn update_video_stats(&mut self, stats: &[RtcStats]) {
        if self.stream.is_none() {
            return;
        }
        let Some(o) = stats.iter().find_map(|s| match s {
            RtcStats::OutboundRtp(o) if o.stream.kind == "video" => Some(o),
            _ => None,
        }) else {
            return;
        };
        let now = Instant::now();
        // Байты — по всем исходящим видеопотокам (после смены пресета старый поток может ещё числиться).
        let bytes: u64 = stats
            .iter()
            .filter_map(|s| match s {
                RtcStats::OutboundRtp(o) if o.stream.kind == "video" => Some(o.sent.bytes_sent),
                _ => None,
            })
            .sum();
        let kbps = match self.last_video_bytes {
            Some((b, t)) if bytes >= b => (bytes - b) as f64 * 8.0 / 1000.0 / now.duration_since(t).as_secs_f64().max(0.001),
            _ => 0.0,
        };
        self.last_video_bytes = Some((bytes, now));
        let enc = o.outbound.encoder_implementation.clone();
        let fps = o.outbound.frames_per_second;
        let lim = format!("{:?}", o.outbound.quality_limitation_reason);
        let loss = stats.iter().find_map(|s| match s {
            RtcStats::RemoteInboundRtp(r) if r.stream.kind == "video" => Some(r.remote_inbound.fraction_lost),
            _ => None,
        });
        let enc_ms = if o.outbound.frames_encoded > 0 {
            o.outbound.total_encode_time * 1000.0 / o.outbound.frames_encoded as f64
        } else {
            0.0
        };
        if self.stats_n % 5 == 1 {
            log::info!(
                "видео: энкодер {enc:?}, {}×{} {fps:.0} к/с, {kbps:.0} кбит/с, цель {:.0} кбит/с, кодирование {enc_ms:.1} мс/кадр, ограничение {lim}, NACK {}, PLI {}, потери у SFU {:?}",
                o.outbound.frame_width,
                o.outbound.frame_height,
                o.outbound.target_bitrate / 1000.0,
                o.outbound.nack_count,
                o.outbound.pli_count,
                loss
            );
        }
        self.set(|s| {
            if let Some(i) = &mut s.stream {
                i.encoder = (!enc.is_empty()).then_some(enc);
                i.fps = fps;
                i.kbps = kbps;
                i.limitation = (lim != "None").then_some(lim);
            }
        });
    }

    // ---------------------------------------------------------------- микрофон/громкость

    fn mic_should_send(&self) -> bool {
        !self.self_muted && !self.deafened && (!self.prefs.ptt_enabled || self.ptt_held)
    }

    fn apply_mic(&mut self) {
        let send = self.mic_should_send();
        if let Some(p) = &self.mic {
            if send && p.is_muted() {
                p.unmute();
            } else if !send && !p.is_muted() {
                p.mute();
            }
        }
        let (muted, deaf, held) = (self.self_muted, self.deafened, self.ptt_held);
        let transmitting = send && self.mic.is_some();
        self.set(|s| {
            s.self_muted = muted;
            s.deafened = deaf;
            s.ptt_held = held;
            s.transmitting = transmitting;
        });
        self.refresh_peers(false);
    }

    /// Громкость дорожки: голос и звук трансляции регулируются отдельно.
    fn volume_for(&self, identity: &str, source: TrackSource) -> f32 {
        if self.deafened {
            return 0.0;
        }
        let map = if source == TrackSource::ScreenshareAudio { &self.prefs.stream_volumes } else { &self.prefs.volumes };
        map.get(identity).copied().unwrap_or(1.0)
    }

    fn apply_volume_for(&self, identity: &str) {
        let Some((room, _)) = &self.room else { return };
        for (id, p) in room.remote_participants() {
            if id.as_str() != identity {
                continue;
            }
            for publication in p.track_publications().values() {
                if let Some(RemoteTrack::Audio(a)) = publication.track() {
                    a.rtc_track().set_volume(self.volume_for(identity, publication.source()) as f64);
                }
            }
        }
    }

    fn apply_volumes(&self) {
        let Some((room, _)) = &self.room else { return };
        for (id, p) in room.remote_participants() {
            for publication in p.track_publications().values() {
                if let Some(RemoteTrack::Audio(a)) = publication.track() {
                    a.rtc_track().set_volume(self.volume_for(id.as_str(), publication.source()) as f64);
                }
            }
        }
    }

    // ---------------------------------------------------------------- просмотр трансляций

    /// Нужна ли нам эта публикация: голос — всегда, трансляция (видео и её звук) — только если смотрим.
    fn wants(&self, identity: &str, publication: &RemoteTrackPublication) -> bool {
        match publication.source() {
            TrackSource::Screenshare | TrackSource::ScreenshareAudio => self.watching.contains(identity),
            _ => publication.kind() == TrackKind::Audio,
        }
    }

    fn sync_subscription(&self, identity: &str, publication: &RemoteTrackPublication) {
        let want = self.wants(identity, publication);
        if want != publication.is_desired() {
            log::debug!("{identity} {:?}: подписка {want}", publication.source());
            publication.set_subscribed(want);
        }
    }

    fn sync_subscriptions(&self) {
        let Some((room, _)) = &self.room else { return };
        for (id, p) in room.remote_participants() {
            for publication in p.track_publications().values() {
                self.sync_subscription(id.as_str(), publication);
            }
        }
    }

    fn set_watching(&mut self, identity: &str, on: bool) {
        let changed = if on { self.watching.insert(identity.to_string()) } else { self.watching.remove(identity) };
        if !changed {
            return;
        }
        log::info!("трансляция {identity}: {}", if on { "смотрим" } else { "не смотрим" });
        if !on {
            self.stop_receiver(identity);
            self.views.remove(identity);
        }
        self.sync_subscriptions();
        self.publish_watching();
    }

    /// Выход из канала / другой канал: ничего не смотрим.
    fn forget_watching(&mut self) {
        self.stop_receivers();
        self.watching.clear();
        self.views.clear();
        self.publish_watching();
    }

    fn publish_watching(&self) {
        let mut list: Vec<String> = self.watching.iter().cloned().collect();
        list.sort();
        self.set(|s| s.watching = list);
    }

    fn stop_receiver(&mut self, identity: &str) {
        if let Some(h) = self.receivers.remove(identity) {
            h.abort();
        }
        self.video.remove(identity);
    }

    fn stop_receivers(&mut self) {
        for (_, h) in self.receivers.drain() {
            h.abort();
        }
        self.video.clear();
    }

    /// Размер на экране → SFU (adaptive stream); невидимая трансляция ставится на паузу (SFU не шлёт пакеты).
    fn apply_view(&self, identity: &str) {
        let (Some((room, _)), Some(&(visible, w, h))) = (&self.room, self.views.get(identity)) else { return };
        let Some(p) = room.remote_participants().into_iter().find(|(id, _)| id.as_str() == identity).map(|(_, p)| p) else {
            return;
        };
        for publication in p.track_publications().values() {
            if publication.source() == TrackSource::Screenshare && publication.is_subscribed() {
                if visible && w > 0 && h > 0 {
                    publication.update_video_dimensions(TrackDimension(w, h));
                }
                if publication.is_enabled() != visible {
                    log::info!("трансляция {identity}: {}", if visible { "видна — поток включён" } else { "скрыта — поток на паузе" });
                    publication.set_enabled(visible);
                }
            }
        }
    }

    /// Пересобирает список участников. `quiet` — из таймера (только уровни/говорит).
    /// id rtc-трека → подпись для оверлея: «Имя: голос / экран / звук экрана», свои — «я: …».
    fn track_labels(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        let Some((room, _)) = &self.room else { return out };
        let what = |s: TrackSource| match s {
            TrackSource::Microphone => "голос",
            TrackSource::Screenshare => "экран",
            TrackSource::ScreenshareAudio => "звук экрана",
            TrackSource::Camera => "камера",
            _ => "?",
        };
        for publication in room.local_participant().track_publications().values() {
            let id = match publication.track() {
                Some(LocalTrack::Audio(a)) => a.rtc_track().id(),
                Some(LocalTrack::Video(v)) => v.rtc_track().id(),
                None => continue,
            };
            out.insert(id, format!("я: {}", what(publication.source())));
        }
        for p in room.remote_participants().values() {
            let name = if p.name().is_empty() { p.identity().to_string() } else { p.name().to_string() };
            for publication in p.track_publications().values() {
                let id = match publication.track() {
                    Some(RemoteTrack::Audio(a)) => a.rtc_track().id(),
                    Some(RemoteTrack::Video(v)) => v.rtc_track().id(),
                    None => continue,
                };
                out.insert(id, format!("{name}: {}", what(publication.source())));
            }
        }
        out
    }

    fn refresh_peers(&mut self, _quiet: bool) {
        let Some((room, _)) = &self.room else { return };
        let mut peers = Vec::new();
        let lp = room.local_participant();
        peers.push(Peer {
            identity: lp.identity().as_str().to_string(),
            name: lp.name(),
            is_local: true,
            speaking: lp.is_speaking() && self.mic_should_send(),
            level: lp.audio_level(),
            mic_muted: !self.mic_should_send(),
            streaming: self.stream.is_some(),
        });
        let mut remote: Vec<Peer> = room
            .remote_participants()
            .into_values()
            .map(|p| {
                let mic_muted = p
                    .track_publications()
                    .values()
                    .find(|t| t.source() == TrackSource::Microphone)
                    .map(|t| t.is_muted())
                    .unwrap_or(true);
                let streaming = p.track_publications().values().any(|t| t.source() == TrackSource::Screenshare);
                // Без имени (боты `lk load-test`, сторонние клиенты) — показываем identity.
                let name = Some(p.name()).filter(|n| !n.is_empty()).unwrap_or_else(|| p.identity().as_str().to_string());
                Peer {
                    identity: p.identity().as_str().to_string(),
                    name,
                    is_local: false,
                    speaking: p.is_speaking(),
                    level: p.audio_level(),
                    mic_muted,
                    streaming,
                }
            })
            .collect();
        remote.sort_by_key(|a| a.name.to_lowercase());
        peers.extend(remote);
        self.last_peers = peers.iter().map(|p| (p.identity.clone(), p.name.clone())).collect();
        self.set(|s| s.peers = peers);
    }
}
