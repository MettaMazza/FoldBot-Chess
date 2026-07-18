#!/usr/bin/env python3
"""Verify sealed FoldBot/Stockfish match receipts and replay every move."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

import chess

ROOT = Path(__file__).resolve().parents[1]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def expected_result(board: chess.Board, bot_white: bool) -> str:
    outcome = board.outcome(claim_draw=True)
    if outcome is None:
        return "draw(cap)"
    if outcome.winner is None:
        return "draw"
    return "win" if (outcome.winner == chess.WHITE) == bot_white else "loss"


def verify(directory: Path, require_current_environment: bool = True) -> dict:
    directory = directory.resolve()
    registration_path = directory / "registration.json"
    match_path = directory / "match.json"
    registration = json.loads(registration_path.read_text())
    match = json.loads(match_path.read_text())
    if registration.get("schema") != "foldbot-stockfish-registration/v1" or \
            registration.get("status") != "registered":
        raise RuntimeError("unsupported FoldBot match registration")
    if match.get("schema") != "foldbot-stockfish-match/v1" or \
            match.get("status") != "completed":
        raise RuntimeError("FoldBot match is not completed")
    registration_sha = sha256(registration_path)
    if match.get("registration_sha256") != registration_sha:
        raise RuntimeError("FoldBot match registration hash mismatch")
    if require_current_environment:
        bindings = {
            "source": registration["source"],
            "move_cli": registration["move_cli"],
            "value_cli": registration["value_cli"],
        }
        for label, binding in bindings.items():
            path = ROOT / binding["path"]
            if not path.is_file() or sha256(path) != binding["sha256"]:
                raise RuntimeError(f"FoldBot registered binding drift: {label}")
        opponent = Path(registration["opponent"]["path"])
        if not opponent.is_file() or sha256(opponent) != registration["opponent"]["sha256"]:
            raise RuntimeError("FoldBot registered opponent drift")
    tally = {}
    for expected_game, binding in enumerate(match.get("games", []), 1):
        game_path = directory / binding["file"]
        if sha256(game_path) != binding["sha256"]:
            raise RuntimeError(f"FoldBot game hash mismatch: {binding['file']}")
        game = json.loads(game_path.read_text())
        if game.get("schema") != "foldbot-stockfish-game/v1" or \
                game.get("status") != "completed":
            raise RuntimeError(f"unsupported FoldBot game receipt: {binding['file']}")
        if game.get("game") != expected_game or \
                game.get("registration_sha256") != registration_sha:
            raise RuntimeError(f"FoldBot game registration/order mismatch: {binding['file']}")
        board = chess.Board()
        bot_turns = 0
        for text in game.get("moves_uci", []):
            move = chess.Move.from_uci(text)
            if move not in board.legal_moves:
                raise RuntimeError(f"FoldBot semantic replay rejected move: {binding['file']}")
            if (board.turn == chess.WHITE) == game["bot_white"]:
                bot_turns += 1
            board.push(move)
        if len(game.get("bot_complete_depths", [])) != bot_turns or \
                len(game.get("bot_move_seconds", [])) != bot_turns:
            raise RuntimeError(f"FoldBot telemetry count mismatch: {binding['file']}")
        if len(game.get("moves_uci", [])) > registration["protocol"]["maximum_plies"]:
            raise RuntimeError(f"FoldBot game exceeds registered ply bound: {binding['file']}")
        if game.get("result") != expected_result(board, game["bot_white"]):
            raise RuntimeError(f"FoldBot result replay mismatch: {binding['file']}")
        tally[game["result"]] = tally.get(game["result"], 0) + 1
    if len(match.get("games", [])) != registration["protocol"]["games"]:
        raise RuntimeError("FoldBot completed match lacks registered games")
    if match.get("result") != tally:
        raise RuntimeError("FoldBot match tally mismatch")
    return {
        "schema": "foldbot-stockfish-match-verification/v1",
        "status": "verified",
        "registration_sha256": registration_sha,
        "games": len(match["games"]),
        "result": tally,
        "semantic_replay": "passed",
        "current_environment_checked": require_current_environment,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("--allow-environment-drift", action="store_true")
    args = parser.parse_args()
    print(json.dumps(verify(
        args.directory,
        require_current_environment=not args.allow_environment_drift),
        sort_keys=True))


if __name__ == "__main__":
    main()
