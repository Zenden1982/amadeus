//! Amadeus patch: обработка микрофона в Platform ADM (см. vendor/PATCHES.md).

/// Биты для [`set_device_audio_flags`].
pub const EC: i32 = 1;
pub const AGC: i32 = 2;
pub const NS: i32 = 4;
pub const HPF: i32 = 8;

/// Колбэк захвата: `(channels, num_channels, frames, rate)`, данные — float в шкале int16.
pub type CaptureHook = unsafe extern "C" fn(*const *mut f32, usize, usize, i32);

extern "C" {
    fn amadeus_set_capture_hook(hook: Option<CaptureHook>);
    fn amadeus_set_device_audio_flags(flags: i32);
}

/// Ставит (или снимает) колбэк, который получает каждый 10-мс блок микрофона после AEC/NS WebRTC.
/// Колбэк вызывается на аудиопотоке WebRTC: быстрый, без паник и блокировок.
pub fn set_capture_hook(hook: Option<CaptureHook>) {
    unsafe { amadeus_set_capture_hook(hook) }
}

/// AudioOptions (EC/AGC/NS/HPF) для микрофонных треков, созданных после вызова.
pub fn set_device_audio_flags(flags: i32) {
    unsafe { amadeus_set_device_audio_flags(flags) }
}
