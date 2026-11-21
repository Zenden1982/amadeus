//! Отладка бага «повторный вход — нет звука»: вход → публикация микрофона → 5 с → выход, несколько раз.
//! Запуск: LK_URL=... LK_TOKEN=... cargo run -p client --example rejoin
use std::time::Duration;

use livekit::options::TrackPublishOptions;
use livekit::prelude::*;
use livekit::webrtc::stats::RtcStats;
use livekit::PlatformAudio;
use msvc_shim as _;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info,libwebrtc=warn")).init();
    let url = std::env::var("LK_URL")?;
    let token = std::env::var("LK_TOKEN")?;
    let rounds: usize = std::env::var("ROUNDS").ok().and_then(|s| s.parse().ok()).unwrap_or(3);
    let fix = std::env::var("FIX").unwrap_or_default();
    let mut audio = PlatformAudio::new()?;
    for round in 0..rounds {
        let mut opts = RoomOptions::default();
        opts.auto_subscribe = true;
        if std::env::var("RELAY").is_ok_and(|v| v == "1") {
            opts.rtc_config.ice_transport_type = livekit::webrtc::prelude::IceTransportsType::Relay;
        }
        let (room, mut events) = Room::connect(&url, &token, opts).await?;
        tokio::spawn(async move {
            while let Some(ev) = events.recv().await {
                if let RoomEvent::TrackSubscribed { publication, participant, .. } = ev {
                    log::warn!("подписка: {} {:?}", participant.identity(), publication.source());
                }
            }
        });
        let track = LocalAudioTrack::create_audio_track("microphone", audio.rtc_source());
        room.local_participant()
            .publish_track(LocalTrack::Audio(track), TrackPublishOptions { source: TrackSource::Microphone, ..Default::default() })
            .await?;
        if fix == "start" && round > 0 {
            log::warn!("start_recording: {:?}", audio.start_recording());
        }
        let hold: u64 = std::env::var("HOLD").ok().and_then(|s| s.parse().ok()).unwrap_or(5);
        tokio::time::sleep(Duration::from_secs(hold)).await;
        let mut sent = 0u64;
        for s in room.get_stats().await?.publisher_stats {
            if let RtcStats::OutboundRtp(o) = s
                && o.stream.kind == "audio"
            {
                sent += o.sent.packets_sent;
            }
        }
        let (mut emitted, mut got) = (0u64, 0u64);
        let st = room.get_stats().await?;
        for s in st.subscriber_stats.into_iter().chain(st.publisher_stats) {
            if let RtcStats::InboundRtp(i) = s
                && i.stream.kind == "audio"
            {
                emitted += i.inbound.jitter_buffer_emitted_count;
                got += i.received.packets_received;
            }
        }
        let st2 = room.get_stats().await?;
        let all: Vec<RtcStats> = st2.publisher_stats.into_iter().chain(st2.subscriber_stats).collect();
        for s in &all {
            if let RtcStats::CandidatePair(cp) = s
                && cp.candidate_pair.nominated
                && cp.candidate_pair.state == Some(livekit::webrtc::stats::IceCandidatePairState::Succeeded)
            {
                let local = all.iter().find_map(|x| match x {
                    RtcStats::LocalCandidate(c) if c.rtc.id == cp.candidate_pair.local_candidate_id => Some(c),
                    _ => None,
                });
                let remote = all.iter().find_map(|x| match x {
                    RtcStats::RemoteCandidate(c) if c.rtc.id == cp.candidate_pair.remote_candidate_id => Some(c),
                    _ => None,
                });
                log::warn!(
                    "ПУТЬ: локальный {:?}/{:?} relay={:?} → удалённый {:?}:{} {:?}, RTT {:.0} мс, на отправку {:.0} кбит/с",
                    local.map(|c| c.local_candidate.candidate_type),
                    local.map(|c| c.local_candidate.protocol.clone()),
                    local.map(|c| c.local_candidate.relay_protocol),
                    remote.map(|c| c.remote_candidate.address.clone()),
                    remote.map(|c| c.remote_candidate.port).unwrap_or(0),
                    remote.map(|c| c.remote_candidate.candidate_type),
                    cp.candidate_pair.current_round_trip_time * 1000.0,
                    cp.candidate_pair.available_outgoing_bitrate / 1000.0
                );
            }
        }
        log::warn!("РАУНД {round}: отправлено аудиопакетов {sent} (ожидается ~250), входящих пакетов {got}, воспроизведено блоков {emitted}");
        room.close().await?;
        drop(room);
        log::warn!("после выхода: запись инициализирована = {}", audio.is_recording_initialized());
        if fix == "stop" {
            log::warn!("stop_recording: {:?}", audio.stop_recording());
        }
        if fix == "recreate" {
            drop(audio);
            audio = PlatformAudio::new()?;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Ok(())
}
