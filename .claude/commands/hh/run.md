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

## Stage 1: collection (browser, sequential)
Loop over the search_urls built in Stage 0, one query at a time (in declared order). For each query:
- Compute the remaining count (config's `count` minus new vacancies already collected so far this run).
- If remaining count is 0 or there are no more queries, stop the loop.
- One Agent (subagent_type: hh-collector, run_in_background: false), in the prompt: this query's search_url, the CURRENT known_ids (seeded from Stage 0's registry ids, then topped up with every id collected by earlier queries in this same loop — so a later query never re-collects a vacancy an earlier query already picked up), and the remaining count.
- Response `error: login_required` → STOP the whole run (all remaining queries included). Report: «Куки протухли — запусти /hh:login».
- Response `error: captcha` → STOP the whole run: «Капча — нужно ручное вмешательство».
- Otherwise: parse the YAML, add the entries to data/vacancies-registry.yaml (status: pending), and add their ids to known_ids before moving to the next query.

After the loop: 0 new collected across ALL queries → report and exit ONLY IF the backlog from Stage 0 is also empty; if there's backlog, don't exit — continue to Stage 2 (pending backlog), Stage 3 (matched backlog without letter), Stage 4 (matched backlog with letter) as applicable.
- If `data/vacancies-registry.yaml` contains an empty flow-list `[]`, replace it with a regular YAML list of entries (don't append after the `[]`).
- Separate top-level vacancy entries with one blank line; write `description` as a single-line string.

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
For each candidate IN TURN: Agent (subagent_type: hh-applier, run_in_background: false), in the prompt id, url, letter, dry_run, resume_title (from config.yaml).
After EACH response, update the registry RIGHT AWAY: status applied (+applied_at ISO timestamp) / failed (+error) / already_applied → applied with a note; dry_run_ok → leave as matched, in the report's detail.
If login_required or captcha shows up in the response's detail/status — STOP the whole stage (leave the remaining matched vacancies untouched); state the reason in the report: login_required → «запусти /hh:login», captcha → «капча, ручное вмешательство».
Pause between responses: `sleep N` (Bash), N — a random value from the config's pause_between_applies_sec.

## Stage 5: report
Write `data/run-reports/<YYYY-MM-DDTHH-MM-SS>.md`:
table | id | title | company | status | reason/detail |, counters: collected/matched/skipped/applied/failed.
For any vacancy carrying a `validation_warning`, show it in the table's reason/detail column so the letter's non-convergence is visible.
Write the report content and the final summary in Russian — the user reads them.
Final message — a short summary of these counters and the path to the report.
