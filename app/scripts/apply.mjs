#!/usr/bin/env node
// Отправка откликов на hh.ru без ИИ — механические сценарии.
//
// Берёт из реестра вакансии со статусом `matched` и готовым письмом и
// откликается: модалка с полем письма (A) или письмо в чат после мгновенного
// отклика (B). Тест работодателя (C) требует осмысленных ответов по резюме —
// такие вакансии скрипт НЕ трогает, а помечает `needs_llm`, чтобы дирижёр
// отдал их агенту. Любая неожиданная разметка — туда же: лучше передать
// человеку/модели, чем кликать наугад.
//
//   node app/scripts/apply.mjs [--live] [--limit=N] [--id=NNN] [--json]
//
// БЕЗ `--live` ничего не отправляется: скрипт только проверяет, что кнопка
// отклика на месте. Это защита от случайной массовой рассылки.

import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { parse as parseYaml } from "yaml";
import { launch } from "./hh-browser.mjs";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "../..");
const CONFIG = join(REPO, "config.yaml");
const REGISTRY = join(REPO, "data/vacancies-registry.yaml");

const args = process.argv.slice(2);
const flag = (name) => args.find((a) => a.startsWith(`--${name}=`))?.split("=")[1];
const LIVE = args.includes("--live");
const JSON_OUT = args.includes("--json");
const log = (m) => {
  if (!JSON_OUT) console.log(m);
};

// ---------- реестр ----------

