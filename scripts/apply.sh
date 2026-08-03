#!/usr/bin/env bash
# Отправка откликов без ИИ. БЕЗ --live ничего не отправляется.
#   ./scripts/apply.sh [--live] [--limit=N] [--id=NNN] [--json]
set -uo pipefail
cd "$(dirname "$0")/.."
command -v node >/dev/null 2>&1 || { echo "не найден node — запусти ./install.sh"; exit 1; }
exec node app/scripts/apply.mjs "$@"
