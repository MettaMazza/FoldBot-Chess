"""N games vs SF at a given Elo, bot = pinned v20 (engine + root-split process
driver, the derived rules). Pinned binaries, full recording, python-chess
referee on every bot move, per-move telemetry (seconds + complete depth).
Usage: python3 tools/match_parallel.py <elo> <concurrent_games> <workers_per_game> <games> <output_dir>
Games run <concurrent_games> at a time; each bot move fans its root moves
across <workers_per_game> full-clock workers. 3-man endings route to the
sequential CLI (certified table probe), as the sequential bot does."""
import subprocess, sys, os, json, shutil, tempfile, atexit, time, hashlib, platform
from datetime import datetime, timezone
from pathlib import Path
import chess, chess.engine
from concurrent.futures import ProcessPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
ELO = int(sys.argv[1]) if len(sys.argv) > 1 else 2100
CONC = int(sys.argv[2]) if len(sys.argv) > 2 else 3
WORKERS = int(sys.argv[3]) if len(sys.argv) > 3 else 9
GAMES = int(sys.argv[4]) if len(sys.argv) > 4 else 12
OUT = os.path.abspath(sys.argv[5]) if len(sys.argv) > 5 else \
    os.path.join(HERE, f"games_par_{ELO}")
SF = "/opt/homebrew/bin/stockfish"
CEILING = 12
ENGINE_TAG = "v20"

def pin(src):
    t = tempfile.NamedTemporaryFile(delete=False, suffix="_fold_pin")
    t.close()
    shutil.copy2(src, t.name)
    os.chmod(t.name, 0o755)
    atexit.register(lambda p=t.name: os.unlink(p))
    return t.name

MOVE_SOURCE = os.path.join(HERE, "..", "tests", "fold_bot_cli_v20")
VALUE_SOURCE = os.path.join(HERE, "..", "tests", "fold_bot_value_cli_v20")
MOVE_PIN = pin(MOVE_SOURCE)
VALUE_PIN = pin(VALUE_SOURCE)

def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()

MOVE_SHA256 = sha256(MOVE_SOURCE)
VALUE_SHA256 = sha256(VALUE_SOURCE)

def json_bytes(record):
    return (json.dumps(record, indent=2, sort_keys=True) + "\n").encode()

def write_new_json(path, record):
    with open(path, "xb") as handle:
        handle.write(json_bytes(record))

def git_commit():
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=os.path.join(HERE, ".."),
            text=True).strip()
    except (OSError, subprocess.CalledProcessError):
        return None

def stockfish_identity():
    engine = chess.engine.SimpleEngine.popen_uci(SF)
    try:
        return dict(engine.id)
    finally:
        engine.quit()

