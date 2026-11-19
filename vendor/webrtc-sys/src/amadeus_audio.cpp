// Amadeus patch: см. include/livekit/amadeus_audio.h и vendor/PATCHES.md.
#include "livekit/amadeus_audio.h"

#include <atomic>
#include <string>

#include "api/audio/audio_processing.h"
#include "modules/audio_processing/audio_buffer.h"

namespace {

std::atomic<amadeus_capture_hook_fn> g_hook{nullptr};
std::atomic<int> g_flags{AMADEUS_AUDIO_EC | AMADEUS_AUDIO_AGC |
                         AMADEUS_AUDIO_NS | AMADEUS_AUDIO_HPF};

class CaptureHook : public webrtc::CustomProcessing {
 public:
  void Initialize(int sample_rate_hz, int /*num_channels*/) override {
    rate_ = sample_rate_hz;
  }
  void Process(webrtc::AudioBuffer* audio) override {
    amadeus_capture_hook_fn f = g_hook.load(std::memory_order_acquire);
    if (f != nullptr && audio != nullptr) {
      f(audio->channels(), audio->num_channels(), audio->num_frames(), rate_);
    }
  }
  std::string ToString() const override { return "AmadeusCaptureHook"; }
  void SetRuntimeSetting(
      webrtc::AudioProcessing::RuntimeSetting /*setting*/) override {}

 private:
  int rate_ = 0;
};

}  // namespace

extern "C" void amadeus_set_capture_hook(amadeus_capture_hook_fn hook) {
  g_hook.store(hook, std::memory_order_release);
}

extern "C" void amadeus_set_device_audio_flags(int flags) {
  g_flags.store(flags, std::memory_order_release);
}

namespace livekit_ffi {

std::unique_ptr<webrtc::CustomProcessing> amadeus_capture_post_processing() {
  return std::make_unique<CaptureHook>();
}

int amadeus_device_audio_flags() {
  return g_flags.load(std::memory_order_acquire);
}

}  // namespace livekit_ffi
