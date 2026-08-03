---
name: hh-letter
description: "Write concise, tailored Russian hh.ru cover letters from a vacancy and resume. Preserve exact contacts and facts; use the X-Y-Z method to select evidence, not as a rigid prose template. Triggers: cover letter, сопроводительное письмо, hh-letter stage."
---

# hh.ru Cover Letter

Candidate data: `resume.md` ("Достижения с метриками" section). Contacts, signature, and all
validation thresholds below: `config.yaml` → `letter_validation` (and top-level `contacts` /
`signature`).

**Letters are ALWAYS written in Russian.**

## Mode switch — read this FIRST

`config.yaml` → `letter.mode` decides who writes the letter:

- **`template`** — no AI writing. Read the file at `letter.template` (default
  `templates/letter.md`), substitute its variables, pick exactly ONE experience block by the
  vacancy topic, and output the result. **Skip the whole validation pipeline below** (the
  template text is human-written; re-running gates on it only invites pointless rewrites).
  Still enforce: contacts/signature byte-for-byte, no invented facts, letter in Russian.
- **`ai`** (default) — everything below applies: X-Y-Z evidence selection, the openers rules,
  drafting rules, and all three gates.

If `letter.mode` is missing or unreadable, assume `ai`.

## Goal

Write an individual, concise reply to this vacancy, not a universal motivation letter. It should
read as though the candidate chose one relevant experience and explains why it applies here.
Natural does **not** mean deliberately adding typos, slang, broken grammar, or invented personal
details.

Use X-Y-Z to choose the evidence:

- **X** — what the candidate personally built, fixed, or owned;
- **Y** — a real outcome or metric, if one makes the point clearer;
- **Z** — the concrete technical decision or method that produced it.

Do not force X, Y, and Z into the wording `я сделал X, что подтверждается Y, благодаря Z`.
Weave them into a normal sentence or two.

## Letter shape

```
Здравствуйте, [имя рекрутера, если известно, иначе «команда <Компания>»]!

<Короткая человеческая фраза-вход (см. «Openers» ниже), затем в том же абзаце — релевантный опыт под конкретную задачу из вакансии.>

<Один абзац с X-Y-Z: действие кандидата, важный контекст и, если уместно, метрика.>

<Необязательная короткая связка с одной задачей вакансии. Не пересказывай вакансию.>

<Необязательное конкретное завершение: что кандидат сможет подробнее разобрать на встрече.>

Портфолио: <contacts.portfolio>
Telegram: <contacts.telegram>
Тел.: <contacts.phone>

С уважением,
<signature>
```

Substitute `<contacts.*>` and `<signature>` with the exact values from `config.yaml` —
byte-for-byte, no reformatting. Never type placeholder text into a real letter.

## Openers — как начинать письмо

The opener is ONE short, human sentence before the substance. **Never open with the form
formula «Откликаюсь на вакансию/позицию …»** — past runs converged on it in nearly every
letter, and a recruiter seeing the same first words across candidates reads it as a template.

Rotate between patterns like these (and their natural variations):

- «Прочитал вакансию целиком — судя по описанию, основная работа у вас <конкретная задача
  или деталь из вакансии>. Последние годы занимаюсь именно этим.» The claim of having read
  the vacancy MUST be backed by naming a specific detail from it in the same or next
  sentence — without that detail the phrase is empty filler and fails Gate 3.
- Straight into the overlap, no meta-sentence at all: «<Задача из вакансии> — то, с чем
  работаю последние годы на …»
- A reaction to one concrete requirement: «У вас упор на <X> — как раз это я делал на <проект>.»

Do not reuse one opener pattern for every letter in a run — vary them across vacancies. The
filler rule applies to the first sentence hardest of all: an opener that would fit an
unrelated vacancy unchanged is filler.

## Drafting rules
- Read the vacancy, `resume.md`, and `config.yaml` before drafting. Pick one main overlap:
  match the vacancy's core topic to an achievement in the same topic from `resume.md` (e.g.
  payments → payment-integration achievement; high-load → performance/scaling achievement);
  for a fullstack vacancy, pick an achievement using technologies from `matching.frontend_stack`.
  Never invent metrics or responsibilities absent from `resume.md`.
- Open per the «Openers» section above: a short human entry sentence tied to a role
  requirement or task, never the «Откликаюсь…» formula and never generic interest in the vacancy.
- Ground the body in at least two concrete anchors from the source materials: a project, a task,
  a technology, a metric, or a product constraint. Mention overlapping technologies in prose,
  not as a stack dump.
