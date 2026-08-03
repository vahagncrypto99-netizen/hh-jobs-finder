const { invoke } = window.__TAURI__.core;

const $ = (id) => document.getElementById(id);

// Fields that map 1:1 onto ConfigView scalars: [element id, ConfigView key, kind].
const FIELDS = [
  ["f-letter-mode", "letter_mode", "str"],
  ["f-salary", "salary", "num"],
  ["f-currency", "currency_code", "str"],
  ["f-experience", "experience", "str"],
  ["f-work-format", "work_format", "str"],
  ["f-period", "search_period", "num"],
  ["f-no-area", "no_default_area", "bool"],
  ["f-snippets", "enable_snippets", "bool"],
  ["f-workers", "workers", "num"],
  ["f-count", "count", "num"],
];

// Everything auto-saves; these debounces decide how soon. The schedule waits
// longer because saving it reloads the launchd agent.
const CONFIG_SAVE_MS = 600;
const SCHEDULE_SAVE_MS = 1000;

let configSaveTimer = null;
let scheduleSaveTimer = null;
let lastMtime = 0; // config.yaml mtime as currently rendered in the form
let lastSetupMessage = null; // so an import result is announced once, not every poll

// ---------- status line (footer) ----------
let statusTimer = null;
function say(msg, isError = false) {
  const el = $("status");
  el.textContent = msg;
  el.classList.toggle("error", isError);
  clearTimeout(statusTimer);
  statusTimer = setTimeout(() => (el.textContent = ""), isError ? 8000 : 2000);
}

const fail = (e) => say(String(e).replace(/^Error:\s*/, ""), true);

/// True while the user is typing in a form control — re-rendering then would
/// move the caret out from under them, so polling must hold off.
function isEditing() {
  const el = document.activeElement;
  return !!el && (el.tagName === "INPUT" || el.tagName === "SELECT");
}

// ---------- tabs ----------
document.querySelectorAll(".tab").forEach((btn) => {
  btn.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach((b) => b.classList.toggle("active", b === btn));
    document.querySelectorAll(".tab-panel").forEach((p) =>
      p.classList.toggle("hidden", p.dataset.tab !== btn.dataset.tab),
    );
  });
});

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") invoke("hide_window").catch(() => {});
});

// ---------- config: render, collect, auto-save ----------
function queryRow(value) {
  const row = document.createElement("div");
  row.className = "query-row";

  const input = document.createElement("input");
  input.type = "text";
  input.className = "query-input";
  input.value = value;
  input.placeholder = "e.g. Laravel";
  input.addEventListener("input", saveConfigSoon);

  const del = document.createElement("button");
  del.className = "row-x";
  del.textContent = "✕";
  del.title = "Remove query";
  del.addEventListener("click", () => {
    row.remove();
    saveConfigSoon();
  });

  row.append(input, del);
  return row;
}

function renderConfig(cfg) {
  const list = $("query-list");
  list.innerHTML = "";
  cfg.text.forEach((q) => list.appendChild(queryRow(q)));

  for (const [id, key, kind] of FIELDS) {
    const el = $(id);
    if (kind === "bool") el.checked = cfg[key];
    else el.value = cfg[key];
  }
}

function collectConfig() {
  const view = {
    text: [...document.querySelectorAll(".query-input")].map((i) => i.value),
  };
  for (const [id, key, kind] of FIELDS) {
    const el = $(id);
    if (kind === "bool") view[key] = el.checked;
    else if (kind === "num") view[key] = Number(el.value) || 0;
    else view[key] = el.value;
  }
  return view;
}

function saveConfigSoon() {
  clearTimeout(configSaveTimer);
  configSaveTimer = setTimeout(async () => {
    configSaveTimer = null;
    try {
      await invoke("save_config", { view: collectConfig() });
      say("Saved");
    } catch (e) {
      fail(e); // e.g. an empty query list — the file is left untouched
    }
  }, CONFIG_SAVE_MS);
}

