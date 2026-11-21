-- ЛС, непрочитанные, настройка «кто может писать в ЛС».
-- Канал ЛС — обычная строка channels (kind 'text', server_id NULL: CHECK на kind не даёт добавить 'dm' без пересборки
-- таблицы) + пара людей здесь. Сообщения и вложения — те же таблицы, что у текстовых каналов.
CREATE TABLE dm_channels (
    channel_id INTEGER PRIMARY KEY REFERENCES channels(id) ON DELETE CASCADE,
    user_a     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_b     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    opened_by  INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (user_a, user_b),
    CHECK (user_a < user_b)
);
CREATE INDEX dm_channels_b ON dm_channels (user_b);

-- Докуда человек прочитал канал. Нет строки — непрочитанным считается всё после вступления на сервер (ЛС — всё).
CREATE TABLE read_states (
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    channel_id   INTEGER NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    last_read_id INTEGER NOT NULL,
    PRIMARY KEY (user_id, channel_id)
);

-- Старую историю считаем прочитанной, чтобы после обновления не загорелось всё сразу.
INSERT INTO read_states (user_id, channel_id, last_read_id)
SELECT sm.user_id, c.id, MAX(m.id)
FROM server_members sm
JOIN channels c ON c.server_id = sm.server_id AND c.kind = 'text'
JOIN messages m ON m.channel_id = c.id
GROUP BY sm.user_id, c.id;

-- 0 — ЛС только от друзей; 1 — ещё и от людей с общих серверов.
ALTER TABLE users ADD COLUMN dm_from_servers INTEGER NOT NULL DEFAULT 0;
