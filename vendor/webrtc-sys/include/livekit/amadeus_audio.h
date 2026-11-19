// Amadeus patch: настройки обработки микрофона и «крючок» захвата (нейросетевой шумодав в Rust).
#pragma once

#include <cstddef>
#include <memory>

namespace webrtc {
class CustomProcessing;
}

// Биты флагов AudioOptions для микрофонного трека (RtcAudioSource::Device).
#define AMADEUS_AUDIO_EC 1
#define AMADEUS_AUDIO_AGC 2
#define AMADEUS_AUDIO_NS 4
#define AMADEUS_AUDIO_HPF 8

// Вызывается на аудиопотоке WebRTC для каждого 10-мс блока после AEC/NS (до AGC2):
// channels[c][i] — float в шкале int16 (±32768), frames — отсчётов на канал, rate — частота обработки.
typedef void (*amadeus_capture_hook_fn)(float* const* channels,
                                        size_t num_channels,
                                        size_t frames,
                                        int rate);

extern "C" {
void amadeus_set_capture_hook(amadeus_capture_hook_fn hook);
void amadeus_set_device_audio_flags(int flags);
}

namespace livekit_ffi {
std::unique_ptr<webrtc::CustomProcessing> amadeus_capture_post_processing();
int amadeus_device_audio_flags();
}  // namespace livekit_ffi
