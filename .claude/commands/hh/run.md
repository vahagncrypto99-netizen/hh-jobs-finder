---
description: Conductor for hh.ru auto-apply - collect, analyze, letters, apply
argument-hint: workers=N count=C [dry-run]
---

You are the conductor of the hh.ru auto-apply pipeline. Arguments: $ARGUMENTS
(workers — parallelism of the LLM stages; count — how many NEW vacancies to process; dry-run — don't actually submit responses).
Take defaults from `config.yaml` → `defaults`.

IRON RULES:
- Browser agents (hh-collector, hh-applier) — strictly one at a time, NEVER in parallel.
- You are the ONLY one who edits `data/vacancies-registry.yaml`. Subagents return YAML as text.
- One vacancy failing doesn't bring down the run: mark it failed and continue.
- Pass data to subagents DIRECTLY IN THE PROMPT (YAML blocks) — they must not go looking for it in files.

## Stage 0: setup
Read `config.yaml` and `data/vacancies-registry.yaml`. Build the known_ids list (all ids in the registry).
Build the backlog from the registry: entries with status `pending` (not yet analyzed), plus entries with status `matched` (letter present or not — not yet applied/failed). This backlog carries over from previous runs and feeds Stages 2-4 below regardless of how many new vacancies Stage 1 collects.

`search.filters.text` is a list of search queries. For EACH query in `search.filters.text` (in declared order) build a search_url from `base_url` + that query as `text` + the remaining filters (URL-encode values; spaces in `text` become `+`; booleans stay lowercase `true`/`false`). You'll end up with one search_url per query, to be worked through in order in Stage 1.

## Stage 1: collection (deterministic script, no LLM)
Run ONE Bash command — the script walks every query itself, deduplicates against the registry
and appends the new vacancies with `status: pending`:

```bash
./scripts/collect.sh --count=<count> --json
```

There is no hh-collector agent to dispatch: opening search pages and reading `data-qa`
attributes is mechanical work that a script does faster, cheaper and identically every time.
You do NOT touch the registry in this stage — the script writes it.

Read the JSON it prints (`collected`, `ids`, `warnings`) and act on the exit code:
- **2** → cookies expired. STOP the whole run: «Куки протухли — запусти /hh:login».
- **1** → report the stderr line and STOP.
- **0** → continue. Log any `warnings` in the final report: they mean a `data-qa` selector
  stopped matching and the parsing skill needs updating.

After the script: 0 new collected → report and exit ONLY IF the backlog from Stage 0 is also
empty; if there's backlog, continue to Stage 2 (pending backlog), Stage 3 (matched backlog
without letter), Stage 4 (matched backlog with letter) as applicable.

## Stage 2: analysis (parallel, workers-many)
Split the pending vacancies into workers roughly-equal batches — this includes both vacancies just collected in Stage 1 AND any pending backlog from Stage 0 (earlier runs). Launch hh-analyzer agents IN PARALLEL (all Agent calls in one message, run_in_background: false), each given its batch of YAML blocks IN FULL (including description).
Collect the verdicts, update the registry: status matched/skipped + verdict_reason.

## Stage 3: letters (parallel, workers-many)
`config.yaml` → `letter.mode` decides HOW letters are written (`ai` — the full skill pipeline; `template` — filling `letter.template` with no AI writing and no validation gates). The hh-letter skill reads the mode itself; pass it in the agent prompt so the agent never has to guess.

Same thing for matched vacancies with an EMPTY letter field (don't regenerate existing letters) via hh-letter agents — this includes matched backlog from earlier runs, not just this run's newly-matched ones. Write the letters into the registry (letter field). If an hh-letter response includes `validation_warning` alongside `letter`, save it in the registry next to the letter (field validation_warning) — it means the letter never converged in the skill's validation pipeline.

## Stage 4: responses (browser, strictly one at a time)
Candidates: matched vacancies with a non-empty `letter` and NO `validation_warning` — including matched backlog from earlier runs whose letter was already written and just never applied. A vacancy
carrying `validation_warning` (letter never converged) or an empty letter is NOT applied to —
leave it as matched and show the reason in the report, so the user can fix the letter and rerun.
Run ONE Bash command — the script walks the queue itself, keeps the configured pause between
responses and updates the registry after each one:

```bash
./scripts/apply.sh --live --json          # боевой режим
./scripts/apply.sh --json                 # dry-run: ничего не отправляется
```

**Omit `--live` whenever the run is a dry-run.** Without that flag the script never clicks
«Откликнуться» — that is the safety switch against an accidental mass mailing.

The script handles the two mechanical scenarios (modal with a letter field, and letter sent
into the response chat). It deliberately does NOT handle the employer test: answering those
questions needs the resume and judgement. Everything it is not sure about comes back as
`needs_llm` instead of a random click.

Read the JSON (`applied`, `needs_llm`, `failed`):
- For **every entry in `needs_llm`** dispatch ONE `hh-applier` agent (run_in_background: false,
  strictly one at a time) with id, url, letter, dry_run and resume_title from config.yaml, and
  update that vacancy's registry status from the agent's answer. This is the only place the
  browser agent is still used.
- `failed` entries stay in the report with their detail; the script has already written their
  status.
- If a detail mentions login_required or captcha — STOP the stage and say so in the report:
  login_required → «запусти /hh:login», captcha → «капча, ручное вмешательство».

## Stage 5: report
Write `data/run-reports/<YYYY-MM-DDTHH-MM-SS>.md`:
table | id | title | company | status | reason/detail |, counters: collected/matched/skipped/applied/failed.
For any vacancy carrying a `validation_warning`, show it in the table's reason/detail column so the letter's non-convergence is visible.
Write the report content and the final summary in Russian — the user reads them.
Final message — a short summary of these counters and the path to the report.
