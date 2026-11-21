//! Профиль: отображаемое имя (отдельно от логина) и аватар.

use axum::{Json, body::Bytes, extract::State};
use proto::{PublicUser, ServerEvent, UpdateMeRequest, User, limits};

use crate::{
    AppState,
    error::{AppError, AppResult},
    events::{Audience, publish},
    extract::{ApiJson, AuthUser},
    images,
};

pub(crate) async fn load_user(db: &sqlx::SqlitePool, id: i64) -> AppResult<User> {
    let (id, name, is_admin, display_name, avatar, dm_from_servers): (i64, String, bool, Option<String>, Option<String>, bool) =
        sqlx::query_as("SELECT id, name, is_admin, display_name, avatar_key, dm_from_servers FROM users WHERE id = ?")
            .bind(id)
            .fetch_one(db)
            .await?;
    Ok(User { id, name, is_admin, display_name, avatar, dm_from_servers })
}

/// Все, кто делит с человеком хоть один сервер (и он сам).
async fn neighbours(db: &sqlx::SqlitePool, id: i64) -> AppResult<Vec<i64>> {
    let mut v: Vec<i64> = sqlx::query_scalar(
        "SELECT DISTINCT b.user_id FROM server_members a JOIN server_members b ON a.server_id = b.server_id WHERE a.user_id = ?",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    // и друзья — они видят имя и аватар в списке друзей; и собеседники в ЛС
    let dms: Vec<i64> = sqlx::query_scalar(
        "SELECT CASE WHEN user_a = ?1 THEN user_b ELSE user_a END FROM dm_channels WHERE user_a = ?1 OR user_b = ?1",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    for f in crate::friends::friend_ids(db, id).await?.into_iter().chain(dms) {
        if !v.contains(&f) {
            v.push(f);
        }
    }
    if !v.contains(&id) {
        v.push(id);
    }
    Ok(v)
}

async fn changed(state: &AppState, id: i64) -> AppResult<Json<User>> {
    let u = load_user(&state.inner.db, id).await?;
    let public = PublicUser { id: u.id, name: u.name.clone(), display_name: u.display_name.clone(), avatar: u.avatar.clone() };
    publish(state, Audience::Users(neighbours(&state.inner.db, id).await?), ServerEvent::UserUpdated { user: public });
    Ok(Json(u))
}

/// `GET /me`
pub async fn me(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<User>> {
    Ok(Json(load_user(&state.inner.db, user.id).await?))
}

/// `PATCH /me` — отображаемое имя (пустая строка — сбросить), кто может писать в ЛС.
pub async fn update(State(state): State<AppState>, user: AuthUser, ApiJson(req): ApiJson<UpdateMeRequest>) -> AppResult<Json<User>> {
    let name_changed = req.display_name.is_some();
    if let Some(raw) = req.display_name {
        let name = raw.trim().to_string();
        if name.chars().count() > limits::DISPLAY_NAME_MAX || name.chars().any(char::is_control) {
            return Err(AppError::Validation(format!("отображаемое имя: до {} символов", limits::DISPLAY_NAME_MAX)));
        }
        sqlx::query("UPDATE users SET display_name = ? WHERE id = ?")
            .bind((!name.is_empty()).then_some(name))
            .bind(user.id)
            .execute(&state.inner.db)
            .await?;
    }
    if let Some(open) = req.dm_from_servers {
        sqlx::query("UPDATE users SET dm_from_servers = ? WHERE id = ?").bind(open).bind(user.id).execute(&state.inner.db).await?;
        if !name_changed {
            // имя и аватар не менялись — соседям рассылать нечего
            return Ok(Json(load_user(&state.inner.db, user.id).await?));
        }
    }
    changed(&state, user.id).await
}

async fn set_avatar(state: &AppState, id: i64, key: Option<String>) -> AppResult<()> {
    let old: Option<String> = sqlx::query_scalar("SELECT avatar_key FROM users WHERE id = ?").bind(id).fetch_one(&state.inner.db).await?;
    sqlx::query("UPDATE users SET avatar_key = ? WHERE id = ?").bind(&key).bind(id).execute(&state.inner.db).await?;
    if let Some(old) = old {
        images::remove(state, &old).await;
    }
    Ok(())
}

/// `PUT /me/avatar` — тело: квадратная картинка.
pub async fn put_avatar(State(state): State<AppState>, user: AuthUser, body: Bytes) -> AppResult<Json<User>> {
    let key = images::store_profile_image(&state, &body).await?;
    set_avatar(&state, user.id, Some(key)).await?;
    changed(&state, user.id).await
}

/// `DELETE /me/avatar`
pub async fn delete_avatar(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<User>> {
    set_avatar(&state, user.id, None).await?;
    changed(&state, user.id).await
}
