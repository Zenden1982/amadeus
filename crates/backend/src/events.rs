//! Адресная рассылка событий `GET /events`: каждое событие доходит только тем, кому оно предназначено.

use std::{collections::HashSet, time::Duration};

use axum::{
    extract::State,
    http::{HeaderValue, header},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures_util::StreamExt;
use proto::ServerEvent;
use tokio::sync::broadcast::{Receiver, error::RecvError};

use crate::{AppState, extract::AuthUser};

const SSE_KEEPALIVE: Duration = Duration::from_secs(20);

#[derive(Debug, Clone)]
pub enum Audience {
    /// Все участники сервера.
    Server(i64),
    User(i64),
    Users(Vec<i64>),
}

#[derive(Debug, Clone)]
pub struct Scoped {
    pub audience: Audience,
    pub event: ServerEvent,
}

pub fn publish(state: &AppState, audience: Audience, event: ServerEvent) {
    let _ = state.inner.events.send(Scoped { audience, event });
}

/// Серверы, на которых состоит пользователь.
async fn my_servers(state: &AppState, user_id: i64) -> HashSet<i64> {
    sqlx::query_scalar("SELECT server_id FROM server_members WHERE user_id = ?")
        .bind(user_id)
        .fetch_all(&state.inner.db)
        .await
        .map(|v: Vec<i64>| v.into_iter().collect())
        .unwrap_or_default()
}

/// Доходит ли событие до пользователя; попутно обновляет его набор серверов.
fn deliver(me: i64, servers: &mut HashSet<i64>, s: &Scoped) -> bool {
    let to_me = match &s.audience {
        Audience::Server(id) => servers.contains(id),
        Audience::User(u) => *u == me,
        Audience::Users(v) => v.contains(&me),
    };
    if to_me {
        match &s.event {
            ServerEvent::ServerUpserted { server } => {
                servers.insert(server.id);
            }
            ServerEvent::ServerRemoved { server_id } => {
                servers.remove(server_id);
            }
            _ => {}
        }
    }
    to_me
}

struct Feed {
    rx: Receiver<Scoped>,
    state: AppState,
    me: i64,
    servers: HashSet<i64>,
    /// Пока поток жив — человек «в сети» для друзей.
    _online: crate::friends::OnlineGuard,
}

/// `GET /events` — поток событий (SSE). Первое событие — `hello`; при отставании — `resync`.
pub async fn events(State(state): State<AppState>, user: AuthUser) -> Response {
    let rx = state.inner.events.subscribe();
    let mut shutdown = state.inner.shutdown.subscribe();
    let servers = my_servers(&state, user.id).await;
    tracing::debug!(user_id = user.id, "events stream opened");
    let online = crate::friends::OnlineGuard::new(&state, user.id);
    let feed = Feed { rx, state, me: user.id, servers, _online: online };
    let live = futures_util::stream::unfold(feed, |mut f| async move {
        loop {
            match f.rx.recv().await {
                Ok(s) => {
                    if deliver(f.me, &mut f.servers, &s) {
                        return Some((s.event, f));
                    }
                }
                Err(RecvError::Lagged(_)) => {
                    f.servers = my_servers(&f.state, f.me).await;
                    return Some((ServerEvent::Resync, f));
                }
                Err(RecvError::Closed) => return None,
            }
        }
    });
    let stream = futures_util::stream::iter([ServerEvent::Hello])
        .chain(live)
        .map(|e| Event::default().json_data(&e))
        .take_until(async move {
            let _ = shutdown.wait_for(|stop| *stop).await;
        });
    let mut resp = Sse::new(stream).keep_alive(KeepAlive::new().interval(SSE_KEEPALIVE)).into_response();
    let h = resp.headers_mut();
    // nginx/NPM: не буферизовать поток
    h.insert("x-accel-buffering", HeaderValue::from_static("no"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_follows_membership() {
        let mut servers: HashSet<i64> = [1].into();
        let ev = |a| Scoped { audience: a, event: ServerEvent::Hello };
        assert!(deliver(7, &mut servers, &ev(Audience::Server(1))));
        assert!(!deliver(7, &mut servers, &ev(Audience::Server(2))));
        assert!(deliver(7, &mut servers, &ev(Audience::User(7))));
        assert!(!deliver(7, &mut servers, &ev(Audience::Users(vec![1, 2]))));
        let removed = Scoped { audience: Audience::User(7), event: ServerEvent::ServerRemoved { server_id: 1 } };
        assert!(deliver(7, &mut servers, &removed));
        assert!(!deliver(7, &mut servers, &ev(Audience::Server(1))));
    }
}
