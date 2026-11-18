//! Кто сейчас в голосовых каналах: вебхуки LiveKit + сверка через RoomService.
//! Комната голосового канала — `srv-{server}-ch-{channel}`; identity участника — id пользователя.

use std::{
    collections::{BTreeSet, HashMap},
    future::Future,
    pin::Pin,
    sync::Mutex,
};

use proto::{ServerEvent, VoiceRoster};

use crate::{
    AppState,
    events::{Audience, publish},
};

pub type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
/// Комнаты и identity участников в них.
pub type RoomListing = Vec<(String, Vec<String>)>;

/// Комнаты LiveKit (RoomService). В тестах — мок.
pub trait Rooms: Send + Sync {
    /// Все комнаты и identity участников в них.
    fn list(&self) -> BoxFut<'_, anyhow::Result<RoomListing>>;
    /// Выкинуть участника из комнаты.
    fn remove<'a>(&'a self, room: &'a str, identity: &'a str) -> BoxFut<'a, anyhow::Result<()>>;
}

/// Без LiveKit (по умолчанию до настройки): ничего не знает и никого не выкидывает.
pub struct NoRooms;

impl Rooms for NoRooms {
    fn list(&self) -> BoxFut<'_, anyhow::Result<RoomListing>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn remove<'a>(&'a self, _room: &'a str, _identity: &'a str) -> BoxFut<'a, anyhow::Result<()>> {
        Box::pin(async { Ok(()) })
    }
}

/// Имя комнаты голосового канала.
pub fn room_name(server_id: i64, channel_id: i64) -> String {
    format!("srv-{server_id}-ch-{channel_id}")
}

/// `srv-3-ch-7` → (3, 7). Чужие и старые (`ch-7`) имена — None.
pub fn parse_room(name: &str) -> Option<(i64, i64)> {
    let rest = name.strip_prefix("srv-")?;
    let (s, c) = rest.split_once("-ch-")?;
    Some((s.parse().ok()?, c.parse().ok()?))
}

/// Присутствие: комната → user id.
#[derive(Default)]
pub struct Presence {
    rooms: Mutex<HashMap<String, BTreeSet<i64>>>,
}

impl Presence {
    pub fn roster(&self, room: &str) -> Vec<i64> {
        self.rooms.lock().unwrap().get(room).map(|s| s.iter().copied().collect()).unwrap_or_default()
    }

    /// Все непустые голосовые каналы сервера.
    pub fn of_server(&self, server_id: i64) -> Vec<VoiceRoster> {
        let rooms = self.rooms.lock().unwrap();
        let mut v: Vec<VoiceRoster> = rooms
            .iter()
            .filter_map(|(room, ids)| {
                let (s, c) = parse_room(room)?;
                (s == server_id && !ids.is_empty()).then(|| VoiceRoster { channel_id: c, user_ids: ids.iter().copied().collect() })
            })
            .collect();
        v.sort_by_key(|r| r.channel_id);
        v
    }

    /// Комнаты сервера, где сейчас этот человек.
    pub fn rooms_of(&self, server_id: i64, user_id: i64) -> Vec<String> {
        self.rooms
            .lock()
            .unwrap()
            .iter()
            .filter(|(room, ids)| parse_room(room).is_some_and(|(s, _)| s == server_id) && ids.contains(&user_id))
            .map(|(r, _)| r.clone())
            .collect()
    }

    /// Изменить состав комнаты; true — если что-то поменялось.
    fn update(&self, room: &str, f: impl FnOnce(&mut BTreeSet<i64>)) -> bool {
        let mut rooms = self.rooms.lock().unwrap();
        let set = rooms.entry(room.to_string()).or_default();
        let before = set.clone();
        f(set);
        let changed = *set != before;
        if set.is_empty() {
            rooms.remove(room);
        }
        changed
    }
}

fn broadcast_roster(state: &AppState, room: &str) {
    if let Some((server_id, channel_id)) = parse_room(room) {
        let user_ids = state.inner.presence.roster(room);
        publish(state, Audience::Server(server_id), ServerEvent::Voice { server_id, roster: VoiceRoster { channel_id, user_ids } });
    }
}

pub fn joined(state: &AppState, room: &str, user_id: i64) {
    if state.inner.presence.update(room, |s| {
        s.insert(user_id);
    }) {
        broadcast_roster(state, room);
    }
}

pub fn left(state: &AppState, room: &str, user_id: i64) {
    if state.inner.presence.update(room, |s| {
        s.remove(&user_id);
    }) {
        broadcast_roster(state, room);
    }
}

pub fn room_finished(state: &AppState, room: &str) {
    if state.inner.presence.update(room, |s| s.clear()) {
        broadcast_roster(state, room);
    }
}

/// Сверить присутствие со списком LiveKit (потерянные вебхуки, перезапуск бэкенда).
pub async fn reconcile(state: &AppState) -> anyhow::Result<()> {
    let listing = state.inner.rooms.list().await?;
    let mut fresh: HashMap<String, BTreeSet<i64>> = HashMap::new();
    for (room, ids) in listing {
        if parse_room(&room).is_none() {
            continue;
        }
        let set: BTreeSet<i64> = ids.iter().filter_map(|i| i.parse().ok()).collect();
        if !set.is_empty() {
            fresh.insert(room, set);
        }
    }
    let changed: Vec<String> = {
        let mut rooms = state.inner.presence.rooms.lock().unwrap();
        let mut keys: BTreeSet<String> = rooms.keys().cloned().collect();
        keys.extend(fresh.keys().cloned());
        let changed = keys.into_iter().filter(|k| rooms.get(k) != fresh.get(k)).collect();
        *rooms = fresh;
        changed
    };
    for room in changed {
        broadcast_roster(state, &room);
    }
    Ok(())
}

/// Выкинуть человека из всех голосовых комнат сервера (кик, бан, удаление).
pub async fn kick_from_server_voice(state: &AppState, server_id: i64, user_id: i64) {
    for room in state.inner.presence.rooms_of(server_id, user_id) {
        remove_from_room(state, &room, user_id).await;
    }
}

pub async fn remove_from_room(state: &AppState, room: &str, user_id: i64) {
    if let Err(e) = state.inner.rooms.remove(room, &user_id.to_string()).await {
        tracing::warn!(%room, user_id, error = ?e, "не удалось выкинуть из комнаты");
    }
    left(state, room, user_id);
}

/// Очистить комнату целиком (канал или сервер удалён).
pub async fn clear_room(state: &AppState, room: &str) {
    for uid in state.inner.presence.roster(room) {
        remove_from_room(state, room, uid).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_room_cases() {
        assert_eq!(parse_room("srv-3-ch-7"), Some((3, 7)));
        assert_eq!(parse_room("ch-7"), None);
        assert_eq!(parse_room("srv-x-ch-7"), None);
        assert_eq!(parse_room("srv-3-ch-"), None);
        assert_eq!(room_name(3, 7), "srv-3-ch-7");
    }
}
