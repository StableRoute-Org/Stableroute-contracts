#!/usr/bin/env bash
# check_wasm_size.sh — enforce the deployable WASM artifact size budget.
#
# Builds the `wasm32v1-none` release artifact, records its byte size, and
# fails (exit 1) when the size exceeds the budget stored in
# `.github/wasm-size-budget`. The budget file contains a single integer:
# the maximum allowed artifact size in bytes. The check is inclusive — an
# artifact exactly at the budget passes.
#
# When `BASE_REF` is set (CI sets it to the PR base branch), the script
# also builds the base branch artifact in a throwaway git worktree and
# prints the size delta. The delta is informational only: a broken or
# unbuildable base branch never fails this check — only the budget does.
#
# Environment overrides (used by the self-tests and for local runs):
#   BUDGET_FILE     path to the budget file   (default .github/wasm-size-budget)
#   WASM_FILE       pre-built artifact to measure — skips the cargo build
#   BASE_REF        git ref to diff against    (e.g. BASE_REF=main locally)
#   BASE_WASM_FILE  pre-built base artifact    — skips the base build
#
# Requires: bash, git, cargo with the `wasm32v1-none` target installed
# (not needed when WASM_FILE/BASE_WASM_FILE overrides are used).
set -euo pipefail

TARGET="wasm32v1-none"
ARTIFACT="stableroute_contracts.wasm"
BUDGET_FILE="${BUDGET_FILE:-.github/wasm-size-budget}"

die() {
    echo "error: $*" >&2
    exit 1
}

file_size() {
    wc -c <"$1" | tr -d '[:space:]'
}

# build_artifact <source-dir> <target-dir>
# Builds the release WASM and prints the artifact path on success.
build_artifact() {
    cargo build --manifest-path "$1/Cargo.toml" --target "$TARGET" --release \
        --target-dir "$2" >&2 || return 1
    local wasm="$2/$TARGET/release/$ARTIFACT"
    [ -f "$wasm" ] || return 1
    printf '%s\n' "$wasm"
}

# --- read and validate the budget -----------------------------------------
[ -f "$BUDGET_FILE" ] || die "budget file '$BUDGET_FILE' not found"
budget="$(tr -d '[:space:]' <"$BUDGET_FILE")"
case "$budget" in
'' | *[!0-9]*)
    die "budget file '$BUDGET_FILE' must contain a single integer byte count, got '$budget'"
    ;;
esac

# --- build and measure the head artifact ----------------------------------
if [ -n "${WASM_FILE:-}" ]; then
    [ -f "$WASM_FILE" ] || die "WASM_FILE '$WASM_FILE' does not exist"
    wasm="$WASM_FILE"
else
    wasm="$(build_artifact . target)" ||
        die "release build for $TARGET failed or produced no artifact"
fi
size="$(file_size "$wasm")"

echo "artifact: $wasm"
echo "size:     $size bytes"
echo "budget:   $budget bytes"

# --- optional: size delta versus the base branch --------------------------
delta_line=""
if [ -n "${BASE_WASM_FILE:-}" ] || [ -n "${BASE_REF:-}" ]; then
    base_size=""
    if [ -n "${BASE_WASM_FILE:-}" ]; then
        [ -f "$BASE_WASM_FILE" ] || die "BASE_WASM_FILE '$BASE_WASM_FILE' does not exist"
        base_size="$(file_size "$BASE_WASM_FILE")"
    else
        worktree="$(mktemp -d)"
        if git fetch --quiet --depth=1 origin "$BASE_REF" &&
            git worktree add --quiet --detach --force "$worktree" FETCH_HEAD; then
            if base_wasm="$(build_artifact "$worktree" "$worktree/target-base")"; then
                base_size="$(file_size "$base_wasm")"
            fi
            git worktree remove --force "$worktree" >/dev/null 2>&1 || true
        fi
    fi
    if [ -n "$base_size" ]; then
        delta=$((size - base_size))
        sign=""
        [ "$delta" -ge 0 ] && sign="+"
        if [ "$base_size" -gt 0 ]; then
            pct="$(awk -v d="$delta" -v b="$base_size" 'BEGIN { printf "%+.2f", d * 100.0 / b }')"
            delta_line="${sign}${delta} bytes (${pct}% vs base, ${base_size} bytes)"
        else
            delta_line="${sign}${delta} bytes (base was empty)"
        fi
        echo "delta:    $delta_line"
    else
        echo "warning: could not build base ref '${BASE_REF:-<unset>}'; skipping delta" >&2
    fi
fi

# --- verdict --------------------------------------------------------------
if [ "$size" -gt "$budget" ]; then
    verdict="FAIL"
    over=$((size - budget))
    echo "FAIL: artifact exceeds the size budget by $over bytes." >&2
    echo "If the growth is intentional, follow the re-baselining procedure in CONTRIBUTING.md." >&2
    status=1
else
    verdict="OK"
    echo "OK: within budget ($((budget - size)) bytes of headroom)"
    status=0
fi

# --- GitHub Actions job summary (no-op outside CI) ------------------------
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
    {
        echo "### WASM size budget"
        echo ""
        echo "| Metric | Value |"
        echo "| --- | --- |"
        echo "| Size | $size bytes |"
        echo "| Budget | $budget bytes |"
        [ -n "$delta_line" ] && echo "| Delta vs base | $delta_line |"
        echo "| Verdict | $verdict |"
    } >>"$GITHUB_STEP_SUMMARY"
fi

exit "$status"
