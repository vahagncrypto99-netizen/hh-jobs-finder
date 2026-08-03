---
name: hh-applying
description: Use when submitting a response to a hh.ru vacancy - three scenarios (modal with cover letter, chat after an instant response, employer test), verification, dry-run. Триггеры - отклик на вакансию, hh-applier stage.
---

# Applying to a hh.ru vacancy

Write every free-text `detail` in Russian — the user reads it.

Selectors only via `data-qa` and visible text (via browser_snapshot + click by ref).
Playwright tools if needed: ToolSearch `select:mcp__playwright__browser_navigate,mcp__playwright__browser_snapshot,mcp__playwright__browser_click,mcp__playwright__browser_type,mcp__playwright__browser_evaluate,mcp__playwright__browser_wait_for,mcp__playwright__browser_take_screenshot,mcp__playwright__browser_fill_form,mcp__playwright__browser_press_key`.

## Step 0: preparation
1. Navigate to the vacancy url. Redirected to login → `failed`, detail: login_required.
2. Evaluate: `() => document.body.innerText.includes('Вы откликнулись')` → true → status `already_applied`, stop.
3. Snapshot the page. Find the «Откликнуться» button (`[data-qa="vacancy-response-link-top"]` or by text in the snapshot).

## DRY-RUN: general rule (check BEFORE Step 1)
In dry-run mode NEVER click the «Откликнуться» button — under no circumstances, no heuristics
about "signs of a mandatory letter". It's enough to confirm from the snapshot that the «Откликнуться»
button exists (Step 0, item 3), and return `dry_run_ok`, detail: "response button found; in live mode
the letter would go through the modal or the chat". Scenarios A/B/C below are the live flow
(dry_run=false); dry-run never reaches them.

## Step 1: click «Откликнуться» (live mode)
After the click, an intermediate "vacancy from another country" modal may appear —
button `[data-qa="relocation-warning-confirm"]` / text «Всё равно откликнуться» → click it.

Then one of 3 scenarios — determine from the snapshot:

### Scenario A: modal with a cover-letter field
Signal: textarea `[data-qa="vacancy-response-popup-form-letter-input"]` (placeholder «Сопроводительное письмо»).
1. If the modal has a resume picker (multiple resumes on the account) — pick the resume named
   in config.yaml → resume_title (passed in the prompt as `resume_title`).
2. Type the letter into the textarea (browser_type or fill_form).
3. Click the «Откликнуться» button in the modal (`[data-qa="vacancy-response-submit-popup"]`).

### Scenario B: response sent instantly (no modal)
Signal: the page shows «Вы откликнулись» / the button changed to «Чат».
1. Click «Чат».
2. In the chat, click «Добавить сопроводительное» (link under the "Отклик на вакансию" message).
3. Type the letter into the message field, send it (send button / Enter).

### Scenario C: employer test
Signal: after the click, a page/form with questions opened (textarea/radio with employer questions).
1. Use a snapshot to collect ALL questions.
2. Answer each one in the first person, professionally, honestly, based on resume.md, **in Russian**
   (or in the question's own language if it isn't Russian). Don't invent experience that isn't
   there (see the «Чего НЕТ» section in `resume.md`). Human style, no filler, 2-5 sentences per open question.
3. Submit the form → then the letter goes through the chat (Scenario B, steps 1-3).

## Step 2: verification (except in dry-run)
Navigate to the vacancy url again. Evaluate `document.body.innerText.includes('Вы откликнулись')`:
- true → `applied`
- false → `failed`, screenshot, detail: what's visible on the page.

## Errors
- Captcha / «Доступ ограничен» → screenshot, `failed`, detail: captcha.
- Unexpected layout → screenshot, `failed` with a description. Do NOT click at random.
