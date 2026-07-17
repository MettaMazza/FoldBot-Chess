# DESIGN — the calculation layer, re-derived and forced (the v19 program)

**Date: 2026-07-17. Status: design for rebuild, per the author's ruling.**

## 0. Ruling and scope

Per the author (2026-07-17): calculation machinery that entered by hand-assembly
rather than derivation is not admissible, and results produced with it are void.
Struck from the record accordingly:

- the v18 build and its gate (v18 carried an unforced form choice — a king
  square counted in two terms without a forcing over the candidate assemblies);
- all play by the Python root-split driver (its rules were stated but not
  derived at the time it played).

The v17 anchor state and its §3 validations stand untouched — they are the
anchor. The pinned-v17 SF-2100 remeasure and its autopsy used the anchor engine
under the standard harness; they are recorded in MATCHES.md §5 and the author
rules on their standing.

**Objective:** rebuild the calculation layer so that every mechanism is
re-derived and forced from the corpus and implemented inside the guarded `.ep`
engine, then retake the ladder from fresh measurements.

## 1. The rule, and the derivation obligations

**The author's rule (2026-07-17, binding):** every mechanism must be either
(a) forced from the model, (b) a forward forcing derived from the model to
lock in the required computational science, or (c) a constitution of
already-existing forcings. Established computer science enters only re-derived
inside the SFT model and its constraints.

The main corpus was read for the existing forcings
(`/Users/mettamazza/Desktop/Smithian Fold Theory`: OneFoldMaster.md,
STANDARDS.md, constants/). Every mechanism of the calculation layer lands in
category (a) or (c) — each row cites the corpus forcing it is constituted
from, with the machine check that must pass in this directory:

| mechanism | corpus forcing it composes from | cat. | machine check |
|---|---|---|---|
| minimax complement (mine = 1 − theirs) | the fold involution / antipodes (`the_one_and_the_fold.ep`; OneFoldMaster Step 286.3: every pair j/q + (q−j)/q sums to the One) — already the chess corpus's law | (a) | anchor suite (existing) |
| exact alpha-beta | Step 263 *Order from complexity*: 2^k states, longest descent exactly k — the tree collapses to the path; bound comparisons are exact cross-multiplications; certified by the Step 285 pattern (an oracle it did not write) | (c) | value-identity to full minimax at fixed depth (existing verify) |
| bounded pass / halting | Step 92 *Computability and halting*: a bounded configuration at depth k halts in exactly k folds — bounded depth means halting-guaranteed | (c) | budget abort + deepest-completed-pass rule (existing) |
| iterative deepening | Step 92 + Step 263: each doubling of complexity adds exactly ONE step of order — the deepening ladder is the fold's own depth ladder | (c) | pass-per-depth, deepest completed plays (existing) |
| TT / killers / history | Step 145 *Memory persistence*: memory is a held orbit, kept by re-exciting; the tables are held orbits of the search's own refutations, every increment 1 (counted census) | (c) | move-identity at equal depth (recorded method) |
| repetition priced at 1/2 | the chess corpus's own orbit rule: a closed orbit never reaches the One — the lock exactly | (a) | anchor suite (existing) |
| a worker = one counted search | Step 181 *Attention capacity*: one focus fully holds one lock; a worker is one unit-capacity selection, whole — never a split focus | (c) | one search state per worker (compile-enforced: borrows cannot cross `spawn`) |
| parallel composition of children | Step 76 *Entanglement*: the joint of independent components is the PRODUCT; Steps 261/265: the fold commutes with composition — disjoint subderivations compose order-free, so concurrent computation changes no value | (c) | **identity: spawned == sequential, move and value, at equal ceiling, on a generated sample** |
| worker isolation | Step 252 *The channel cycle — no smuggled signal*: no third channel exists to bias; workers exchange values only over the counted channel, only at pass completion | (c) | no shared mutable state (compile-enforced isolation + GuardMalloc pass) |
| the decision depth d\* | the sealed-measurement law (STANDARDS: values at different depths are different measurements; measurements never mix) + Step 92 (a depth-d pass halts at exactly d): the root argmax exists only at the deepest depth ALL children completed | (c) — forced, no choice exists | d\* computed, never chosen; audit in game records |
| root argmax + tie rule | the chess corpus's existing root law (generated order; previous pass's winner first; strictly-greater replaces) | (a) | inherited code path |

