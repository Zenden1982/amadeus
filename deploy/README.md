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
`COMPOSE_PROFILES=backend`, `BACKEND_BIND=0.0.0.0:8080` (или адрес, доступный прокси), в прокси — `api.` → этот адрес (WebSocket включить).

## Сервер без доступа к Docker Hub / crates.io
- Прокси для сборки: раскомментировать `HTTP_PROXY/HTTPS_PROXY` в `.env` (передаются как build-args).
- Или перенести образы готовыми: `docker compose build caddy` на своём ПК, затем `docker save livekit/livekit-server:v1.13.11 svoy-discord/caddy-l4:2.11.9 | ssh host docker load`.

## Проверка
```sh
curl https://<LK_DOMAIN>/            # → OK
docker compose -f deploy/docker-compose.yml logs -f caddy   # получение сертификатов
```
Подключение: `lk token create --join --room test --identity me --valid-for 1h` → https://meet.livekit.io (Custom: `wss://<LK_DOMAIN>` + токен).