PROMO = {chess.QUEEN: 5, chess.ROOK: 4, chess.BISHOP: 3, chess.KNIGHT: 2}
CP = {v: k for k, v in PROMO.items()}
def enc(m): return (PROMO[m.promotion] if m.promotion else 0)*4096 + m.from_square*64 + m.to_square
def dec(v):
    p, r = v//4096, v % 4096
    return chess.Move(r//64, r % 64, promotion=CP[p] if p else None)

def seq_move(hist):
    feed = "\n".join([str(CEILING)]+[str(m) for m in hist]+["8888"])+"\n"
    return int(subprocess.run([MOVE_PIN], input=feed, capture_output=True, text=True,
                              timeout=1800).stdout.strip().splitlines()[-1])

def play_one(g):
    sys.path.insert(0, HERE)
    import parallel_bot
    parallel_bot.VALUE_CLI = VALUE_PIN
    parallel_bot.MOVE_CLI = MOVE_PIN
    bot_white = g % 2 == 0
    eng = chess.engine.SimpleEngine.popen_uci(SF)
    eng.configure({"UCI_LimitStrength": True, "UCI_Elo": ELO})
    board = chess.Board(); hist = []; uci = []; depths = []; times = []
    while not board.is_game_over(claim_draw=True) and len(hist) < 240:
        if (board.turn == chess.WHITE) == bot_white:
            t0 = time.time()
            if len(board.piece_map()) <= 3:
                v = seq_move(hist); d = 0        # certified table territory
            else:
                v, d = parallel_bot.parallel_move(hist, CEILING, WORKERS)
            dt = time.time() - t0
            mv = dec(v)
            if mv not in board.legal_moves:
                eng.quit(); return (g, "ILLEGAL", uci, bot_white, depths, times)
            board.push(mv); hist.append(enc(mv)); uci.append(mv.uci())
            depths.append(d); times.append(round(dt, 1))
            print(f"  g{g+1} ply {len(hist)}: {mv.uci()} d{d} {dt:.0f}s", flush=True)
        else:
            r = eng.play(board, chess.engine.Limit(time=0.05))
            board.push(r.move); hist.append(enc(r.move)); uci.append(r.move.uci())
    eng.quit()
    o = board.outcome(claim_draw=True)
    if o is None: res = "draw(cap)"
    elif o.winner is None: res = "draw"
    else: res = "win" if (o.winner == chess.WHITE) == bot_white else "loss"
    return (g, res, uci, bot_white, depths, times)

if __name__ == "__main__":
    output = Path(OUT)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.mkdir()
    registration = {
        "schema": "foldbot-stockfish-registration/v1",
        "status": "registered",
        "registered_at_utc": datetime.now(timezone.utc).isoformat(),
        "source_commit": git_commit(),
        "source": {"path": "tools/match_parallel.py",
                   "sha256": sha256(__file__)},
        "move_cli": {"path": "tests/fold_bot_cli_v20",
                     "sha256": MOVE_SHA256},
        "value_cli": {"path": "tests/fold_bot_value_cli_v20",
                      "sha256": VALUE_SHA256},
        "opponent": {"path": SF, "sha256": sha256(SF),
                     "uci_identity": stockfish_identity(),
                     "limit_strength": True, "uci_elo": ELO,
                     "move_time_seconds": 0.05},
        "protocol": {"games": GAMES, "concurrent_games": CONC,
                     "workers_per_game": WORKERS, "search_ceiling": CEILING,
                     "colour_schedule": "FoldBot White in odd-numbered games and Black in even-numbered games",
                     "maximum_plies": 240,
                     "terminal_rule": "python-chess outcome with claim_draw=True"},
        "hardware": {"platform": platform.platform(), "machine": platform.machine(),
                     "logical_cpu_count": os.cpu_count()},
        "governance_authority": False,
        "interpretation": "measured match receipt; Maria Smith assigns rank and publication conclusions",
    }
    registration_bytes = json_bytes(registration)
    registration_sha = hashlib.sha256(registration_bytes).hexdigest()
    with open(output / "registration.json", "xb") as handle:
        handle.write(registration_bytes)
    tally = {}
    game_bindings = []
    with ProcessPoolExecutor(max_workers=CONC) as pool:
        for g, res, uci, bot_white, depths, times in pool.map(play_one, range(GAMES)):
            if res == "ILLEGAL":
                raise RuntimeError(f"FoldBot emitted an illegal move in game {g + 1}")
            tally[res] = tally.get(res, 0) + 1
            rec = {"schema": "foldbot-stockfish-game/v1", "status": "completed",
                   "registration_sha256": registration_sha,
                   "game": g+1, "elo": ELO, "engine": ENGINE_TAG,
                   "move_cli_sha256": MOVE_SHA256, "value_cli_sha256": VALUE_SHA256,
                   "bot_white": bot_white, "result": res,
                   "plies": len(uci), "moves_uci": uci,
                   "bot_complete_depths": depths, "bot_move_seconds": times}
            game_name = f"game_{g+1:02d}.json"
            game_path = output / game_name
            write_new_json(game_path, rec)
            game_bindings.append({"file": game_name, "sha256": sha256(game_path)})
            md = min((d for d in depths if d), default=0)
            mt = max(times) if times else 0
            print(f"game {g+1} ({'White' if bot_white else 'Black'}): {res} "
                  f"[{len(uci)} plies, min depth {md}, max move {mt}s]", flush=True)
    print(f"MEASUREMENT {ELO} parallel ({GAMES} games):", tally)
    match = {"schema": "foldbot-stockfish-match/v1", "status": "completed",
             "completed_at_utc": datetime.now(timezone.utc).isoformat(),
             "registration_sha256": registration_sha, "games": game_bindings,
             "result": tally}
    write_new_json(output / "match.json", match)
