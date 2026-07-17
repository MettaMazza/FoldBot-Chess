# AGENT.md — the law this workspace is held to

You are working on **FoldBot Chess**: a complete, legal chess engine whose every
number is **counted from the board's own geometry**, never tuned, never trained.
It carries **zero parameters**. This file is binding. Read it before you change
anything, and route every change back through the validation in **this directory**.

---

## 0. The one validated anchor (do not regress it)

There is exactly one validated state, and it is the baseline every derivation and
expansion must return to. In this folder it is **proven, reproducibly, by the
commands in §3**. As validated, the engine holds:

- **Rules substrate — certified against the published perft census, zero disagreements:**
  start 20 / 400 / 8,902; Kiwipete 48 / 2,039 / 97,862; en-passant-pin endgame (d4) 43,238;
  four-way promotion (d3) 9,483.
- **Material is counted, not given:** knight commands 2 from a corner, 8 from the centre;
  rook 14 from anywhere; queen 21 from a corner, 27 from the centre. Every "piece value"
  is a count the engine performs.
- **Position value is the mover's exact share of the One:** `units_mine / (units_mine + units_theirs)`,
  an exact rational in `(0,1)`. The start is **exactly the half-One `1/2`** (the self-antipodal
  lock at perfect balance). Mate approaches the One; being mated sits at the floor.
- **Search:** exact negamax at the counted depth (colour = **3** plies), values complemented
  through the antipode `mine = 1 − theirs`.
- **Endgame oracle:** KQK / KRK mates certified (finds the mate, mate-valued, mate on the board,
  drives the mate net).
- **Refereed play (python-chess validates every emitted move):** 10–0 vs a random legal mover
  on both colours, zero illegal moves; held Stockfish at its minimum exposed strength (Elo floor,
  skill 0, 1-node) to 5 draws in 6 games (1 loss), zero illegal moves. Full record in
  [`tools/MATCHES.md`](tools/MATCHES.md).

If any change makes any of the above stop reproducing, the change is wrong until proven
otherwise. **The anchor is the arbiter, not your intent.**

---

## 1. The zero-parameter law (inherited from the Smithian Fold Theory)

Every derivation in the corpus obeys these constraints. They are the same standard the
main theory is held to (`OneFoldMaster.md` / `STANDARDS.md` in the parent project). A
violation is not a style issue — it makes a "forced" number a fitted one, and it must halt.

1. **Zero parameters.** No hand-tuned constant, no fitted table, no trained weight ever
   enters. Every quantity is **counted** (from geometry / the rules) or **forced** (assembled
   from already-derived quantities). If you cannot say *what counts it*, it does not belong.
2. **Exact arithmetic only.** No decimal ever enters a derivation. Everything is an exact
   whole number or exact fraction (`foundation/exact_integers.ep`, `foundation/exact_fractions.ep`).
   A decimal is a human read-out at the very end and is never fed back.
3. **Every value traces back to the One.** The only assumed thing is the One; everything
   else is built by the two permitted moves (fold and take) and the two counted generators
   **`b = 2`, `c = 3`**. Nothing is smuggled in — no forward references, no borrowed numbers.
4. **The form is forced, not just its parts.** An assembled value must be minimal and unique
   over a *generated* candidate space, not a hand-picked list — the `forced_to_be` /
   `forbid_form_selection` guards (`foundation/assembly_enumeration.ep`,
   `foundation/form_enforcement.ep`). If a simpler or coincident-size assembly exists, the
   engine **halts**.
5. **Measured values are sealed.** A measured/target number is a distinct type
   (`foundation/measured_values.ep`) with **no** conversion into the derivation side; handing
   one to a forcing is a compile error, and `forbid_target_input` halts at runtime. A target
   may only be read at the final comparison boundary to take a yes/no difference.

