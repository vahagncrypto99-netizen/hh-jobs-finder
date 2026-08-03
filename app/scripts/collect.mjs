#!/usr/bin/env node
// Детерминированный сбор вакансий с hh.ru — без ИИ.
//
// Обходит поисковые запросы из config.yaml по очереди, отбрасывает уже
// известные/откликнутые/скрытые карточки, открывает страницу каждой новой
// вакансии и дописывает её в реестр со статусом `pending`.
//
//   node app/scripts/collect.mjs [--count=N] [--dry-run] [--json]
//
// Коды выхода: 0 — успех, 2 — нужен вход на hh.ru, 1 — ошибка.

import { readFileSync, appendFileSync, existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { parse as parseYaml } from "yaml";
import { launch, checkLogin, waitForVacancy } from "./hh-browser.mjs";

const REPO = join(dirname(fileURLToPath(import.meta.url)), "../..");
const CONFIG = join(REPO, "config.yaml");
const REGISTRY = join(REPO, "data/vacancies-registry.yaml");

const args = process.argv.slice(2);
const flag = (name) => args.find((a) => a.startsWith(`--${name}=`))?.split("=")[1];
const has = (name) => args.includes(`--${name}`);

const DRY = has("dry-run");
const JSON_OUT = has("json");

const log = (msg) => {
  if (!JSON_OUT) console.log(msg);
};

// ---------- config ----------

function loadConfig() {
  if (!existsSync(CONFIG)) {
    throw new Error("нет config.yaml — заполни его кнопкой «Set resume…» или из config.yaml.example");
  }
  return parseYaml(readFileSync(CONFIG, "utf8"));
}

/** URL поиска: базовый адрес + фильтры из конфига + один поисковый запрос. */
function searchUrl(cfg, text, page) {
  const f = cfg.search?.filters ?? {};
  const params = new URLSearchParams();
  params.set("text", text);
  for (const [key, value] of Object.entries(f)) {
    if (key === "text" || value === null || value === undefined) continue;
    params.set(key, String(value));
  }
  if (page > 0) params.set("page", String(page));
  return `${cfg.search.base_url}?${params}`;
}

// ---------- registry ----------

/** id уже собранных вакансий — по ним идёт дедупликация между запусками. */
function knownIds() {
  if (!existsSync(REGISTRY)) return new Set();
  const text = readFileSync(REGISTRY, "utf8");
  return new Set([...text.matchAll(/^- id: "(\d+)"/gm)].map((m) => m[1]));
}

const yamlString = (value) =>
  `"${String(value).replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;

/** Запись реестра в том же стиле, в каком её писал LLM-агент. */
function renderEntry(v) {
  const skills = (v.skills ?? []).map(yamlString).join(", ");
  return [
    `- id: ${yamlString(v.id)}`,
    `  url: ${v.url}`,
    `  title: ${yamlString(v.title ?? "")}`,
    `  company: ${yamlString(v.company ?? "")}`,
    `  salary: ${v.salary ? yamlString(v.salary) : "null"}`,
    `  experience: ${v.experience ? yamlString(v.experience) : "null"}`,
    `  work_format: ${v.work_format ? yamlString(v.work_format) : "null"}`,
    `  employment: ${v.employment ? yamlString(v.employment) : "null"}`,
    `  skills: [${skills}]`,
    `  description: ${yamlString(v.description ?? "")}`,
    `  status: pending`,
    "",
  ].join("\n");
}

function appendToRegistry(vacancies) {
  if (!vacancies.length) return;
  const existing = existsSync(REGISTRY) ? readFileSync(REGISTRY, "utf8") : "";
  // Реестр однажды уже склеился из-за отсутствия перевода строки в конце файла.
  const separator = existing && !existing.endsWith("\n") ? "\n" : "";
  appendFileSync(REGISTRY, separator + vacancies.map(renderEntry).join("\n"));
}

// ---------- парсинг страниц ----------

const CARDS_JS = () =>
  Array.from(document.querySelectorAll('article[data-qa="vacancy-serp__vacancy"]')).map((a) => {
    const link = a.querySelector('a[data-qa="serp-item__title"]');
    const url = (link?.href || "").split("?")[0];
    return {
      id: (url.match(/vacancy\/(\d+)/) || [])[1] || null,
      title: a.querySelector('[data-qa="serp-item__title-text"]')?.textContent.trim() || null,
      url,
      responded: !!a.querySelector('[data-qa="vacancy-serp__vacancy_responded"]'),
      discarded: !!a.querySelector('[data-qa="vacancy-serp__vacancy_discard"]'),
    };
  }).filter((v) => v.id);

const DETAILS_JS = () => ({
  title: document.querySelector('[data-qa="vacancy-title"]')?.textContent.trim() || null,
  salary:
    document.querySelector('[data-qa="vacancy-salary"]')?.textContent.replace(/ /g, " ").trim() ||
    null,
  experience: document.querySelector('[data-qa="vacancy-experience"]')?.textContent.trim() || null,
  employment: document.querySelector('[data-qa="common-employment-text"]')?.textContent.trim() || null,
  work_format:
    Array.from(document.querySelectorAll('[data-qa="work-formats-text"]'))
      // На странице подпись поля склеена со значением: «Формат работы: удалённо».
      .map((e) => e.textContent.trim().replace(/^Формат работы:\s*/i, ""))
      .join(", ") || null,
  company: document.querySelector('[data-qa="vacancy-company-name"]')?.textContent.trim() || null,
  description:
    document.querySelector('[data-qa="vacancy-description"]')?.innerText.trim().replace(/\s+/g, " ") ||
    null,
  skills: Array.from(document.querySelectorAll('[data-qa="skills-element"]')).map((e) =>
    e.textContent.trim(),
  ),
  // Баннер «Вы откликнулись» держится только сразу после отправки; позже
  // остаётся ссылка на отклик. Проверяем оба признака.
  already_responded:
    document.body.innerText.includes("Вы откликнулись") ||
    !!document.querySelector('[data-qa="vacancy-response-link-view-topic"]'),
});

// ---------- основной проход ----------

async function main() {
  const cfg = loadConfig();
  const limit = Number(flag("count")) || cfg.defaults?.count || 20;
  const maxPages = cfg.parsing?.max_pages ?? 10;
  // Разовые переопределения (config.yaml не трогают): удобно, когда нужно
  // добрать вакансий по конкретному запросу или заглянуть глубже по времени.
  const queryOverride = flag("query");
  const periodOverride = flag("period");
  if (periodOverride) cfg.search.filters.search_period = Number(periodOverride);

  const queries = queryOverride ? [queryOverride] : (cfg.search?.filters?.text ?? []);
  if (!queries.length) throw new Error("в config.yaml нет поисковых запросов");

  const known = knownIds();
  const collected = [];
  const warnings = [];

  const { context, profile } = await launch();
  log(`профиль: ${profile.name} (${profile.cookies} cookies)`);
  const page = await context.newPage();

  try {
    for (const query of queries) {
      if (collected.length >= limit) break;

      for (let pageNum = 0; pageNum < maxPages; pageNum++) {
        if (collected.length >= limit) break;

        await page.goto(searchUrl(cfg, query, pageNum), {
          waitUntil: "domcontentloaded",
          timeout: 30_000,
        });

        if (pageNum === 0 && query === queries[0]) {
          const auth = await checkLogin(page);
          if (!auth.loggedIn) {
            console.log("error: login_required");
            process.exitCode = 2;
            return;
          }
        }

        const cards = await page.evaluate(CARDS_JS);
        if (!cards.length) break; // выдача кончилась

        const fresh = cards.filter(
          (c) => !known.has(c.id) && !c.responded && !c.discarded,
        );
        log(`«${query}» стр.${pageNum + 1}: ${cards.length} карточек, новых ${fresh.length}`);

        for (const card of fresh) {
          if (collected.length >= limit) break;
          known.add(card.id); // запросы пересекаются — второй раз не открываем

          try {
            await page.goto(card.url, { waitUntil: "domcontentloaded", timeout: 30_000 });
            await waitForVacancy(page);
            const details = await page.evaluate(DETAILS_JS);

            if (details.already_responded) continue; // отклик уже был
            if (!details.title || !details.description) {
              warnings.push(`${card.id}: не извлеклись title/description — селекторы могли поменяться`);
              continue;
            }

            collected.push({ id: card.id, url: card.url, ...details });
            log(`  + ${details.title} — ${details.company}`);
          } catch (e) {
            warnings.push(`${card.id}: ${String(e.message || e).split("\n")[0]}`);
          }
        }
      }
    }
  } finally {
    await context.close().catch(() => {});
  }

  if (DRY) {
    // Показать ровно то, что ушло бы в реестр — иначе dry-run проверяет
    // только обход страниц, но не формат записи.
    if (!JSON_OUT && collected.length) {
      log("\n--- что было бы дописано в реестр ---");
      log(collected.map(renderEntry).join("\n"));
    }
  } else {
    appendToRegistry(collected);
  }

  const summary = {
    collected: collected.length,
    ids: collected.map((v) => v.id),
    queries_used: queries.length,
    dry_run: DRY,
    warnings,
  };
  if (JSON_OUT) console.log(JSON.stringify(summary, null, 2));
  else {
    log(`\nсобрано новых: ${collected.length}${DRY ? " (dry-run, реестр не тронут)" : ""}`);
    for (const w of warnings) log(`внимание: ${w}`);
  }
}

main().catch((e) => {
  console.error(String(e.message || e));
  process.exitCode = 1;
});
