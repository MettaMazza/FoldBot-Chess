"""Fold bot vs Stockfish (python-chess referee on every fold move).
Stockfish at its minimum strength: UCI_LimitStrength + UCI_Elo floor (1320),
moving on a 1-node search budget -- the weakest configuration it exposes."""
import subprocess, sys
import chess, chess.engine

BOT = "/Users/mettamazza/Desktop/FoldBot Chess/tests/fold_bot_cli"
SF = "/opt/homebrew/bin/stockfish"
PROMO_CODE = {chess.QUEEN: 5, chess.ROOK: 4, chess.BISHOP: 3, chess.KNIGHT: 2}
CODE_PROMO = {v: k for k, v in PROMO_CODE.items()}

def encode(m): return (PROMO_CODE[m.promotion] if m.promotion else 0)*4096 + m.from_square*64 + m.to_square
def decode(v):
    p, r = v // 4096, v % 4096
    return chess.Move(r // 64, r % 64, promotion=CODE_PROMO[p] if p else None)

def bot_move(depth, history):
    feed = "\n".join([str(depth)] + [str(m) for m in history] + ["8888"]) + "\n"
    out = subprocess.run([BOT], input=feed, capture_output=True, text=True, timeout=600)
    return int(out.stdout.strip().splitlines()[-1])

def play(depth, bot_white, engine, max_plies=300):
    board = chess.Board(); history = []
    while not board.is_game_over(claim_draw=True) and len(history) < max_plies:
        if (board.turn == chess.WHITE) == bot_white:
            v = bot_move(depth, history); mv = decode(v)
            if mv not in board.legal_moves:
                return "ILLEGAL"
            board.push(mv); history.append(v)
        else:
            r = engine.play(board, chess.engine.Limit(nodes=1))
            board.push(r.move); history.append(encode(r.move))
    o = board.outcome(claim_draw=True)
    if o is None: return "draw(cap)"
    if o.winner is None: return "draw"
    return "win" if (o.winner == chess.WHITE) == bot_white else "loss"

depth = int(sys.argv[1]); games = int(sys.argv[2])
eng = chess.engine.SimpleEngine.popen_uci(SF)
eng.configure({"UCI_LimitStrength": True, "UCI_Elo": 1320, "Skill Level": 0})
tally = {}
for g in range(games):
    r = play(depth, g % 2 == 0, eng)
    tally[r] = tally.get(r, 0) + 1
    print(f"game {g+1} (bot as {'White' if g%2==0 else 'Black'}): {r}", flush=True)
eng.quit()
print("TALLY:", tally)
