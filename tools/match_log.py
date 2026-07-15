"""Pinned 12-game match vs SF at a given Elo, with FULL game recording.
Usage: python3 tools/match_log.py <elo> [outdir]
Same protocol as measure_sf.py (pinned binary, alternating colours, depth-12
iterative budget, SF at 50ms real time, python-chess referee, 240-ply cap)
plus: every game saved as JSON (UCI movetext, encoded ints, result, colours)
into <outdir> (default tools/games_<elo>/) for the autopsy."""
import subprocess, sys, os, json, shutil, tempfile, atexit
import chess, chess.engine
from concurrent.futures import ProcessPoolExecutor

ELO = int(sys.argv[1]) if len(sys.argv) > 1 else 2100
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = sys.argv[2] if len(sys.argv) > 2 else os.path.join(HERE, f"games_{ELO}")
os.makedirs(OUT, exist_ok=True)

_pin = tempfile.NamedTemporaryFile(delete=False, suffix="_fold_bot")
_pin.close()
shutil.copy2(os.path.join(HERE, "..", "tests", "fold_bot_cli"), _pin.name)
os.chmod(_pin.name, 0o755)
atexit.register(lambda: os.unlink(_pin.name))
BOT = _pin.name
SF = "/opt/homebrew/bin/stockfish"
PROMO = {chess.QUEEN: 5, chess.ROOK: 4, chess.BISHOP: 3, chess.KNIGHT: 2}
CP = {v: k for k, v in PROMO.items()}

def enc(m): return (PROMO[m.promotion] if m.promotion else 0)*4096 + m.from_square*64 + m.to_square
def dec(v):
    p, r = v//4096, v % 4096
    return chess.Move(r//64, r % 64, promotion=CP[p] if p else None)

def bot_move(depth, hist):
    feed = "\n".join([str(depth)]+[str(m) for m in hist]+["8888"])+"\n"
    return int(subprocess.run([BOT], input=feed, capture_output=True, text=True,
                              timeout=900).stdout.strip().splitlines()[-1])

def play_one(g):
    bot_white = g % 2 == 0
    eng = chess.engine.SimpleEngine.popen_uci(SF)
    eng.configure({"UCI_LimitStrength": True, "UCI_Elo": ELO})
    board = chess.Board(); hist = []; uci = []
    while not board.is_game_over(claim_draw=True) and len(hist) < 240:
        if (board.turn == chess.WHITE) == bot_white:
            mv = dec(bot_move(12, hist))
            if mv not in board.legal_moves:
                eng.quit(); return (g, "ILLEGAL", uci, bot_white)
            board.push(mv); hist.append(enc(mv)); uci.append(mv.uci())
        else:
            r = eng.play(board, chess.engine.Limit(time=0.05))
            board.push(r.move); hist.append(enc(r.move)); uci.append(r.move.uci())
    eng.quit()
    o = board.outcome(claim_draw=True)
    if o is None: res = "draw(cap)"
    elif o.winner is None: res = "draw"
    else: res = "win" if (o.winner == chess.WHITE) == bot_white else "loss"
    return (g, res, uci, bot_white)

if __name__ == "__main__":
    tally = {}
    with ProcessPoolExecutor(max_workers=12) as pool:
        for g, res, uci, bot_white in pool.map(play_one, range(12)):
            tally[res] = tally.get(res, 0) + 1
            rec = {"game": g+1, "elo": ELO, "bot_white": bot_white,
                   "result": res, "plies": len(uci), "moves_uci": uci}
            with open(os.path.join(OUT, f"game_{g+1:02d}.json"), "w") as f:
                json.dump(rec, f, indent=1)
            print(f"game {g+1} ({'White' if bot_white else 'Black'}): {res} [{len(uci)} plies]", flush=True)
    print(f"MEASUREMENT {ELO} (12 games):", tally)
    with open(os.path.join(OUT, "tally.json"), "w") as f:
        json.dump(tally, f)
