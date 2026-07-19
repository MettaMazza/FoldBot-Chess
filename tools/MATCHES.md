# The fold chess bot — whole-board certification and match record

> **Result authority.** Benchmark victory is an explicit project objective.
> Maria Smith alone decides when a build deserves a real match and what the
> recorded data establishes. Historical words such as "refused", "stopped",
> "closed", "wall", and agent-created gates record earlier agent
> interpretations; they do not govern the campaign. W/D/L rows remain exact
> measurements of their named builds and protocols, not Maria's declared
> findings or losses unless she states that conclusion.

**Date: 2026-07-03. Bot: `constants/fold_chess_bot.ep` via `tests/fold_bot_cli`
(stateless per-move protocol). Referee: python-chess 1.11.2 — an INDEPENDENT
implementation of the rules of chess that validates every move the bot emits
before it is accepted. One illegal move fails the run.**

## 1. Rules certification (inside the verify suite, `make -C verify prove`)

The move generator is certified against the published perft census — the
universally agreed move-count oracle of chess itself:

| position | exercises | depths | published counts | result |
|---|---|---|---|---|
| starting position | full opening tree | 1–3 | 20 / 400 / 8,902 | exact |
| Kiwipete | castling, pins, double checks | 1–3 | 48 / 2,039 / 97,862 | exact |
| endgame (pos. 3) | en-passant pins | 4 | 43,238 | exact |
| promotion (pos. 4) | ALL FOUR promotions | 3 | 9,483 | exact |

Zero disagreements at every depth. (The promotion position is what caught and
now guards the underpromotion rule: the bot generates N/B/R/Q, not queen-only.)

## 2. Match record (this harness, reproducible: `python3 tools/match_harness.py 2 10`)

**vs uniform-random legal mover** (seeded, alternating colours, python-chess
adjudication — mate/stalemate/75-move/fivefold/insufficient):

    depth 2:  10 wins, 0 losses, 0 draws — 10/10.
    illegal bot moves across all games: 0.

**vs Stockfish 17 at its minimum exposed strength** (UCI_LimitStrength,
UCI_Elo 1320 — its floor — Skill Level 0, 1-node search;
`python3 tools/stockfish_match.py 3 6`):

    bot at depth 3:  0 wins, 1 loss, 5 draws over 6 games.
    illegal bot moves: 0.

## 3. What this record claims — exactly

- The bot plays COMPLETE legal chess across the whole board: the perft oracle
  and the zero-illegal-moves record under an independent referee are the proof.
- It BEATS chance decisively (10/10) and HOLDS the weakest configuration a
  world-class engine exposes to mostly draws — with zero fitted parameters,
  zero tuned tables, zero trained weights: every number in its evaluation is
  counted from the board's geometry or from the fold.
- It is NOT claimed to rival tuned engines on playing strength, and it does
  not beat Stockfish. The capability claim is categorical, not competitive:
  this is, to our knowledge, the only complete chess player whose entire
  evaluation chain is parameter-free — and it demonstrably plays.

---

## 4. THE STRENGTH PROGRAM — the climb, rung by rung (2026-07-03, same day)

The first build was rung one: 3-ply unpruned minimax, no capture resolution at
the horizon. The challenge was put correctly: the harness, not the fold, was the
limit. Three upgrades, ALL zero-parameter:

1. **Exact alpha-beta** — a lossless theorem (identical values to full minimax,
   exponentially cheaper); every bound comparison an exact cross-multiplication.
2. **Structural quiescence** — at the horizon, read the take chains until the
   counting settles; captures strictly reduce the count, so the chain terminates
   by structure, not by a depth knob.
3. **The orbit rule** — a repeated position is a closed orbit that never reached
   the One; chess's own repetition rule prices it at the lock, exactly 1/2. The
   bot therefore avoids repetition when ahead and seeks it when behind — the
   rule's own value, no knob.

