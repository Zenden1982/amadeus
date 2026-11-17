//! Голосовой движок: LiveKit-комната + платформенный ADM (WASAPI, AEC/NS/AGC из WebRTC).
//!
//! Работает в tokio-задаче. UI шлёт команды (`Cmd`) и читает общий снимок состояния
//! (`VoiceState`) — UI-поток никогда не ждёт сеть или звук.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use livekit::options::{AudioEncoding, TrackPublishOptions};
use livekit::prelude::*;
use livekit::webrtc::stats::RtcStats;
use livekit::{AudioProcessingOptions, PlatformAudio, PlayoutDeviceId, RecordingDeviceId};
use proto::Channel;
use tokio::sync::mpsc;

use crate::api::Api;
use crate::sounds::{self, Sound};

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
    SetInputDevice(Option<String>),
    SetOutputDevice(Option<String>),
    SetProcessing { aec: bool, ns: bool, agc: bool },
    SetSounds(bool),
    RefreshDevices,
    /// Новый API-клиент (после входа/смены сервера).
    SetApi(Api),
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
}

pub fn spawn(rt: &tokio::runtime::Handle, api: Api, prefs: AudioPrefs, repaint: Arc<dyn Fn() + Send + Sync>) -> VoiceHandle {
    let (tx, rx) = mpsc::unbounded_channel();
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
    };
    rt.spawn(engine.run(rx));
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
    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<Cmd>) {
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
                _ = tick.tick() => self.on_tick().await,
            }
        }
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
                self.leave().await;
                self.join(ch, 0).await;
            }
            Cmd::Leave => {
                self.rejoin = None;
                let was_in = self.room.is_some();
                self.leave().await;
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
        opts.auto_subscribe = true;
        opts.adaptive_stream = true;
        opts.dynacast = true;
        let (room, events) = Room::connect(&join.livekit_url, &join.livekit_token, opts).await?;

        if self.audio.is_none() {
            self.init_audio();
        }
        let mut mic = None;
        if let Some(audio) = &self.audio {
            let track = LocalAudioTrack::create_audio_track("microphone", audio.rtc_source());
            let publish = TrackPublishOptions {
                source: TrackSource::Microphone,
                audio_encoding: Some(AudioEncoding { max_bitrate: 32_000 }),
                dtx: true,
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
        self.room = Some((room, events));
        self.mic = mic;
        self.last_peers.clear();
        self.apply_mic();
        self.apply_volumes();
        Ok(())
    }

    fn drop_room(&mut self) {
        self.mic = None;
        self.room = None;
    }

    async fn leave(&mut self) {
        self.mic = None;
        if let Some((room, _)) = self.room.take()
            && let Err(e) = room.close().await {
                log::warn!("закрытие комнаты: {e}");
            }
        self.last_peers.clear();
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
                self.play(Sound::LeaveOther);
                self.refresh_peers(false);
            }
            RoomEvent::TrackSubscribed { track, participant, .. } => {
                if let RemoteTrack::Audio(a) = track {
                    let v = self.volume_for(participant.identity().as_str());
                    a.rtc_track().set_volume(v as f64);
                }
                self.refresh_peers(false);
            }
            RoomEvent::TrackMuted { .. }
            | RoomEvent::TrackUnmuted { .. }
            | RoomEvent::TrackPublished { .. }
            | RoomEvent::TrackUnpublished { .. }
            | RoomEvent::TrackUnsubscribed { .. }
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
                    self.set(|s| s.conn = Conn::Idle);
                } else if reason == livekit::DisconnectReason::DuplicateIdentity {
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
            if self.last_stats.elapsed() >= STATS_EVERY {
                self.last_stats = Instant::now();
                self.update_rtt().await;
            }
        }
    }

    async fn update_rtt(&mut self) {
        let Some((room, _)) = &self.room else { return };
        let Ok(stats) = room.get_stats().await else { return };
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
            self.set(|s| s.rtt = rtt);
        }
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

    fn volume_for(&self, identity: &str) -> f32 {
        if self.deafened { 0.0 } else { self.prefs.volumes.get(identity).copied().unwrap_or(1.0) }
    }

    fn apply_volume_for(&self, identity: &str) {
        let Some((room, _)) = &self.room else { return };
        let v = self.volume_for(identity);
        for (id, p) in room.remote_participants() {
            if id.as_str() != identity {
                continue;
            }
            for publication in p.track_publications().values() {
                if let Some(RemoteTrack::Audio(a)) = publication.track() {
                    a.rtc_track().set_volume(v as f64);
                }
            }
        }
    }

    fn apply_volumes(&self) {
        let Some((room, _)) = &self.room else { return };
        for (id, p) in room.remote_participants() {
            let v = self.volume_for(id.as_str());
            for publication in p.track_publications().values() {
                if let Some(RemoteTrack::Audio(a)) = publication.track() {
                    a.rtc_track().set_volume(v as f64);
                }
            }
        }
    }

    /// Пересобирает список участников. `quiet` — из таймера (только уровни/говорит).
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
                Peer {
                    identity: p.identity().as_str().to_string(),
                    name: p.name(),
                    is_local: false,
                    speaking: p.is_speaking(),
                    level: p.audio_level(),
                    mic_muted,
                }
            })
            .collect();
        remote.sort_by_key(|a| a.name.to_lowercase());
        peers.extend(remote);
        self.last_peers = peers.iter().map(|p| (p.identity.clone(), p.name.clone())).collect();
        self.set(|s| s.peers = peers);
    }
}
