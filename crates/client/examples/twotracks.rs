//! Разведка (simulcast своими силами): может ли участник опубликовать ДВА трека Screenshare и подписывается ли зритель
//! на каждый отдельно. MODE=pub — публикует «screen» и «screen-low» (программные кадры); MODE=sub — смотрит, что видит.
//! LK_URL=... LK_TOKEN=... MODE=pub|sub cargo run -p client --example twotracks
use std::time::Duration;

use futures_util::StreamExt;
use livekit::options::TrackPublishOptions;
use livekit::prelude::*;
use livekit::webrtc::video_frame::{I420Buffer, VideoFrame, VideoRotation};
use livekit::webrtc::video_source::native::NativeVideoSource;
use livekit::webrtc::video_source::{RtcVideoSource, VideoResolution};
use livekit::webrtc::video_stream::native::NativeVideoStream;
use msvc_shim as _;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,libwebrtc=warn,livekit=warn")).init();
    let url = std::env::var("LK_URL")?;
    let token = std::env::var("LK_TOKEN")?;
    let mode = std::env::var("MODE").unwrap_or_default();
    let mut opts = RoomOptions::default();
    opts.auto_subscribe = false;
    let (room, mut events) = Room::connect(&url, &token, opts).await?;
    if mode == "pub" {
        let mut sources = Vec::new();
        for (name, w, h) in [("screen", 640u32, 360u32), ("screen-low", 320, 180)] {
            let src = NativeVideoSource::new(VideoResolution { width: w, height: h }, true);
            let track = LocalVideoTrack::create_video_track(name, RtcVideoSource::Native(src.clone()));
            let p = room
                .local_participant()
                .publish_track(LocalTrack::Video(track), TrackPublishOptions { source: TrackSource::Screenshare, simulcast: false, ..Default::default() })
                .await;
            println!("публикация {name}: {:?}", p.as_ref().map(|p| p.sid()));
            sources.push((src, w, h));
        }
        for i in 0..(30 * 25) {
            for (src, w, h) in &sources {
                let mut buf = I420Buffer::new(*w, *h);
                let (y, _, _) = buf.data_mut();
                y.fill((i % 255) as u8);
                src.capture_frame(&VideoFrame { rotation: VideoRotation::VideoRotation0, timestamp_us: 0, frame_metadata: None, buffer: buf });
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        return Ok(());
    }
    // sub: дождаться публикаций, подписаться на обе, считать кадры
    tokio::time::sleep(Duration::from_secs(3)).await;
    for p in room.remote_participants().values() {
        for (sid, publ) in p.track_publications() {
            println!("вижу: {} {sid} name={} source={:?} kind={:?}", p.identity(), publ.name(), publ.source(), publ.kind());
            publ.set_subscribed(true);
        }
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let mut counters = Vec::new();
    while tokio::time::Instant::now() < deadline {
        tokio::select! {
            ev = events.recv() => {
                if let Some(RoomEvent::TrackSubscribed { track: RemoteTrack::Video(v), publication, .. }) = ev {
                    let name = publication.name();
                    println!("подписан: {name}");
                    let mut stream = NativeVideoStream::new(v.rtc_track());
                    counters.push(tokio::spawn(async move {
                        let mut n = 0u32;
                        let mut size = (0, 0);
                        let end = tokio::time::Instant::now() + Duration::from_secs(5);
                        while let Ok(Some(f)) = tokio::time::timeout_at(end, stream.next()).await {
                            n += 1;
                            size = (f.buffer.width(), f.buffer.height());
                        }
                        (name, n, size)
                    }));
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(200)) => {}
        }
    }
    for c in counters {
        println!("кадров: {:?}", c.await?);
    }
    Ok(())
}