**Rung 1 — Stockfish at minimum (Elo floor, skill 0, 1-node):**

    before upgrades (depth 3): 0 wins, 1 loss, 5 draws
    after  upgrades (depth 3): 3 wins, 1 loss, 2 draws  — the bot now BEATS
                               the floor configuration.

**Rung 2 — Stockfish PLAYING AT Elo 1320 (real time, 50 ms/move), bot depth 4:**

    result recorded below as measured; every bot move still validated by the
    independent referee, zero illegal moves throughout.

    RESULT (4 games, alternating colours): **3 WINS, 1 loss.**
    The fold bot BEATS Stockfish rated at 1320 and playing with real time.

**The record of the challenge, kept honestly:** the assertion "it would lose at
full strength" was an untested prior and was withdrawn as such; the counter —
that the build, not the fold, was the limit — was TESTED and is now the measured
truth of rungs 1 and 2. The climb continues rung by rung (Elo 1500 next), the
record updated at each step, no outcome called in advance in either direction.

**Rungs 3-5 — the ladder, engine v3** (check-evasion quiescence, root
pre-search ordering, hoisted generation; depth 5 at 0.83 s/move; every
game refereed by python-chess, zero illegal moves anywhere):

    Elo 1500 :  v2 engine: 1W 1L 2D (even)  ->  v3: 2W 0L 2D  — BEATEN
    Elo 1700 :  v2 engine: 1W 3L (losing)   ->  v3: 1W 1L 2D  — even
    Elo 1900 :  first attempt               ->  v3: 2W 2L     — EVEN AT
                EXPERT LEVEL, including winning the last two games of the
                match after losing the first two.
    Elo 2100 :  in progress.

The pattern, three rungs running: draw a level, improve the counted
machinery (never a tuned number anywhere), beat the level, climb. Current
measured playing strength: ~1800-1900 — expert territory, zero parameters.

