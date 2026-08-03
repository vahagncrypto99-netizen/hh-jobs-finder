#!/usr/bin/env bash
# Проверка авторизации hh.ru — открывает профиль Chrome, которым ходит Playwright MCP.
# Без ИИ: обычный headless-браузер, ~5 секунд.
# Коды выхода: 0 — авторизован, 2 — не авторизован, 1 — окружение/ошибка.
set -uo pipefail
cd "$(dirname "$0")/.."

command -v node >/dev/null 2>&1 || { echo "AUTH_FAIL не найден node — запусти ./install.sh"; exit 1; }
[[ -d app/node_modules/playwright-core ]] || { echo "AUTH_FAIL нет playwright-core — запусти ./install.sh"; exit 1; }

exec node app/scripts/check-auth.mjs