**The guards are the point, not an obstacle.** If the engine halts, a constraint fired.
You fix the derivation so it is genuinely forced — you never edit, weaken, or route around
`forced_to_be`, `forbid_form_selection`, `forbid_target_input`, or the perft/oracle checks.

---

## 2. The routing rule (this is the instruction)

**Every derivation and every expansion of the corpus must route through the engine and
return to the one validated anchor with no law or constraint violation.** Concretely, for
any change — a new piece rule, a deeper search, a new endgame, a new evaluation term:

1. Express the new quantity as **counted or forced** in the `.ep` source
   (`constants/fold_chess_bot.ep`, using only `foundation/`). No literal you cannot justify
   by a count or a forcing.
2. **Rebuild through the engine** (`ernos`, §3) — if it halts, a guard rejected an unforced
   value. That is the engine doing its job; fix the derivation, do not bypass the guard.
3. **Re-run the full validation in this directory** (§3). The perft census, the counted-material
   checks, the mate/oracle proofs, and the referee's zero-illegal-move record must all still hold.
4. Only a change that **passes every check and preserves the anchor** is admissible. Anything
   else is reverted.

There is no "temporarily fit it and clean up later." A fitted number that plays well is a
regression, because the entire claim of this engine is that it carries none.

### Repository and release boundary

Completed Chess work is committed locally in this repository with only the intended Chess paths staged. Never sweep unrelated probes, binaries, games, or live author changes into a commit. A local commit does not authorize publication or a push. Push only when Maria explicitly requests it. If she requests **sync and push**, first re-prove this file's engine standards, synchronize any required derivation and the established Chess result into the main SFT corpus, master, registry, certificates, evidence links, and related papers, then commit and push the Chess and main-theory repositories separately to their own remotes.

---

## 3. How to validate — in THIS directory

All commands run from this folder (`/Users/mettamazza/Desktop/FoldBot Chess`).

**A. The self-contained proofs (need only a C compiler):**
```sh
cd verify
cc -O2 -o test_fold_chess_bot   test_fold_chess_bot.c   && ./test_fold_chess_bot
cc -O2 -o test_endgame_oracle   test_endgame_oracle.c   && ./test_endgame_oracle
```
Every line must read `ok` and end in `=== done ===`. A single `FAIL` is a stop-the-line event.

**B. Rebuild the runnable engine from source (needs `ernos`, already on PATH at `~/.local/bin`):**
```sh
cd tests
ernos fold_bot_cli.ep          # -> ./fold_bot_cli (native binary)
ernos test_fold_chess_bot.ep   # -> source-level test binary; run it, expect all ok
```
`ernos` compiles the `.ep` through the ErnosPlain toolchain (bundled in `compiler/`). If it
halts on a forced-value guard, that is the constraint firing — see §1/§2.

**C. Refereed play (needs `python3` + `python-chess`; `stockfish` optional):**
```sh
python3 tools/stockfish_match.py     # every fold move validated by python-chess
```
The bot must emit **zero illegal moves**. Paths in `tools/*.py` and `tests/fold_bot_cli.ep`
point at this folder; keep them pointing here.

---

## 4. Where things live

- `constants/fold_chess_bot.ep` — the engine (the corpus you expand). Imports only `foundation/`.
- `foundation/*.ep` — exact integers/fractions, counted numbers, the One and the fold, and the
  enforcement guards. The law lives here; treat it as fixed substrate.
- `verify/test_*.c` — the self-contained C proofs (the primary validation).
- `tests/*.ep` — the runnable CLI and source-level tests.
- `data/kqk_packed.txt`, `data/krk_packed.txt` — the certified endgame tables the CLI loads.
- `tools/` — the referee harness and the match record (the findings).
- `papers/` — the write-up of the findings.
- `compiler/` — the bundled ErnosPlain toolchain (`ernos` is also installed on PATH).

The finding this workspace exists to protect: **a full, legal, competitive chess player whose
every number is counted and whose every evaluation is an exact rational — zero parameters, zero
gradient steps.** Keep it that way.