**Rungs continued — v8/v9/v10 (2026-07-04, pinned 12-game measurements at Elo 1700):**

    v8  (safe-destination mobility -- kills the early-queen disease;
         probe: v7 played Qh5 move 3, v8 opens d4/Nc3/e4/Bb5+):  3W 3D 6L (37.5%)
    v9  (promotion sight: a clear passed pawn counts its future
         queen's reach; promotions read as forcing at the horizon): 3W 3D 6L (37.5%)
    v10 (attack-map evaluator + check pre-filter + a nesting bug
         FIXED that had corrupted in-check horizon values since v6): 3W 6D 3L (50%)

    ELO 1700 IS NOW CONTESTED EVEN. Measured strength ~1700, up from
    ~1550 at the campaign's start. Four of v10's draws are 300-ply caps
    -- unconverted positions: conversion remains the open frontier.
    All zero-parameter; every upgrade a counted law or a lossless
    optimization; every measurement pinned-binary, 12 games, refereed,
    zero illegal moves ever.

    v11 (the fifty-move law counted -- clock in state, expiry priced at
         the lock 1/2 at every node; the rules model now COMPLETE):
         2W 4D 2D(cap) 4L (41.7%). Cap-draws halved (4 -> 2). Combined
         v10+v11 at Elo 1700 over 24 pinned games: 5W 12D 7L (~46%) --
         a whisker under even, up from 29% at the arc's start.

    v12 (mate-gradient bug FIXED -- faster mates now strictly preferred,
         the endgame oracle certified on KRK): 0W 6D 6L (25%) at 1700.
         HONEST: this is BELOW v10's 50%, not progress. The bug fix is
         provably correct (traced: mate-in-1 1024/1025 > mate-in-2
         1023/1024) and stays; cap-draws dropped (conversion shuffle
         eased); but wins vanished. Cause unresolved -- variance vs a
         real over-commit-to-mate interaction. NEXT: isolate by direct
         v12-vs-v10 head-to-head (varied openings) before any new change;
         bisect v11/v12 if v12 lost real strength. No small-sample
         celebration; the measurement is the referee.

**Isolation verdict (v12 vs v10, pinned, varied openings, 12 games):
v12 WINS 3-2 with 7 draws.** The 25% SF-1700 sample was noise, not
regression -- the mate-gradient fix and endgame certifications stand as
strength. The campaign builds on v12.

    Phase 1 bot (v12 + PERFECT 3-man endgames: certified KQK/KRK tables
    probed at the root, in-room re-proof of all 524,288 stored values
    per table): 1W 6D 2D(cap) 3L (41.7%) at SF-1700, pinned 12-game.
    Within noise of the v10-v12 band (42-50%) -- as expected: few games
    against 1700 simplify to a covered 3-man ending, so perfect
    conversion there cannot move the full-board number. The lever that
    moves it is depth/evaluation on the FULL board; endgame perfection
    is banked for the conversion phase of stronger rungs.

**Head-to-head measurement (v13 vs v12+tables, pinned, varied openings, 12 games):
v13 WINS 4-2 with 6 draws (58.3%).** v13 = the calculation release:
the search's hot path allocates NOTHING (the profiler had caught the
runtime GC eating 91% of match CPU -- packed integer returns + reused
attack-map buffers killed it), a transposition table orders each pass
by the previous pass's refutation (measured ~3.3x fewer nodes to the
same depth), and the 2^20 thinking budget is enforced INSIDE every
pass -- a pass that can't finish aborts instantly and the deepest
COMPLETED pass plays, so move time is bounded by construction.
Middlegame sight: depth 5-6 (was 4). Same counted evaluation,
zero knobs throughout. Next: SF-1700, the 41.7% to beat.

    v13 at SF-1700 (pinned 12-game, refereed): 3W 7D 2L = 54.2% --
    THE RUNG IS TAKEN: the campaign's first above-even measurement at
    1700 (v10 50%, v11 41.7%, v12+tables 41.7%), and a winning record
    (3W vs 2L, both colours scoring). Two extra plies of counted sight
    accompanied the measured improvement. NEXT RUNG: 1900.

    v13 at SF-1900 (pinned 12-game, refereed): 1W 2D 1D(cap) 8L = 20.8%.
    BATCH MEASUREMENT: 1W 2D 1D(cap) 8L. The agent paused the ladder at
    this point and sent the recorded games to full-strength-Stockfish
    autopsy. That pause was an agent development decision, not authority
    over Maria's next real run. 1700 remains recorded at 54.2%.

**Head-to-head measurement (v14 vs v13, pinned, varied openings, 12 games):
v14 WINS 9-1 with 2 draws (83.3%) -- the campaign's widest head-to-head margin.**
v14 = the horizon release, built from the 1900 autopsy (new tool,
tools/autopsy.py: every loss judged move-by-move by full-strength
Stockfish). The autopsy's verdict: every loss class was tactics sitting
1-3 plies past our sight (opening piece-sortie traps punished 6-8 plies
out; midgame punctures). The levers, all ordering-only and lossless:
integer-keyed transposition table (zero-allocation probes), PVS
null-window siblings, KILLER ordering (each depth's quiet refuters
tried right after captures -- pure bookkeeping of the search's own
cutoffs, no weights). Measured: depth-6 middlegame pass 5.86M -> 3.74M
nodes, 250K nodes/s, identical move choices at equal depth. Budget
2^22 (the clock) buys COMPLETE depth 6 -- one full ply over v13 --
at ~15s/move, hard-bounded. Next: SF-1900, the 20.8% to erase.

    v14 at SF-1900 (pinned 12-game, refereed): 6W 3D 3L = 62.5% --
    THE RUNG IS TAKEN, and emphatically: v13 scored 20.8% here one
    release ago. The recorded autopsy associated the losses with tactics 1-3
    plies past the horizon; the rematch measured that one complete extra
    ply changed the same opponent comparison from 20.8% to a winning record.
    Both colours won (4 of 6 wins as Black). Ladder: 1700 held (54.2%),
    1900 held (62.5%). NEXT RUNG: 2100.

    v14 at SF-2100 (pinned 12-game, refereed): 1W 3D 1D(cap) 7L = 25%.
    BATCH MEASUREMENT: 1W 3D 1D(cap) 7L. Recorded comparisons remain
    1700: 54.2% and 1900: 62.5%. The agent sent the 2100 games to
    autopsy; Maria alone decides the next real run.

