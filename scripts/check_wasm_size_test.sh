#!/usr/bin/env bash
# check_wasm_size_test.sh — self-tests for check_wasm_size.sh.
#
# Exercises the budget-enforcement logic without invoking cargo by feeding
# fixture files through the WASM_FILE / BASE_WASM_FILE / BUDGET_FILE
# overrides. Covers the happy path, the inclusive boundary, every failure
# path, budget-file parsing edge cases, delta reporting in all three
# directions, and the GitHub step-summary output.
#
# Usage: bash scripts/check_wasm_size_test.sh
set -euo pipefail

SCRIPT="$(cd "$(dirname "$0")" && pwd)/check_wasm_size.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

pass=0
fail=0

# make_bytes <path> <n> — create a file of exactly n bytes.
make_bytes() {
    head -c "$2" /dev/zero >"$1"
}

# run_case <name> <expected-exit> <expected-output-substring> [env VAR=val ...]
run_case() {
    local name="$1" want_status="$2" want_substr="$3"
    shift 3
    local out status=0
    out="$(env "$@" bash "$SCRIPT" 2>&1)" || status=$?
    if [ "$status" -ne "$want_status" ]; then
        echo "FAIL $name: expected exit $want_status, got $status"
        echo "$out" | sed 's/^/    /'
        fail=$((fail + 1))
        return
    fi
    if ! printf '%s' "$out" | grep -qF -- "$want_substr"; then
        echo "FAIL $name: output missing '$want_substr'"
        echo "$out" | sed 's/^/    /'
        fail=$((fail + 1))
        return
    fi
    echo "ok   $name"
    pass=$((pass + 1))
}

make_bytes "$WORK/head.wasm" 1000
make_bytes "$WORK/base_smaller.wasm" 900
make_bytes "$WORK/base_bigger.wasm" 1100
make_bytes "$WORK/base_equal.wasm" 1000
make_bytes "$WORK/base_empty.wasm" 0

printf '2048' >"$WORK/budget_ok"
printf '1000' >"$WORK/budget_exact"
printf '999' >"$WORK/budget_tight"
printf '  2048\n' >"$WORK/budget_padded"
printf 'lots' >"$WORK/budget_words"
printf '' >"$WORK/budget_blank"
printf -- '-5' >"$WORK/budget_negative"

# --- budget enforcement ---------------------------------------------------
run_case "under budget passes" 0 "OK: within budget (1048 bytes of headroom)" \
    WASM_FILE="$WORK/head.wasm" BUDGET_FILE="$WORK/budget_ok"

run_case "exactly at budget passes (inclusive)" 0 "OK: within budget (0 bytes of headroom)" \
    WASM_FILE="$WORK/head.wasm" BUDGET_FILE="$WORK/budget_exact"

run_case "over budget fails with overage" 1 "exceeds the size budget by 1 bytes" \
    WASM_FILE="$WORK/head.wasm" BUDGET_FILE="$WORK/budget_tight"

run_case "over budget points at re-baselining docs" 1 "re-baselining procedure in CONTRIBUTING.md" \
    WASM_FILE="$WORK/head.wasm" BUDGET_FILE="$WORK/budget_tight"

# --- budget file parsing --------------------------------------------------
run_case "whitespace-padded budget is accepted" 0 "budget:   2048 bytes" \
    WASM_FILE="$WORK/head.wasm" BUDGET_FILE="$WORK/budget_padded"

run_case "missing budget file fails" 1 "not found" \
    WASM_FILE="$WORK/head.wasm" BUDGET_FILE="$WORK/no_such_budget"

run_case "non-numeric budget fails" 1 "must contain a single integer" \
    WASM_FILE="$WORK/head.wasm" BUDGET_FILE="$WORK/budget_words"

run_case "empty budget fails" 1 "must contain a single integer" \
    WASM_FILE="$WORK/head.wasm" BUDGET_FILE="$WORK/budget_blank"

run_case "negative budget fails" 1 "must contain a single integer" \
    WASM_FILE="$WORK/head.wasm" BUDGET_FILE="$WORK/budget_negative"

# --- artifact handling ----------------------------------------------------
run_case "missing artifact override fails" 1 "does not exist" \
    WASM_FILE="$WORK/no_such.wasm" BUDGET_FILE="$WORK/budget_ok"

# --- delta versus base ----------------------------------------------------
run_case "growth prints positive delta and percent" 0 "+100 bytes (+11.11% vs base, 900 bytes)" \
    WASM_FILE="$WORK/head.wasm" BUDGET_FILE="$WORK/budget_ok" \
    BASE_WASM_FILE="$WORK/base_smaller.wasm"

run_case "shrink prints negative delta" 0 "-100 bytes (-9.09% vs base, 1100 bytes)" \
    WASM_FILE="$WORK/head.wasm" BUDGET_FILE="$WORK/budget_ok" \
    BASE_WASM_FILE="$WORK/base_bigger.wasm"

run_case "no change prints zero delta" 0 "+0 bytes (+0.00% vs base, 1000 bytes)" \
    WASM_FILE="$WORK/head.wasm" BUDGET_FILE="$WORK/budget_ok" \
    BASE_WASM_FILE="$WORK/base_equal.wasm"

run_case "empty base avoids division by zero" 0 "+1000 bytes (base was empty)" \
    WASM_FILE="$WORK/head.wasm" BUDGET_FILE="$WORK/budget_ok" \
    BASE_WASM_FILE="$WORK/base_empty.wasm"

run_case "missing base artifact override fails" 1 "does not exist" \
    WASM_FILE="$WORK/head.wasm" BUDGET_FILE="$WORK/budget_ok" \
    BASE_WASM_FILE="$WORK/no_such_base.wasm"

run_case "delta still reported when over budget" 1 "+100 bytes (+11.11% vs base, 900 bytes)" \
    WASM_FILE="$WORK/head.wasm" BUDGET_FILE="$WORK/budget_tight" \
    BASE_WASM_FILE="$WORK/base_smaller.wasm"

# --- unreachable base ref is informational, not fatal ---------------------
run_case "unbuildable base ref warns but passes" 0 "skipping delta" \
    WASM_FILE="$WORK/head.wasm" BUDGET_FILE="$WORK/budget_ok" \
    BASE_REF="refs/definitely/not/a/ref"

# --- GitHub step summary --------------------------------------------------
summary="$WORK/summary.md"
: >"$summary"
run_case "step summary written when requested" 0 "OK: within budget" \
    WASM_FILE="$WORK/head.wasm" BUDGET_FILE="$WORK/budget_ok" \
    BASE_WASM_FILE="$WORK/base_smaller.wasm" GITHUB_STEP_SUMMARY="$summary"
for want in "### WASM size budget" "| Size | 1000 bytes |" "| Budget | 2048 bytes |" \
    "| Delta vs base | +100 bytes (+11.11% vs base, 900 bytes) |" "| Verdict | OK |"; do
    if grep -qF -- "$want" "$summary"; then
        echo "ok   step summary contains '$want'"
        pass=$((pass + 1))
    else
        echo "FAIL step summary missing '$want'"
        fail=$((fail + 1))
    fi
done

echo ""
echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
