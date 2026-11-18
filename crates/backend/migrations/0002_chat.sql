-- Чат текстовых каналов (фаза 7, вариант A) + заделы под вариант B/C.
CREATE TABLE messages (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id INTEGER NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    author_id  INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    content    TEXT    NOT NULL,
    reply_to   INTEGER REFERENCES messages(id) ON DELETE SET NULL, -- вариант B: ответы
    created_at INTEGER NOT NULL,
    edited_at  INTEGER,                                            -- вариант B: правка
    deleted_at INTEGER                                             -- вариант B: удаление (мягкое)
);
CREATE INDEX messages_channel ON messages(channel_id, id);

-- Загруженные файлы. message_id = NULL — загружен, но ещё не отправлен (чистится через час).
CREATE TABLE attachments (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id  INTEGER REFERENCES messages(id) ON DELETE CASCADE,
    uploader_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kind        TEXT    NOT NULL DEFAULT 'image',                  -- вариант C: 'file'
    mime        TEXT    NOT NULL,
    size        INTEGER NOT NULL,
    width       INTEGER,
    height      INTEGER,
    file_name   TEXT    NOT NULL DEFAULT '',
    storage_key TEXT    NOT NULL UNIQUE,                           -- имя файла на диске (случайное)
    created_at  INTEGER NOT NULL
);
CREATE INDEX attachments_message ON attachments(message_id);
CREATE INDEX attachments_pending ON attachments(created_at) WHERE message_id IS NULL;
