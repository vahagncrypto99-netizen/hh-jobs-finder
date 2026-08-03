#!/usr/bin/env python3
"""Форматтер stream-json вывода `claude -p` в читаемый лог пайплайна."""
import sys
import json

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        e = json.loads(line)
    except json.JSONDecodeError:
        continue

    t = e.get("type")
    if t == "system" and e.get("subtype") == "init":
        print("[init] headless-сессия запущена", flush=True)
    elif t == "assistant":
        for b in e.get("message", {}).get("content", []):
            if b.get("type") == "text" and b.get("text", "").strip():
                print(b["text"].strip(), flush=True)
            elif b.get("type") == "tool_use":
                name = b.get("name", "?")
                inp = b.get("input", {}) or {}
                brief = ""
                for key in ("subagent_type", "description", "skill", "url",
                            "command", "file_path", "prompt"):
                    if key in inp:
                        val = str(inp[key]).replace("\n", " ")[:110]
                        brief = f" [{key}: {val}]"
                        break
                print(f"  → {name}{brief}", flush=True)
    elif t == "result":
        cost = e.get("total_cost_usd") or 0
        print(f"[done] {e.get('subtype', '')} | turns: {e.get('num_turns')} "
              f"| cost: ${cost:.2f}", flush=True)
