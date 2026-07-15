#!/usr/bin/env bash
#
# run_tests.sh — Compile and run each test, checking:
#   1. Compilation succeeds (or fails if # expected_compile_error)
#   2. Execution succeeds (exit 0)
#   3. Output matches companion .expected file (if it exists)
#
# To add expected output for a test: create tests/test_foo.expected
# with the exact stdout the test should produce.
#
# Usage: ./run_tests.sh [--verbose]
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

VERBOSE=0
[[ "${1:-}" == "--verbose" ]] && VERBOSE=1

PASS=0
FAIL=0
SKIP=0
FAILURES=""

for TEST_FILE in tests/test_*.ep conformance/test_*.ep; do
    [[ -f "$TEST_FILE" ]] || continue
    NAME=$(basename "$TEST_FILE" .ep)
    BINARY="$(dirname "$TEST_FILE")/$NAME"

    # ── Check for expected compile error ──
    EXPECT_COMPILE_ERROR=0
    if head -5 "$TEST_FILE" | grep -q '# expected_compile_error'; then
        EXPECT_COMPILE_ERROR=1
    fi

    # ── Compile ──
    if ! COMPILE_OUT=$(cargo run -- "$TEST_FILE" 2>&1); then
        if [[ $EXPECT_COMPILE_ERROR -eq 1 ]]; then
            echo "PASS  $NAME  (expected compile error)"
            PASS=$((PASS + 1))
        else
            echo "FAIL  $NAME  (compilation failed)"
            [[ $VERBOSE -eq 1 ]] && echo "$COMPILE_OUT" | sed 's/^/      /'
            FAIL=$((FAIL + 1))
            FAILURES="$FAILURES  $NAME (compile)\n"
        fi
        continue
    fi

    if [[ $EXPECT_COMPILE_ERROR -eq 1 ]]; then
        echo "FAIL  $NAME  (expected compile error but succeeded)"
        FAIL=$((FAIL + 1))
        FAILURES="$FAILURES  $NAME (should have failed)\n"
        rm -f "$BINARY" "$(dirname "$TEST_FILE")/${NAME}_compiled.c"
        continue
    fi

    if [[ ! -x "$BINARY" ]]; then
        echo "FAIL  $NAME  (binary not produced)"
        FAIL=$((FAIL + 1))
        FAILURES="$FAILURES  $NAME (no binary)\n"
        continue
    fi

    # ── Run ──
    EXIT_CODE=0
    ACTUAL=$("$BINARY" 2>/dev/null) || EXIT_CODE=$?

    if [[ $EXIT_CODE -ne 0 ]]; then
        echo "FAIL  $NAME  (exit code $EXIT_CODE)"
        [[ $VERBOSE -eq 1 ]] && echo "      output: $ACTUAL"
        FAIL=$((FAIL + 1))
        FAILURES="$FAILURES  $NAME (exit $EXIT_CODE)\n"
        rm -f "$BINARY" "$(dirname "$TEST_FILE")/${NAME}_compiled.c"
        continue
    fi

    # ── Check expected output ──
    # Single source of truth: companion .expected file
    EXPECTED_FILE="${TEST_FILE%.ep}.expected"
    if [[ -f "$EXPECTED_FILE" ]]; then
        EXPECTED_FROM_FILE=$(cat "$EXPECTED_FILE")
        if [[ "$ACTUAL" == "$EXPECTED_FROM_FILE" ]]; then
            echo "PASS  $NAME"
            PASS=$((PASS + 1))
        else
            echo "FAIL  $NAME  (output mismatch)"
            diff <(echo "$EXPECTED_FROM_FILE") <(echo "$ACTUAL") | head -10 | sed 's/^/      /'
            FAIL=$((FAIL + 1))
            FAILURES="$FAILURES  $NAME (output)\n"
        fi
    else
        # No .expected file — pass on exit code alone
        echo "PASS  $NAME  (no .expected file, exit 0)"
        PASS=$((PASS + 1))
    fi

    # Clean up compiled artifacts
    rm -f "$BINARY" "$(dirname "$TEST_FILE")/${NAME}_compiled.c"
done

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  Results: $PASS passed, $FAIL failed"
if [[ $FAIL -gt 0 ]]; then
    echo ""
    echo "  Failures:"
    echo -e "$FAILURES"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    exit 1
else
    echo "  All tests passed ✓"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    exit 0
fi
