#!/usr/bin/env python3
"""Run every local gate from CLAUDE.md in order and print one report.

    python tools/gates.py              # all seven gates
    python tools/gates.py --only rust  # just the cargo ones
    python tools/gates.py --list

Each step gets a timeout, so a hung gate fails the run instead of stalling it
forever. While a step runs, progress is written as a bar on a terminal and as a
periodic "still running" line when the output is piped or read by an agent --
either way silence never means "probably fine".

Exit code is 0 only if every step that ran passed.
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TAURI = ROOT / "src-tauri"

# name, argv, cwd, group -- the two commands CLAUDE.md's "Gates" section names,
# split into one step each so a failure points at a single command.
STEPS = [
    ("typecheck", ["pnpm", "typecheck"], ROOT, "frontend"),
    ("lint", ["pnpm", "lint"], ROOT, "frontend"),
    ("test", ["pnpm", "test"], ROOT, "frontend"),
    ("build", ["pnpm", "exec", "vite", "build"], ROOT, "frontend"),
    ("fmt", ["cargo", "fmt", "--check"], TAURI, "rust"),
    ("clippy", ["cargo", "clippy", "--all-targets", "--", "-D", "warnings"], TAURI, "rust"),
    ("cargo-test", ["cargo", "test"], TAURI, "rust"),
]

HEARTBEAT = 30.0  # seconds between "still running" lines when not on a terminal
TAIL_LINES = 40  # how much of a failing step's output to show


def gate_env() -> dict[str, str]:
    """PATH with cargo on it -- a fresh Windows shell here does not have it."""
    env = os.environ.copy()
    cargo_bin = Path.home() / ".cargo" / "bin"
    if cargo_bin.is_dir():
        env["PATH"] = f"{cargo_bin}{os.pathsep}{env.get('PATH', '')}"
    return env


def resolve(argv: list[str], env: dict[str, str]) -> list[str] | None:
    """Absolute path for argv[0], or None if it is not installed.

    Needed on Windows, where `pnpm` is really `pnpm.CMD` and bare Popen misses it.
    """
    exe = shutil.which(argv[0], path=env["PATH"])
    return [exe, *argv[1:]] if exe else None


def render(idx: int, total: int, name: str, elapsed: float, tty: bool) -> None:
    if not tty:
        return
    done = "#" * idx
    todo = "." * (total - idx)
    sys.stdout.write(f"\r[{done}{todo}] {idx + 1}/{total} {name} {elapsed:5.0f}s ")
    sys.stdout.flush()


def run_step(idx: int, total: int, name: str, argv: list[str], cwd: Path,
             timeout: float, env: dict[str, str], tty: bool) -> tuple[str, float, str]:
    """Run one gate. Returns (status, seconds, captured output)."""
    resolved = resolve(argv, env)
    if resolved is None:
        return "SKIP", 0.0, f"{argv[0]} is not on PATH"

    if not tty:
        print(f"[{idx + 1}/{total}] {name}: {' '.join(argv)}", flush=True)

    started = time.monotonic()
    with tempfile.TemporaryFile("w+", encoding="utf-8", errors="replace") as sink:
        proc = subprocess.Popen(
            resolved, cwd=cwd, env=env, stdout=sink, stderr=subprocess.STDOUT
        )
        next_beat = started + HEARTBEAT
        while True:
            try:
                proc.wait(timeout=1.0)
                break
            except subprocess.TimeoutExpired:
                pass
            now = time.monotonic()
            elapsed = now - started
            if elapsed > timeout:
                proc.kill()
                proc.wait()
                sink.seek(0)
                return "TIMEOUT", elapsed, sink.read()
            render(idx, total, name, elapsed, tty)
            if not tty and now >= next_beat:
                print(f"    ... still running ({elapsed:.0f}s)", flush=True)
                next_beat = now + HEARTBEAT

        elapsed = time.monotonic() - started
        sink.seek(0)
        output = sink.read()

    status = "PASS" if proc.returncode == 0 else f"FAIL({proc.returncode})"
    if tty:
        sys.stdout.write("\r" + " " * 70 + "\r")
    print(f"[{idx + 1}/{total}] {name}: {status} in {elapsed:.0f}s", flush=True)
    return status, elapsed, output


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--only", metavar="MATCH", action="append", default=[],
                    help="run only steps whose name or group contains MATCH (repeatable)")
    ap.add_argument("--timeout", type=float, default=900.0,
                    help="per-step timeout in seconds (default 900)")
    ap.add_argument("--list", action="store_true", help="list the steps and exit")
    args = ap.parse_args()

    # Tool output is full of box-drawing and check marks; the Windows console is
    # cp1252 by default and raises on them mid-report.
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

    if args.list:
        for name, argv, cwd, group in STEPS:
            print(f"{name:10} [{group}] {' '.join(argv)}  (in {cwd.name})")
        return 0

    steps = [s for s in STEPS
             if not args.only or any(m in s[0] or m in s[3] for m in args.only)]
    if not steps:
        print(f"no step matches {args.only}", file=sys.stderr)
        return 2

    env = gate_env()
    tty = sys.stdout.isatty()
    results = []
    for idx, (name, argv, cwd, _group) in enumerate(steps):
        status, elapsed, output = run_step(
            idx, len(steps), name, argv, cwd, args.timeout, env, tty
        )
        results.append((name, status, elapsed, output))
        if status.startswith(("FAIL", "TIMEOUT")):
            break  # later gates run on the same tree; the first failure is the one to fix

    print("\n=== gates ===")
    for name, status, elapsed, _ in results:
        print(f"  {name:10} {status:10} {elapsed:6.0f}s")
    ran = {r[0] for r in results}
    for step in steps:
        if step[0] not in ran:
            print(f"  {step[0]:10} {'NOT RUN':10}")

    bad = [r for r in results if r[1].startswith(("FAIL", "TIMEOUT"))]
    if not bad:
        print("all green")
        return 0

    name, status, _, output = bad[0]
    tail = output.strip().splitlines()[-TAIL_LINES:]
    print(f"\n--- {name} {status}, last {len(tail)} lines ---")
    print("\n".join(tail))
    return 1


if __name__ == "__main__":
    sys.exit(main())
