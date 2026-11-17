//! Трансляция окна/экрана: пресеты качества, приёмник кадров для LiveKit, публикация трека.

use std::sync::Arc;

use capture_win::{CaptureConfig, Event, I420Buffer, Sink};
use livekit::options::{TrackPublishOptions, VideoCodec, VideoEncoding};
use livekit::prelude::*;
use livekit::webrtc::video_frame::{VideoBuffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Preset {
    P720_30,
    #[default]
    P1080_30,
    P1080_60,
    /// 2K (2560×1440), 60 к/с.
    P1440_60,
    /// Исходное разрешение, 60 к/с.
    Native,
}

impl Preset {
    pub const ALL: [Preset; 5] = [Preset::P720_30, Preset::P1080_30, Preset::P1080_60, Preset::P1440_60, Preset::Native];

    pub fn label(self) -> &'static str {
        match self {
            Preset::P720_30 => "720p · 30",
            Preset::P1080_30 => "1080p · 30",
            Preset::P1080_60 => "1080p · 60",
            Preset::P1440_60 => "2K (1440p) · 60",
            Preset::Native => "Исходное · 60",
        }
    }

    pub fn config(self) -> CaptureConfig {
        let (max_width, max_height, fps) = match self {
            Preset::P720_30 => (1280, 720, 30),
            Preset::P1080_30 => (1920, 1080, 30),
            Preset::P1080_60 => (1920, 1080, 60),
            Preset::P1440_60 => (2560, 1440, 60),
            Preset::Native => (0, 0, 60),
        };
        CaptureConfig { max_width, max_height, fps }
    }

    /// Потолок битрейта; реальный подстраивает WebRTC (GCC) под канал.
    pub fn max_bitrate(self) -> u64 {
        match self {
            Preset::P720_30 => 3_000_000,
            Preset::P1080_30 => 5_000_000,
            Preset::P1080_60 => 8_000_000,
            Preset::P1440_60 => 12_000_000,
            Preset::Native => 12_000_000,
        }
    }
}

/// Событие захвата с номером поколения (события старого захвата после смены окна игнорируются).
#[derive(Debug)]
pub struct CaptureEvent {
    pub generation: u64,
    pub event: Event,
}

/// Ссылка на буфер как `AsRef<dyn VideoBuffer>` — без копирования кадра.
struct BufRef<'a>(&'a I420Buffer);

impl AsRef<dyn VideoBuffer> for BufRef<'_> {
    fn as_ref(&self) -> &(dyn VideoBuffer + 'static) {
        self.0
    }
}

/// Приёмник кадров захвата → NativeVideoSource (дальше кодирует WebRTC).
pub struct LkSink {
    pub source: NativeVideoSource,
    pub generation: u64,
    pub events: mpsc::UnboundedSender<CaptureEvent>,
}

impl Sink for LkSink {
    fn frame(&self, frame: &I420Buffer, timestamp_us: i64) {
        let f = VideoFrame { rotation: VideoRotation::VideoRotation0, timestamp_us, frame_metadata: None, buffer: BufRef(frame) };
        self.source.capture_frame(&f);
    }

    fn event(&self, event: Event) {
        let _ = self.events.send(CaptureEvent { generation: self.generation, event });
    }
}

/// Источник видео под размер окна/экрана (подсказка энкодеру; реальные кадры могут отличаться).
pub fn new_source(width: u32, height: u32, preset: Preset) -> NativeVideoSource {
    let cfg = preset.config();
    let (w, h) = capture_win::capture::target_size(width.max(2), height.max(2), &cfg);
    NativeVideoSource::new(
        VideoResolution { width: w, height: h },
        true,
    )
}

pub async fn publish(room: &Room, source: &NativeVideoSource, preset: Preset) -> RoomResult<LocalTrackPublication> {
    let track = LocalVideoTrack::create_video_track("screen", RtcVideoSource::Native(source.clone()));
    let opts = TrackPublishOptions {
        source: TrackSource::Screenshare,
        video_codec: VideoCodec::H264,
        simulcast: false,
        video_encoding: Some(VideoEncoding { max_bitrate: preset.max_bitrate(), max_framerate: preset.config().fps as f64 }),
        ..Default::default()
    };
    room.local_participant().publish_track(LocalTrack::Video(track), opts).await
}

pub fn sink(source: &NativeVideoSource, generation: u64, events: &mpsc::UnboundedSender<CaptureEvent>) -> Arc<dyn Sink> {
    Arc::new(LkSink { source: source.clone(), generation, events: events.clone() })
}

