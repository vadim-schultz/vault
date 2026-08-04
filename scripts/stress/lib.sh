#!/usr/bin/env bash
# Shared helpers for scripts/stress/*.sh — real daemon + real filesystem stress runs.
# These are manual profiling tools, not CI gates (see .plans/benchmark.plan.md).

STRESS_LIB_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VAULT_BIN="${VAULT_BIN:-$STRESS_LIB_ROOT/target/release/vault}"
DEBOUNCE_WAIT="${DEBOUNCE_WAIT:-3}" # default debounce_ms is 2000; leave headroom

require_vault_bin() {
    if [[ ! -x "$VAULT_BIN" ]]; then
        echo "error: $VAULT_BIN not found; run: cargo build --release" >&2
        exit 1
    fi
}

# High-resolution wall clock in milliseconds (BSD `date` has no %N, so use perl).
now_ms() {
    perl -MTime::HiRes=time -e 'printf "%d\n", time()*1000'
}

# Create a fresh, disposable vault at $1, with its own registry/daemon-lock state dir so
# it never touches the real per-user global state. The state dir is a *sibling* of $1, not
# nested inside it — nesting it inside the watched worktree would make the daemon's own
# heartbeat/log writes show up as tracked vault content and skew snapshot counts.
new_stress_vault() {
    local dir="$1"
    mkdir -p "$dir"
    export VAULT_STATE_DIR="${dir}.state"
    mkdir -p "$VAULT_STATE_DIR"
    (cd "$dir" && "$VAULT_BIN" init --no-service >/dev/null)
}

# Start the daemon in the foreground as a background job; prints its PID. Log output goes to
# a *sibling* of $dir, not inside it — inside it, the daemon's own stdout/stderr file would be
# watched and tracked as vault content (same class of bug as the state-dir nesting above).
start_daemon() {
    local dir="$1"
    local log="${dir}.daemon.out"
    (cd "$dir" && exec "$VAULT_BIN" daemon --foreground >"$log" 2>&1) &
    echo $!
}

stop_daemon() {
    local pid="$1"
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
}

snapshot_count() {
    local dir="$1"
    sqlite3 "$dir/.vault/meta.db" "SELECT COUNT(*) FROM snapshots;" 2>/dev/null || echo 0
}

git_object_count() {
    local dir="$1"
    find "$dir/.vault/.git/objects" -type f 2>/dev/null | wc -l | tr -d ' '
}

git_dir_bytes() {
    local dir="$1"
    du -sk "$dir/.vault/.git" 2>/dev/null | cut -f1
}

# Poll snapshot_count until it reaches $2 (or timeout $3 seconds elapses).
# Prints elapsed milliseconds to stdout; prints "TIMEOUT" and returns 1 on timeout.
wait_for_snapshot_count() {
    local dir="$1" target="$2" timeout_s="${3:-60}"
    local start deadline_s
    start=$(now_ms)
    deadline_s=$(( $(date +%s) + timeout_s ))
    while (( $(date +%s) < deadline_s )); do
        if (( $(snapshot_count "$dir") >= target )); then
            echo $(( $(now_ms) - start ))
            return 0
        fi
        sleep 0.1
    done
    echo "TIMEOUT"
    return 1
}

# Poll snapshot_count until it stops changing for two consecutive 1s-spaced polls, up to
# timeout $2 seconds. Prints "<elapsed_ms> <final_count>".
wait_for_settle() {
    local dir="$1" timeout_s="${2:-120}"
    local start deadline_s last=-1 current stable_polls=0
    start=$(now_ms)
    deadline_s=$(( $(date +%s) + timeout_s ))
    while (( $(date +%s) < deadline_s )); do
        current=$(snapshot_count "$dir")
        if [[ "$current" == "$last" ]]; then
            stable_polls=$((stable_polls + 1))
            if (( stable_polls >= 2 )); then
                echo "$(( $(now_ms) - start )) $current"
                return 0
            fi
        else
            stable_polls=0
        fi
        last="$current"
        sleep 1
    done
    echo "TIMEOUT $(snapshot_count "$dir")"
    return 1
}
