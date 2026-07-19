# DESIGN — the calculation layer, re-derived and forced (the v19 program)

**Date: 2026-07-18. Status: design for rebuild, per the author's ruling.**

## 0. Provenance and scope

The author required calculation machinery to be forced, forward-forced, or
constitutionally re-derived. Earlier agents incorrectly removed named
measurements from the evidence record. That agent conclusion is removed.
Preserve the exact v18 and Python-root-split
measurements with their producing implementations and provenance; the engine
alone determines whether a mechanism satisfies the forcing constitution, and
Maria determines the published conclusion.

The v17 anchor state and its §3 validations stand untouched — they are the
anchor. The pinned-v17 SF-2100 remeasure and its autopsy used the anchor engine
under the standard harness; they are recorded in MATCHES.md §5 and the author
rules on their standing.

**Objective:** rebuild the calculation layer so that every mechanism is
re-derived and forced from the corpus and implemented inside the guarded `.ep`
engine, then continue the benchmark-victory campaign through Maria-authorized
real matches.

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
   decision exists. No choice is available. If any searched child completes no
   pass, or no common root depth exists, the parallel driver must return the
   exact one-worker engine decision; omitting that legal child from the argmax
   is forbidden.
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
- The Python driver now enforces the all-child condition directly and has a
  regression test for both the empty-worker fallback and common-depth argmax.
- `tools/verify_parallel_identity.py` generates its sample systematically from
  the engine's own root order, then requires both the selected move and exact
  rational value to equal the one-worker engine at the common completed depth.
  It seals the source and binary hashes with every position row.
  `tools/parallel_identity_v20_common_depth_20260718.json` records four
  systematically generated two-ply positions at ceiling 3: move identity 4/4,
  exact rational value identity 4/4, and zero disagreements. Its independent
  receipt verifier passes with SHA-256
  `dcd993b55499646ac0d18a9ccd78922aa8568c1617ad7cce49caf8eed6fcc2a6`.
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
4. **Comparison:** v19 versus v17, pinned binaries, 12 games, when Maria orders
   that measurement.
5. **Real matches:** Maria decides whether the next run is SF-2100, a ground-up
   ladder comparison, or full-strength Stockfish. Every run preserves its named
   protocol and receipt.

## 5. Sync with the main corpus

Main project located at `/Users/mettamazza/Desktop/Smithian Fold Theory`
(chess tools: `autopsy.py`, `h2h_gate.py`, `match_harness.py`; ledgers:
`MATCHES.md`, `GO_MATCHES.md`). After the rebuild passes §4 here: diff the
chess artifacts both ways, reconcile the ledgers, and sync under the author's
direction.

## 6. Measured finite-reach cache variant (2026-07-18)

The complete 2 × 6 × 64 empty-board reach census was tested as a worker-local
lookup table. It was value-identical at all 768 inputs and preserved the
starting-position choices through depth 3, but measured slower in the named
throughput comparison.
Over the same 76.8 million reads and identical checksum 746,400,000, direct
`counted_reach` used about 0.12 seconds of CPU and the list-backed cache about
0.48 seconds. Runtime list access costs more than reconstructing this small
integer fact, so the cache was removed. These figures select against that
implementation only; they do not alter the counted reach law or rank evidence.

## 7. Lossless heavy-capture classification (2026-07-18)

The capture buckets ask only whether the victim's empty-board reach is greater
than eight. The finite piece definitions close most cases without rebuilding a
reach: rooks and queens always pass; pawns, knights, and kings never pass; only
bishops depend on their square. The hot ordering sites now express that exact
classification inline and call `counted_reach` only for bishops.

`tests/heavy_capture_identity.ep` enumerates both colours, all six kinds, and
all 64 squares: 0/768 disagreements. A 76.8-million-classification microbenchmark
gave the same heavy count 32,800,000 while reducing measured CPU from about
0.13 seconds to 0.03 seconds on this machine. This is a local hot-operation
measurement, not yet a whole-search throughput claim and not a rank result.

## 8. Exact knight-jump generation (2026-07-18)

Five hot geometry sites previously scanned every pair in the 5 × 5 offset box
and filtered that 25-pair census down to the knight relation: both offsets
nonzero and `|dr| + |df| = 3`. The engine now generates that same relation
directly: the four nonzero rank offsets each force `|df| = 3 - |dr|`, with its
two signs. This visits exactly eight candidate jumps and introduces no table,
value, weight, or selected constant.

`tests/knight_geometry_identity.ep` exhausts both colours, all 64 origins, and
all 64 targets. It independently reconstructs the original relation and checks
`square_attacked`, the minimum-reach attack map, lesser-attacker detection,
`counted_reach`, and live mobility: zero disagreements. The complete source
anchor remains 22/22 and the depth-7 starting result remains move 731 at
122/238. A single before/after depth-7 timing was neutral within noise (3.53 s
versus 3.52 s user CPU), so this is retained as an exact finite-enumeration
simplification, not claimed as a whole-search acceleration or rank advance.

## 9. Ceiling-derived worker message form (2026-07-19)

The spawned-root channel no longer reserves a fixed 64-depth matrix, a fixed
eight-bit depth field, a `255` completion sentinel, or a fixed `2^40` index
stride. Each worker reuses the existing exact fraction-pack square as its depth
base; its completion depth is `ceiling + 1`; the index stride is the complete
derived depth field `(ceiling + 2) * depth_base`; and the collector allocates
exactly the supplied ceiling's rows. The message contains the same
`(child index, completed depth, exact packed value)` relation without a
separately selected width or sentinel. Source tests bind the derived bases.
