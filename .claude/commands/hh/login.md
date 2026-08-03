---
description: One-time hh.ru login - opens a Playwright browser, cookies are saved in its profile
---

Help the user log into hh.ru once, so the cookies persist in the Playwright MCP persistent profile.

1. Load the Playwright tools (ToolSearch: `select:mcp__playwright__browser_navigate,mcp__playwright__browser_evaluate,mcp__playwright__browser_wait_for,mcp__playwright__browser_snapshot`).
2. Open `https://hh.ru/account/login`.
3. Tell the user (in Russian): «Окно браузера открыто — залогинься в hh.ru вручную (логин/пароль или код). Напиши мне "готово", когда закончишь». Stop and wait for the reply.
4. After «готово»: open `https://hh.ru/applicant/resumes`. Evaluate: `() => ({loggedIn: !location.href.includes('login'), url: location.href, hasResume: document.body.innerText.includes('Мои резюме')})`.
5. `loggedIn: true` → tell the user (in Russian) «Логин сохранён, система готова. Запускай ./hhru-jobs». Otherwise — repeat from step 3.
