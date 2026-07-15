"""Autopsy: every recorded game judged move-by-move by FULL-STRENGTH Stockfish.
Usage: python3 tools/autopsy.py <games_dir> [analysis_time_per_move=0.3]
For each game JSON written by match_log.py, full-strength SF evaluates the
position before and after every BOT move. Output per game:
  - eval curve (cp, bot's perspective, after each ply)
  - every bot blunder: ply, move, eval before -> after, cp loss, SF's best move
  - death ply: the last ply the bot's eval was >= -100cp before a permanent
    slide (never recovers above -100cp) -- where the game went bad for good
  - phase at death (opening <= ply 20, middlegame, endgame <= 10 pieces)
Written to <games_dir>/autopsy_game_NN.json plus a printed summary.
Judgment only -- nothing here feeds the engine; measured values stay sealed."""
import sys, os, json, glob
import chess, chess.engine

DIR = sys.argv[1]
TPM = float(sys.argv[2]) if len(sys.argv) > 2 else 0.3
SF = "/opt/homebrew/bin/stockfish"
MATE_CP = 10000

def cp(score, pov_white):
    s = score.pov(chess.WHITE if pov_white else chess.BLACK)
    return s.score(mate_score=MATE_CP)

def phase(board, ply):
    if ply <= 20: return "opening"
    if len(board.piece_map()) <= 10: return "endgame"
    return "middlegame"

def autopsy(path, eng):
    rec = json.load(open(path))
    bot_white = rec["bot_white"]
    board = chess.Board()
    curve = []          # (ply, cp after that ply, bot's perspective)
    blunders = []
    for ply, u in enumerate(rec["moves_uci"], 1):
        mv = chess.Move.from_uci(u)
        bot_move = (board.turn == chess.WHITE) == bot_white
        if bot_move:
            info = eng.analyse(board, chess.engine.Limit(time=TPM))
            before = cp(info["score"], bot_white)
            best = info.get("pv", [None])[0]
        board.push(mv)
        info = eng.analyse(board, chess.engine.Limit(time=TPM))
        after = cp(info["score"], bot_white)
        curve.append({"ply": ply, "cp": after,
                      "mover": "bot" if bot_move else "sf", "move": u})
        if bot_move and before - after >= 100:
            board.pop()
            blunders.append({"ply": ply, "move": u, "before": before,
                             "after": after, "loss": before - after,
                             "sf_best": best.uci() if best else None,
                             "phase": phase(board, ply),
                             "fen": board.fen()})
            board.push(mv)
    # death ply: last ply with bot cp >= -100 that is never reached again
    death = None
    for pt in curve:
        if pt["cp"] >= -100: death = None
        elif death is None: death = pt["ply"]
    out = {"game": rec["game"], "result": rec["result"],
           "bot_white": bot_white, "death_ply": death,
           "death_phase": None, "blunders": blunders, "curve": curve}
    if death:
        b2 = chess.Board()
        for u in rec["moves_uci"][:death]: b2.push(chess.Move.from_uci(u))
        out["death_phase"] = phase(b2, death)
    dst = os.path.join(DIR, f"autopsy_game_{rec['game']:02d}.json")
    json.dump(out, open(dst, "w"), indent=1)
    return out

if __name__ == "__main__":
    eng = chess.engine.SimpleEngine.popen_uci(SF)  # FULL strength: no limits
    games = sorted(glob.glob(os.path.join(DIR, "game_*.json")))
    for path in games:
        a = autopsy(path, eng)
        bl = a["blunders"]
        print(f"game {a['game']:2d} [{a['result']:9s}] "
              f"death ply {a['death_ply']} ({a['death_phase']}), "
              f"{len(bl)} bot blunders >= 100cp: "
              + "; ".join(f"ply {b['ply']} {b['move']} ({b['loss']}cp, {b['phase']}, best {b['sf_best']})"
                          for b in bl[:6]), flush=True)
    eng.quit()