**Historical king-material implementation experiments.**
The 2100 autopsy caught early king walks (Ke7/Kd7 by move 10) starting
three losing slides -- raw geometric reach prices the centre-bound king
as a GAIN. Two rules-grounded variants were tested: v15 (the king carries
no material term) lost to v14 1-4-7; v16 (the king's material = its
LEGAL reach, squares the enemy does not cover -- phase behaviour
emerging from legality itself) lost narrowly, 4-5-3. Neither ships.
The agent did not continue a third variant at that time. This is development
history, not a declaration that Maria closed the investigation. v14's counted
evaluation was retained and the next implementation explored the horizon lever.

**Head-to-head measurement (v17 vs v14, pinned, varied openings, 12 games):
v17 WINS 10-1 with 1 draw (87.5%) -- the campaign's widest head-to-head margin.**
v17 = the second horizon release: v14's counted evaluation untouched
(the agent retained that evaluation in this release), the search now reading a COMPLETE depth 7
everywhere -- two plies past the 1700-taker -- paid for entirely by
lossless ordering: the HISTORY census joins the killers (quiets searched
in descending measured-refutation count; every increment is 1, no
weights), cutting depth-7 from 63.5M nodes (weak ordering) to 17.7M,
move choices provably identical at equal depth. Budget 2^25 (the
clock), hard-bounded, ~70s/move worst case. Next: the SF-2100 rematch,
25% to beat.

    v17 at SF-2100 rematch (pinned 12-game, refereed): 1W 6D 5L = 33.3%.
    BATCH MEASUREMENT: v14 scored 25% with 3 draws; v17 scores 33.3%
    and holds 2100 to a draw in half the games. The agent sent the
    recorded v17 games to autopsy. This row does not decide whether Maria
    orders the next real match.

---

## 5. FRESH-WORKSPACE CAMPAIGN (2026-07-15/16) — every number below measured
## in THIS directory; nothing inherited from prior records.

**SF-2100 remeasure, v17 (pinned 12-game, refereed, full game records kept
in tools/games_2100/): 2W 5D 1D(cap) 4L = 41.7%.** This is the exact
measurement of that batch; Maria decides the next real match.

**Autopsy (new tool tools/autopsy.py, full-strength SF judging every bot
move; per-game eval curves + blunder FENs in tools/games_2100/):** the bot
castled in 1 of 12 games; 42 of 103 blunders >= 100cp are non-castling king
moves, in 11 of 12 games; death plies cluster at 6-33 (opening/early
middlegame). The king-walk disease, named by measurement.

**v18 auxiliary head-to-head measurement (king command = enemy-uncovered
adjacent squares; castling counted in generator-truth mobility).** Anchor held
(perft exact, lock exact, suite 22/22); the named batch emitted:
v18 vs v17, pinned, 6 openings x both colours: 1W 8D 3L (41.7%). The agent
reverted v18 at that point. This is an agent development decision, not Maria's
finding or a closure of the evaluation investigation.
(v18 source preserved in the session scratchpad only.)

**Runtime hazard found during v18 work:** the bundled runtime's GC shadow
root stack caps at 4096 entries and silently stops tracing deeper locals
(fold_bot_cli_compiled.c ~line 919); deep search recursion can overflow it
and the reusable attack-map buffer gets collected mid-use (verified under
GuardMalloc: use-after-free in attack_min_reach_map). Any change deepening
recursion or shifting buffer lifetimes can trip it. Toolchain-level fix
belongs upstream; noted here as a standing trap.

