#!/usr/bin/env bash
# Релиз на GitHub с установщиком: тег v<версия> на текущий коммит (он должен быть запушен) + amadeus-setup-<версия>.exe.
#
#   scripts/gh-release.sh <версия> "Что нового"
#
# Нужен GitHub CLI: `gh` в PATH или .tools/gh/bin/gh.exe, вход — один раз `gh auth login`.
# Идёт к GitHub напрямую, мимо HTTPS_PROXY (v2rayN рвёт загрузку больших файлов); GH_USE_PROXY=1 — через прокси.
# Если релиз уже есть — только догружает установщик (--clobber).
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="${1:?версия}"
NOTES="${2:-}"
REPO="${GH_REPO:-Zenden1982/amadeus}"
SETUP="dist/$VERSION/amadeus-setup-$VERSION.exe"
[[ -f "$SETUP" ]] || { echo "нет $SETUP"; exit 1; }

GH=$(command -v gh || true)
[[ -n "$GH" ]] || GH=".tools/gh/bin/gh.exe"
[[ -x "$GH" ]] || { echo "нет GitHub CLI (gh): поставить и выполнить gh auth login"; exit 1; }
if [[ -z "${GH_USE_PROXY:-}" ]]; then
    unset HTTPS_PROXY HTTP_PROXY https_proxy http_proxy
fi

if "$GH" release view "v$VERSION" -R "$REPO" >/dev/null 2>&1; then
    "$GH" release upload "v$VERSION" "$SETUP" -R "$REPO" --clobber
else
    SHA=$(git rev-parse HEAD)
    git branch -r --contains "$SHA" | grep -q . || { echo "коммит $SHA не запушен"; exit 1; }
    BODY="${NOTES:+$NOTES

}Установка: скачайте amadeus-setup-$VERSION.exe и запустите (права администратора не нужны). Уже установленный Amadeus обновится сам."
    "$GH" release create "v$VERSION" "$SETUP" -R "$REPO" --target "$SHA" --title "Amadeus $VERSION" --notes "$BODY"
fi
echo "== https://github.com/$REPO/releases/tag/v$VERSION"
