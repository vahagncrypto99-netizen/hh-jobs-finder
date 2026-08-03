use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use tauri::{Manager, State};

mod tray;

// ---------- paths ----------

fn repo_root() -> PathBuf {
    if let Ok(p) = std::env::var("HHRU_ROOT") {
        return PathBuf::from(p);
    }
    // app/src-tauri -> app -> repo root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("cannot resolve repo root")
}

fn config_path() -> PathBuf {
    repo_root().join("config.yaml")
}

fn plist_path() -> PathBuf {
    repo_root().join("scheduler/com.hh-jobs.plist")
}

fn manual_log_path() -> PathBuf {
    repo_root().join("data/run-reports/app-manual.log")
}

fn auth_log_path() -> PathBuf {
    repo_root().join("data/run-reports/app-auth-check.log")
}

fn registry_path() -> PathBuf {
    repo_root().join("data/vacancies-registry.yaml")
}

fn resume_path() -> PathBuf {
    repo_root().join("resume.md")
}

fn init_log_path() -> PathBuf {
    repo_root().join("data/run-reports/app-init.log")
}

fn launchd_log_path() -> PathBuf {
    repo_root().join("data/run-reports/launchd.log")
}

// ---------- run progress ----------

/// Vacancy counts per status, straight from the registry the pipeline writes.
/// A line scan rather than a YAML parse: the file carries full descriptions
/// (~300 KB) and this runs on every poll.
#[derive(Serialize, Clone, Copy, Default, Debug, PartialEq)]
pub struct RegistryCounts {
    total: i64,
    pending: i64,
    matched: i64,
    skipped: i64,
    applied: i64,
    failed: i64,
}

fn count_registry(text: &str) -> RegistryCounts {
    let mut c = RegistryCounts::default();
    for line in text.lines() {
        if line.starts_with("- id:") {
            c.total += 1;
            continue;
        }
        // Only a real key counts: `status:` inside a quoted description never
        // starts its own line.
        if let Some(value) = line.trim_start().strip_prefix("status:") {
            match value.trim() {
                "pending" => c.pending += 1,
                "matched" => c.matched += 1,
                "skipped" => c.skipped += 1,
                "applied" => c.applied += 1,
                "failed" => c.failed += 1,
                _ => {}
            }
        }
    }
    c
}

fn registry_counts() -> RegistryCounts {
    count_registry(&std::fs::read_to_string(registry_path()).unwrap_or_default())
}

/// Which pipeline stage the log last reported. The conductor dispatches named
/// subagents, and the stream formatter prints them as `[subagent_type: hh-*]`.
fn stage_from_log(text: &str) -> &'static str {
    for line in text.lines().rev() {
        if line.contains("hh-applier") {
            return "Submitting applications";
        }
        if line.contains("hh-letter") {
            return "Writing cover letters";
        }
        if line.contains("hh-analyzer") {
            return "Analyzing vacancies";
        }
        if line.contains("hh-collector") {
            return "Searching vacancies";
        }
        if line.starts_with("[done]") {
            return "Finished";
        }
        if line.starts_with("[init]") {
            return "Starting up";
        }
    }
    ""
}

/// Turn one raw log line into a human-readable activity line.
/// `None` = noise the user does not need to see.
fn humanize_log_line(line: &str) -> Option<String> {
    // Insight banners come wrapped in backticks (`★ Insight ───…`), so strip
    // those before deciding what is noise — otherwise the check never fires.
    let trimmed = line.trim().trim_matches('`').trim();
    if trimmed.is_empty() || trimmed.starts_with('★') || trimmed.starts_with('─') {
        return None;
    }

    if let Some(rest) = trimmed.strip_prefix("[done]") {
        let turns = rest.split("turns:").nth(1).and_then(|s| s.split('|').next());
        return Some(match turns {
            Some(t) => format!("Finished — {} steps", t.trim()),
            None => "Finished".into(),
        });
    }
    if trimmed.starts_with("[init]") {
        return Some("Session started".into());
    }

    // Tool calls: `  → Name [key: value]`
    if let Some(rest) = trimmed.strip_prefix("→ ") {
        let (name, arg) = match rest.split_once(" [") {
            Some((n, a)) => (n, a.trim_end_matches(']')),
            None => (rest, ""),
        };
        let value = arg.split_once(": ").map(|(_, v)| v).unwrap_or("");

        return match name {
            "Agent" | "Task" => Some(match value {
                v if v.contains("collector") => "▸ Collector — searching vacancies".into(),
                v if v.contains("analyzer") => "▸ Analyzer — matching against the resume".into(),
                v if v.contains("letter") => "▸ Writing a cover letter".into(),
                v if v.contains("applier") => "▸ Applier — submitting a response".into(),
                v => format!("▸ {v}"),
            }),
            "mcp__playwright__browser_navigate" => {
                if let Some(id) = value.rsplit("/vacancy/").next().filter(|s| *s != value) {
                    Some(format!("opening vacancy {}", id.split(['?', '&']).next().unwrap_or(id)))
                } else if value.contains("/search/vacancy") {
                    let query = value
                        .split("text=")
                        .nth(1)
                        .and_then(|s| s.split('&').next())
                        .map(|s| s.replace('+', " "))
                        .unwrap_or_default();
                    Some(if query.is_empty() {
                        "opening hh.ru search".into()
                    } else {
                        format!("searching: {query}")
                    })
                } else {
                    Some("opening a page".into())
                }
            }
            "mcp__playwright__browser_click" => Some("clicking".into()),
            "mcp__playwright__browser_type" | "mcp__playwright__browser_fill_form" => {
                Some("filling the form".into())
            }
            "Read" | "Write" | "Edit" => {
                let file = value.rsplit('/').next().unwrap_or(value);
                let verb = if name == "Read" { "reading" } else { "writing" };
                Some(format!("{verb} {file}"))
            }
            "Skill" => Some(format!("skill: {value}")),
            // Bash, ToolSearch and page reads (`browser_evaluate`/`snapshot`)
            // fire between every navigation and only pad the feed — the
            // pulsing status dot already says work is happening.
            _ => None,
        };
    }

    // Plain narration from the model — the most informative part of the log.
    let clean = trimmed.replace("**", "").replace('`', "");
    if clean.chars().count() > 160 {
        return None; // long essays belong in the log file, not the panel
    }
    Some(clean)
}

