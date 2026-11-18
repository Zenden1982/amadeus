# Развёртывание сервера

Серверная часть — три сервиса в Docker; всё, что зависит от места установки, — в `deploy/.env`.

| Сервис | Профиль | Что делает |
|---|---|---|
| **livekit** | `media` | SFU (голос, видео, звук стрима) + встроенный TURN. `network_mode: host` |
| **caddy** | `caddy` | TLS по имени (SNI): `lk.` → LiveKit (wss), `turn.` → TURN/TLS, `api.` → бэкенд. Сертификаты Let's Encrypt сам (TLS-ALPN) |
| **backend** | `backend` | аккаунты по инвайтам, каналы, выдача токенов LiveKit, чат |

## Быстрый старт: один сервер «под ключ» (standalone)

```sh
git clone <repo> && cd <repo>
sh scripts/gen-keys.sh          # создаст deploy/.env и случайные ключи LiveKit (на экран не выводятся)
nano deploy/.env                # LK_DOMAIN, TURN_DOMAIN, API_DOMAIN, ACME_EMAIL, LIVEKIT_PUBLIC_URL
sh deploy/up.sh                 # сгенерирует конфиги и поднимет всё
```

### DNS
A-записи `lk.`, `turn.`, `api.` → публичный IP сервера (без проксирования CDN — Cloudflare «DNS only»).

### Firewall (входящие)
| Порт | Для чего |
|---|---|
| 443/tcp | Caddy: wss-сигнализация, TURN/TLS, API |
| 7881/tcp (`LK_TCP_PORT`) | медиа по TCP, если UDP режется |
| 7882/udp (`LK_UDP_PORT`) | медиа (основной путь, один мультиплекс-порт) |
| 3478/udp (`TURN_UDP_PORT`) | TURN/UDP |

7880 (HTTP LiveKit) и 5349 (TURN после снятия TLS) наружу **не** открывать — их слушает только Caddy через loopback.
Порт 80 не нужен (HTTP-challenge выключен, сертификаты через TLS-ALPN на 443).

## Медиа-хост за существующим SNI-прокси (behind-proxy)

Если 443 на хосте уже занят nginx (`stream` + `ssl_preread`) или HAProxy:
```ini
COMPOSE_PROFILES=media,caddy
CADDY_LISTEN=127.0.0.1:8443
CADDY_SERVE_API=0               # API живёт на другом хосте
```
и добавить в прокси маршруты по имени — пример: [`examples/nginx-stream-sni.conf`](examples/nginx-stream-sni.conf).
PROXY protocol от прокси принимается (только с 127.0.0.1) — LiveKit/TURN видят настоящие IP клиентов. Без него тоже работает.

## Бэкенд отдельно (за своим reverse proxy)
`COMPOSE_PROFILES=backend`, `CADDY_SERVE_API=0`, `BACKEND_BIND=0.0.0.0:8080` (или адрес, доступный прокси),
`TRUSTED_PROXY_HOPS=1`; в прокси — `api.` → этот адрес. Поток событий чата (`GET /events`) — обычный HTTP (SSE):
WebSocket не нужен, буферизацию nginx бэкенд отключает сам заголовком `X-Accel-Buffering: no`; таймаут чтения прокси
должен быть больше 15 с (у nginx по умолчанию 60 с). Лимит тела запроса в прокси — не меньше 9 МБ (картинки до 8 МБ;
у nginx по умолчанию 1 МБ — `client_max_body_size 10m`). Прокси должен дописывать
`X-Forwarded-For` с настоящим IP клиента (иначе rate limit будет общим на всех).

## Бэкенд: первый запуск
```sh
cd deploy
docker compose run --rm -it backend create-admin --name <ник>     # пароль спросит (или ADMIN_PASSWORD=…)
docker compose run --rm backend create-invite --uses 1 --hours 168 # инвайт-код для друга
```
Остальное (каналы, инвайты, отзыв) — через API от имени админа (позже — из клиента).
База — SQLite в `/data/app.db`, картинки чата — `/data/uploads` (том `backend-data` или папка хоста `BACKEND_DATA`).
Резервная копия базы не включает картинки — копировать папку `uploads` отдельно.
Резервная копия на ходу (согласованный снимок через `VACUUM INTO`, контейнер не останавливать):
```sh
docker compose exec -T backend backend backup /data/backup/app.db   # файл появится в томе/папке данных
```

### API (кратко, JSON, `Authorization: Bearer <token>`)
| Метод | Путь | Кто | Что |
|---|---|---|---|
| GET | `/health` | все | версия |
| POST | `/auth/register` | все | `{invite_code, name, password}` → сессия (только по инвайту) |
| POST | `/auth/login` / `/auth/logout` | все / пользователь | сессия 30 дней |
| GET | `/me` | пользователь | кто я |
| GET / POST | `/channels` | пользователь / админ | список / создать `{name, kind: voice\|text}` |
| DELETE | `/channels/{id}` | админ | удалить |
| POST | `/channels/{id}/join` | пользователь | `{livekit_url, livekit_token, room}` (токен на 6 ч) |
| GET / POST | `/invites` | админ | список / создать `{uses?, expires_in_hours?}` (по умолчанию 1 раз, 7 дней; 0 ч — бессрочно) |
| DELETE | `/invites/{code}` | админ | отозвать |
| GET | `/channels/{id}/messages?before=&after=&limit=` | пользователь | история текстового канала (по возрастанию id; без параметров — последние 50) |
| POST | `/channels/{id}/messages` | пользователь | `{content, attachments: [id]}` → сообщение (20 сообщений / 10 с) |
| POST | `/uploads` | пользователь | тело — картинка PNG/JPEG/GIF/WebP до 8 МБ → `{id, mime, size, width, height}` |
| GET | `/attachments/{id}` | пользователь | файл вложения |
| GET | `/events` | пользователь | поток событий (SSE): `hello`, `message_created`, `resync`; пинг каждые 15 с |

Rate limit: 10 попыток входа/регистрации за 5 минут с одного IP (ответ 429 + `Retry-After`).

## Сервер без доступа к Docker Hub / crates.io
- Прокси для сборки: раскомментировать `HTTP_PROXY/HTTPS_PROXY` в `.env` (передаются как build-args).
- Или перенести образы готовыми: `docker compose build caddy` на своём ПК, затем `docker save livekit/livekit-server:v1.13.11 amadeus/caddy-l4:2.11.9 | ssh host docker load`.

## Проверка
```sh
curl https://<LK_DOMAIN>/            # → OK
docker compose -f deploy/docker-compose.yml logs -f caddy   # получение сертификатов
```
Подключение: `lk token create --join --room test --identity me --valid-for 1h` → https://meet.livekit.io (Custom: `wss://<LK_DOMAIN>` + токен).
