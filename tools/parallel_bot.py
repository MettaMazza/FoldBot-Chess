"""Root-split parallel driver for the fold bot -- every value is the engine's
own exact search value; parallelism only changes WHERE they are computed.

Method: the engine lists the root moves in its own order (sentinel 8887).
Each root move's child position is searched by an independent worker process
(sentinel 8889, full node budget, ceiling-1), which reports every completed
pass "depth num den". The driver takes d* = the deepest pass ALL children
completed, complements each child's exact value through the antipode
(mine = 1 - theirs), and plays the argmax -- ties broken by the engine's own
root order (first wins), mirroring the sequential root's strict-greater rule.
A root move that repeats a game position is a closed orbit, priced exactly
1/2, no search (the sequential root's own rule).

Usage as library: parallel_move(hist, ceiling, workers) -> (move, d_star)
CLI: python3 tools/parallel_bot.py [ceiling] [workers]  (start position demo)
"""
import subprocess, sys, os
from concurrent.futures import ThreadPoolExecutor
from fractions import Fraction
import chess

HERE = os.path.dirname(os.path.abspath(__file__))
ENGINE_TAG = os.environ.get("FOLDBOT_ENGINE_TAG", "v20")
VALUE_CLI = os.environ.get(
    "FOLDBOT_VALUE_CLI",
    os.path.join(HERE, "..", "tests", f"fold_bot_value_cli_{ENGINE_TAG}"),
)
MOVE_CLI = os.environ.get(
    "FOLDBOT_MOVE_CLI",
    os.path.join(HERE, "..", "tests", f"fold_bot_cli_{ENGINE_TAG}"),
)

def require_binary(path, role):
    if not os.path.isfile(path) or not os.access(path, os.X_OK):
        raise RuntimeError(f"missing executable {role} binary: {path}")

def dec(v):
    p, r = v // 4096, v % 4096
    pr = {5: chess.QUEEN, 4: chess.ROOK, 3: chess.BISHOP, 2: chess.KNIGHT}
    return chess.Move(r // 64, r % 64, promotion=pr.get(p))

def run_cli(binary, ceiling, tokens):
    require_binary(binary, "FoldBot")
    feed = "\n".join([str(ceiling)] + [str(t) for t in tokens]) + "\n"
    out = subprocess.run([binary], input=feed, capture_output=True, text=True, timeout=3600)
    if out.returncode != 0:
        raise RuntimeError(
            f"{binary} halted with exit {out.returncode}: {out.stderr.strip()}"
        )
    return out.stdout.strip().splitlines()

def root_moves(hist, ceiling):
    lines = run_cli(VALUE_CLI, ceiling, list(hist) + [8887])
    n = int(lines[0])
    return [int(x) for x in lines[1:1 + n]]

def child_passes(hist, move, ceiling):
    lines = run_cli(VALUE_CLI, ceiling, list(hist) + [move, 8889])
    passes = {}
    for ln in lines:
        if ln == "done":
            break
        d, num, den = ln.split()
        passes[int(d)] = Fraction(int(num), int(den))
    return passes

def parallel_move(hist, ceiling=12, workers=None):
    workers = workers or max(4, (os.cpu_count() or 8) - 4)
    board = chess.Board()
    seen = {board._transposition_key()}
    for v in hist:
        board.push(dec(v))
        seen.add(board._transposition_key())
    moves = root_moves(hist, ceiling)
    if len(moves) == 1:
        return moves[0], 0
    orbit = {}
    to_search = []
    for m in moves:
        board.push(dec(m))
        if board._transposition_key() in seen:
            orbit[m] = Fraction(1, 2)   # closed orbit: the lock, exactly
        else:
            to_search.append(m)
        board.pop()
    with ThreadPoolExecutor(max_workers=workers) as pool:
        results = dict(zip(to_search, pool.map(
            lambda m: child_passes(hist, m, ceiling - 1), to_search)))
    d_star = min((max(p) for p in results.values() if p), default=0)

    def value_at(m, d):
        if m in orbit:
            return orbit[m]
        if results.get(m) and d in results[m]:
            return 1 - results[m][d]           # the antipode
        return None
    # Mirror the sequential root's deepening tie-break: each pass considers
    # the previous pass's winner FIRST, then the engine's own move order;
    # strictly-greater replaces. Exact values only; ordering only.
    pre = None
    d_min = min((min(p) for p in results.values() if p), default=1)
    best = None
    for d in range(d_min, d_star + 1):
        order = ([pre] if pre is not None else []) + [m for m in moves if m != pre]
        best, best_val = None, None
        for m in order:
            mine = value_at(m, d)
            if mine is None:
                continue
            if best_val is None or mine > best_val:
                best, best_val = m, mine
        pre = best
    return best, d_star + 1

if __name__ == "__main__":
    ceiling = int(sys.argv[1]) if len(sys.argv) > 1 else 12
    workers = int(sys.argv[2]) if len(sys.argv) > 2 else None
    import time
    t0 = time.time()
    mv, depth = parallel_move([], ceiling, workers)
    t1 = time.time()
    print(f"parallel: move {mv} ({dec(mv).uci()}), complete depth {depth}, {t1-t0:.1f}s")