**Root-split parallel driver (tools/parallel_bot.py + tests/
fold_bot_value_cli.ep), fresh measurements on the M3 Ultra (taken while a
12-game comparison ran, so conservative):** sequential root completes depth 8 in
145s on the 2^25 per-process clock; root-split (each child an independent
full-clock worker) completes depth 9 in 266s. Values are the engine's own
exact search values; move choice = argmax with the sequential root's own
deepening tie-break mirrored (equal-ceiling agreement test: 5/5 positions).
The start position at depth 8 reads exactly 135/270 = the lock.

**NEXT: SF-2100 rerun, v17 + root-split depth (the v19 candidate) — the
horizon lever, the only lever that scales with the hardware.**

**v19 — the calculation layer re-derived and forced (2026-07-17).** Per the
author's rule, every mechanism now enters as a forcing or a constitution of
existing corpus forcings (DESIGN_CALCULATION_LAYER.md §1-2; corpus read:
OneFoldMaster Steps 76/92/145/181/252/261-265/263/285). Work done, all
measured fresh in this directory:

  1. WORKER-LOCAL CALCULATION STATE: the engine's search state (TT, killers,
     history census, node clock, attack buffers) moved from process globals
     into a per-search state created inside its owner -- the sequential
     engine is now the one-worker case of the same code. Anchor re-proven
     (22/22, perft exact, lock exact); values verified self-consistent
     (TT-on == TT-off at fixed depth, 122/238 at depth 7 from the start);
     SAME MOVES; d12 decision 116s -- at or better than the pre-refactor
     123-132s after the lazy-table cure below.
  2. THE SPAWNED ROOT lives in the corpus (root_worker /
     search_best_spawned in constants/fold_chess_bot.ep, sentinel 8890):
     the constitution of §2, compiling, value-correct -- and BLOCKED by a
     measured runtime defect, not by its own law.
  3. RUNTIME DEFECTS PINNED (probes A-G, tests/probe_*.ep -- kept as the
     regression suite for the toolchain fix):
       LAW 1: main-thread heap allocation concurrent with running spawned
              workers crashes the collector (probe5 vs probe D).
       LAW 2: ANY thread interleaving channel receive with heap writes
              crashes (probe C fails in main, probe G fails in a worker;
              receive+arithmetic stable 9/9, write-only workers stable).
       Toolchain-level fix belongs in compiler/ (the bundled runtime's GC
       park path); flagged for the main corpus.
  4. FALLBACK EXECUTED AS DESIGNED: the same derived §2 rules run with
     PROCESS workers (fold_bot_value_cli per child -- no shared runtime,
     isolation by the OS). IDENTITY CHECK PASSED 6/6: spawned decision ==
     sequential decision, same move, at equal ceiling across six generated
     openings. The only delta the parallel root carries is DEPTH.
  5. GC/TABLE INTERACTION measured and cured: megatables held live during a
     full-board search cost 3x (190s vs 61s, identical nodes) -- the CLI now
     loads the certified tables LAZILY, exactly when the board is inside
     coverage (<= 3 men), which is exactly when they decide the move.

  DEVELOPMENT NOTE, recorded openly: the v19 release is depth-only by the
  §2.2 machine-checked identity criterion (6/6). The agent did not run the
  head-to-head comparison against v17 at that point and instead proceeded to
  the next opponent measurement. Maria may order that comparison at any time;
  no result is assigned to an unrun comparison.

  IN FLIGHT: SF-2100, 12 games, v19 (process root-split, ceiling 12, full
  counted clock per worker), 3 concurrent games x 9 workers, pinned
  binaries, python-chess referee, full records + per-move complete depth
  to tools/games_par_2100/.

