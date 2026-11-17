#!/bin/sh
# Создаёт deploy/.env из .env.example (если нет) и вписывает случайные LIVEKIT_API_KEY/SECRET.
# Существующие ключи не перезаписывает (ключ --force — перевыпустить). Секреты на экран не выводятся.
set -eu
cd "$(dirname "$0")/../deploy"
[ -f .env ] || { cp .env.example .env; echo "создан deploy/.env из .env.example"; }
chmod 600 .env
rand() { LC_ALL=C tr -dc 'A-Za-z0-9' </dev/urandom | head -c "$1"; }
set_kv() { # set_kv KEY VALUE — заменить строку KEY=... или дописать
  if grep -q "^$1=" .env; then
    tmp=$(mktemp); awk -v k="$1" -v v="$2" 'BEGIN{FS=OFS="="} $1==k{print k"="v; next} {print}' .env > "$tmp"
    cat "$tmp" > .env; rm -f "$tmp"
  else printf '%s=%s\n' "$1" "$2" >> .env; fi
}
cur=$(grep -E '^LIVEKIT_API_SECRET=.+' .env || true)
if [ -n "$cur" ] && [ "${1:-}" != "--force" ]; then
  echo "ключи уже есть в deploy/.env (перевыпустить: $0 --force)"; exit 0
fi
set_kv LIVEKIT_API_KEY "API$(rand 12)"
set_kv LIVEKIT_API_SECRET "$(rand 48)"
echo "ок: LIVEKIT_API_KEY/SECRET записаны в deploy/.env (права 600)"
