#!/usr/bin/env bash
# Установка всего, что нужно системе авто-откликов hh.ru.
#
#   ./install.sh                 — зависимости + playwright MCP + браузеры + сборка приложения
#   ./install.sh --check         — только диагностика, ничего не меняет
#   ./install.sh --with-schedule — плюс установка launchd-агента по расписанию
#   ./install.sh --no-build      — без сборки .app (только npm install)
set -uo pipefail
cd "$(dirname "$0")"

CHECK_ONLY=0
WITH_SCHEDULE=0
BUILD=1
for arg in "$@"; do
  case "$arg" in
    --check)         CHECK_ONLY=1 ;;
    --with-schedule) WITH_SCHEDULE=1 ;;
    --no-build)      BUILD=0 ;;
    -h|--help) sed -n '2,9p' "$0"; exit 0 ;;
    *) echo "неизвестный флаг: $arg" >&2; exit 1 ;;
  esac
done

FAILED=0
ok()   { printf '  \033[32m✓\033[0m %s\n' "$1"; }
warn() { printf '  \033[33m!\033[0m %s\n' "$1"; }
bad()  { printf '  \033[31m✗\033[0m %s\n' "$1"; FAILED=1; }
step() { printf '\n\033[1m%s\033[0m\n' "$1"; }

# ---------- 1. базовые инструменты ----------
step "Базовые инструменты"

if command -v node >/dev/null 2>&1; then
  ok "node $(node -v)"
else
  bad "node не найден — поставь Node.js 18+ (brew install node)"
fi

if command -v npm >/dev/null 2>&1; then
  ok "npm $(npm -v)"
else
  bad "npm не найден"
fi

if command -v claude >/dev/null 2>&1; then
  ok "claude CLI $(claude --version 2>/dev/null | head -1)"
else
  bad "claude CLI не найден — https://claude.com/claude-code"
fi

if command -v cargo >/dev/null 2>&1; then
  ok "cargo $(cargo --version | awk '{print $2}')"
else
  warn "cargo не найден — приложение не соберётся (brew install rustup && rustup-init)"
fi

if command -v python3 >/dev/null 2>&1; then
  ok "python3 (нужен для живого лога пайплайна)"
else
  warn "python3 не найден — режим --verbose будет без форматирования"
fi

[[ $FAILED -eq 1 && $CHECK_ONLY -eq 0 ]] && { echo; echo "Сначала поставь недостающее выше, потом запусти снова."; exit 1; }

# ---------- 2. playwright MCP ----------
step "Playwright MCP"

if claude mcp list 2>/dev/null | grep -q '^playwright:'; then
  ok "сервер playwright зарегистрирован"
elif [[ $CHECK_ONLY -eq 1 ]]; then
  bad "сервер playwright не зарегистрирован"
else
  echo "  регистрирую playwright MCP (scope: user)…"
  if claude mcp add playwright --scope user -- npx -y @playwright/mcp@latest; then
    ok "playwright MCP добавлен"
  else
    bad "не удалось добавить playwright MCP"
  fi
fi

# ---------- 2.5. комплектность пакета ----------
step "Скиллы, команды и агенты"

MISSING=""
for p in .claude/agents/hh-collector.md .claude/agents/hh-analyzer.md \
         .claude/agents/hh-letter.md .claude/agents/hh-applier.md \
         .claude/commands/hh/init.md .claude/commands/hh/login.md .claude/commands/hh/run.md \
         .claude/skills/hh-parsing/SKILL.md .claude/skills/hh-matching/SKILL.md \
         .claude/skills/hh-letter/SKILL.md .claude/skills/hh-applying/SKILL.md \
         .claude/skills/ru-ai-check/SKILL.md; do
  [[ -f "$p" ]] || MISSING="$MISSING $p"
done

if [[ -z "$MISSING" ]]; then
  ok "4 агента, 3 команды, 5 скиллов на месте (включая ru-ai-check)"
else
  bad "не хватает:$MISSING"
fi

# ---------- 3. браузеры playwright ----------
step "Браузеры Playwright"

BROWSER_CACHE="$HOME/Library/Caches/ms-playwright"
[[ "$(uname)" != "Darwin" ]] && BROWSER_CACHE="$HOME/.cache/ms-playwright"

if [[ -d "$BROWSER_CACHE" ]] && compgen -G "$BROWSER_CACHE/chromium*" >/dev/null; then
  ok "chromium установлен ($BROWSER_CACHE)"
elif [[ $CHECK_ONLY -eq 1 ]]; then
  warn "chromium не найден в $BROWSER_CACHE"
else
  echo "  ставлю chromium…"
  npx -y playwright install chromium && ok "chromium установлен" || bad "не удалось поставить chromium"
fi

if [[ $CHECK_ONLY -eq 0 ]]; then
  # канал chrome нужен, если MCP запускается с --browser chrome; необязателен
  npx -y playwright install chrome >/dev/null 2>&1 && ok "канал chrome доступен" || warn "канал chrome недоступен (не критично)"
fi

# ---------- 4. приложение ----------
step "Приложение hh-jobs"

if [[ $CHECK_ONLY -eq 1 ]]; then
  [[ -d app/node_modules ]] && ok "app/node_modules на месте" || warn "app/node_modules нет — нужен npm install"
else
  (cd app && npm install >/dev/null 2>&1) && ok "зависимости приложения установлены" || bad "npm install в app/ упал"
  if [[ $BUILD -eq 1 ]] && command -v cargo >/dev/null 2>&1; then
    echo "  собираю .app (первая сборка занимает несколько минут)…"
    if (cd app && npm run build >/dev/null 2>&1); then
      APP_PATH="$(find app/src-tauri/target/release/bundle -name '*.app' -maxdepth 3 2>/dev/null | head -1)"
      ok "собрано: ${APP_PATH:-app/src-tauri/target/release/bundle}"
    else
      warn "сборка .app не удалась — приложение всё равно работает через 'cd app && npm run dev'"
    fi
  fi
fi

# ---------- 5. расписание ----------
step "Планировщик"

if launchctl list 2>/dev/null | grep -q com.hh-jobs; then
  ok "launchd-агент загружен"
elif [[ $WITH_SCHEDULE -eq 1 ]]; then
  ./scheduler/install.sh && ok "launchd-агент установлен" || bad "не удалось установить агента"
else
  warn "агент не загружен (поставить: ./install.sh --with-schedule или кнопкой в приложении)"
fi

# ---------- 6. авторизация hh.ru ----------
step "Авторизация hh.ru"

if [[ ! -d app/node_modules/playwright-core ]]; then
  warn "playwright-core не установлен — проверка входа недоступна"
else
  AUTH_OUT=$(./scripts/check-auth.sh 2>&1); AUTH_CODE=$?
  case $AUTH_CODE in
    0) ok "вход на hh.ru активен" ;;
    2) warn "входа нет — выполни один раз: claude → /hh:login" ;;
    *) warn "${AUTH_OUT##*AUTH_FAIL }" ;;
  esac
fi

echo
if [[ $FAILED -eq 1 ]]; then
  echo "Есть проблемы (✗ выше) — исправь и запусти снова."
  exit 1
fi
echo "Готово. Запуск приложения: cd app && npm run dev"