$("add-query").addEventListener("click", () => {
  const row = queryRow("");
  $("query-list").appendChild(row);
  row.querySelector("input").focus();
});

FIELDS.forEach(([id]) => {
  $(id).addEventListener("input", saveConfigSoon);
  $(id).addEventListener("change", saveConfigSoon);
});

$("set-resume").addEventListener("click", async () => {
  try {
    await invoke("set_resume");
  } catch (e) {
    fail(e);
  }
});

$("open-config").addEventListener("click", () => invoke("open_config").catch(fail));
$("quit").addEventListener("click", () => invoke("quit_app").catch(fail));

// ---------- schedule: render + auto-save ----------
function schedChip(hour, minute) {
  const chip = document.createElement("span");
  chip.className = "chip-branch sched-chip";

  const input = document.createElement("input");
  input.type = "time";
  input.value = `${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
  input.addEventListener("input", saveScheduleSoon);

  const del = document.createElement("span");
  del.className = "chip-x";
  del.textContent = "✕";
  del.title = "Remove time";
  del.addEventListener("click", () => {
    chip.remove();
    saveScheduleSoon();
  });

  chip.append(input, del);
  return chip;
}

const schedSignature = (times) =>
  times.map((t) => `${t.hour}:${t.minute}`).join(",");

function renderSchedule(times) {
  // Rebuilding the chips on every poll would yank them out from under the
  // cursor mid-click, so only redraw when the plist actually differs.
  if (schedSignature(times) === schedSignature(collectSchedule())) return;
  const list = $("schedule-list");
  list.innerHTML = "";
  times.forEach((t) => list.appendChild(schedChip(t.hour, t.minute)));
}

function collectSchedule() {
  return [...document.querySelectorAll(".sched-chip input")]
    .map((i) => i.value)
    .filter(Boolean)
    .map((v) => {
      const [hour, minute] = v.split(":").map(Number);
      return { hour, minute };
    });
}

function saveScheduleSoon() {
  clearTimeout(scheduleSaveTimer);
  scheduleSaveTimer = setTimeout(async () => {
    scheduleSaveTimer = null;
    try {
      await invoke("save_schedule", { times: collectSchedule() });
      say("Schedule saved, agent reloaded");
    } catch (e) {
      fail(e); // e.g. the last time removed — the plist is left untouched
    }
  }, SCHEDULE_SAVE_MS);
}

$("add-time").addEventListener("click", () => {
  $("schedule-list").appendChild(schedChip(9, 0));
  saveScheduleSoon();
});

// ---------- run controls ----------
$("run-now").addEventListener("click", async () => {
  const dry = $("dry-run").checked;
  try {
    await invoke("run_now", { dry });
    say(dry ? "Dry run started" : "Pipeline started");
    refresh();
  } catch (e) {
    fail(e);
  }
});

$("stop-run").addEventListener("click", async () => {
  try {
    await invoke("stop_run");
    say("Stopped");
    refresh();
  } catch (e) {
    fail(e);
  }
});

$("open-log").addEventListener("click", () => invoke("open_log").catch(fail));

// The raw stream stays one click away; the counters are the default view.
let logOpen = false;
$("toggle-log").addEventListener("click", () => {
  logOpen = !logOpen;
  $("log-row").classList.toggle("hidden", !logOpen);
  $("toggle-log").textContent = logOpen ? "Hide" : "Details";
});

$("check-auth").addEventListener("click", async () => {
  try {
    await invoke("check_auth");
    say("Checking hh.ru session…");
    refresh();
  } catch (e) {
    fail(e);
  }
});

// ---------- polling ----------
function paintBadge(el, text, kind = "", title = "") {
  el.textContent = text;
  el.className = "dot-badge" + (kind ? " " + kind : "");
  el.title = title;
}

async function refresh(first = false) {
  let s;
  try {
    s = await invoke("get_state");
  } catch {
    return; // backend momentarily unavailable
  }

  // pipeline
  const running = s.pipeline.running || s.pipeline.external;
  if (s.pipeline.running) paintBadge($("pipeline-status"), "running (manual)", "busy");
  else if (s.pipeline.external) paintBadge($("pipeline-status"), "running (scheduled)", "busy");
  else if (s.pipeline.last_exit === 0) paintBadge($("pipeline-status"), "finished", "ok");
  else if (s.pipeline.last_exit != null)
    paintBadge($("pipeline-status"), `failed (${s.pipeline.last_exit})`, "bad");
  else paintBadge($("pipeline-status"), "idle");
  $("run-now").disabled = running;
  $("stop-run").classList.toggle("hidden", !s.pipeline.running);

  // scheduler
  paintBadge(
    $("agent-status"),
    s.agent_loaded ? "loaded" : "not loaded",
    s.agent_loaded ? "ok" : "bad",
  );

  // hh.ru session
  $("check-auth").disabled = s.auth.checking || running;
  if (s.auth.checking) paintBadge($("auth-status"), "checking…", "busy");
  else if (s.auth.result)
    paintBadge(
      $("auth-status"),
      s.auth.result.ok ? "signed in" : "signed out",
      s.auth.result.ok ? "ok" : "bad",
      s.auth.result.message,
    );
  else paintBadge($("auth-status"), "not checked");

  // setup: are config.yaml and resume.md in place
  const setup = s.setup;
  $("set-resume").disabled = setup.running;
  if (setup.running) paintBadge($("setup-status"), "importing…", "busy");
  else if (setup.ready) paintBadge($("setup-status"), "ready", "ok", setup.message || "");
  else if (setup.has_config) paintBadge($("setup-status"), "resume.md missing", "bad");
  else if (setup.has_resume) paintBadge($("setup-status"), "config.yaml missing", "bad");
  else paintBadge($("setup-status"), "not set up", "bad");
  if (setup.message && setup.message !== lastSetupMessage) {
    lastSetupMessage = setup.message;
    say(setup.message, !setup.message.startsWith("Resume imported"));
  }

  // progress: what the pipeline is doing right now, in plain numbers
  const p = s.progress;
  const stageEl = $("stage-label");
  stageEl.textContent = p.stage;
  stageEl.classList.toggle("busy", running);
  $("progress-title").textContent = p.since_run ? "Progress — this run" : "Progress — registry total";
  const stats = [
    ["st-found", p.found],
    ["st-analyzed", p.analyzed],
    ["st-skipped", p.skipped],
    ["st-applied", p.applied],
    ["st-failed", p.failed],
  ];
  for (const [id, value] of stats) {
    const el = $(id);
    el.textContent = value;
    el.parentElement.classList.toggle("on", value > 0 && id === "st-applied");
  }
  $("st-failed-wrap").classList.toggle("hidden", p.failed === 0);

  // live log tail — only fetched while the details pane is open
  if (logOpen && (s.pipeline.running || s.pipeline.last_exit != null)) {
    const text = await invoke("tail_log", { lines: 80 });
    const pre = $("log-tail");
    const stick = pre.scrollTop + pre.clientHeight >= pre.scrollHeight - 8;
    pre.textContent = text || "no output yet";
    if (stick) pre.scrollTop = pre.scrollHeight;
  }

  // config.yaml is the single source of truth: pull in outside edits, but never
  // while a save is queued or the user has a control focused.
  const busyEditing = configSaveTimer !== null || isEditing();
  if (s.config_error) {
    if (first) say("config.yaml: " + s.config_error, true);
  } else if (first || (!busyEditing && s.config_mtime !== lastMtime)) {
    renderConfig(s.config);
    lastMtime = s.config_mtime;
  }

  if (s.schedule_error) {
    if (first) say("plist: " + s.schedule_error, true);
  } else if (first || (scheduleSaveTimer === null && !isEditing())) {
    renderSchedule(s.schedule);
  }
}

if (navigator.platform.startsWith("Mac")) document.body.classList.add("mac");
refresh(true);
setInterval(refresh, 2000);