**Category (b) is currently empty.** No forward forcing is needed for this
layer — it assembles entirely as constitutions of existing forcings. If
implementation exposes a genuine residual choice anywhere, that is precisely
where a forward forcing must be derived and put under the guards
(`forced_to_be` / `forbid_form_selection` over a *generated* candidate space)
— or the change halts. Hand-selection is not an option that exists.

## 2. The parallel root — the constitution, spelled out

1. **The root law (a):** the move played is the argmax over the generated root
   moves of the complement of the child's exact value; deepening passes
   consider the previous pass's winner first; strictly-greater replaces.
   Inherited unchanged.
2. **Concurrency admissibility (c — Steps 76/261/265):** root-child subtrees
   are disjoint derivations. The corpus's composition law — the joint of
   independent components is their product, and the fold commutes with
   composition — makes evaluation order a non-fact: concurrent computation
   cannot alter any value. Binding criterion, machine-checked: **no value may
   differ from the sequential engine's.**
3. **Isolation (c — Step 252):** no smuggled signal. Workers share no mutable
   state and speak only over the counted channel, only completed passes. The
   language enforces it: borrowed references cannot cross `spawn`.
4. **The decision depth (c — forced):** exact values at different depths are
   different measurements and measurements never mix (the sealed-measurement
   law). The root argmax is defined only at depths every child has completed;
   the deepest such depth d\* is therefore the unique depth at which the
   decision exists. No choice is available.
5. **The clock (a):** the budget is defined per counted search; each worker is
   one counted search (Step 181: one whole lock per focus) and carries the
   clock. Inherited definition.
6. **Orbits (a):** a root child repeating a game position is a closed orbit,
   priced exactly at the lock 1/2 — the existing rule, applied at the same
   place the sequential root applies it.

## 3. Implementation inside the guard system

- ErnosPlain provides native `spawn` and channels
  (`compiler/LANGUAGE_REFERENCE.md`, concurrency section;
  `compiler/concurrency_test.ep`). The parallel root is built in the `.ep`
  corpus — no orchestration outside the guards.
- **Binding constraint discovered:** the engine's search state is currently
  process-global (`TT_*`, `KILLER_*`, `HIST`, `NODES_LEFT`, `PASS_ABORTED`,
  the reach buffers). Spawned workers sharing that state would race and
  corrupt values. The rebuild makes all search state **worker-local**: created
  inside each worker (borrowed references cannot cross `spawn` — the language
  enforces the isolation that the derivation's independence step requires).
- Worker protocol: the main thread generates the root moves in the engine's
  own order; one worker per child receives (child state, clock), runs the
  counted deepening, and sends every completed pass `(index, depth, num, den)`
  over its channel; the main thread computes d\* and the forced argmax.
- **The runtime trap stands** (MATCHES.md §5): the GC's shadow root stack is
  per-thread (4096 entries each) and silently stops tracing beyond the cap;
  the buffer-collection hazard was verified under GuardMalloc. The threaded
  layout is re-tested under GuardMalloc before any match play.

## 4. Validation — in this directory, in this order

1. **Anchor:** §3 A/B of AGENT.md — every line ok, perft census exact,
   lock exact, oracles certified.
2. **Identity (the forced criterion of §2.2):** at equal ceiling, the spawned
   engine and the sequential engine produce the same move and the same exact
   value on a *generated* position sample (openings × depths — generated,
   not hand-picked). Any mismatch is a stop-the-line event.
3. **Throughput:** measured fresh on this machine. No inherited numbers.
4. **Gate:** v19 vs v17, pinned binaries, 12 games, standard protocol.
5. **Rungs:** SF-2100; then the ladder ground-up (1320 → 2100) on the shipped
   engine; then the main event — the full-strength loop — per the campaign
   plan.

## 5. Sync with the main corpus

Main project located at `/Users/mettamazza/Desktop/Smithian Fold Theory`
(chess tools: `autopsy.py`, `h2h_gate.py`, `match_harness.py`; ledgers:
`MATCHES.md`, `GO_MATCHES.md`). After the rebuild passes §4 here: diff the
chess artifacts both ways, reconcile the ledgers, and sync under the author's
direction.
