-- Друзья: одна строка на пару. requester_id отправил заявку addressee_id; status: pending → accepted.
-- Отклонить / отменить / удалить из друзей — удалить строку.
CREATE TABLE friendships (
    requester_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    addressee_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status       TEXT    NOT NULL CHECK (status IN ('pending', 'accepted')),
    created_at   INTEGER NOT NULL,
    PRIMARY KEY (requester_id, addressee_id),
    CHECK (requester_id <> addressee_id)
);
CREATE INDEX friendships_addressee ON friendships (addressee_id);