/// The last few activity lines, consecutive duplicates collapsed.
fn activity_feed(text: &str, limit: usize) -> Vec<String> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for line in text.lines() {
        let Some(msg) = humanize_log_line(line) else { continue };
        match out.last_mut() {
            Some((prev, n)) if *prev == msg => *n += 1,
            _ => out.push((msg, 1)),
        }
    }
    let start = out.len().saturating_sub(limit);
    out[start..]
        .iter()
        .map(|(msg, n)| if *n > 1 { format!("{msg} ×{n}") } else { msg.clone() })
        .collect()
}

#[derive(Serialize)]
pub struct Progress {
    stage: String,
    /// Human-readable feed of what the pipeline is doing, oldest first.
    activity: Vec<String>,
    /// Counts are deltas for the run started from this app; otherwise lifetime.
    since_run: bool,
    found: i64,
    analyzed: i64,
    matched: i64,
    skipped: i64,
    applied: i64,
    failed: i64,
    total: i64,
}

/// The log of the run happening now.
///
/// Picking by "is it external" breaks the moment the app restarts mid-run: it
/// no longer owns the process, calls it external and reads the scheduler's log
/// — which still holds *last night's* run. So take whichever log was written
/// most recently, and keep only the part after the last `[init]` marker, so a
/// previous run's tail can never be shown as current.
fn read_run_log() -> String {
    let newest = [manual_log_path(), launchd_log_path()]
        .into_iter()
        .filter_map(|p| {
            let modified = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
            Some((modified, p))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, p)| p);

    let Some(path) = newest else { return String::new() };
    text_after_last_init(&std::fs::read_to_string(path).unwrap_or_default())
}

/// Keep only the current run: everything from the last `[init]` marker on.
fn text_after_last_init(text: &str) -> String {
    match text.rfind("\n[init]") {
        Some(i) => text[i + 1..].to_string(),
        None if text.starts_with("[init]") => text.to_string(),
        // No marker at all: the file is from before this run — show nothing
        // rather than yesterday's summary.
        None => String::new(),
    }
}

fn progress(state: &RunState, pipe: &PipelineStatus) -> Progress {
    let now = registry_counts();
    let baseline = *state.baseline.lock().unwrap();
    let log = read_run_log();

    let mut stage = stage_from_log(&log).to_string();
    if stage.is_empty() {
        stage = if pipe.running || pipe.external { "Starting up" } else { "Idle" }.into();
    }
    let activity = activity_feed(&log, 14);

    match baseline {
        Some(b) => Progress {
            stage,
            activity,
            since_run: true,
            found: now.total - b.total,
            // everything that left `pending` since the run began
            analyzed: (now.matched + now.skipped + now.applied + now.failed)
                - (b.matched + b.skipped + b.applied + b.failed),
            matched: now.matched - b.matched,
            skipped: now.skipped - b.skipped,
            applied: now.applied - b.applied,
            failed: now.failed - b.failed,
            total: now.total,
        },
        None => Progress {
            stage,
            activity,
            since_run: false,
            found: now.total,
            analyzed: now.matched + now.skipped + now.applied + now.failed,
            matched: now.matched,
            skipped: now.skipped,
            applied: now.applied,
            failed: now.failed,
            total: now.total,
        },
    }
}

const PIPELINE_BIN: &str = "hhru-jobs";

/// A GUI app launched from Finder inherits a bare PATH, so spawned processes
/// would not find `claude`, `node` or `npx`. Rebuild the usual login PATH.
fn path_env() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    format!("{home}/.local/bin:{home}/.bun/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin")
}

// ---------- config view ----------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ConfigView {
    text: Vec<String>,
    salary: i64,
    currency_code: String,
    experience: String,
    work_format: String,
    no_default_area: bool,
    enable_snippets: bool,
    search_period: i64,
    workers: i64,
    count: i64,
    letter_mode: String,
}

fn yaml_get<'a>(v: &'a serde_yaml::Value, path: &[&str]) -> Option<&'a serde_yaml::Value> {
    let mut cur = v;
    for key in path {
        cur = cur.get(key)?;
    }
    Some(cur)
}

