#!/usr/bin/env bash
# Детерминированный сбор вакансий — без ИИ, обычный Playwright.
#   ./scripts/collect.sh [--count=N] [--dry-run] [--json]
# Коды выхода: 0 — успех, 2 — нужен вход на hh.ru (/hh:login), 1 — ошибка.
set -uo pipefail
cd "$(dirname "$0")/.."

command -v node >/dev/null 2>&1 || { echo "не найден node — запусти ./install.sh"; exit 1; }
[[ -d app/node_modules/playwright-core ]] || { echo "нет playwright-core — запусти ./install.sh"; exit 1; }

exec node app/scripts/collect.mjs "$@"
