"""Pre-mortem probe: at the exact positions where the recorded 2100 games
went wrong, does the candidate release decide DIFFERENTLY?
Usage: python3 tools/probe_positions.py [games_dir=tools/games_2100] [scope=loss]
  scope: 'loss' = blunders + death plies of lost games only; 'all' = every game.
For each probed ply: replay the recorded game to that point, ask the pinned v20
spawned root (full match settings) for its move, and print it next to the
recorded move and full-strength Stockfish's best. Auxiliary diagnosis only—
nothing here feeds the engine or authorizes, delays, or vetoes a real match.
Every completed position is written immediately to a hash-bound JSONL receipt."""
import sys, os, json, glob, hashlib
from datetime import datetime, timezone
import chess

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import parallel_bot

DIR = sys.argv[1] if len(sys.argv) > 1 else os.path.join(HERE, "games_2100")
SCOPE = sys.argv[2] if len(sys.argv) > 2 else "loss"
WORKERS = 26
CEILING = 12
ENGINE_TAG = "v20"
VALUE_CLI = os.path.join(HERE, "..", "tests", "fold_bot_value_cli_v20")
MOVE_CLI = os.path.join(HERE, "..", "tests", "fold_bot_cli_v20")
parallel_bot.ENGINE_TAG = ENGINE_TAG
parallel_bot.VALUE_CLI = VALUE_CLI
parallel_bot.MOVE_CLI = MOVE_CLI
RECEIPT = os.path.join(HERE, "probe_v20_2100.jsonl")

def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()

for binary, role in ((VALUE_CLI, "value"), (MOVE_CLI, "move")):
    parallel_bot.require_binary(binary, role)

header = {
    "type": "header",
    "created_utc": datetime.now(timezone.utc).isoformat(),
    "engine": ENGINE_TAG,
    "value_cli": os.path.abspath(VALUE_CLI),
    "value_cli_sha256": sha256(VALUE_CLI),
    "move_cli": os.path.abspath(MOVE_CLI),
    "move_cli_sha256": sha256(MOVE_CLI),
    "games_dir": os.path.abspath(DIR),
    "scope": SCOPE,
    "workers": WORKERS,
    "ceiling": CEILING,
}

completed = set()
if os.path.exists(RECEIPT):
    with open(RECEIPT) as f:
        rows = [json.loads(line) for line in f if line.strip()]
    if not rows or rows[0].get("type") != "header":
        raise RuntimeError(f"receipt lacks header: {RECEIPT}")
    old = rows[0]
    bound = ("engine", "value_cli_sha256", "move_cli_sha256", "games_dir", "scope", "workers", "ceiling")
    if any(old.get(k) != header.get(k) for k in bound):
        raise RuntimeError(f"receipt identity differs; preserve it and choose a new path: {RECEIPT}")
    completed = {(row["game"], row["ply"]) for row in rows[1:] if row.get("type") == "position"}
else:
    with open(RECEIPT, "w") as f:
        f.write(json.dumps(header, sort_keys=True) + "\n")

PROMO = {chess.QUEEN: 5, chess.ROOK: 4, chess.BISHOP: 3, chess.KNIGHT: 2}
def enc(m): return (PROMO[m.promotion] if m.promotion else 0)*4096 + m.from_square*64 + m.to_square

probes = []
for path in sorted(glob.glob(os.path.join(DIR, "autopsy_game_*.json"))):
    a = json.load(open(path))
    if SCOPE == "loss" and a["result"] != "loss":
        continue
    g = json.load(open(path.replace("autopsy_", "")))
    plies = set()
    for b in a["blunders"]:
        plies.add(b["ply"])
    if a.get("death_ply"):
        plies.add(a["death_ply"])
    for ply in sorted(plies):
        # the recorded move at this ply must be the bot's own move
        rec = next((b for b in a["blunders"] if b["ply"] == ply), None)
        probes.append({"game": a["game"], "ply": ply,
                       "moves": g["moves_uci"][:ply-1],
                       "played": g["moves_uci"][ply-1],
                       "sf_best": rec["sf_best"] if rec else None,
                       "loss_cp": rec["loss"] if rec else None,
                       "phase": rec["phase"] if rec else "death"})

print(f"{len(probes)} positions to probe ({SCOPE} scope), engine {ENGINE_TAG}", flush=True)
print(f"receipt: {RECEIPT}", flush=True)
changed = kept = agree_sf = 0
for i, p in enumerate(probes):
    if (p["game"], p["ply"]) in completed:
        print(f"[{i+1}/{len(probes)}] g{p['game']} ply {p['ply']}: already receipted", flush=True)
        continue
    board = chess.Board(); hist = []
    for u in p["moves"]:
        mv = chess.Move.from_uci(u)
        hist.append(enc(mv)); board.push(mv)
    if len(board.piece_map()) <= 3:
        row = {"type": "position", **p, "status": "table-territory-skipped"}
        with open(RECEIPT, "a") as f:
            f.write(json.dumps(row, sort_keys=True) + "\n")
        print(f"[{i+1}/{len(probes)}] g{p['game']} ply {p['ply']}: table territory, skipped", flush=True)
        continue
    v, d = parallel_bot.parallel_move(hist, CEILING, WORKERS)
    new = parallel_bot.dec(v).uci()
    delta = "CHANGED" if new != p["played"] else "same"
    sf = " =SF" if p["sf_best"] and new == p["sf_best"] else ""
    if new != p["played"]: changed += 1
    else: kept += 1
    if sf: agree_sf += 1
    row = {"type": "position", **p, "status": "completed", "new_move": new,
           "complete_depth": d, "changed": new != p["played"],
           "matches_sf_best": bool(sf)}
    with open(RECEIPT, "a") as f:
        f.write(json.dumps(row, sort_keys=True) + "\n")
    print(f"[{i+1}/{len(probes)}] g{p['game']} ply {p['ply']} ({p['phase']}, "
          f"{p['loss_cp']}cp): played {p['played']} -> {ENGINE_TAG} {new} d{d} {delta}{sf}", flush=True)
print(f"\nSUMMARY: {changed} changed, {kept} unchanged, {agree_sf} now match SF best "
      f"of {changed+kept} probed")