fn read_config_view() -> Result<ConfigView, String> {
    let raw = std::fs::read_to_string(config_path()).map_err(|e| e.to_string())?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&raw).map_err(|e| e.to_string())?;

    let f = |p: &[&str]| yaml_get(&doc, p).cloned();
    let text = f(&["search", "filters", "text"])
        .and_then(|v| v.as_sequence().cloned())
        .map(|seq| {
            seq.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Ok(ConfigView {
        text,
        salary: f(&["search", "filters", "salary"]).and_then(|v| v.as_i64()).unwrap_or(0),
        currency_code: f(&["search", "filters", "currency_code"]).and_then(|v| v.as_str().map(String::from)).unwrap_or_default(),
        experience: f(&["search", "filters", "experience"]).and_then(|v| v.as_str().map(String::from)).unwrap_or_default(),
        work_format: f(&["search", "filters", "work_format"]).and_then(|v| v.as_str().map(String::from)).unwrap_or_default(),
        no_default_area: f(&["search", "filters", "no_default_area"]).and_then(|v| v.as_bool()).unwrap_or(false),
        enable_snippets: f(&["search", "filters", "enable_snippets"]).and_then(|v| v.as_bool()).unwrap_or(false),
        search_period: f(&["search", "filters", "search_period"]).and_then(|v| v.as_i64()).unwrap_or(1),
        workers: f(&["defaults", "workers"]).and_then(|v| v.as_i64()).unwrap_or(1),
        count: f(&["defaults", "count"]).and_then(|v| v.as_i64()).unwrap_or(10),
        letter_mode: f(&["letter", "mode"])
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "ai".into()),
    })
}

// ---------- comment-preserving YAML editing ----------

/// Split a simple YAML line into (content, comment) where comment starts at the
/// first `#` outside of quotes. Returns the comment with its leading whitespace.
fn split_comment(line: &str) -> (&str, &str) {
    let mut in_squote = false;
    let mut in_dquote = false;
    for (i, ch) in line.char_indices() {
        match ch {
            '\'' if !in_dquote => in_squote = !in_squote,
            '"' if !in_squote => in_dquote = !in_dquote,
            '#' if !in_squote && !in_dquote => {
                let start = line[..i].trim_end().len();
                return (&line[..start], &line[start..]);
            }
            _ => {}
        }
    }
    (line, "")
}

/// Top-level section of a config line ("search", "defaults", ...).
fn is_top_level_key(line: &str) -> Option<&str> {
    if line.starts_with(' ') || line.starts_with('#') || line.trim().is_empty() {
        return None;
    }
    line.split_once(':').map(|(k, _)| k.trim())
}

/// Replace the value of `key:` inside top-level `section`, preserving indent,
/// trailing comment and its column.
fn update_scalar(lines: &mut Vec<String>, section: &str, key: &str, new_value: &str) -> Result<(), String> {
    let mut current_section = String::new();
    let mut hits = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(sec) = is_top_level_key(line) {
            current_section = sec.to_string();
        }
        if current_section == section {
            let trimmed = line.trim_start();
            if trimmed.starts_with(&format!("{key}:")) {
                hits.push(i);
            }
        }
    }
    if hits.len() != 1 {
        return Err(format!("key `{section}.{key}`: found {} matches, expected 1", hits.len()));
    }
    let i = hits[0];
    let old = &lines[i];
    let indent: String = old.chars().take_while(|c| c.is_whitespace()).collect();
    let (content, comment) = split_comment(old);
    let comment_col = if comment.is_empty() { 0 } else { content.len() + (old.len() - content.len() - comment.trim_start().len()) };
    let mut rebuilt = format!("{indent}{key}: {new_value}");
    if !comment.is_empty() {
        if rebuilt.len() + 1 < comment_col {
            rebuilt.push_str(&" ".repeat(comment_col - rebuilt.len()));
        } else {
            rebuilt.push(' ');
        }
        rebuilt.push_str(comment.trim_start());
    }
    lines[i] = rebuilt;
    Ok(())
}