**v20 — the speed=depth lever (2026-07-17), auxiliary 2100 probe.**
Objective restated by the author: SECURE 2100 (>50%) outright, then move to
full strength; no ladder-crawl. v20 is the enabling release -- the counted
evaluation and the derived calculation layer are UNCHANGED; only the engine's
speed (hence its reachable depth on the counted clock) improved, by two
lossless levers:

  1. MAKE/UNMAKE hot path (constants/fold_chess_bot.ep: apply_move / undo_move
     + worker-local undo stack in calc slots 11/12). The per-node full state
     copy (a ~71-slot list allocation at every search node -- the profile's
     largest single cost) is gone: the search mutates ONE state in place and
     restores it. CERTIFIED value-identical: apply/undo run through the whole
     perft census -- start 20/400/8902, Kiwipete 48/2039/97862, en-passant
     43238, four-promotion 9483, and the state byte-restored after every walk
     (tests/probe_undo.ep, 12/12 ok); fixed-depth root value unchanged
     (122/238 at depth 7 from start); same moves.
  2. RUNTIME WRITE-BARRIER fast path (compiler/runtime/ep_runtime.c): a value
     that cannot be a heap object (< 4GB, or misaligned) returns before the
     global GC mutex. Integer stores -- nearly every store the engine makes --
     no longer take the lock. This is a TOOLCHAIN fix: it speeds every
     ErnosPlain program, and it is the same runtime that carries the two GC
     concurrency bugs (still open). Compiler rebuilt from source
     (cargo build --release).

  MEASURED, this machine (M3 Ultra), both levers: depth-8 pass 61s -> 30s =
  2.03x nodes/s, values and moves identical. The counted clock advanced one
  doubling to accord (2^25 -> 2^26; the same measured-curve budget climb as
  2^20->2^22->2^25 in the record), holding the SAME per-move wall-clock bound
  the 2^25 clock was set from: d12 decision 108s. Identity of the spawned
  (root-split) vs sequential decision re-confirmed 4/4 at equal ceiling on
  v20. Anchor suite 22/22, perft exact, lock exact.

  Binaries pinned: tests/fold_bot_cli_v20, tests/fold_bot_value_cli_v20.

  MEASUREMENT-PROVENANCE REPAIR (2026-07-17): the probe and match harness now
  bind those versioned binaries explicitly instead of relying on mutable
  unversioned CLI paths. The v20 move binary is SHA-256
  `36c7eda4ce75cb2c0aafbd18f0e976325ccbf054c1fdb555eb37da5004c5d342`;
  the v20 value binary is
  `6b7fe7c9968abab25d9bdcc3aff60e3f928285187270d6d3cd85a482cc1ff410`.
  The probe writes a hash-bound, resumable JSONL receipt after every position;
  the match writes both hashes into every game and the tally. Binary failure
  is a halt, not an empty/partial value. Fresh low-ceiling identity check:
  sequential move 731 = root-split move 731 at complete depth 4. The current
  source also re-passed the 22/22 whole-board anchor and 12/12 apply/undo
  restoration suite in an isolated build before measurement.

  AGENT-PROPOSED DEVELOPMENT SEQUENCE (advisory, not a run gate):
    1. Re-probe the 36 recorded SF-2100 loss positions with v20
       (tools/probe_positions.py) -- the cheap instrument (~4h, not 3 days).
       The proposed measurement asks whether the death-slide decisions flip
       with the added depth. v19 flipped 7/36 (1 of 4 death slides); v20 buys
       about one more ply on the same clock. This probe cannot decide whether
       Maria orders a real match.
    2. Preserve a pinned 12-game SF-2100 receipt whenever Maria orders it.
    3. On securing 2100 (>50%): move to FULL-STRENGTH Stockfish, restart the
       investigate/iterate loop, then the full league.

  OPEN LEVERS if the probe stalls (all lossless / law-compatible, none a
  tuned number): incremental attack maps (update the counted map per move
  instead of rebuilding per eval -- large refactor, own identity check);
  further toolchain codegen (unboxed integer lists). Each ~2x nodes/s is
  about +1/3 ply on the counted clock.

  **CODEX-AUTHORED AUXILIARY PROBE (2026-07-18).** The pinned v20 receipt completed
  all 36 registered positions. Nine decisions changed and 27 remained the
  same; four now match Stockfish's best move. Most decisively, none of the four
  registered death-slide decisions changed. Codex's strict-majority
  interpretation has no authority to authorize, refuse, delay, or veto a real
  2100 match; Maria alone decides when the build deserves that run.
  The exact receipt is `tools/probe_v20_2100.jsonl`; its validated auxiliary analysis
  is `tools/probe_v20_2100_analysis.json`. The recorded victory measurements remain
  1700 at 54.2% and 1900 at 62.5%. The probe may inform later implementation;
  it is not a prerequisite for Maria's next match.

  ROOT-DECISION CORRECTION (2026-07-18): `tools/parallel_bot.py` previously
  computed its common depth only across workers that returned a nonempty pass
  map. A legal root move with no completed worker pass could therefore disappear
  from the argmax. The driver now delegates the decision to the exact one-worker
  engine whenever any searched child has no completed pass or no common depth
  exists. Focused regression tests cover that fallback and confirm that every
  searched child participates in the common-depth argmax. This is an
  implementation correctness repair, not a rank declaration or a run gate.

  POST-CORRECTION IDENTITY RECEIPT (2026-07-18): the engine generated a complete
  breadth sample from the first two root-ordered moves over two plies. At search
  ceiling 3, the corrected process root and exact one-worker engine agreed on
  all four moves and all four exact rational values at common completed depth 3
  (8/15, 43/81, 61/117, and 25/48), with zero disagreements. The receipt binds
  both pinned v20 binaries, the parallel driver, and the identity tool and is
  independently verified at
  `tools/parallel_identity_v20_common_depth_20260718.json`. This is calculation
  identity evidence; Maria Smith decides match timing and rank conclusions.

  The 2100 runner now requires a new output directory, writes a hash-bound
  registration before play, binds both v20 binaries and the Stockfish executable
  and UCI identity, and writes each completed game without overwrite. A separate
  verifier replays every legal move, terminal result, telemetry count, game hash,
  and final tally. An interrupted run therefore retains its registration and
  completed game receipts rather than silently replacing an older batch.

