// Общее для всех скриптов, которые ходят на hh.ru: поиск профиля Playwright MCP,
// запуск браузера с этим профилем и проверка входа.
//
// Профиль тот же, что использует пайплайн, поэтому cookies уже на месте и
// логиниться отдельно не нужно.

import { existsSync, readdirSync, statSync } from "node:fs";
import { homedir, platform } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";

function profileRoots() {
  const home = homedir();
  return platform() === "darwin"
    ? [join(home, "Library/Caches/ms-playwright-mcp"), join(home, "Library/Caches/ms-playwright")]
    : [join(home, ".cache/ms-playwright-mcp"), join(home, ".cache/ms-playwright")];
}

/** Сколько cookies hh.ru лежит в профиле (чтение SQLite, без расшифровки). */
function hhCookieCount(profileDir) {
  const db = join(profileDir, "Default", "Cookies");
  if (!existsSync(db)) return 0;
  try {
    const out = execFileSync(
      "sqlite3",
      [`file:${db}?immutable=1`, "select count(*) from cookies where host_key like '%hh.ru'"],
      { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] },
    );
    return Number(out.trim()) || 0;
  } catch {
    return 0;
  }
}

/**
 * Самый свежий профиль Playwright MCP, в котором есть cookies hh.ru.
 * `HH_PROFILE_DIR` переопределяет выбор — нужен, чтобы прогнать скрипт на копии
 * профиля, пока основной занят работающим пайплайном.
 */
export function findProfile() {
  const override = process.env.HH_PROFILE_DIR;
  if (override) {
    return { dir: override, mtime: 0, cookies: hhCookieCount(override), name: "override" };
  }
  const candidates = [];
  for (const root of profileRoots()) {
    if (!existsSync(root)) continue;
    for (const name of readdirSync(root)) {
      if (!name.startsWith("mcp-")) continue;
      const dir = join(root, name);
      let mtime;
      try {
        mtime = statSync(dir).mtimeMs;
      } catch {
        continue;
      }
      const cookies = hhCookieCount(dir);
      if (cookies > 0) candidates.push({ dir, mtime, cookies, name });
    }
  }
  candidates.sort((a, b) => b.mtime - a.mtime);
  return candidates[0] ?? null;
}

/**
 * Открыть браузер на профиле MCP.
 *
 * Канал строго `chrome` — тот же, которым профиль создан. Возьмёшь бандлед
 * chromium, и macOS Keychain не отдаст ключ шифрования: cookies не
 * расшифруются, и будет выглядеть как «разлогинен».
 */
export async function launch({ headless = true } = {}) {
  const profile = findProfile();
  if (!profile) {
    throw new Error("профиль браузера без cookies hh.ru — выполни вход: claude → /hh:login");
  }

  let chromium;
  try {
    ({ chromium } = await import("playwright-core"));
  } catch {
    throw new Error("нет playwright-core — запусти ./install.sh");
  }

  try {
    const context = await chromium.launchPersistentContext(profile.dir, {
      channel: "chrome",
      headless,
      args: ["--no-first-run", "--no-default-browser-check"],
    });
    return { context, profile };
  } catch (e) {
    const msg = String(e.message || e);
    if (/ProcessSingleton|already (in use|running)|SingletonLock/i.test(msg)) {
      throw new Error("профиль занят — браузер пайплайна уже открыт");
    }
    throw new Error(`не удалось открыть браузер: ${msg.split("\n")[0]}`);
  }
}

/** Проверка входа: hh.ru не должен редиректить на форму логина. */
export async function checkLogin(page) {
  return page.evaluate(() => ({
    loggedIn:
      !document.querySelector('[data-qa="login"]') && !location.href.includes("account/login"),
    url: location.href,
  }));
}

/** Ждём, пока страница вакансии отрисуется. */
export async function waitForVacancy(page) {
  await page.waitForSelector('[data-qa="vacancy-title"]', { timeout: 20_000 });
}
