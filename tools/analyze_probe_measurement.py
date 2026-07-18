#!/usr/bin/env python3
"""Validate and summarize the pinned Codex v20 auxiliary probe receipt."""
from __future__ import annotations

import glob
import hashlib
import json
import os


HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
RECEIPT = os.path.join(HERE, "probe_v20_2100.jsonl")


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def main():
    with open(RECEIPT) as source:
        rows = [json.loads(line) for line in source if line.strip()]
    if not rows or rows[0].get("type") != "header":
        raise RuntimeError("probe receipt has no identity header")
    header, positions = rows[0], rows[1:]
    for field, relative in (("move_cli_sha256", "tests/fold_bot_cli_v20"),
                            ("value_cli_sha256", "tests/fold_bot_value_cli_v20")):
        if sha256(os.path.join(ROOT, relative)) != header[field]:
            raise RuntimeError(f"pinned binary drift: {relative}")

    expected = set()
    death = set()
    pattern = os.path.join(HERE, "games_2100", "autopsy_game_*.json")
    for path in sorted(glob.glob(pattern)):
        with open(path) as source:
            autopsy = json.load(source)
        if autopsy["result"] != "loss":
            continue
        for blunder in autopsy["blunders"]:
            expected.add((autopsy["game"], blunder["ply"]))
        if autopsy.get("death_ply"):
            key = (autopsy["game"], autopsy["death_ply"])
            expected.add(key)
            death.add(key)

    keyed = {(row["game"], row["ply"]): row for row in positions}
    if len(keyed) != len(positions):
        raise RuntimeError("duplicate probe position")
    if set(keyed) != expected:
        raise RuntimeError(f"probe coverage differs: missing={sorted(expected-set(keyed))} "
                           f"extra={sorted(set(keyed)-expected)}")
    if any(row.get("status") != "completed" for row in positions):
        raise RuntimeError("probe contains an incomplete position")

    changed = sum(bool(row["changed"]) for row in positions)
    matches = sum(bool(row["matches_sf_best"]) for row in positions)
    death_changed = sum(bool(keyed[key]["changed"]) for key in death)
    agent_probe_rule_met = changed * 2 > len(positions) and death_changed * 2 > len(death)
    evidence = {
        "schema": "foldbot-chess-v20-probe-analysis/v1",
        "result_type": "Codex-authored auxiliary probe",
        "agent_identity": "Codex gpt-5.6-sol, high reasoning",
        "governance_authority": False,
        "engine": header["engine"],
        "ceiling": header["ceiling"],
        "workers": header["workers"],
        "receipt_sha256": sha256(RECEIPT),
        "move_cli_sha256": header["move_cli_sha256"],
        "value_cli_sha256": header["value_cli_sha256"],
        "positions_completed": len(positions),
        "decisions_changed": changed,
        "decisions_unchanged": len(positions) - changed,
        "matches_stockfish_best": matches,
        "death_slides": len(death),
        "death_slides_changed": death_changed,
        "agent_probe_rule": "changed decisions > 50% and changed death slides > 50%",
        "agent_probe_rule_met": agent_probe_rule_met,
        "run_authority": "Maria Smith alone decides when the build receives a real match",
        "interpretation": "measurement only; does not authorize, delay, or veto a match",
    }
    print(json.dumps(evidence, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
