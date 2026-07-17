"""Head-to-head gate: NEW engine vs OLD engine, pinned binaries, 12 games,
6 varied openings x both colours, python-chess referee on every move.
Usage: python3 tools/h2h_gate.py <new_binary> <old_binary> [outdir]
Result is reported from the NEW engine's side. The gate rule: the new
engine must WIN the match to ship."""
import subprocess, sys, os, json, shutil, tempfile, atexit
import chess
from concurrent.futures import ProcessPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
NEW_SRC = sys.argv[1]
OLD_SRC = sys.argv[2]
OUT = sys.argv[3] if len(sys.argv) > 3 else os.path.join(HERE, "gate_games")
os.makedirs(OUT, exist_ok=True)

def pin(src):
    t = tempfile.NamedTemporaryFile(delete=False, suffix="_fold_pin")
    t.close()
    shutil.copy2(src, t.name)
    os.chmod(t.name, 0o755)
    atexit.register(lambda p=t.name: os.unlink(p))
    return t.name

NEW = pin(NEW_SRC)
OLD = pin(OLD_SRC)

PROMO = {chess.QUEEN: 5, chess.ROOK: 4, chess.BISHOP: 3, chess.KNIGHT: 2}
CP = {v: k for k, v in PROMO.items()}
def enc(m): return (PROMO[m.promotion] if m.promotion else 0)*4096 + m.from_square*64 + m.to_square
def dec(v):
    p, r = v//4096, v % 4096
    return chess.Move(r//64, r % 64, promotion=CP[p] if p else None)

# 6 four-ply openings (encoded ints), each played with both colour assignments
OPENINGS = [
    [796, 3364, 405, 3690],   # 1.e4 e5 2.Nf3 Nc6
    [731, 3299, 666, 3372],   # 1.d4 d5 2.c4 e6
    [796, 3234, 405, 3307],   # 1.e4 c5 2.Nf3 d6
    [731, 4013, 666, 3372],   # 1.d4 Nf6 2.c4 e6
    [796, 3372, 731, 3299],   # 1.e4 e6 2.d4 d5
    [666, 3364, 82, 4013],    # 1.c4 e5 2.Nc3 Nf6
]

def bot_move(binary, depth, hist):
    feed = "\n".join([str(depth)]+[str(m) for m in hist]+["8888"])+"\n"
    return int(subprocess.run([binary], input=feed, capture_output=True, text=True,
                              timeout=1800).stdout.strip().splitlines()[-1])

def play_one(g):
    opening = OPENINGS[g % 6]
    new_white = g < 6
    board = chess.Board(); hist = []
    for v in opening:
        mv = dec(v)
        assert mv in board.legal_moves
        board.push(mv); hist.append(v)
    while not board.is_game_over(claim_draw=True) and len(hist) < 240:
        new_to_move = (board.turn == chess.WHITE) == new_white
        binary = NEW if new_to_move else OLD
        mv = dec(bot_move(binary, 12, hist))
        if mv not in board.legal_moves:
            return (g, "ILLEGAL-" + ("new" if new_to_move else "old"), hist, new_white)
        board.push(mv); hist.append(enc(mv))
    o = board.outcome(claim_draw=True)
    if o is None: res = "draw(cap)"
    elif o.winner is None: res = "draw"
    else: res = "win" if (o.winner == chess.WHITE) == new_white else "loss"
    return (g, res, hist, new_white)

if __name__ == "__main__":
    tally = {}
    with ProcessPoolExecutor(max_workers=12) as pool:
        for g, res, hist, new_white in pool.map(play_one, range(12)):
            tally[res] = tally.get(res, 0) + 1
            rec = {"game": g+1, "opening": g % 6, "new_white": new_white,
                   "result": res, "plies": len(hist), "moves_enc": hist}
            with open(os.path.join(OUT, f"gate_{g+1:02d}.json"), "w") as f:
                json.dump(rec, f)
            print(f"game {g+1} (opening {g%6}, new as {'White' if new_white else 'Black'}): {res} [{len(hist)} plies]", flush=True)
    print("GATE (new engine's score):", tally)
