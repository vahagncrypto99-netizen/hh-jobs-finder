---
name: hh-parsing
description: Use when collecting/parsing hh.ru vacancy search results and vacancy pages into structured YAML - selectors (data-qa), pagination, dedup rules, output schema. Триггеры - сбор вакансий hh, парсинг выдачи hh.ru, hh-collector pipeline stage.
---

# Parsing hh.ru: search results and vacancy page

## Rules
- Selectors ONLY via `data-qa`. CSS classes like `magritte-*___hash` are unstable — do not use them.
  Known exception: the search-card salary has no `data-qa`; use the partial-class selector
  `[class*="compensation-labels"]` from the extractor below. If it stops matching, return
  `salary: null` — do not hunt for hashed class names.
- Extract via `mcp__playwright__browser_evaluate` in a single JS call (fast, no giant snapshots).
- If the Playwright tools aren't loaded — ToolSearch first: `select:mcp__playwright__browser_navigate,mcp__playwright__browser_evaluate,mcp__playwright__browser_wait_for,mcp__playwright__browser_snapshot,mcp__playwright__browser_take_screenshot,mcp__playwright__browser_click`.

## Login check (do this FIRST)
After navigating to search_url, run evaluate:
```js
() => ({
  loggedIn: !document.querySelector('[data-qa="login"]') && !location.href.includes('account/login'),
  url: location.href
})
```
If `loggedIn: false` or redirected to `account.hh.ru` — IMMEDIATELY return `error: login_required`, do nothing else.

## Parsing the search results
Cards: `article[data-qa="vacancy-serp__vacancy"]`. JS for evaluate:

```js
() => Array.from(document.querySelectorAll('article[data-qa="vacancy-serp__vacancy"]')).map(a => {
  const link = a.querySelector('a[data-qa="serp-item__title"]');
  const url = (link?.href || '').split('?')[0];
  return {
    id: (url.match(/vacancy\/(\d+)/) || [])[1] || null,
    title: a.querySelector('[data-qa="serp-item__title-text"]')?.textContent.trim() || null,
    url,
    company: a.querySelector('[data-qa="vacancy-serp__vacancy-employer-text"]')?.textContent.trim() || null,
    salary: a.querySelector('[class*="compensation-labels"] > span')?.textContent.replace(/\u00A0/g, ' ').trim() || null,
    experience: a.querySelector('[data-qa^="vacancy-serp__vacancy-work-experience"]')?.textContent.trim() || null,
    remote: !!a.querySelector('[data-qa="vacancy-label-work-schedule-remote"]'),
    responded: !!a.querySelector('[data-qa="vacancy-serp__vacancy_responded"]'),
    discarded: !!a.querySelector('[data-qa="vacancy-serp__vacancy_discard"]')
  };
}).filter(v => v.id)
```

## Filtering cards (before opening pages)
Skip: `id` already in known_ids; `responded: true` (already applied); `discarded: true` (rejected).

## Pagination
Parameter `&page=N`, 0-indexed. Keep going through pages until `count` new vacancies are
collected, or a page returns 0 cards (end of results). Maximum `parsing.max_pages` pages (see
`config.yaml`) — guards against looping forever.

## Vacancy page (for each new vacancy)
Navigate to `url`, wait for `[data-qa="vacancy-title"]` (browser_wait_for on the vacancy text, or evaluate-polling), then evaluate:

```js
() => ({
  title: document.querySelector('[data-qa="vacancy-title"]')?.textContent.trim() || null,
  salary: document.querySelector('[data-qa="vacancy-salary"]')?.textContent.replace(/\u00A0/g, ' ').trim() || null,
  experience: document.querySelector('[data-qa="vacancy-experience"]')?.textContent.trim() || null,
  employment: document.querySelector('[data-qa="common-employment-text"]')?.textContent.trim() || null,
  work_format: Array.from(document.querySelectorAll('[data-qa="work-formats-text"]')).map(e => e.textContent.trim()).join(', ') || null,
  company: document.querySelector('[data-qa="vacancy-company-name"]')?.textContent.trim() || null,
  description: document.querySelector('[data-qa="vacancy-description"]')?.innerText.trim().replace(/\s+/g, ' ') || null,
  skills: Array.from(document.querySelectorAll('[data-qa="skills-element"]')).map(e => e.textContent.trim()),
  already_responded: document.body.innerText.includes('Вы откликнулись')
})
```

If a selector didn't hit (null in the critical title/description fields) — take a browser_snapshot
and pull the data from it; the data-qa attribute may have changed — flag this in the response as a warning.

## Output schema (YAML, return as text)
```yaml
- id: "134213825"
  url: https://hh.ru/vacancy/134213825
  title: Fullstack Software Engineer
  company: Tinkers
  salary: "3 000 – 4 000 $ за месяц, на руки"
  experience: "3–6 лет"
  work_format: "Удалённо"
  employment: "Полная занятость"
  skills: [<skill tags as listed on the vacancy page>]
  description: "<full description text — single line: collapse all whitespace/newlines to single spaces; do not truncate>"
  status: pending
```
The `description` field — full text, no truncation: the analyzer works off of it. Write it as a single-line
quoted string — collapse all whitespace/newlines to single spaces; do not truncate.
