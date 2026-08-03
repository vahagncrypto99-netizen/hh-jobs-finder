---
name: hh-matching
description: Use when deciding whether a hh.ru vacancy matches the candidate's resume - hard-skip rules (title blacklist, wrong primary framework), match rules (core-stack match, learnable secondary tech — all lists live in config.yaml). Триггеры - анализ вакансии, вердикт match/skip, hh-analyzer stage.
---

# Matching a vacancy against the resume

**First step: read `config.yaml` → `matching`.** All lists and thresholds below live there; this
skill only describes what to do with them. The user rotates the config's values without touching
this file — if a list looks empty or stale, re-read the config, don't fall back on memory.

Source of truth for the candidate: `resume.md` at the project root.

## Hard skips (any item hits → skip, don't look further)
1. Title contains (RU/EN, any case) any word from `matching.title_blacklist` — the authoritative
   list lives in config.yaml. IMPORTANT: words in `matching.title_ok_levels` are NOT a skip —
   they're match-level signals, not lead/head roles.
2. The vacancy's primary framework is in `matching.framework_blacklist`. "Primary" = named in the
   title or listed in the requirements as mandatory experience. Mentioned as "a plus" / "nice to
   have" → NOT a skip, regardless of which framework it is.
3. Primary language isn't in `matching.primary_languages`: the vacancy is about a different
   language entirely — a language not listed in `matching.primary_languages` as the main stack —
   with a listed language mentioned only in passing or as "a plus".

## Matches
- A `matching.primary_languages` language with no framework specified → match (our framework
  experience from `matching.core_stack` is relevant).
- Anything in `matching.core_stack` in any form → match.
- Fullstack with `matching.frontend_stack` → match. Anything in `matching.acceptable_secondary`
  as a secondary skill → match.
- Secondary, easily learnable technologies do NOT count as a gap: anything in
  `matching.learnable_tech` — tools at the "learn in a week or two" level.
- Never credit resume skills listed in `matching.ignore_resume_skills` toward a match — they
  don't count as relevant experience even though they appear in `resume.md`.
- Experience range and remote work are already filtered by the URL, but double-check per
  `matching.recheck`: more years of experience required than `recheck.max_experience_years` in
  the description → skip; `recheck.reject_if_office_only` and the description says office-only → skip.

## Principle
The match only needs to hold on the CORE requirements (`matching.core_stack` /
`matching.frontend_stack`) — secondary ones can be discounted. When in doubt →
`matching.doubt_policy` (currently: match — a letter and a response are cheap, a missed vacancy
is expensive).

## Verdict format (YAML, as text)
Write `reason` in Russian — the user reads it.
```yaml
- id: "135220333"
  verdict: skip
  reason: "Основной стек <framework из matching.framework_blacklist> — в заголовке и обязательных требованиях"
- id: "134213825"
  verdict: match
  reason: "Прямое совпадение с ядром стека (matching.core_stack)"
```
