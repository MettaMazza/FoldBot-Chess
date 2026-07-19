# FoldBot Chess

> **Part of the [Smithian Fold Theory of Everything](https://github.com/MettaMazza/Smithian-Fold-Theory-Of-Everything).** Sibling zero-parameter engines: [Fold Go](https://github.com/MettaMazza/Fold-Go) · [Fold Protein](https://github.com/MettaMazza/Fold-Protein).

## A full, legal chess engine with **zero parameters and zero training.**

Every chess engine ever built carries fitted numbers — the hand-tuned 1/3/3/5/9 piece
values, tuned positional tables, or millions of trained weights. **FoldBot carries none.**
A piece is worth the squares it commands from where it stands. A position's value is the
mover's exact share of the One. The counted colour, three, supplies the exact search anchor;
the current calculation layer iteratively deepens and plays the deepest completed common
pass under its source-bound calculation clock. Nothing is tuned; nothing is trained; every
evaluation is an exact rational.

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

## Current calculation stage

V20 preserves every legal root move to a common completed depth and has an independently verified 4/4 receipt for sequential/parallel move identity and 4/4 exact rational value identity, with zero disagreements. Worker messages are derived from the supplied search ceiling rather than fixed depth fields or sentinels, and transposition, history, and killer addressing now read the capacities of the live calculation state rather than repeating separate authored sizes.

The latest source-bound real-position evidence contains three matched surfaces:

- current and pinned v20 at ceiling 3 selected the same move and completed the same depth on all **36/36** recorded 2100 positions;
- the current calculation completed all **36** positions at ceiling 12, reaching depths **8–11**; **27/36** selections matched the historically played FoldBot move and **4/36** matched Stockfish's recorded best move;
- the current 2100 development game receipt preserves two completed depth-9 moves before explicit termination, rather than converting a partial run into a result.

These are applied development measurements of the named builds, not Maria Smith's official games, findings, losses, or rank conclusion. The next state is to use the real-position panel to derive the next lossless calculation improvement, validate it on the same source-bound positions, and execute the 2100 match when Maria designates the build. The secured 1900 victory, exact legality, endgame proofs, common-depth identity, and demonstrated depth-8–11 real-position calculation provide a constructive route forward; no theorem-derived wall is established.

The first new calculation candidate reused each already generated root child for worker handoff. It preserved 36/36 real-position moves and depths and 4/4 generated-position exact rational values. The matched panel measured 2.075576833 seconds before and 2.124612667 seconds with the candidate, so the live source and binaries were restored while the complete candidate executable, patch, and receipts were retained as development evidence.

The next typed exact-value transposition candidate also executed. It held
depth, exact numerator/denominator and exact/lower/upper relation and included
the half-move clock in value identity. It preserved **36/36** real-position
moves and depths and **4/4** exact rational values, but total panel time
regressed from `2.075576833` to `2.421576875` seconds (**16.67% slower**), with
only 11/36 positions faster. The candidate source, binaries and complete
receipts are preserved, while the active engine was restored exactly to the
ordering-only baseline. This could indicate that materialising four extra
262,144-row arrays costs more at depth 3 than the saved transpositions return.
The next calculation should measure a compact held-value representation or a
deeper matched panel where exact transpositions recur more often, without
changing value identity or the active baseline first.

That compact two-array representation has now been implemented and measured.
It again preserved **36/36** real-position moves and completed depths and
**4/4** generated-position moves and exact rational values. Packing the four
typed-value arrays into two improved the candidate total from `2.421576875` to
`2.370992332` seconds, but the active ordering-only baseline remained faster at
`2.075576833` seconds: the compact candidate was **14.23% slower**, with 14/36
positions faster. The candidate is preserved as applied evidence and the live
source and binaries are restored to the faster baseline. The result indicates
that packing reduces the table overhead, but at the measured depth the exact
value reuses still do not repay two additional full-capacity arrays. The next
calculation should therefore measure recurrence and reuse on a deeper
source-bound subset before allocating another complete value table.

A new full 2100 match is **not yet the recommended next execution** from this
specific candidate because it changed neither the selected move nor completed
depth and made the matched calculation slower. The recommended next execution
is the deeper source-bound reuse panel; a full match remains available whenever
Maria orders it, and this advisory recommendation does not gate her decision.

The recommended deeper panel has now completed and changes the implementation
decision. Across the same **36 real 2100 positions** at ceiling 6, the compact
exact-value table again preserved **36/36 moves and completed depths**, while
total calculation time improved from `34.142311874` to `30.986964794` seconds
(**9.24% faster**); the candidate was faster on **30/36** positions. Together
with the earlier **4/4 exact rational value identity**, this shows that exact
reuse does not repay its allocation at depth 3 but becomes net-positive by
depth 6, consistent with greater transposition recurrence as calculation
deepens. The compact source and rebuilt active binaries are now activated; no
evaluation quantity, search depth, selected move, or trained parameter changed.

A complete 2100 cumulative development match is now recommended under the
current receipt protocol. The active benchmark path historically reaches
depths 8–11, the candidate is already positive by depth 6, and all matched
moves, depths, and exact-value controls remained identical. It is not an
official run unless Maria explicitly registers it as one.

## The rule that governs this workspace

**Read [`AGENT.md`](AGENT.md) first.** Every derivation and expansion must route through the
engine and return to this validated state with **no law or constraint violation** — counted or
forced values only, exact arithmetic, everything traced to the One, the `forced_to_be` guards
left intact. A fitted number that plays well is a regression, because the whole point is that
this engine carries none.

## Papers & findings

- **[From the Self-Proven Theorem to Master-Level Chess — and the Law Inside Neural Networks](papers/From_The_Self_Proven_Theorem_To_Master_Chess_And_The_Law_Inside_Neural_Networks.md)**
- Full match record: [`tools/MATCHES.md`](tools/MATCHES.md)
- Release manifest: [`release/chess_release_manifest_v2.2.json`](release/chess_release_manifest_v2.2.json)

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
