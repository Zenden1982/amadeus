# Вендорные крейты (патчи Amadeus)

Подключены через `[patch.crates-io]` в корневом `Cargo.toml`. Версии должны совпадать с теми, что тянет `livekit`
(сейчас livekit 0.11.0 → libwebrtc 0.4.1, webrtc-sys 0.3.54). При обновлении livekit: скопировать новые версии
из `~/.cargo/registry/src/*/` и заново применить патч ниже (он маленький).

## Громкость удалённого аудиотрека (`set_volume`)
В SDK нет регулировки громкости отдельного участника. В WebRTC она есть: `AudioTrackInterface::SetVolume(double)`
(0.0..=10.0) — у удалённого трека это усиление приёмного потока **до** микшера ADM, поэтому эхоподавление
(AEC в Platform ADM) по-прежнему получает правильный опорный сигнал.

- `webrtc-sys/src/audio_track.rs` — в cxx-мост добавлено `fn set_volume(self: &AudioTrack, volume: f64)`.
- `webrtc-sys/include/livekit/audio_track.h`, `webrtc-sys/src/audio_track.cpp` — `AudioTrack::set_volume` → `track()->SetVolume(v)`.
- `libwebrtc/src/native/audio_track.rs`, `libwebrtc/src/audio_track.rs` — публичный `RtcAudioTrack::set_volume(f64)`.

Все места помечены комментарием `Amadeus patch`.

## Удалено из webrtc-sys
- `src/lazy_load_deps_for/` (4,6 МБ) — нужен только для Linux-сборки desktop_capturer. Для сборки под Linux вернуть
  из оригинального крейта.
- `test/`, `Cargo.lock`, `.cargo_vcs_info.json`.