/** Заменить `status:` (и добавить пометку) внутри записи с данным id. */
function setStatus(id, status, detail) {
  const lines = readFileSync(REGISTRY, "utf8").split("\n");
  const start = lines.findIndex((l) => l.startsWith(`- id: "${id}"`));
  if (start === -1) return false;

  let end = lines.length;
  for (let i = start + 1; i < lines.length; i++) {
    if (lines[i].startsWith("- id: ")) {
      end = i;
      break;
    }
  }
  const statusLine = lines.slice(start, end).findIndex((l) => l.startsWith("  status:"));
  if (statusLine === -1) return false;

  const at = start + statusLine;
  lines[at] = `  status: ${status}`;
  if (detail) {
    const escaped = String(detail).replace(/\\/g, "\\\\").replace(/"/g, '\\"');
    const noteAt = lines.slice(start, end).findIndex((l) => l.startsWith("  apply_detail:"));
    const note = `  apply_detail: "${escaped}"`;
    if (noteAt === -1) lines.splice(at + 1, 0, note);
    else lines[start + noteAt] = note;
  }
  writeFileSync(REGISTRY, lines.join("\n"));
  return true;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---------- сценарии отклика ----------

const RESPOND_BUTTON = '[data-qa="vacancy-response-link-top"]';
const LETTER_INPUT = '[data-qa="vacancy-response-popup-form-letter-input"]';
const SUBMIT_POPUP = '[data-qa="vacancy-response-submit-popup"]';
const RELOCATION_OK = '[data-qa="relocation-warning-confirm"]';
/** Ссылка «перейти к отклику» — она же вход в чат по этой вакансии. */
const VIEW_TOPIC = '[data-qa="vacancy-response-link-view-topic"]';

/**
 * Отклик уже отправлен?
 *
 * Баннер «Вы откликнулись» hh.ru показывает только сразу после отправки, а на
 * странице, открытой позже, его нет — остаётся ссылка на отклик. Проверять
 * надо оба признака, иначе вакансия будет выглядеть неоткликнутой.
 * Слово «Чат» для этого не годится: чат-активатор есть в шапке всех страниц.
 */
const alreadyApplied = (page) =>
  page.evaluate(
    (topic) =>
      document.body.innerText.includes("Вы откликнулись") || !!document.querySelector(topic),
    VIEW_TOPIC,
  );

/** Сценарий A: модалка с полем письма. */
async function applyViaModal(page, letter, resumeTitle) {
  // Несколько резюме на аккаунте — выбрать нужное по названию из config.yaml.
  const picker = page.locator(`text=${resumeTitle}`).first();
  if (resumeTitle && (await picker.count()) > 0 && (await picker.isVisible().catch(() => false))) {
    await picker.click().catch(() => {});
  }
  await page.fill(LETTER_INPUT, letter);
  await page.click(SUBMIT_POPUP);
  await page.waitForTimeout(2500);
}

/** Сценарий B: отклик ушёл сразу, письмо добавляется сообщением в чат. */
async function applyViaChat(page, letter) {
  const chat = page.locator(VIEW_TOPIC).first();
  if ((await chat.count()) === 0) throw new Error("не найдена ссылка на отклик/чат");
  await chat.click();
  await page.waitForLoadState("domcontentloaded");

  const addLetter = page.locator('text="Добавить сопроводительное"').first();
  if ((await addLetter.count()) === 0) throw new Error("нет ссылки «Добавить сопроводительное»");
  await addLetter.click();

  const field = page.locator("textarea").first();
  await field.waitFor({ timeout: 10_000 });
  await field.fill(letter);
  await field.press("Enter");
  await page.waitForTimeout(2500);
}

/** Один отклик. Возвращает {status, detail}. */
async function applyToVacancy(page, vacancy, cfg) {
  await page.goto(vacancy.url, { waitUntil: "domcontentloaded", timeout: 30_000 });

  if (page.url().includes("account/login")) return { status: "failed", detail: "нужен вход на hh.ru" };
  if (await alreadyApplied(page)) return { status: "applied", detail: "отклик уже был на странице" };

  const respond = page.locator(RESPOND_BUTTON).first();
  if ((await respond.count()) === 0) {
    return { status: "needs_llm", detail: "кнопка отклика не найдена — нестандартная страница" };
  }

  if (!LIVE) {
    return { status: "dry_run_ok", detail: "кнопка отклика на месте; в боевом режиме ушло бы письмо" };
  }

  await respond.click();
  await page.waitForTimeout(2000);

  // Промежуточная модалка «вакансия из другой страны».
  const relocation = page.locator(RELOCATION_OK).first();
  if ((await relocation.count()) > 0) {
    await relocation.click().catch(() => {});
    await page.waitForTimeout(1500);
  }

  try {
    if ((await page.locator(LETTER_INPUT).count()) > 0) {
      await applyViaModal(page, vacancy.letter, cfg.resume_title);
    } else if (await alreadyApplied(page)) {
      await applyViaChat(page, vacancy.letter);
    } else {
      // Форма с вопросами работодателя: отвечать на них по резюме — работа
      // для модели, а не для скрипта.
      return { status: "needs_llm", detail: "похоже на тест работодателя — нужен агент" };
    }
  } catch (e) {
    return { status: "needs_llm", detail: `механика не отработала: ${String(e.message || e).split("\n")[0]}` };
  }

  // Проверка результата — единственный источник правды об успехе.
  await page.goto(vacancy.url, { waitUntil: "domcontentloaded", timeout: 30_000 });
  return (await alreadyApplied(page))
    ? { status: "applied", detail: "письмо отправлено" }
    : { status: "failed", detail: "после отправки страница не показывает «Вы откликнулись»" };
}

// ---------- основной проход ----------

async function main() {
  if (!existsSync(REGISTRY)) throw new Error("нет реестра вакансий");
  const cfg = parseYaml(readFileSync(CONFIG, "utf8"));
  const registry = parseYaml(readFileSync(REGISTRY, "utf8")) ?? [];

  const onlyId = flag("id");
  const limit = Number(flag("limit")) || Infinity;
  const queue = registry
    .filter((v) => (onlyId ? String(v.id) === onlyId : v.status === "matched" && v.letter))
    .slice(0, limit);

  if (!queue.length) {
    const summary = { applied: 0, needs_llm: [], failed: [], note: "нечего отправлять" };
    console.log(JSON_OUT ? JSON.stringify(summary, null, 2) : "нечего отправлять");
    return;
  }

  const [minPause, maxPause] = cfg.pause_between_applies_sec ?? [5, 15];
  const { context, profile } = await launch({ headless: false });
  log(`профиль: ${profile.name}${LIVE ? "" : " · DRY-RUN, отклики не отправляются"}`);
  const page = await context.newPage();

  const result = { applied: 0, already: 0, needs_llm: [], failed: [], dry_run: !LIVE };
  try {
    for (const [i, vacancy] of queue.entries()) {
      const r = await applyToVacancy(page, vacancy, cfg);
      log(`${vacancy.id} ${vacancy.title ?? ""} → ${r.status}: ${r.detail}`);

      if (LIVE && (r.status === "applied" || r.status === "failed")) {
        setStatus(vacancy.id, r.status, r.detail);
      }
      if (r.status === "applied") result.applied++;
      if (r.status === "needs_llm") result.needs_llm.push({ id: vacancy.id, detail: r.detail });
      if (r.status === "failed") result.failed.push({ id: vacancy.id, detail: r.detail });

      if (i < queue.length - 1) {
        const pause = minPause + Math.random() * (maxPause - minPause);
        await sleep(pause * 1000);
      }
    }
  } finally {
    await context.close().catch(() => {});
  }

  if (JSON_OUT) console.log(JSON.stringify(result, null, 2));
  else log(`\nотправлено: ${result.applied} · передать агенту: ${result.needs_llm.length} · ошибок: ${result.failed.length}`);
}

main().catch((e) => {
  console.error(String(e.message || e));
  process.exitCode = 1;
});
