#!/usr/bin/env node
// Проверка авторизации hh.ru — без ИИ: открываем тот же профиль Chrome,
// которым ходит Playwright MCP, и смотрим, отдаёт ли hh.ru страницу резюме.
//
// Вывод: последняя строка AUTH_OK <детали> | AUTH_FAIL <причина>
// Коды выхода: 0 — авторизован, 2 — не авторизован, 1 — окружение/ошибка.

import { existsSync, readdirSync, statSync } from "node:fs";
import { homedir, platform } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";

const RESUME_URL = "https://hh.ru/applicant/resumes";

function profileRoots() {
  const home = homedir();
  return platform() === "darwin"
    ? [join(home, "Library/Caches/ms-playwright-mcp"), join(home, "Library/Caches/ms-playwright")]
    : [join(home, ".cache/ms-playwright-mcp"), join(home, ".cache/ms-playwright")];
}

/** How many hh.ru cookies a profile holds (plain SQLite read, no decryption). */
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

/** Newest Playwright-MCP profile that actually carries hh.ru cookies. */
function findProfile() {
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

async function main() {
  const profile = findProfile();
  if (!profile) {
    console.log("AUTH_FAIL профиль браузера без cookies hh.ru — выполни вход: claude → /hh:login");
    process.exit(2);
  }

  let chromium;
  try {
    ({ chromium } = await import("playwright-core"));
  } catch {
    console.log("AUTH_FAIL нет playwright-core — запусти ./install.sh");
    process.exit(1);
  }

  // Профиль создан каналом chrome: открывать надо тем же каналом,
  // иначе macOS Keychain не отдаст ключ и cookies не расшифруются.
  let context;
  try {
    context = await chromium.launchPersistentContext(profile.dir, {
      channel: "chrome",
      headless: true,
      args: ["--no-first-run", "--no-default-browser-check"],
    });
  } catch (e) {
    const msg = String(e.message || e);
    if (/ProcessSingleton|already (in use|running)|SingletonLock/i.test(msg)) {
      console.log("AUTH_FAIL профиль занят — браузер пайплайна уже открыт");
      process.exit(1);
    }
    console.log(`AUTH_FAIL не удалось открыть браузер: ${msg.split("\n")[0]}`);
    process.exit(1);
  }

  try {
    const page = await context.newPage();
    await page.goto(RESUME_URL, { waitUntil: "domcontentloaded", timeout: 30_000 });
    const url = page.url();
    const loggedIn = !url.includes("/account/login") && !url.includes("/auth");
    const hasResume = await page
      .locator('[data-qa="resume-title"], [data-qa="mainmenu_applicantResumes"]')
      .first()
      .isVisible()
      .catch(() => false);

    if (loggedIn && hasResume) {
      console.log(`AUTH_OK резюме доступны (профиль ${profile.name}, ${profile.cookies} cookies)`);
      process.exitCode = 0;
    } else if (loggedIn) {
      console.log(`AUTH_OK сессия жива, но список резюме не отрисовался (${url})`);
      process.exitCode = 0;
    } else {
      console.log("AUTH_FAIL hh.ru редиректит на вход — выполни: claude → /hh:login");
      process.exitCode = 2;
    }
  } catch (e) {
    console.log(`AUTH_FAIL ошибка проверки: ${String(e.message || e).split("\n")[0]}`);
    process.exitCode = 1;
  } finally {
    await context.close().catch(() => {});
  }
}

main();
