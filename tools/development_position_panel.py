#!/usr/bin/env python3
"""Measure the current engine on every recorded 2100 loss position.

This is a source-bound development panel, not an official match or a gate on
Maria Smith's benchmark authority.  It replays real recorded positions and
preserves every result, including unchanged or slower decisions.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import glob
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

import chess

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
import parallel_bot  # noqa: E402

PROMO = {chess.QUEEN: 5, chess.ROOK: 4, chess.BISHOP: 3, chess.KNIGHT: 2}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def encode(move: chess.Move) -> int:
    return (PROMO[move.promotion] if move.promotion else 0) * 4096 + \
        move.from_square * 64 + move.to_square


def append(path: Path, row: dict) -> None:
    with path.open("a") as handle:
        handle.write(json.dumps(row, sort_keys=True) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    parser.add_argument("--games", type=Path, default=ROOT / "tools/games_2100")
    parser.add_argument("--ceiling", type=int, default=3)
    parser.add_argument("--workers", type=int, default=9)
    parser.add_argument("--engine-tag", choices=("current", "v20"),
                        default="current")
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        raise FileExistsError(f"development panel exists: {output}")

    suffix = "" if args.engine_tag == "current" else "_v20"
    parallel_bot.ENGINE_TAG = (
        "current-development" if args.engine_tag == "current" else "v20")
    parallel_bot.MOVE_CLI = str(ROOT / f"tests/fold_bot_cli{suffix}")
    parallel_bot.VALUE_CLI = str(ROOT / f"tests/fold_bot_value_cli{suffix}")
    parallel_bot.require_binary(parallel_bot.MOVE_CLI, "move")
    parallel_bot.require_binary(parallel_bot.VALUE_CLI, "value")
    commit = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()

    probes = []
    for autopsy_path in sorted(glob.glob(str(args.games.resolve() / "autopsy_game_*.json"))):
        autopsy = json.loads(Path(autopsy_path).read_text())
        if autopsy["result"] != "loss":
            continue
        game_path = Path(autopsy_path.replace("autopsy_", ""))
        game = json.loads(game_path.read_text())
        by_ply = {row["ply"]: row for row in autopsy["blunders"]}
        plies = set(by_ply)
        if autopsy.get("death_ply"):
            plies.add(autopsy["death_ply"])
        for ply in sorted(plies):
            event = by_ply.get(ply)
            probes.append({
                "game": autopsy["game"],
                "ply": ply,
                "history_uci": game["moves_uci"][:ply - 1],
                "played": game["moves_uci"][ply - 1],
                "stockfish_best": event["sf_best"] if event else None,
                "centipawn_loss": event["loss"] if event else None,
                "phase": event["phase"] if event else "death",
                "source_game_sha256": sha256(game_path),
                "source_autopsy_sha256": sha256(Path(autopsy_path)),
            })

    header = {
        "type": "header",
        "schema": "foldbot-current-development-position-panel/v1",
        "status": "running",
        "governance_authority": False,
        "authority": "Real-position development evidence; Maria Smith assigns conclusions and official status.",
        "created_at_utc": datetime.now(timezone.utc).isoformat(),
        "source_commit": commit,
        "engine_tag": args.engine_tag,
        "source": {"path": "tools/development_position_panel.py", "sha256": sha256(Path(__file__))},
        "move_cli": {"path": "tests/fold_bot_cli", "sha256": sha256(Path(parallel_bot.MOVE_CLI))},
        "value_cli": {"path": "tests/fold_bot_value_cli", "sha256": sha256(Path(parallel_bot.VALUE_CLI))},
        "games": str(args.games.resolve()),
        "positions": len(probes),
        "ceiling": args.ceiling,
        "workers": args.workers,
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    append(output, header)
    for index, probe in enumerate(probes, 1):
        history = [encode(chess.Move.from_uci(move)) for move in probe.pop("history_uci")]
        started = time.monotonic()
        encoded, depth = parallel_bot.parallel_move(
            history, args.ceiling, args.workers)
        elapsed = time.monotonic() - started
        move = parallel_bot.dec(encoded).uci()
        row = {
            "type": "position",
            "status": "completed",
            **probe,
            "current_move": move,
            "complete_depth": depth,
            "seconds": elapsed,
            "changed_from_played": move != probe["played"],
            "matches_stockfish_best": (
                probe["stockfish_best"] is not None and
                move == probe["stockfish_best"]),
        }
        append(output, row)
        print(f"[{index}/{len(probes)}] g{row['game']} p{row['ply']} "
              f"{row['played']} -> {move} d{depth} {elapsed:.3f}s", flush=True)
    append(output, {
        "type": "summary",
        "status": "completed",
        "positions": len(probes),
        "completed_at_utc": datetime.now(timezone.utc).isoformat(),
    })


if __name__ == "__main__":
    main()
