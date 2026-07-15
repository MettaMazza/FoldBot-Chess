#!/usr/bin/env bash
#
# run_epc_parity.sh — the self-hosting parity scoreboard.
#
# For every runnable test (tests/test_*.ep without '# expected_compile_error'):
#   1. compile it with the SELF-HOSTED compiler (./epc)
#   2. run the binary (exit 0 required)
#   3. if tests/<name>.expected exists, stdout must match exactly
#
# Rejection section: every '# expected_compile_error' test must FAIL to compile
# with ./epc once the self-hosted semantic passes land (until then they are
# reported, not counted, so the scoreboard stays honest about what epc enforces).
#
# Usage: tests/run_epc_parity.sh [--verbose]
# Exit code: 0 only if every runnable test passes AND every rejection test rejects.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$SCRIPT_DIR"

VERBOSE=0
[[ "${1:-}" == "--verbose" ]] && VERBOSE=1

EPC=./epc
if [[ ! -x "$EPC" ]]; then
    echo "error: $EPC not found — build it first: ./target/release/ernos epc.ep" >&2
    exit 2
fi

PASS=0; FAIL=0; FAILED=""
REJ_OK=0; REJ_MISS=0; REJ_MISSED=""

for TEST_FILE in tests/test_*.ep; do
    [[ -f "$TEST_FILE" ]] || continue
    NAME=$(basename "$TEST_FILE" .ep)
    BINARY="tests/$NAME"

    if head -5 "$TEST_FILE" | grep -q '# expected_compile_error'; then
        # ── Rejection section ──
        if "$EPC" "$TEST_FILE" >/dev/null 2>&1; then
            REJ_MISS=$((REJ_MISS+1)); REJ_MISSED="$REJ_MISSED $NAME"
            [[ $VERBOSE -eq 1 ]] && echo "REJECT-MISS  $NAME (epc accepted a program it must reject)"
        else
            REJ_OK=$((REJ_OK+1))
            [[ $VERBOSE -eq 1 ]] && echo "REJECT-OK    $NAME"
        fi
        continue
    fi

    # ── Runnable section ──
    if ! COMPILE_OUT=$("$EPC" "$TEST_FILE" 2>&1); then
        FAIL=$((FAIL+1)); FAILED="$FAILED $NAME(compile)"
        [[ $VERBOSE -eq 1 ]] && { echo "FAIL  $NAME (compile)"; echo "$COMPILE_OUT" | grep -iE "error" | head -2; }
        continue
    fi
    # epc currently writes the binary into CWD (basename); ernos writes next to
    # the source. Accept either so the scoreboard measures compilation, not the
    # output-path divergence (tracked separately in Phase 4).
    if [[ ! -x "$BINARY" && -x "./$NAME" ]]; then
        BINARY="./$NAME"
    fi
    if [[ ! -x "$BINARY" ]]; then
        FAIL=$((FAIL+1)); FAILED="$FAILED $NAME(no-binary)"
        [[ $VERBOSE -eq 1 ]] && echo "FAIL  $NAME (compiled but no binary found)"
        continue
    fi
    RUN_OUT=$("$BINARY" 2>/dev/null); RC=$?
    if [[ $RC -ne 0 ]]; then
        FAIL=$((FAIL+1)); FAILED="$FAILED $NAME(exit=$RC)"
        [[ $VERBOSE -eq 1 ]] && echo "FAIL  $NAME (exit $RC)"
        continue
    fi
    EXPECTED_FILE="tests/$NAME.expected"
    if [[ -f "$EXPECTED_FILE" ]] && ! diff -q <(printf '%s\n' "$RUN_OUT") "$EXPECTED_FILE" >/dev/null 2>&1; then
        FAIL=$((FAIL+1)); FAILED="$FAILED $NAME(output)"
        [[ $VERBOSE -eq 1 ]] && { echo "FAIL  $NAME (output mismatch)"; diff <(printf '%s\n' "$RUN_OUT") "$EXPECTED_FILE" | head -6; }
        continue
    fi
    PASS=$((PASS+1))
    [[ $VERBOSE -eq 1 ]] && echo "PASS  $NAME"
done

# Clean up CWD-dropped binaries and generated C from the epc runs.
for TEST_FILE in tests/test_*.ep; do
    NAME=$(basename "$TEST_FILE" .ep)
    [[ -f "./$NAME" && -x "./$NAME" ]] && rm -f "./$NAME"
    rm -f "./${NAME}_compiled.c"
done

TOTAL=$((PASS+FAIL))
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  epc parity: PASS $PASS/$TOTAL runnable"
[[ -n "$FAILED" ]] && echo "  failing:$FAILED" | tr ' ' '\n' | sed 's/^/    /' | sed '1s/    failing:/  failing:/'
echo "  rejection: $REJ_OK rejected, $REJ_MISS wrongly accepted"
[[ -n "$REJ_MISSED" ]] && echo "  wrongly accepted:$REJ_MISSED"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

[[ $FAIL -eq 0 && $REJ_MISS -eq 0 ]] && exit 0 || exit 1
