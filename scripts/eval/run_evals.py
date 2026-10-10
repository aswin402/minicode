#!/usr/bin/env python3
"""minicode orchestration eval harness.

Runs fixed prompts against the minicode binary in throwaway workspaces and
scores the *orchestration behaviour* (did it ask, scaffold, plan, use blocks,
write before deciding a foundation) from the session JSONL that minicode
writes to <workspace>/.minicode/sessions/.

The model is non-deterministic, so every case should be run several times.
Results are written to scripts/eval/results/<timestamp>/.

Usage:
  python3 scripts/eval/run_evals.py --runs 3
  python3 scripts/eval/run_evals.py --cases greenfield_vague,existing_bugfix --runs 2
  python3 scripts/eval/run_evals.py --dry-run
  # Score existing session logs without calling the model (validates the scorer):
  python3 scripts/eval/run_evals.py --score-session path/to/session.jsonl --case greenfield_rich_no_stack
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

EVAL_DIR = Path(__file__).resolve().parent

# Tools that put bytes on disk or commit to a technical foundation.
MUTATING_TOOLS = {
    "write_file",
    "patch_file",
    "replace_in_files",
    "kit_stack_add",
    "block_insert",
    "block_scaffold",
}
FILE_EDIT_TOOLS = {"write_file", "patch_file", "replace_in_files"}


# --------------------------------------------------------------------------- #
# Session parsing & scoring
# --------------------------------------------------------------------------- #
def parse_session(path: Path) -> dict:
    calls: list[dict] = []
    results: dict[str, dict] = {}
    plan_updates = 0
    for raw in path.read_text(encoding="utf-8", errors="replace").splitlines():
        raw = raw.strip()
        if not raw:
            continue
        try:
            ev = json.loads(raw)
        except json.JSONDecodeError:
            continue
        kind = ev.get("event")
        if kind == "tool_call":
            calls.append({"id": ev.get("tool_id"), "tool": ev.get("tool"), "args": ev.get("args") or {}})
        elif kind == "tool_result":
            results[ev.get("tool_id")] = {"success": ev.get("success", True), "output": ev.get("output", "")}
        elif kind == "plan_updated":
            plan_updates += 1
    for c in calls:
        r = results.get(c["id"], {})
        c["success"] = r.get("success", True)
        c["output_head"] = str(r.get("output", ""))[:200]
        # Calls returned by the harness foundation checkpoint were never executed.
        out = str(r.get("output", ""))
        c["checkpointed"] = "foundation_checkpoint" in out or "plan_approval_checkpoint" in out
        c["plan_held"] = "plan_approval_checkpoint" in out
    return {"calls": calls, "plan_updates": plan_updates}


def _path_arg(args: dict) -> str:
    for key in ("path", "file", "file_path", "target"):
        v = args.get(key)
        if isinstance(v, str):
            return v
    return ""


def score(parsed: dict, expect: dict) -> dict:
    calls = parsed["calls"]
    seq = [c["tool"] for c in calls]

    def first(pred) -> int | None:
        for i, c in enumerate(calls):
            if pred(c):
                return i
        return None

    # An ask_user that directly answers a plan-approval checkpoint is an approval
    # round, not a clarifying question; "should not ask" checks ignore it.
    approval_ids = set()
    for i, c in enumerate(calls):
        if c["tool"] == "ask_user" and i > 0 and calls[i - 1].get("plan_held"):
            approval_ids.add(i)
    ask_idx = first(lambda c: c["tool"] == "ask_user")
    clarify_asked = any(c["tool"] == "ask_user" and i not in approval_ids for i, c in enumerate(calls))
    mut_idx = first(lambda c: c["tool"] in MUTATING_TOOLS and not c.get("checkpointed"))
    kit_calls = [c for c in calls if c["tool"] == "kit_stack_add"]
    write_fail = [c for c in calls if c["tool"] in FILE_EDIT_TOOLS and not c["success"] and not c.get("checkpointed")]

    m = {
        "tool_calls": len(calls),
        "sequence": seq,
        "first_tool": seq[0] if seq else None,
        "asked": ask_idx is not None,
        "clarify_asked": clarify_asked,
        "approvals": len(approval_ids),
        "ask_before_write": ask_idx is not None and (mut_idx is None or ask_idx < mut_idx),
        "first_mutation": (calls[mut_idx]["tool"] + ":" + _path_arg(calls[mut_idx]["args"])) if mut_idx is not None else None,
        "scaffolded": bool(kit_calls),
        "stacks": [c["args"].get("stack_name") or c["args"].get("stack") for c in kit_calls],
        "plan_created": "create_plan" in seq,
        "progress_updates": seq.count("update_progress"),
        "blocks_used": any(t and t.startswith("block_") for t in seq),
        "write_failures": len(write_fail),
        "failed_tools": [c["tool"] for c in calls if not c["success"] and not c.get("checkpointed")],
        "checkpoints": sum(1 for c in calls if c.get("checkpointed")),
    }

    checks: dict[str, bool] = {}
    if "ask_before_write" in expect:
        checks["ask_before_write"] = m["ask_before_write"] == expect["ask_before_write"]
    if "ask" in expect:
        checks["ask"] = m["clarify_asked"] == expect["ask"]
    if "scaffold" in expect:
        checks["scaffold"] = m["scaffolded"] == expect["scaffold"]
    if "edits_file" in expect:
        target = expect["edits_file"]
        checks["edits_file"] = any(
            c["tool"] in FILE_EDIT_TOOLS and c["success"] and _path_arg(c["args"]).endswith(target) for c in calls
        )
    m["checks"] = checks
    m["passed"] = all(checks.values()) if checks else None
    return m


# --------------------------------------------------------------------------- #
# Running
# --------------------------------------------------------------------------- #
def load_cases(selected: set[str] | None) -> list[dict]:
    cases = json.loads((EVAL_DIR / "cases.json").read_text())
    for c in cases:
        if "prompt_file" in c:
            c["prompt"] = (EVAL_DIR / c["prompt_file"]).read_text()
    if selected:
        unknown = selected - {c["id"] for c in cases}
        if unknown:
            sys.exit(f"Unknown case id(s): {', '.join(sorted(unknown))}")
        cases = [c for c in cases if c["id"] in selected]
    return cases


def prepare_workspace(case: dict) -> Path:
    ws = Path(tempfile.mkdtemp(prefix=f"minicode-eval-{case['id']}-"))
    if case.get("fixture"):
        shutil.copytree(EVAL_DIR / case["fixture"], ws, dirs_exist_ok=True)
    return ws


def build_cmd(args, case: dict, ws: Path) -> list[str]:
    cmd = [args.bin, "run", case["prompt"], "-d", str(ws), "--json-stream", "-y",
           "--max-iterations", str(args.max_iterations), "--auto-continue", "false"]
    if args.model:
        cmd += ["-m", args.model]
    if args.provider:
        cmd += ["-p", args.provider]
    return cmd


def run_one(args, case: dict, run_idx: int, out_dir: Path) -> dict:
    ws = prepare_workspace(case)
    cmd = build_cmd(args, case, ws)
    log_path = out_dir / f"{case['id']}.run{run_idx}.stdout.ndjson"
    started = time.time()
    timed_out = False
    exit_code = None
    with open(log_path, "w") as log:
        try:
            proc = subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT, timeout=args.timeout,
                                  env={**os.environ, "NO_COLOR": "1"})
            exit_code = proc.returncode
        except subprocess.TimeoutExpired:
            timed_out = True
    duration = round(time.time() - started, 1)

    sessions = sorted((ws / ".minicode" / "sessions").glob("*.jsonl"))
    if sessions:
        session_copy = out_dir / f"{case['id']}.run{run_idx}.session.jsonl"
        shutil.copy(sessions[-1], session_copy)
        metrics = score(parse_session(sessions[-1]), case.get("expect", {}))
    else:
        metrics = {"error": "no session file produced", "passed": False, "checks": {}}

    if not args.keep_workspaces:
        shutil.rmtree(ws, ignore_errors=True)

    return {"case": case["id"], "run": run_idx, "exit_code": exit_code, "timed_out": timed_out,
            "duration_s": duration, "workspace": str(ws) if args.keep_workspaces else None, **metrics}


# --------------------------------------------------------------------------- #
# Reporting
# --------------------------------------------------------------------------- #
def rate(rows: list[dict], key: str) -> str:
    vals = [bool(r.get(key)) for r in rows if "error" not in r]
    return f"{sum(vals)}/{len(vals)}" if vals else "-"


def write_summary(rows: list[dict], cases: list[dict], out_dir: Path) -> str:
    lines = ["# minicode orchestration eval", "",
             f"Generated: {dt.datetime.now().isoformat(timespec='seconds')}", "",
             "| case | pass | asked | ask→write | scaffold | plan | blocks | avg calls | write fails | checkpoints |",
             "|---|---|---|---|---|---|---|---|---|---|"]
    total_pass = total = 0
    for case in cases:
        rs = [r for r in rows if r["case"] == case["id"]]
        if not rs:
            continue
        passed = sum(1 for r in rs if r.get("passed"))
        total_pass += passed
        total += len(rs)
        ok = [r for r in rs if "error" not in r]
        avg_calls = round(sum(r["tool_calls"] for r in ok) / len(ok), 1) if ok else "-"
        wf = sum(r.get("write_failures", 0) for r in ok)
        lines.append(f"| {case['id']} | {passed}/{len(rs)} | {rate(rs, 'asked')} | {rate(rs, 'ask_before_write')} | "
                     f"{rate(rs, 'scaffolded')} | {rate(rs, 'plan_created')} | {rate(rs, 'blocks_used')} | {avg_calls} | {wf} | {sum(r.get('checkpoints', 0) for r in ok)} |")
    lines += ["", f"**Overall: {total_pass}/{total} runs met expectations.**", "", "## Expectations", ""]
    for case in cases:
        lines.append(f"- `{case['id']}`: {json.dumps(case.get('expect', {}))} — {case.get('description', '')}")
    lines += ["", "## Per-run sequences", ""]
    for r in rows:
        seq = " → ".join(r.get("sequence", [])[:20])
        lines.append(f"- `{r['case']}` run{r['run']} pass={r.get('passed')} checks={r.get('checks')} "
                     f"first_mutation={r.get('first_mutation')}\n  - {seq}")
    text = "\n".join(lines) + "\n"
    (out_dir / "summary.md").write_text(text)
    return text


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--bin", default=os.environ.get("MINICODE_BIN", "minicode"))
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--cases", help="comma-separated case ids (default: all)")
    ap.add_argument("--max-iterations", type=int, default=12,
                    help="tool iterations per run; orchestration decisions happen early, so keep this low to save cost")
    ap.add_argument("--timeout", type=int, default=600, help="seconds per run")
    ap.add_argument("--model")
    ap.add_argument("--provider")
    ap.add_argument("--out", help="output dir (default: scripts/eval/results/<timestamp>)")
    ap.add_argument("--keep-workspaces", action="store_true")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--score-session", help="score an existing session JSONL instead of running")
    ap.add_argument("--case", help="case id whose expectations to use with --score-session")
    args = ap.parse_args()

    if args.score_session:
        cases = load_cases({args.case} if args.case else None)
        expect = cases[0].get("expect", {}) if args.case else {}
        print(json.dumps(score(parse_session(Path(args.score_session)), expect), indent=2))
        return

    selected = set(args.cases.split(",")) if args.cases else None
    cases = load_cases(selected)

    if args.dry_run:
        for c in cases:
            cmd = build_cmd(args, c, Path("<tmp-workspace>"))
            cmd[2] = (c["prompt"][:60] + "…") if len(c["prompt"]) > 60 else c["prompt"]
            print(f"{c['id']}: {' '.join(repr(x) if ' ' in x else x for x in cmd)}")
        print(f"\n{len(cases)} cases × {args.runs} runs = {len(cases) * args.runs} model sessions")
        return

    if shutil.which(args.bin) is None and not Path(args.bin).exists():
        sys.exit(f"minicode binary not found: {args.bin}")

    out_dir = Path(args.out) if args.out else EVAL_DIR / "results" / dt.datetime.now().strftime("%Y%m%d-%H%M%S")
    out_dir.mkdir(parents=True, exist_ok=True)

    rows: list[dict] = []
    results_path = out_dir / "results.jsonl"
    for case in cases:
        for i in range(1, args.runs + 1):
            print(f"[{case['id']}] run {i}/{args.runs} …", flush=True)
            row = run_one(args, case, i, out_dir)
            rows.append(row)
            with open(results_path, "a") as f:
                f.write(json.dumps(row) + "\n")
            print(f"   pass={row.get('passed')} asked={row.get('asked')} scaffold={row.get('scaffolded')} "
                  f"calls={row.get('tool_calls')} {row['duration_s']}s", flush=True)

    print("\n" + write_summary(rows, cases, out_dir))
    print(f"Results: {out_dir}")


if __name__ == "__main__":
    main()
