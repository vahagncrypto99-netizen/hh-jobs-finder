#!/usr/bin/env bash
# Установка/перезагрузка launchd-задачи hh-jobs.
# Реальный plist генерируется из com.hh-jobs.plist.example (пути машины
# подставляются) и в git не попадает. Запускать после правки расписания:
#   ./scheduler/install.sh
set -euo pipefail

LABEL="com.hh-jobs"
DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$DIR/.." && pwd)"
PLIST_SRC="$DIR/${LABEL}.plist"
PLIST_TEMPLATE="$DIR/${LABEL}.plist.example"
PLIST_LINK="$HOME/Library/LaunchAgents/${LABEL}.plist"
DOMAIN="gui/$(id -u)"

# Первый запуск: развернуть шаблон под текущую машину.
if [[ ! -f "$PLIST_SRC" ]]; then
  [[ -f "$PLIST_TEMPLATE" ]] || { echo "нет ни $PLIST_SRC, ни шаблона $PLIST_TEMPLATE" >&2; exit 1; }
  sed -e "s|__REPO_ROOT__|$REPO_ROOT|g" -e "s|__HOME__|$HOME|g" "$PLIST_TEMPLATE" > "$PLIST_SRC"
  echo "создан $PLIST_SRC из шаблона"
fi

plutil -lint "$PLIST_SRC"


# Выгрузить старую версию (если была), обновить symlink, загрузить заново
launchctl bootout "$DOMAIN/$LABEL" 2>/dev/null || true
ln -sf "$PLIST_SRC" "$PLIST_LINK"
launchctl bootstrap "$DOMAIN" "$PLIST_LINK"

echo "OK: $LABEL перезагружен (symlink -> $PLIST_SRC)"
launchctl list | grep "$LABEL"
