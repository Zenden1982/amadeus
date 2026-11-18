-- Фаза 9.1: серверы-сообщества, роли, баны, категории; профиль (имя, аватар).
-- Нынешние данные переезжают на сервер №1. channels.server_id добавлен через ALTER (без пересборки таблицы:
-- миграция идёт в транзакции, а пересборка с включёнными внешними ключами стёрла бы сообщения каскадом),
-- поэтому NOT NULL обеспечивает код бэкенда.

CREATE TABLE servers (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT    NOT NULL,
    icon_key   TEXT,
    owner_id   INTEGER NOT NULL REFERENCES users(id),
    created_at INTEGER NOT NULL
);
CREATE INDEX servers_owner ON servers(owner_id);

-- Роль = набор прав (битовая маска proto::perm). Сейчас 4 стандартные, потом — свои.
CREATE TABLE server_roles (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    server_id   INTEGER NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    name        TEXT    NOT NULL,
    position    INTEGER NOT NULL,
    permissions INTEGER NOT NULL,
    is_default  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX server_roles_server ON server_roles(server_id);

CREATE TABLE server_members (
    server_id INTEGER NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    user_id   INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id   INTEGER NOT NULL REFERENCES server_roles(id) ON DELETE CASCADE,
    joined_at INTEGER NOT NULL,
    PRIMARY KEY (server_id, user_id)
);
CREATE INDEX server_members_user ON server_members(user_id);

CREATE TABLE server_bans (
    server_id  INTEGER NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    user_id    INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    by_user    INTEGER NOT NULL,
    reason     TEXT,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (server_id, user_id)
);

CREATE TABLE channel_categories (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    server_id INTEGER NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    name      TEXT    NOT NULL,
    position  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX channel_categories_server ON channel_categories(server_id);

ALTER TABLE channels ADD COLUMN server_id INTEGER REFERENCES servers(id) ON DELETE CASCADE;
ALTER TABLE channels ADD COLUMN category_id INTEGER REFERENCES channel_categories(id) ON DELETE SET NULL;
ALTER TABLE channels ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
ALTER TABLE channels ADD COLUMN user_limit INTEGER;
CREATE INDEX channels_server ON channels(server_id, position);

ALTER TABLE invites ADD COLUMN server_id INTEGER REFERENCES servers(id) ON DELETE CASCADE;

ALTER TABLE users ADD COLUMN display_name TEXT;
ALTER TABLE users ADD COLUMN avatar_key TEXT;

-- ---------------------------------------------------------------- перенос на сервер №1
INSERT INTO servers (id, name, owner_id, created_at)
SELECT 1, 'Amadeus',
       COALESCE((SELECT MIN(id) FROM users WHERE is_admin = 1), (SELECT MIN(id) FROM users)),
       CAST(strftime('%s', 'now') AS INTEGER)
WHERE EXISTS (SELECT 1 FROM users);

INSERT INTO server_roles (server_id, name, position, permissions, is_default)
SELECT 1, r.name, r.position, r.permissions, r.is_default
FROM (SELECT 'Владелец' AS name, 3 AS position, 127 AS permissions, 0 AS is_default
      UNION ALL SELECT 'Админ', 2, 127, 0
      UNION ALL SELECT 'Модератор', 1, 15, 0
      UNION ALL SELECT 'Участник', 0, 1, 1) r
WHERE EXISTS (SELECT 1 FROM servers WHERE id = 1);

INSERT INTO server_members (server_id, user_id, role_id, joined_at)
SELECT 1, u.id,
       (SELECT id FROM server_roles WHERE server_id = 1 AND position =
            CASE WHEN u.id = (SELECT owner_id FROM servers WHERE id = 1) THEN 3
                 WHEN u.is_admin = 1 THEN 2
                 ELSE 0 END),
       u.created_at
FROM users u
WHERE EXISTS (SELECT 1 FROM servers WHERE id = 1);

UPDATE channels
SET server_id = 1,
    position = (SELECT COUNT(*) FROM channels c2 WHERE c2.id < channels.id)
WHERE EXISTS (SELECT 1 FROM servers WHERE id = 1);

UPDATE invites SET server_id = 1 WHERE EXISTS (SELECT 1 FROM servers WHERE id = 1);
