#!/usr/bin/env python3
"""Seal generated-position move-and-value identity for the parallel root."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import subprocess

from tools import parallel_bot

ROOT = Path(__file__).resolve().parents[1]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git_commit() -> str | None:
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def generated_histories(ply: int, branch: int, ceiling: int) -> list[list[int]]:
    if ply < 0 or branch < 1:
        raise ValueError("ply must be non-negative and branch positive")
    frontier = [[]]
    for _ in range(ply):
        expanded = []
        for history in frontier:
            moves = parallel_bot.root_moves(history, ceiling)
            expanded.extend(history + [move] for move in moves[:branch])
        frontier = expanded
    return frontier


def fraction_text(value) -> str:
    return f"{value.numerator}/{value.denominator}"


def verify_position(history: list[int], ceiling: int, workers: int) -> dict:
    sequential = parallel_bot.sequential_move(history, ceiling)
    parallel, root_depth = parallel_bot.parallel_move(history, ceiling, workers)
    if sequential != parallel:
        raise RuntimeError("parallel/sequential move identity mismatch")
    if root_depth < 1:
        raise RuntimeError("parallel root produced no common completed depth")
    root_values = parallel_bot.position_passes(history, ceiling)
    if root_depth not in root_values:
        raise RuntimeError("sequential root did not complete the parallel decision depth")
    child_values = parallel_bot.child_passes(history, parallel, ceiling - 1)
    child_depth = root_depth - 1
    if child_depth not in child_values:
        raise RuntimeError("chosen child lacks the parallel decision depth")
    parallel_value = 1 - child_values[child_depth]
    sequential_value = root_values[root_depth]
    if parallel_value != sequential_value:
        raise RuntimeError("parallel/sequential exact value identity mismatch")
    return {
        "history": history,
        "history_length": len(history),
        "ceiling": ceiling,
        "common_root_depth": root_depth,
        "sequential_move": sequential,
        "parallel_move": parallel,
        "sequential_value": fraction_text(sequential_value),
        "parallel_value": fraction_text(parallel_value),
        "status": "identical",
    }


def verify(output: Path, ply: int, branch: int, ceiling: int, workers: int) -> dict:
    output = output.resolve()
    if output.exists():
        raise FileExistsError(f"parallel identity receipt already exists: {output}")
    histories = generated_histories(ply, branch, ceiling)
    rows = [verify_position(history, ceiling, workers) for history in histories]
    bindings = {}
    for label, path in {
            "move_cli": Path(parallel_bot.MOVE_CLI),
            "value_cli": Path(parallel_bot.VALUE_CLI),
            "parallel_driver": ROOT / "tools/parallel_bot.py",
            "identity_tool": Path(__file__).resolve(),
    }.items():
        resolved = path.resolve()
        bindings[label] = {"path": str(resolved.relative_to(ROOT)),
                           "bytes": resolved.stat().st_size,
                           "sha256": sha256(resolved)}
    record = {
        "schema": "foldbot-parallel-identity/v1",
        "status": "completed",
        "result_type": "measured implementation result",
        "governance_authority": False,
        "benchmark_authority": False,
        "generated_at_utc": datetime.now(timezone.utc).isoformat(),
        "checkout_base_commit": git_commit(),
        "sample_generation": {
            "rule": "breadth expansion by the first branch engine-generated root moves",
            "ply": ply,
            "branch": branch,
            "positions": len(histories),
        },
        "search": {"ceiling": ceiling, "workers": workers},
        "bindings": bindings,
        "rows": rows,
        "identity": {"moves": len(rows), "values": len(rows),
                     "disagreements": 0},
        "interpretation": (
            "generated-position calculation identity only; Maria Smith decides "
            "real-match timing and rank conclusions"
        ),
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    return record


def verify_receipt(path: Path) -> dict:
    path = path.resolve()
    record = json.loads(path.read_text())
    if record.get("schema") != "foldbot-parallel-identity/v1" or \
            record.get("status") != "completed":
        raise RuntimeError("unsupported parallel identity receipt")
    for label, binding in record.get("bindings", {}).items():
        bound = ROOT / binding["path"]
        if not bound.is_file() or bound.stat().st_size != binding["bytes"] or \
                sha256(bound) != binding["sha256"]:
            raise RuntimeError(f"parallel identity binding drift: {label}")
    rows = record.get("rows", [])
    for row in rows:
        if row.get("status") != "identical" or \
                row.get("sequential_move") != row.get("parallel_move") or \
                row.get("sequential_value") != row.get("parallel_value"):
            raise RuntimeError("parallel identity row disagreement")
    identity = record.get("identity", {})
    if identity != {"moves": len(rows), "values": len(rows), "disagreements": 0}:
        raise RuntimeError("parallel identity tally mismatch")
    return {
        "schema": "foldbot-parallel-identity-verification/v1",
        "status": "verified",
        "receipt_sha256": sha256(path),
        "positions": len(rows),
        "move_disagreements": 0,
        "value_disagreements": 0,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument("--ply", type=int, default=2)
    parser.add_argument("--branch", type=int, default=2)
    parser.add_argument("--ceiling", type=int, default=3)
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()
    record = (verify_receipt(args.output) if args.verify_only else verify(
        args.output, args.ply, args.branch, args.ceiling, args.workers))
    print(json.dumps(record, sort_keys=True))


if __name__ == "__main__":
    main()
