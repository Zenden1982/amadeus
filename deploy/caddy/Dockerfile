# Caddy + layer4 (SNI-маршрутизация TLS, PROXY protocol) — собирается из исходников (xcaddy).
# Свой образ вместо livekit/caddyl4: там старый caddy-l4, который ломается, когда PROXY-заголовок и TLS ClientHello
# приходят одним пакетом (так шлёт nginx stream) — см. NOTES.md 2026-11-17.
ARG CADDY_VERSION=2.11.9
FROM caddy:${CADDY_VERSION}-builder AS build
ARG CADDY_L4_VERSION=v0.1.4-0.20261114182104-1353123db5e2
ARG HTTP_PROXY
ARG HTTPS_PROXY
RUN xcaddy build --with github.com/mholt/caddy-l4@${CADDY_L4_VERSION}

FROM caddy:${CADDY_VERSION}-alpine
COPY --from=build /usr/bin/caddy /usr/bin/caddy
ENTRYPOINT ["caddy"]
