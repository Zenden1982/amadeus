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

## Обработка микрофона: флаги AudioOptions и колбэк захвата (2026-11-19)
В SDK `create_device_audio_track` создавал микрофонный источник с жёстко включёнными AEC+AGC+NS WebRTC, а
`PlatformAudio::configure_audio_processing` переключает только **аппаратную** обработку — переключатели клиента
на программный APM не действовали (NS WebRTC оставался всегда, голос «глухой»).

- `webrtc-sys/include/livekit/amadeus_audio.h`, `webrtc-sys/src/amadeus_audio.cpp` (новые, добавлены в `build.rs`):
  - `amadeus_set_device_audio_flags(flags)` — EC/AGC/NS/HPF для микрофонных треков, созданных **после** вызова;
  - `amadeus_set_capture_hook(fn)` — колбэк на каждый 10-мс блок захвата: `CustomProcessing`, поставленный в APM через
    `BuiltinAudioProcessingBuilder::SetCapturePostProcessing` (после AEC/NS, на полной полосе 48 кГц, float в шкале int16).
- `webrtc-sys/src/peer_connection_factory.cpp` — APM строится с этим постпроцессором; `create_device_audio_track`
  берёт AudioOptions из флагов.
- `webrtc-sys/src/amadeus.rs` + `pub mod amadeus` в `lib.rs` — Rust-обёртки (обычный `extern "C"`, не cxx).
- Клиент: `crates/client/src/denoise.rs` (DeepFilterNet3 / RNNoise в колбэке), `voice.rs::apply_processing`.

## Встроенный AEC Windows выключен навсегда (2026-11-19)
`webrtc-sys/src/adm_proxy.cpp`: `BuiltInAECIsAvailable()` → `false`, `EnableBuiltInAEC(true)` → `-1` (false → 0 без вызова ADM).
Иначе `WebRtcVoiceEngine::ApplyOptions` при echo_cancellation=true включал Voice Capture DMO, а он пишет микрофон
**16 кГц моно** (полоса до 8 кГц — «глухой» звук; в логе «захват микрофона: 16000 Гц»), и включение «залипало» до
выхода из программы (повторное выключение при инициализированной записи — «Attempt to set Windows AEC with recording
already initialized»). Эхоподавление теперь только программное AEC3 (48 кГц).

## AdmProxy::Terminate() не закрывает устройства (2026-11-21)
`webrtc-sys/src/adm_proxy.cpp`: `Terminate()` только останавливает запись/воспроизведение, sub-ADM терминирует лишь деструктор.
Когда закрывалась последняя комната, где ты был один, WebRTC уничтожал движок голоса и звал `adm->Terminate()`, а прокси
закрывал платформенный ADM Windows навсегда (`Init()` у прокси — no-op). При следующем входе: «Unable to set playout device»,
«Failed to initialize recording», 0 исходящих пакетов и тишина в динамиках — «голос завис после смены канала/сервера».
Проверка: `cargo run -p client --example rejoin` (вход → 5 с → выход ×3; до патча раунды 1–2 — 0 пакетов).

## Удалено из webrtc-sys
- `src/lazy_load_deps_for/` (4,6 МБ) — нужен только для Linux-сборки desktop_capturer. Для сборки под Linux вернуть
  из оригинального крейта.
- `test/`, `Cargo.lock`, `.cargo_vcs_info.json`.
