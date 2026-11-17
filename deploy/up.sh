#!/bin/sh
# Сгенерировать конфиги и поднять сервисы этого хоста (COMPOSE_PROFILES из .env).
set -eu
cd "$(dirname "$0")"
sh ./render.sh
docker compose up -d --remove-orphans
docker compose ps
