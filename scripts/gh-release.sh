#!/usr/bin/env bash
# Релиз на GitHub с установщиком: тег v<версия> на текущий коммит (он должен быть запушен) + amadeus-setup-<версия>.exe.
#
#   scripts/gh-release.sh <версия> "Что нового"
#
# Токен — из Git Credential Manager (тот же вход, что у git push); gh не нужен. Токен не печатается.
# Если релиз с таким тегом уже есть — только догружает недостающий установщик.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="${1:?версия}"
NOTES="${2:-}"
REPO="${GH_REPO:-Zenden1982/amadeus}"
SETUP="dist/$VERSION/amadeus-setup-$VERSION.exe"
[[ -f "$SETUP" ]] || { echo "нет $SETUP"; exit 1; }

TOKEN=$(printf 'protocol=https\nhost=github.com\n\n' | git credential fill | sed -n 's/^password=//p')
[[ -n "$TOKEN" ]] || { echo "нет токена GitHub в Credential Manager (сделайте git push один раз)"; exit 1; }
HDR=$(mktemp)
trap 'rm -f "$HDR" "$HDR.json"' EXIT
chmod 600 "$HDR"
printf 'Authorization: Bearer %s\nAccept: application/vnd.github+json\nX-GitHub-Api-Version: 2022-11-28\n' "$TOKEN" > "$HDR"
unset TOKEN
api() { curl -sS -H @"$HDR" "$@"; }

json_get() { python -X utf8 -I -c 'import json,sys; d=json.load(sys.stdin); print(d.get(sys.argv[1]) or "")' "$1"; }

REL=$(api "https://api.github.com/repos/$REPO/releases/tags/v$VERSION")
ID=$(printf '%s' "$REL" | json_get id)
if [[ -z "$ID" ]]; then
    SHA=$(git rev-parse HEAD)
    git branch -r --contains "$SHA" | grep -q . || { echo "коммит $SHA не запушен"; exit 1; }
    VERSION="$VERSION" NOTES="$NOTES" SHA="$SHA" python -X utf8 -I -c '
import json, os
e = os.environ
body = (e["NOTES"] + "\n\n" if e["NOTES"] else "") + "Установка: скачайте amadeus-setup-" + e["VERSION"] + ".exe и запустите (права администратора не нужны). Уже установленный Amadeus обновится сам."
print(json.dumps({"tag_name": "v" + e["VERSION"], "target_commitish": e["SHA"], "name": "Amadeus " + e["VERSION"], "body": body}, ensure_ascii=False))
' > "$HDR.json"
    REL=$(api -X POST --data-binary @"$HDR.json" "https://api.github.com/repos/$REPO/releases")
    ID=$(printf '%s' "$REL" | json_get id)
    [[ -n "$ID" ]] || { echo "не создался релиз: $(printf '%s' "$REL" | json_get message)"; exit 1; }
    echo "== GitHub: релиз v$VERSION создан"
fi

NAME=$(basename "$SETUP")
if printf '%s' "$REL" | grep -q "\"name\": *\"$NAME\""; then
    echo "== GitHub: $NAME уже загружен"
else
    OUT=$(api -X POST -H "Content-Type: application/octet-stream" --data-binary @"$SETUP" \
        "https://uploads.github.com/repos/$REPO/releases/$ID/assets?name=$NAME")
    [[ -n "$(printf '%s' "$OUT" | json_get id)" ]] || { echo "не загрузился $NAME: $(printf '%s' "$OUT" | json_get message)"; exit 1; }
    echo "== GitHub: загружен $NAME"
fi
echo "== https://github.com/$REPO/releases/tag/v$VERSION"
