# FoldBot Chess

> **Part of the [Smithian Fold Theory of Everything](https://github.com/MettaMazza/Smithian-Fold-Theory-Of-Everything).** Sibling zero-parameter engines: [Fold Go](https://github.com/MettaMazza/Fold-Go) · [Fold Protein](https://github.com/MettaMazza/Fold-Protein).

A complete, legal chess engine whose every number is **counted from the board's own
geometry** — piece worth = squares commanded, position value = the mover's exact share
of the One, search depth = the counted colour (3). **Zero parameters. Zero training.
Every evaluation an exact rational.**

Duplicated from the Smithian Fold Theory project as an independent workspace at the one
validated state (chess as of commit `7a58d1e`).

## Quick start

```sh
# 1. Prove it (C compiler only) — every line must read "ok"
cd verify
cc -O2 -o test_fold_chess_bot test_fold_chess_bot.c && ./test_fold_chess_bot
cc -O2 -o test_endgame_oracle test_endgame_oracle.c && ./test_endgame_oracle

# 2. Build the playable engine from source (ernos is on PATH)
cd ../tests
ernos fold_bot_cli.ep

# 3. Play the bot (stateless per call): line 1 = depth, then move ints, then 8888
printf '3\n8888\n' | ./fold_bot_cli      # -> the bot's opening move (encoded)
#   move int = promo*4096 + from*64 + to   (squares 0-63, a1=0)

# 4. Refereed match (needs python3 + python-chess; stockfish optional)
python3 ../tools/stockfish_match.py
```

## The rule that governs this workspace

**Read [`AGENT.md`](AGENT.md) first.** Every derivation and expansion of the corpus must
route through the engine and return to the one validated anchor with **no law or constraint
violation** — counted or forced values only, exact arithmetic, everything traced to the One,
the `forced_to_be` / `forbid_form_selection` guards left intact. A fitted number that plays
well is a regression. Validation is done in this directory (see `AGENT.md` §3).

## Layout

| Path | What |
|------|------|
| `constants/fold_chess_bot.ep` | the engine (the corpus you expand) |
| `foundation/*.ep` | exact integers/fractions, the One and the fold, the enforcement guards |
| `verify/test_*.c` | self-contained C proofs (primary validation) |
| `tests/*.ep` | runnable CLI + source-level tests |
| `data/*_packed.txt` | certified KQK / KRK endgame tables |
| `tools/` | referee harness + `MATCHES.md` (the match findings) |
| `papers/` | the write-up of the findings |
| `compiler/` | bundled ErnosPlain toolchain (`ernos` also on PATH) |

## Validated record (reproduced by the commands above)

- Perft census, zero disagreements: 20/400/8,902 · Kiwipete 48/2,039/97,862 · en-passant 43,238 · promotions 9,483
- Counted material: N 2/8, R 14, Q 21/27 · start = exactly `1/2` (the lock) · mate → the One
- KQK/KRK mates certified · 10–0 vs random mover (zero illegal) · 5 draws in 6 vs Stockfish's floor

## Papers & findings

- [From One Axiom to Master-Level Chess — and the Law Inside Neural Networks](papers/From_One_Axiom_To_Master_Chess_And_The_Law_Inside_Neural_Networks.md)
- Full match record: [tools/MATCHES.md](tools/MATCHES.md)

---

Part of the **[Smithian Fold Theory of Everything](https://github.com/MettaMazza/Smithian-Fold-Theory-Of-Everything)** — one axiom, zero parameters, everything forced from the One.
