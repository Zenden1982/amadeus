//! Настройки «Внешний вид», нужные при отрисовке (анимации, индикатор речи, пинг, плитки, кот), и правила перерисовки.
//!
//! Главное правило: украшения не перерисовывают кадр постоянно. Только `request_repaint_after` с нужным интервалом
//! и только пока анимация видна; окно свёрнуто или не в фокусе (по умолчанию) — анимации стоят.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering::Relaxed};
use std::time::Duration;

use eframe::egui;

use crate::config::{AnimPref, PingDisplay, Settings, SpeechIndicator};

static LEVEL: AtomicU8 = AtomicU8::new(0);
static PAUSE_BG: AtomicBool = AtomicBool::new(true);
static SPEECH: AtomicU8 = AtomicU8::new(0);
static PING: AtomicU8 = AtomicU8::new(0);
static COMPACT: AtomicBool = AtomicBool::new(false);
static LIVE_CAT: AtomicBool = AtomicBool::new(true);

/// Анимации речи: до 30 к/с.
pub const SPEECH_FRAME: Duration = Duration::from_millis(33);
/// Анимация плохого пинга: до 15 к/с.
pub const PING_FRAME: Duration = Duration::from_millis(66);

/// Взять значения из настроек (при запуске и после каждого изменения в «Внешнем виде»).
pub fn apply(s: &Settings) {
    LEVEL.store(s.animations_effective() as u8, Relaxed);
    PAUSE_BG.store(s.pause_anim_in_background, Relaxed);
    SPEECH.store(s.speech_indicator as u8, Relaxed);
    PING.store(s.ping_display as u8, Relaxed);
    COMPACT.store(s.compact_tiles, Relaxed);
    LIVE_CAT.store(s.live_cat, Relaxed);
}

pub fn level() -> AnimPref {
    match LEVEL.load(Relaxed) {
        1 => AnimPref::Important,
        2 => AnimPref::Off,
        _ => AnimPref::All,
    }
}

pub fn speech() -> SpeechIndicator {
    if SPEECH.load(Relaxed) == 1 { SpeechIndicator::Ring } else { SpeechIndicator::Wave }
}

pub fn ping() -> PingDisplay {
    match PING.load(Relaxed) {
        1 => PingDisplay::MsOnly,
        2 => PingDisplay::Hidden,
        _ => PingDisplay::WaveAndMs,
    }
}

pub fn compact_tiles() -> bool {
    COMPACT.load(Relaxed)
}

/// Окно на виду: не свёрнуто и (если включена пауза в фоне) в фокусе.
pub fn window_active(ctx: &egui::Context) -> bool {
    let (minimized, focused) = ctx.input(|i| (i.viewport().minimized.unwrap_or(false), i.viewport().focused.unwrap_or(true)));
    !minimized && (focused || !PAUSE_BG.load(Relaxed))
}

/// Декоративные анимации (кот в заголовке, дрожащий пинг): только «Все» и окно на виду.
pub fn decor(ctx: &egui::Context) -> bool {
    level() == AnimPref::All && window_active(ctx)
}

/// Важные анимации (волна речи, кот «подключаемся»): «Все» или «Важные», окно на виду.
pub fn important(ctx: &egui::Context) -> bool {
    level() != AnimPref::Off && window_active(ctx)
}

/// Индикатор речи — волна (иначе — статичная обводка-«кольцо»). «Анимации: Выкл.» — всегда кольцо.
pub fn speech_wave() -> bool {
    speech() == SpeechIndicator::Wave && level() != AnimPref::Off
}

/// Настройка «Живой кот» включена и анимации не выключены (статичные позы — например, «спит» — можно рисовать и в фоне).
pub fn live_cat_enabled() -> bool {
    LIVE_CAT.load(Relaxed) && level() != AnimPref::Off
}

/// Живой кот в заголовке окна.
pub fn live_cat(ctx: &egui::Context) -> bool {
    LIVE_CAT.load(Relaxed) && decor(ctx)
}

/// Сглаженная громкость речи участника 0..1: атака ~30 мс, спад ~200 мс (хранится в памяти egui по ключу).
pub fn smooth_level(ctx: &egui::Context, key: &str, target: f32) -> f32 {
    let id = egui::Id::new(("speech-amp", key));
    let now = ctx.input(|i| i.time);
    let (prev, at) = ctx.data(|d| d.get_temp::<(f32, f64)>(id)).unwrap_or((0.0, now));
    let dt = (now - at).clamp(0.0, 0.5) as f32;
    let tau = if target > prev { 0.030 } else { 0.200 };
    let v = prev + (target - prev) * (1.0 - (-dt / tau).exp());
    let v = if v < 0.005 { 0.0 } else { v };
    ctx.data_mut(|d| d.insert_temp(id, (v, now)));
    v
}

/// Системная настройка Windows «Показывать анимацию в Windows» (Параметры → Специальные возможности → Визуальные эффекты).
pub fn system_animations_enabled() -> bool {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW};
        let mut on: i32 = 1;
        if SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&mut on as *mut i32).cast(), 0) != 0 {
            return on != 0;
        }
    }
    true
}
