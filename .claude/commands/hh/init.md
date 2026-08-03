---
description: Первичная настройка - заполнить config.yaml и resume.md по твоему резюме
argument-hint: "[путь к резюме (pdf/docx/md/txt) | ссылка на резюме hh.ru | просто текст резюме]"
---

Fill `config.yaml` and `resume.md` from the user's CV. Arguments: `$ARGUMENTS`

Both files are private (git-ignored) and are the input for every other stage: `config.yaml`
drives search and matching, `resume.md` is the only source of facts for cover letters.

**Never invent facts.** Everything written into `resume.md` must come from the CV the user
gave you. A metric you cannot find is not written — letters cite these numbers verbatim.

## Step 0 — Get the CV

From `$ARGUMENTS`:

- **A file path** (`.pdf`, `.docx`, `.md`, `.txt`) → read it. PDFs: read directly; `.docx`:
  `unzip -p <file> word/document.xml | sed 's/<[^>]*>//g'`.
- **An hh.ru resume URL** → load the Playwright tools and open it (the session from
  `/hh:login` is enough). If the browser is not logged in, ask the user to run `/hh:login`.
- **Pasted text** → use it as is.
- **Nothing** → look for an obvious CV in the repo root and in `~/Downloads`; if nothing
  turns up, ask the user for the file or the text. One question, then stop and wait.

## Step 1 — Extract

Pull out, and write down what you could NOT find:

| Field | Goes to |
|---|---|
| Name | `signature` |
| Job title / target role | `resume_title` (must match the resume title on hh.ru itself) |
| Portfolio / GitHub / GitLab link | `contacts.portfolio` |
| Telegram | `contacts.telegram` |
| Phone | `contacts.phone` |
| Primary language, frameworks, databases, infra | `matching.core_stack`, `matching.primary_languages` |
| Frontend technologies | `matching.frontend_stack` |
| Secondary / touched-once technologies | `matching.acceptable_secondary` |
| Years of experience | `matching.recheck.max_experience_years` |
| Achievements with numbers | `resume.md` → «Достижения с метриками» |

## Step 2 — Settle what a CV never contains

**If `$ARGUMENTS` ends with `auto`** (the app's «Set resume» button runs it that way, and
nobody is at the keyboard to answer): do not ask anything. Derive each value below from the
CV, keep the example's default when the CV says nothing, and list every such guess in the
final report — the user edits them in the app's Settings tab, which is exactly what that tab
is for.

Otherwise ask these together, in ONE message, with your suggestion for each so the user can
just say «ок». Do not guess silently:

1. Minimum salary and currency (`search.filters.salary`, `currency_code`).
2. Search queries (`search.filters.text`) — propose 2–4 built from the stack, e.g.
   «Laravel», «Backend разработчик», «Backend developer».
3. Work format and experience filter (`work_format`, `experience`) — propose from the CV.
4. Which frameworks are a hard skip (`matching.framework_blacklist`) — propose the usual
   suspects for the stack, e.g. legacy frameworks the user does not want to touch.
5. Anything in the CV that must NOT be used for matching (`matching.ignore_resume_skills`) —
   e.g. a framework listed once that the user never wants offers for.

Wait for the answers. Then continue.

## Step 3 — Write config.yaml

Start from `config.yaml.example`, keeping ALL its comments and structure, and substitute the
values. Never write a placeholder (`Имя Фамилия`, `@username`, `+00-00-00-00-00`) into the
real file — an unfilled contact goes into every letter.

If `config.yaml` already exists: do NOT overwrite it silently. Show the user the diff of what
you would change and ask.

## Step 4 — Write resume.md

Follow the structure of `resume.md.example`: who I am, hard skills, experience, «Достижения
с метриками», «Чего НЕТ».

The achievements section is what letters are built from, so each entry needs an action, a
real number and the technical decision behind it. If the CV has no numbers anywhere, say so
plainly and ask the user for two or three — without them letters fall back to generic prose
and the validation gates will fight you.

Fill «Чего НЕТ» from what is absent in the CV but common in the target vacancies — this is
the guard that stops letters from claiming experience the user does not have.

## Step 5 — Verify and report

Re-read both files, then report in Russian:

1. What was filled in, in one line each (`signature`, `resume_title`, contacts, stack).
2. What you could NOT find and left as is — explicitly, as a list of what to fill by hand.
3. The next step: `/hh:login` if the browser session was never set up, otherwise
   `./hhru-jobs --dry-run --verbose` for a trial run without real applications.