/// Replace the `- ...` entries directly under `search.filters.text:`.
fn update_text_list(lines: &mut Vec<String>, entries: &[String]) -> Result<(), String> {
    let mut current_section = String::new();
    let mut text_idx = None;
    for (i, line) in lines.iter().enumerate() {
        if let Some(sec) = is_top_level_key(line) {
            current_section = sec.to_string();
        }
        if current_section == "search" && line.trim_start().starts_with("text:") {
            text_idx = Some(i);
            break;
        }
    }
    let text_idx = text_idx.ok_or("search.filters.text not found")?;

    // consecutive `- ` lines right after text:
    let mut end = text_idx + 1;
    let mut entry_indent = None;
    while end < lines.len() {
        let t = lines[end].trim_start();
        if t.starts_with("- ") || t == "-" {
            if entry_indent.is_none() {
                entry_indent = Some(
                    lines[end]
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .collect::<String>(),
                );
            }
            end += 1;
        } else {
            break;
        }
    }
    let indent = entry_indent.unwrap_or_else(|| {
        let base: String = lines[text_idx].chars().take_while(|c| c.is_whitespace()).collect();
        format!("{base}  ")
    });

    let new_lines: Vec<String> = entries
        .iter()
        .map(|e| format!("{indent}- \"{}\"", e.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect();
    lines.splice(text_idx + 1..end, new_lines);
    Ok(())
}

/// Apply the view onto the original YAML text, preserving comments and layout.
/// Pure: no I/O, so it can be tested against the real config file.
fn render_config(original: &str, view: &ConfigView) -> Result<String, String> {
    let mut lines: Vec<String> = original.lines().map(String::from).collect();

    let queries: Vec<String> = view
        .text
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if queries.is_empty() {
        return Err("нужен хотя бы один поисковый запрос".into());
    }

    update_text_list(&mut lines, &queries)?;
    update_scalar(&mut lines, "search", "salary", &view.salary.to_string())?;
    update_scalar(&mut lines, "search", "currency_code", &format!("\"{}\"", view.currency_code))?;
    update_scalar(&mut lines, "search", "experience", &format!("\"{}\"", view.experience))?;
    update_scalar(&mut lines, "search", "work_format", &format!("\"{}\"", view.work_format))?;
    update_scalar(&mut lines, "search", "no_default_area", if view.no_default_area { "true" } else { "false" })?;
    update_scalar(&mut lines, "search", "enable_snippets", if view.enable_snippets { "true" } else { "false" })?;
    update_scalar(&mut lines, "search", "search_period", &view.search_period.to_string())?;
    update_scalar(&mut lines, "defaults", "workers", &view.workers.to_string())?;
    update_scalar(&mut lines, "defaults", "count", &view.count.to_string())?;

    if !matches!(view.letter_mode.as_str(), "ai" | "template") {
        return Err(format!("letter.mode: ожидается ai|template, получено {}", view.letter_mode));
    }
    update_scalar(&mut lines, "letter", "mode", &view.letter_mode)?;

    let mut out = lines.join("\n");
    if original.ends_with('\n') {
        out.push('\n');
    }

    // must still be valid YAML — otherwise the caller keeps the original file
    serde_yaml::from_str::<serde_yaml::Value>(&out)
        .map_err(|e| format!("итог невалиден, файл не тронут: {e}"))?;
    Ok(out)
}

fn write_config_view(view: &ConfigView) -> Result<(), String> {
    let path = config_path();
    let original = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let out = render_config(&original, view)?;
    std::fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------- schedule (launchd plist) ----------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TimeEntry {
    hour: u8,
    minute: u8,
}

fn read_schedule() -> Result<Vec<TimeEntry>, String> {
    let v = plist::Value::from_file(plist_path()).map_err(|e| e.to_string())?;
    let dict = v.as_dictionary().ok_or("plist: not a dict")?;
    let mut out = Vec::new();
    if let Some(arr) = dict.get("StartCalendarInterval").and_then(|x| x.as_array()) {
        for item in arr {
            if let Some(d) = item.as_dictionary() {
                let hour = d.get("Hour").and_then(|x| x.as_signed_integer()).unwrap_or(0) as u8;
                let minute = d.get("Minute").and_then(|x| x.as_signed_integer()).unwrap_or(0) as u8;
                out.push(TimeEntry { hour, minute });
            }
        }
    }
    Ok(out)
}

fn write_schedule(times: &[TimeEntry]) -> Result<String, String> {
    if times.is_empty() {
        return Err("нужно хотя бы одно время запуска".into());
    }
    let mut times: Vec<TimeEntry> = times.to_vec();
    times.sort_by_key(|t| (t.hour, t.minute));
    times.dedup_by_key(|t| (t.hour, t.minute));
    for t in &times {
        if t.hour > 23 || t.minute > 59 {
            return Err(format!("некорректное время {:02}:{:02}", t.hour, t.minute));
        }
    }

    let path = plist_path();
    let v = plist::Value::from_file(&path).map_err(|e| e.to_string())?;
    let mut dict = v.as_dictionary().cloned().ok_or("plist: not a dict")?;

    let arr: Vec<plist::Value> = times
        .iter()
        .map(|t| {
            let mut d = plist::Dictionary::new();
            d.insert("Hour".into(), plist::Value::Integer((t.hour as i64).into()));
            d.insert("Minute".into(), plist::Value::Integer((t.minute as i64).into()));
            plist::Value::Dictionary(d)
        })
        .collect();
    dict.insert("StartCalendarInterval".into(), plist::Value::Array(arr));

    // config.yaml is the single source of truth for workers/count: scheduled
    // runs must not override them with CLI flags. `--verbose` stays, so the
    // scheduled log carries the same stage markers the progress view reads.
    let bin = repo_root().join(PIPELINE_BIN).to_string_lossy().into_owned();
    dict.insert(
        "ProgramArguments".into(),
        plist::Value::Array(vec![
            plist::Value::String(bin),
            plist::Value::String("--verbose".into()),
        ]),
    );

    plist::Value::Dictionary(dict)
        .to_file_xml(&path)
        .map_err(|e| e.to_string())?;

    // reload the launchd agent
    let out = Command::new("bash")
        .arg(repo_root().join("scheduler/install.sh"))
        .current_dir(repo_root())
        .output()
        .map_err(|e| e.to_string())?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if !out.status.success() {
        return Err(format!("install.sh failed: {combined}"));
    }
    Ok(combined)
}

fn agent_loaded() -> bool {
    let uid = Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    Command::new("launchctl")
        .args(["print", &format!("gui/{uid}/com.hh-jobs")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ---------- pipeline run ----------

#[derive(Default)]
pub struct RunState {
    child: Mutex<Option<Child>>,
    last_exit: Mutex<Option<i32>>,
    auth_child: Mutex<Option<Child>>,
    auth_result: Mutex<Option<AuthResult>>,
    /// Registry snapshot taken when a run starts, so progress can be shown as
    /// "this run" rather than lifetime totals.
    baseline: Mutex<Option<RegistryCounts>>,
    /// The `/hh:init` onboarding run started from "Set resume".
    init_child: Mutex<Option<Child>>,
    init_result: Mutex<Option<String>>,
    /// Was a scheduled run visible on the previous poll — the edge that tells
    /// us a launchd run started or finished while the app was open.
    prev_external: Mutex<bool>,
    /// A finished run waiting to be announced, consumed by the notifier.
    finish_pending: Mutex<Option<FinishEvent>>,
}

#[derive(Clone, Copy)]
pub struct FinishEvent {
    scheduled: bool,
    exit: Option<i32>,
}

/// Watch the run edges: a start snapshots the registry (so progress and the
/// notification report this run, not lifetime totals), a finish queues the
/// notification. Called once per poll — the only place with side effects.
fn track_run_edges(state: &RunState, pipe: &PipelineStatus) {
    let mut prev_external = state.prev_external.lock().unwrap();
    if pipe.external && !*prev_external {
        *state.baseline.lock().unwrap() = Some(registry_counts());
    } else if !pipe.external && *prev_external {
        // Never overwrite a manual finish queued in the same poll: the tail of
        // a manual run can register as "external" for a single tick and would
        // otherwise produce a second, wrong notification.
        let mut pending = state.finish_pending.lock().unwrap();
        if pending.is_none() {
            *pending = Some(FinishEvent {
                scheduled: true,
                exit: None,
            });
        }
    }
    *prev_external = pipe.external;
}

/// Show a system notification for a run that just ended.
fn announce_finish(app: &tauri::AppHandle, event: FinishEvent, p: &Progress) {
    use tauri_plugin_notification::NotificationExt;

    let what = if event.scheduled { "Scheduled run" } else { "Run" };
    let (title, body) = match event.exit {
        Some(code) if code != 0 => (
            format!("{what} failed"),
            format!("Exit code {code} — open the log for details"),
        ),
        _ => (
            format!("{what} finished"),
            format!(
                "Found {}, applied {}, skipped {}",
                p.found, p.applied, p.skipped
            ),
        ),
    };

    let _ = app
        .notification()
        .builder()
        .title(title)
        .body(body)
        .show();
}

// ---------- onboarding from a CV ----------

#[derive(Serialize)]
pub struct SetupStatus {
    /// Both private files exist, so the pipeline has what it needs.
    ready: bool,
    has_config: bool,
    has_resume: bool,
    running: bool,
    message: Option<String>,
}

fn setup_status(state: &RunState) -> SetupStatus {
    let mut guard = state.init_child.lock().unwrap();
    let mut running = false;
    if let Some(child) = guard.as_mut() {
        match child.try_wait() {
            Ok(Some(status)) => {
                let msg = if status.success() {
                    "Resume imported".to_string()
                } else {
                    let log = std::fs::read_to_string(init_log_path()).unwrap_or_default();
                    let tail = log.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
                    format!("Import failed: {}", tail.chars().take(120).collect::<String>())
                };
                *state.init_result.lock().unwrap() = Some(msg);
                *guard = None;
            }
            Ok(None) => running = true,
            Err(_) => *guard = None,
        }
    }
    let has_config = config_path().exists();
    let has_resume = resume_path().exists();
    SetupStatus {
        ready: has_config && has_resume,
        has_config,
        has_resume,
        running,
        message: state.init_result.lock().unwrap().clone(),
    }
}

/// Run `/hh:init <cv>` headlessly: it fills config.yaml and resume.md from the
/// CV. `auto` tells the command not to ask questions — the numeric preferences
/// (salary, queries, filters) are edited in this app's Settings tab anyway.
fn start_init(state: &RunState, cv: &std::path::Path) -> Result<(), String> {
    let log_path = init_log_path();
    if let Some(dir) = log_path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let log = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;
    let log_err = log.try_clone().map_err(|e| e.to_string())?;

    let child = Command::new("claude")
        .arg("-p")
        .arg(format!("/hh:init {} auto", cv.display()))
        .args([
            "--allowedTools",
            "Read,Write,Edit,Bash,Glob,Grep,Skill,ToolSearch",
            "--output-format",
            "text",
        ])
        .current_dir(repo_root())
        .env("PATH", path_env())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err))
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| format!("не удалось запустить claude: {e}"))?;

    *state.init_child.lock().unwrap() = Some(child);
    *state.init_result.lock().unwrap() = None;
    Ok(())
}

#[derive(Serialize, Clone, Debug)]
pub struct AuthResult {
    ok: bool,
    message: String,
}

#[derive(Serialize)]
pub struct AuthStatus {
    checking: bool,
    result: Option<AuthResult>,
}

/// Pull the `AUTH_OK` / `AUTH_FAIL` marker out of the check log.
fn parse_auth_output(text: &str, exit_code: Option<i32>) -> AuthResult {
    for line in text.lines().rev() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("AUTH_OK") {
            return AuthResult {
                ok: true,
                message: if rest.trim().is_empty() { "вход активен".into() } else { rest.trim().into() },
            };
        }
        if let Some(rest) = line.strip_prefix("AUTH_FAIL") {
            return AuthResult {
                ok: false,
                message: if rest.trim().is_empty() { "вход не найден".into() } else { rest.trim().into() },
            };
        }
    }
    AuthResult {
        ok: false,
        message: match exit_code {
            Some(c) => format!("проверка завершилась без ответа (код {c})"),
            None => "проверка завершилась без ответа".into(),
        },
    }
}

fn auth_status(state: &RunState) -> AuthStatus {
    let mut guard = state.auth_child.lock().unwrap();
    let mut checking = false;
    if let Some(child) = guard.as_mut() {
        match child.try_wait() {
            Ok(Some(status)) => {
                let text = std::fs::read_to_string(auth_log_path()).unwrap_or_default();
                *state.auth_result.lock().unwrap() = Some(parse_auth_output(&text, status.code()));
                *guard = None;
            }
            Ok(None) => checking = true,
            Err(_) => *guard = None,
        }
    }
    AuthStatus {
        checking,
        result: state.auth_result.lock().unwrap().clone(),
    }
}

#[derive(Serialize)]
pub struct PipelineStatus {
    running: bool,
    external: bool,
    last_exit: Option<i32>,
}

fn pipeline_status(state: &RunState) -> PipelineStatus {
    let mut child_guard = state.child.lock().unwrap();
    let mut running = false;
    if let Some(child) = child_guard.as_mut() {
        match child.try_wait() {
            Ok(Some(status)) => {
                *state.last_exit.lock().unwrap() = status.code();
                *state.finish_pending.lock().unwrap() = Some(FinishEvent {
                    scheduled: false,
                    exit: status.code(),
                });
                *child_guard = None;
            }
            Ok(None) => running = true,
            Err(_) => {
                *child_guard = None;
            }
        }
    }
    // a run started by launchd (not by us)
    let external = !running
        && Command::new("pgrep")
            .args(["-f", PIPELINE_BIN])
            .stdout(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    PipelineStatus {
        running,
        external,
        last_exit: *state.last_exit.lock().unwrap(),
    }
}

// ---------- state for the UI ----------

#[derive(Serialize)]
pub struct AppState {
    config: Option<ConfigView>,
    config_error: Option<String>,
    config_mtime: u64,
    schedule: Vec<TimeEntry>,
    schedule_error: Option<String>,
    agent_loaded: bool,
    pipeline: PipelineStatus,
    auth: AuthStatus,
    progress: Progress,
    setup: SetupStatus,
}

#[tauri::command]
fn get_state(app: tauri::AppHandle, state: State<RunState>) -> AppState {
    let (config, config_error) = match read_config_view() {
        Ok(c) => (Some(c), None),
        Err(e) => (None, Some(e)),
    };
    let (schedule, schedule_error) = match read_schedule() {
        Ok(s) => (s, None),
        Err(e) => (Vec::new(), Some(e)),
    };
    let config_mtime = std::fs::metadata(config_path())
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let pipeline = pipeline_status(&state);
    track_run_edges(&state, &pipeline);
    let progress = progress(&state, &pipeline);
    if let Some(event) = state.finish_pending.lock().unwrap().take() {
        announce_finish(&app, event, &progress);
    }
    AppState {
        config,
        config_error,
        config_mtime,
        schedule,
        schedule_error,
        agent_loaded: agent_loaded(),
        pipeline,
        auth: auth_status(&state),
        progress,
        setup: setup_status(&state),
    }
}

/// "Set resume": pick a CV, then let `/hh:init` write config.yaml + resume.md.
#[tauri::command]
fn set_resume(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_dialog::DialogExt;

    if app.state::<RunState>().init_child.lock().unwrap().is_some() {
        return Err("импорт резюме уже идёт".into());
    }

    // Non-blocking: the picker must not stall the main thread.
    app.dialog()
        .file()
        .set_title("Select your resume")
        .add_filter("Resume", &["pdf", "docx", "doc", "rtf", "md", "txt"])
        .pick_file(move |picked| {
            let Some(file) = picked else { return }; // cancelled
            let state = app.state::<RunState>();
            let path = match file.into_path() {
                Ok(p) => p,
                Err(_) => return,
            };
            if let Err(e) = start_init(&state, &path) {
                *state.init_result.lock().unwrap() = Some(e);
            }
        });
    Ok(())
}

#[tauri::command]
fn check_auth(state: State<RunState>) -> Result<(), String> {
    if state.auth_child.lock().unwrap().is_some() {
        return Err("проверка уже идёт".into());
    }
    let pipe = pipeline_status(&state);
    if pipe.running || pipe.external {
        return Err("пайплайн выполняется — профиль браузера занят".into());
    }

    let log_path = auth_log_path();
    if let Some(dir) = log_path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let log = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;
    let log_err = log.try_clone().map_err(|e| e.to_string())?;

    let child = Command::new("bash")
        .arg(repo_root().join("scripts/check-auth.sh"))
        .current_dir(repo_root())
        .env("PATH", path_env())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err))
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;

    *state.auth_child.lock().unwrap() = Some(child);
    *state.auth_result.lock().unwrap() = None;
    Ok(())
}

#[tauri::command]
fn save_config(view: ConfigView) -> Result<(), String> {
    write_config_view(&view)
}

#[tauri::command]
fn save_schedule(times: Vec<TimeEntry>) -> Result<String, String> {
    write_schedule(&times)
}

#[tauri::command]
fn run_now(dry: bool, state: State<RunState>) -> Result<(), String> {
    let status = pipeline_status(&state);
    if status.running || status.external {
        return Err("пайплайн уже выполняется".into());
    }
    let log_path = manual_log_path();
    if let Some(dir) = log_path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let log = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;
    let log_err = log.try_clone().map_err(|e| e.to_string())?;

    let mut cmd = Command::new(repo_root().join(PIPELINE_BIN));
    cmd.arg("--verbose");
    if dry {
        cmd.arg("--dry-run");
    }
    cmd.current_dir(repo_root())
        .env("PATH", path_env())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err))
        .stdin(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0); // own group so stop_run can kill the whole tree
    }
    let child = cmd.spawn().map_err(|e| e.to_string())?;
    *state.child.lock().unwrap() = Some(child);
    *state.last_exit.lock().unwrap() = None;
    // Snapshot the registry so progress reads as "this run", not lifetime.
    *state.baseline.lock().unwrap() = Some(registry_counts());
    Ok(())
}

