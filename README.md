# FoldBot Chess

> **Part of the [Smithian Fold Theory of Everything](https://github.com/MettaMazza/Smithian-Fold-Theory-Of-Everything).** Sibling zero-parameter engines: [Fold Go](https://github.com/MettaMazza/Fold-Go) · [Fold Protein](https://github.com/MettaMazza/Fold-Protein).

## A full, legal chess engine with **zero parameters and zero training.**

Every chess engine ever built carries fitted numbers — the hand-tuned 1/3/3/5/9 piece
values, tuned positional tables, or millions of trained weights. **FoldBot carries none.**
A piece is worth the squares it commands from where it stands. A position's value is the
mover's exact share of the One. The search depth is the counted colour, three. Nothing is
tuned; nothing is trained; every evaluation is an exact rational.

### What it does — measured and replayable

- ♟️ **Secured the Stockfish Elo-1900 rung at 62.5%** in the pinned 12-game protocol
  (6W–3D–3L), after the preceding engine scored 20.8% at the same rung. Elo 1700 is also
  held at 54.2%. The 2100 rung remains the active competitive objective: the latest completed
  v17 remeasurement scored 41.7%, below the project's strictly-over-50% victory gate.
- 📜 Earlier anchors remain preserved: 10–0 against a uniform-random legal mover and five
  draws in six games against Stockfish's minimum exposed configuration, every bot move
  checked by the independent python-chess referee.
- ✅ **Rules certified against the published perft census, zero disagreements:**
  20 / 400 / 8,902 · Kiwipete 48 / 2,039 / 97,862 · en-passant pins 43,238 · four-way promotion 9,483.
- 🎯 **Finds mate, plays whole legal games** — castling, en passant, promotion, check, stalemate —
  and **solves KQK / KRK to mate.**
- 🔢 **Zero parameters, zero gradient steps.** Piece worth is *counted* (knight 2 in the corner /
  8 in the centre, rook 14, queen 21 / 27); the start position evaluates to **exactly 1/2** (the
  self-antipodal lock); mate approaches the One.

```sh
# see it for yourself
cd verify
cc -O2 -o test_fold_chess_bot test_fold_chess_bot.c && ./test_fold_chess_bot   # every line: ok
cc -O2 -o test_endgame_oracle test_endgame_oracle.c && ./test_endgame_oracle
cd ../tests && ernos fold_bot_cli.ep && printf '3\n8888\n' | ./fold_bot_cli     # the bot's opening move
```

## How it works

- **Material is counted, not given.** A piece's worth is the number of squares it commands
  from where it stands on an empty board — pure counting, no tuning.
- **Position value is the share of the One:** `units_mine / (units_mine + units_theirs)`, an
  exact fraction. Perfect balance is `1/2` (the lock); mate approaches `1`; being mated is the floor.
- **Search is exact negamax** at the counted depth (colour = 3 plies), values complemented
  through the antipode `mine = 1 − theirs` — the fold's own involution as the minimax step.

Competitive parity is defined at each registered rung as **strictly over 50%**. Securing 1900
does not claim that 2100 is already secured; the open 2100 work is a development objective,
not a theoretical wall on the engine or the theory.

## The rule that governs this workspace

**Read [`AGENT.md`](AGENT.md) first.** Every derivation and expansion must route through the
engine and return to this validated state with **no law or constraint violation** — counted or
forced values only, exact arithmetic, everything traced to the One, the `forced_to_be` guards
left intact. A fitted number that plays well is a regression, because the whole point is that
this engine carries none.

## Papers & findings

- **[From the Self-Proven Theorem to Master-Level Chess — and the Law Inside Neural Networks](papers/From_One_Axiom_To_Master_Chess_And_The_Law_Inside_Neural_Networks.md)**
- Full match record: [`tools/MATCHES.md`](tools/MATCHES.md)
- Release manifest: [`release/chess_release_manifest_v2.1.json`](release/chess_release_manifest_v2.1.json)

## Layout

| Path | What |
|------|------|
| `constants/fold_chess_bot.ep` | the engine (the corpus you expand) |
| `foundation/*.ep` | exact integers/fractions, the One and the fold, the enforcement guards |
| `verify/test_*.c` | self-contained C proofs (primary validation) |
| `tests/*.ep` | runnable CLI + source-level tests |
| `data/*_packed.txt` | certified KQK / KRK endgame tables |
| `tools/` | referee harness + `MATCHES.md` |
| `compiler/` | bundled ErnosPlain toolchain (`ernos` also on PATH) |

---

Part of the **[Smithian Fold Theory of Everything](https://github.com/MettaMazza/Smithian-Fold-Theory-Of-Everything)** — one machine-checked, self-proven theorem (*there is no nothing*), zero parameters, with the One and fold forced rather than assumed.