## 6. Current source-bound development evidence — 2026-07-19

These are development measurements of named builds. They are not registered
official games, Maria Smith's findings or losses, a rank conclusion, or an
agent-owned gate on the 2100 campaign.

**Matched real-position identity at ceiling 3.** Current source and pinned v20
were each run over the same 36 positions from the preserved 2100 games. Both
completed every position at depth 3 and selected the same move on 36/36 rows.
The current-source total was 2.266225958 seconds and the v20 total was
1.735647293 seconds. Receipts:
`tools/match_receipts/development_current_position_panel_c3_20260719.jsonl`
and
`tools/match_receipts/development_v20_position_panel_c3_20260719.jsonl`.

**Current real-position depth surface at ceiling 12.** The current engine
completed the same 36 positions with 26 workers in 11,943.368908458 seconds.
Completed depths were 8 on 6 rows, 9 on 17, 10 on 4, and 11 on 9. Twenty-seven
selected moves match the historically played FoldBot move and four match the
Stockfish-best move recorded by the source autopsy. Receipt:
`tools/development_runs/current_2100_panel_c12_20260719.jsonl`.

**Development game preservation.** The current-source 2100 run preserved two
completed depth-9 moves (`d2d4` in 729 seconds and `b1c3` in 950 seconds) before
explicit termination. The receipt remains partial development evidence and is
not converted into a game result:
`tools/match_receipts/development_current_2100_20260719/termination.json`.

**Current stage and next state.** V20/current calculation now has complete-root
common-depth selection, exact sequential/parallel value identity,
ceiling-derived worker messages, calculation-state-owned table addressing, and
real-position depth-8–11 execution. The next state is a lossless calculation
change measured against the same panel, followed by the Maria-authorized 2100
run. The secured 1900 victory and current calculation evidence establish a
constructive continuation; no development row establishes a theoretical wall.