- Add a connection to one specific vacancy requirement only when it is real. Do not praise the
  company or claim to understand its projects unless that follows from the vacancy text.
- Use first person and direct verbs: `собрал`, `настроил`, `вынес`, `подключал`. Prefer one
  precise term over synonym cycling.
- Let paragraph length and sentence rhythm follow the thought. Two substantive paragraphs are
  usually enough; use a third only when it contributes a distinct, relevant fact. Do not make
  every paragraph perform an identical "claim → proof → conclusion" arc.
- Keep the tone polite, businesslike, and conversational. Use normal Russian grammar and
  punctuation; no emoji, no artificial informality, and no deliberately planted mistakes.
- Never use these empty formulas or close variants: `Меня заинтересовала вакансия`, `у меня
  есть релевантный опыт`, `готов сразу включиться`, `меня привлекают ваши проекты`, `буду рад
  обсудить детали на собеседовании`, `legacy-код меня не пугает`.
- Avoid канцелярит and AI-speak: `в динамично развивающуюся компанию`, `обладаю широким
  спектром`, `данная вакансия`, `в рамках`, `осуществлял`, `ключевой`, `уникальная возможность`.
- A sentence that would fit unchanged in an application to an unrelated employer is filler.
  Delete it or replace it with a sourced fact.
- Length: `letter_validation.length_range` characters, excluding the contacts block and signature.
- Name `letter_validation.required_tech_mentions` vacancy technologies that overlap with the
  resume explicitly (not as a list).
- Job title and company name must match the vacancy exactly.

## Validation pipeline

Every drafted letter must pass three gates, in order, before it can appear in the output.
**Any gate fails → rewrite the letter → restart from Gate 1.** Repeat until all three gates
pass within the same iteration. **Maximum `letter_validation.max_iterations` iterations.** If
the letter still hasn't converged after that many iterations, output the best draft with
`validation_warning: <what failed>` in the YAML (see Output format below) instead of looping
further.

### Gate 1 — Text quality and AI-pattern check (ru-ai-check)
The letter is a concise professional Russian letter. Invoke the `ru-ai-check` skill and score the
letter body only (without the contacts block and signature) by its scoring rubric
(`.claude/skills/ru-ai-check/references/scoring.md`, bundled with this repository — composite 0–10
across typography, clarity, grammar, structure, reader precision). The gate passes when the
composite score is at least
`letter_validation.ru_ai_check_min_score` («Хороший» and above) and no High-severity addendum
(AD-14/AD-15) fired.

If it fails, inspect the per-dimension issues and rewrite the exact fragments they quote. Start by
removing generic openers, formulaic transitions, over-smooth paragraph structure, канцелярит, and
templated closers; replace them with a concrete fact already present in the source material. Do not
try to game the score with planted errors or forced colloquialisms — faked sloppiness lowers the
grammar dimension just as over-polish lowers clarity.

Apply ru-ai-check's cited fixes rather than freestyle rewriting when a targeted fix still leaves
several issues. Preserve every fact, number, URL, name, job title, company name, contact, and
signature byte-for-byte, then verify the resulting text against the vacancy yourself. Restart at
Gate 1.

### Gate 2 — Length
`letter_validation.length_range` characters, excluding the contacts block and signature.
- Too long → cut filler, never cut facts.
- Too short → add one concrete detail from the vacancy description (never invented).
Either case → restart at Gate 1.

### Gate 3 — Context checklist
Every item below must hold:
- job title and company name match the vacancy exactly
- `letter_validation.required_tech_mentions` overlapping technologies from the vacancy are
  named explicitly
- the letter contains one specific, truthful connection to the vacancy description
- the X-Y-Z achievement is drawn from `resume.md` (metrics not invented)
- the body contains at least two concrete anchors from the resume or vacancy
- the opening is not generic, does NOT start with «Откликаюсь на вакансию/позицию …», and
  (in a batch run) does not repeat the opener pattern of the other letters in the same run;
  the closing, if present, is specific rather than ritual
- no filler sentence could be reused unchanged for an unrelated employer
- the contacts block and signature are present byte-for-byte
- the letter is written in Russian

Any item fails → restart at Gate 1.

**Do not resume from the gate that failed — always restart the whole check from Gate 1.**
A letter edited to fix Gate 3 can reintroduce an AI-sounding phrase or break the length, so
Gate 1 and Gate 2 must be re-verified against every rewrite, not assumed still valid.

## Output format (YAML, as text)
```yaml
- id: "134213825"
  letter: |
    Здравствуйте, команда Tinkers!
    ...
  validation_warning: "Gate 2 length still out of range after max_iterations"  # only if gates never converged
```