#[tauri::command]
fn stop_run(state: State<RunState>) -> Result<(), String> {
    let mut guard = state.child.lock().unwrap();
    if let Some(child) = guard.as_mut() {
        let pid = child.id();
        // negative pid -> the whole process group
        let _ = Command::new("kill").args(["-TERM", &format!("-{pid}")]).status();
        let _ = child.wait();
        *guard = None;
        Ok(())
    } else {
        Err("ручной запуск не активен".into())
    }
}

#[tauri::command]
fn tail_log(lines: usize) -> String {
    let path = manual_log_path();
    let mut buf = String::new();
    if let Ok(mut f) = std::fs::File::open(path) {
        let _ = f.read_to_string(&mut buf);
    }
    let all: Vec<&str> = buf.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}

#[tauri::command]
fn open_config() -> Result<(), String> {
    Command::new("open")
        .arg(config_path())
        .status()
        .map_err(|e| e.to_string())
        .and_then(|s| if s.success() { Ok(()) } else { Err("open failed".into()) })
}

#[tauri::command]
fn open_log() -> Result<(), String> {
    Command::new("open")
        .arg(manual_log_path())
        .status()
        .map_err(|e| e.to_string())
        .and_then(|s| if s.success() { Ok(()) } else { Err("open failed".into()) })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"# Конфигурация
search:
  base_url: "https://hh.ru/search/vacancy"
  filters:
    text:                        # поисковые запросы
      - "Laravel"
      - "Backend разработчик"

    salary: 2000                 # минимальная зарплата
    currency_code: "USD"
    search_period: 3 # искать за три сутки

defaults:
  workers: 1   # параллельных субагентов
  count: 20     # максимум вакансий
"#;

    fn lines(s: &str) -> Vec<String> {
        s.lines().map(String::from).collect()
    }

    #[test]
    fn scalar_update_keeps_comment_and_column() {
        let mut l = lines(SAMPLE);
        update_scalar(&mut l, "search", "salary", "3000").unwrap();
        let line = l.iter().find(|x| x.contains("salary:")).unwrap();
        assert_eq!(line, "    salary: 3000                 # минимальная зарплата");
    }

    #[test]
    fn scalar_update_inline_comment_without_padding() {
        let mut l = lines(SAMPLE);
        update_scalar(&mut l, "search", "search_period", "7").unwrap();
        let line = l.iter().find(|x| x.contains("search_period:")).unwrap();
        assert_eq!(line, "    search_period: 7 # искать за три сутки");
    }

    #[test]
    fn scalar_update_respects_section() {
        let mut l = lines(SAMPLE);
        update_scalar(&mut l, "defaults", "count", "5").unwrap();
        assert!(l.iter().any(|x| x.starts_with("  count: 5")));
        // a key that exists only in another section must error
        assert!(update_scalar(&mut l, "defaults", "salary", "1").is_err());
    }

    #[test]
    fn text_list_replaced_in_place() {
        let mut l = lines(SAMPLE);
        update_text_list(&mut l, &["PHP".into(), "Go \"backend\"".into()]).unwrap();
        let joined = l.join("\n");
        assert!(joined.contains("      - \"PHP\""));
        assert!(joined.contains("      - \"Go \\\"backend\\\"\""));
        assert!(!joined.contains("Laravel"));
        // the blank line and the following key survive
        assert!(joined.contains("\n\n    salary:"));
        // still valid YAML
        serde_yaml::from_str::<serde_yaml::Value>(&joined).unwrap();
    }

    /// The real config.yaml must survive a no-op edit byte-for-byte: every
    /// comment, blank line and column stays put. Guards against config drift.
    #[test]
    fn real_config_round_trips_unchanged() {
        let original = std::fs::read_to_string(config_path()).expect("config.yaml readable");
        let view = read_config_view().expect("config.yaml parses");
        let rendered = render_config(&original, &view).expect("render succeeds");
        assert_eq!(rendered, original, "no-op save must not alter config.yaml");
    }

    #[test]
    fn letter_mode_is_validated() {
        let view = read_config_view().unwrap();
        let bad = ConfigView { letter_mode: "magic".into(), ..view };
        let original = std::fs::read_to_string(config_path()).unwrap();
        assert!(render_config(&original, &bad).is_err());
    }

    #[test]
    fn registry_counts_by_status() {
        let reg = r#"# Реестр
- id: "1"
  title: A
  status: applied
- id: "2"
  description: "мы пишем status: applied в тексте вакансии"
  status: skipped
- id: "3"
  status: pending
"#;
        let c = count_registry(reg);
        assert_eq!(c.total, 3);
        assert_eq!(c.applied, 1);
        assert_eq!(c.skipped, 1);
        assert_eq!(c.pending, 1);
    }

    #[test]
    fn stage_comes_from_the_last_marker() {
        let log = "[init] headless-сессия запущена\n  → Task [subagent_type: hh-collector]\n  → Task [subagent_type: hh-analyzer]\n";
        assert_eq!(stage_from_log(log), "Analyzing vacancies");
        assert_eq!(stage_from_log("[init] ok\n"), "Starting up");
        assert_eq!(stage_from_log("nothing useful\n"), "");
    }

    #[test]
    fn activity_feed_reads_like_a_human_wrote_it() {
        // Lines copied from a real run log.
        let log = r#"[init] headless-сессия запущена
  → Read [file_path: /Users/x/job-dispacher/config.yaml]
  → Bash [command: grep -c "^- id:" data/vacancies-registry.yaml]
  → ToolSearch
Запускаю сбор по первому запросу («Laravel»), лимит 20.
  → Agent [subagent_type: hh-collector]
  → mcp__playwright__browser_navigate [url: https://hh.ru/search/vacancy?text=Laravel&salary=2000]
  → mcp__playwright__browser_evaluate
  → mcp__playwright__browser_evaluate
  → mcp__playwright__browser_navigate [url: https://hh.ru/vacancy/135826974]
`★ Insight ─────────────────────────────────────`
`─────────────────────────────────────────────────`
Собрано 2 новых.
"#;
        let feed = activity_feed(log, 20);
        assert_eq!(
            feed,
            vec![
                "Session started",
                "reading config.yaml",                  // абсолютный путь срезан
                "Запускаю сбор по первому запросу («Laravel»), лимит 20.",
                "▸ Collector — searching vacancies",
                "searching: Laravel",                   // запрос вытащен из URL
                "opening vacancy 135826974",            // browser_evaluate отброшен как шум
                "Собрано 2 новых.",
            ]
        );
        // Bash и ToolSearch — шум, их в ленте быть не должно
        assert!(!feed.iter().any(|l| l.contains("grep") || l.contains("ToolSearch")));
    }

    #[test]
    fn run_log_starts_at_the_last_init() {
        let text = "[init] старый прогон\nстарый хвост\n[init] новый прогон\n  → Agent [subagent_type: hh-collector]\n";
        // хвост предыдущего прогона отрезан
        assert!(!text_after_last_init(text).contains("старый"));
        assert!(text_after_last_init(text).starts_with("[init] новый"));
        // без маркера показывать нечего — иначе всплывёт вчерашний отчёт
        assert_eq!(text_after_last_init("итоги вчерашнего запуска\n"), "");
    }

    /// Прогоняет ленту по настоящему логу последнего запуска: она должна быть
    /// читаемой и без абсолютных путей. Пропускается, если лога ещё нет.
    #[test]
    fn real_log_feed_is_readable() {
        let log = read_run_log(); // тот же путь выбора, что и в приложении
        if log.trim().is_empty() {
            return;
        }
        let feed = activity_feed(&log, 14);
        assert!(!feed.is_empty(), "лог есть, а лента пустая");
        for line in &feed {
            println!("| {line}");
            assert!(!line.contains("/Users/"), "абсолютный путь в ленте: {line}");
            assert!(line.chars().count() <= 170, "слишком длинная строка: {line}");
        }
    }

    #[test]
    fn activity_feed_keeps_only_the_tail() {
        let log = (1..=30).map(|i| format!("строка {i}\n")).collect::<String>();
        let feed = activity_feed(&log, 5);
        assert_eq!(feed.len(), 5);
        assert_eq!(feed.last().unwrap(), "строка 30");
    }

    #[test]
    fn real_registry_parses() {
        let c = registry_counts();
        assert!(c.total > 0, "registry should have entries");
        assert_eq!(
            c.total,
            c.pending + c.matched + c.skipped + c.applied + c.failed,
            "every entry must carry exactly one known status"
        );
    }

    #[test]
    fn split_comment_ignores_hash_in_quotes() {
        let (content, comment) = split_comment(r#"  url: "http://x#y" # real"#);
        assert_eq!(content, r#"  url: "http://x#y""#);
        assert_eq!(comment.trim_start(), "# real");
    }
}

#[tauri::command]
fn hide_window(app: tauri::AppHandle) {
    tray::hide_main(&app);
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

pub fn run() {
    let builder = tauri::Builder::default();
    // A non-activating NSPanel takes keyboard focus without activating the
    // app, so macOS never switches Spaces to follow it (Spotlight approach).
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(RunState::default())
        .setup(|app| {
            // Pure menu-bar app: no Dock icon.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            tray::setup(app)?;

            #[cfg(target_os = "macos")]
            #[allow(deprecated)] // the plugin's API still takes the cocoa type
            {
                use tauri_nspanel::cocoa::appkit::NSWindowCollectionBehavior;
                use tauri_nspanel::{panel_delegate, WebviewWindowExt as _};

                let w = app
                    .get_webview_window("main")
                    .expect("main window must exist");

                // Native translucent material under the UI. State must be
                // forced Active: a non-activating panel is never "active" in
                // AppKit terms and the material would render flat.
                let _ = window_vibrancy::apply_vibrancy(
                    &w,
                    window_vibrancy::NSVisualEffectMaterial::Popover,
                    Some(window_vibrancy::NSVisualEffectState::Active),
                    Some(16.0),
                );

                let panel = w.to_panel().expect("failed to convert window to panel");
                panel.set_level(4); // NSFloatingWindowLevel
                panel.set_style_mask(1 << 7); // NSWindowStyleMaskNonActivatingPanel
                panel.set_collection_behaviour(
                    NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
                        | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary,
                );

                // Popover behavior: hide when focus leaves. The delayed
                // keyWindow check keeps our own dialogs from closing it.
                let delegate = panel_delegate!(HhJobsPanelDelegate {
                    window_did_resign_key
                });
                let handle = app.handle().clone();
                delegate.set_listener(Box::new(move |name: String| {
                    if name != "window_did_resign_key" {
                        return;
                    }
                    let handle = handle.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(150));
                        let h = handle.clone();
                        let _ = handle.run_on_main_thread(move || {
                            use objc2::MainThreadMarker;
                            use objc2_app_kit::NSApplication;
                            let Some(mtm) = MainThreadMarker::new() else {
                                return;
                            };
                            if NSApplication::sharedApplication(mtm).keyWindow().is_none() {
                                tray::hide_main(&h);
                            }
                        });
                    });
                }));
                panel.set_delegate(delegate);
            }

            tray::make_windows_join_all_spaces(app.handle());
            tray::show_main(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            check_auth,
            set_resume,
            save_config,
            save_schedule,
            run_now,
            stop_run,
            tail_log,
            open_config,
            open_log,
            hide_window,
            quit_app
        ])
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    tray::hide_main(window.app_handle());
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building hh-jobs")
        .run(|_app_handle, _event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                tray::show_main(_app_handle);
            }
        });
}
