#!/usr/bin/env bash
# Dimension 7: concurrent readers vs. writer. meta.db (WAL, busy_timeout=5000) and the bare git
# repo are read by CLI commands while the daemon is mid-commit. An existing test proves this
# works at trivial scale (read_during_write_does_not_busy_error); this pushes concurrency much
# higher — a sustained writer plus many parallel readers — to find latency percentiles and
# confirm no SQLITE_BUSY / lock errors ever surface to a user.
set -euo pipefail
STRESS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$STRESS_DIR/lib.sh"

require_vault_bin
REPO_ROOT="$(cd "$STRESS_DIR/../.." && pwd)"

READERS="${READERS:-8}"
CALLS_PER_READER="${CALLS_PER_READER:-25}"
WRITE_DURATION_S="${WRITE_DURATION_S:-20}"

WORKDIR="$(mktemp -d)/vault"
mkdir -p "$(dirname "$WORKDIR")"
LOGDIR="$(mktemp -d)"
DAEMON_PID=""
cleanup() { stop_daemon "${DAEMON_PID:-}" 2>/dev/null; rm -rf "$(dirname "$WORKDIR")" "$LOGDIR"; }
trap cleanup EXIT

new_stress_vault "$WORKDIR"

# Seed some real history so reads have non-trivial work to do, not an empty vault.
(cd "$REPO_ROOT" && cargo run --quiet --release --example simulate_history -- "$WORKDIR" 2000 5 >/dev/null)

DAEMON_PID=$(start_daemon "$WORKDIR")
sleep 1

# Sustained writer: keep editing a rotating set of files for the whole test window.
writer() {
    local deadline_s=$(( $(date +%s) + WRITE_DURATION_S ))
    local i=0
    while (( $(date +%s) < deadline_s )); do
        printf -v fname 'history-%03d.md' "$((i % 5))"
        printf 'writer edit %d' "$i" > "$WORKDIR/$fname"
        i=$((i + 1))
        sleep 0.3
    done
}
writer &
WRITER_PID=$!

# One reader worker: CALLS_PER_READER CLI invocations, rotating show/log/list/status,
# each call's wall-clock latency (ms) appended to its own log file.
reader_worker() {
    local id="$1" out="$LOGDIR/reader-$1.log" errout="$LOGDIR/reader-$1.err"
    local cmds=(show log list status)
    for ((c = 0; c < CALLS_PER_READER; c++)); do
        local cmd="${cmds[$((c % 4))]}"
        local start end
        start=$(now_ms)
        case "$cmd" in
            show) (cd "$WORKDIR" && "$VAULT_BIN" show history-000.md --at "$(date -u +%Y-%m-%dT%H:%M:%SZ)") ;;
            log) (cd "$WORKDIR" && "$VAULT_BIN" log history-000.md) ;;
            list) (cd "$WORKDIR" && "$VAULT_BIN" list) ;;
            status) (cd "$WORKDIR" && "$VAULT_BIN" status) ;;
        esac >/dev/null 2>>"$errout" || true
        end=$(now_ms)
        echo "$((end - start))" >> "$out"
    done
}

for ((r = 0; r < READERS; r++)); do
    reader_worker "$r" &
done

wait
wait "$WRITER_PID" 2>/dev/null || true

all_latencies="$LOGDIR/all.txt"
cat "$LOGDIR"/reader-*.log > "$all_latencies"
total=$(wc -l < "$all_latencies" | tr -d ' ')
sorted="$LOGDIR/sorted.txt"
sort -n "$all_latencies" > "$sorted"

percentile() {
    local p="$1"
    local idx=$(( (total * p / 100) ))
    (( idx >= total )) && idx=$((total - 1))
    sed -n "$((idx + 1))p" "$sorted"
}

echo "total_calls,p50_ms,p95_ms,p99_ms,max_ms"
echo "$total,$(percentile 50),$(percentile 95),$(percentile 99),$(tail -1 "$sorted")"

echo ""
echo "error scan (SQLITE_BUSY / locked / busy) across daemon log + reader stderr:"
if grep -riE "busy|locked" "${WORKDIR}.daemon.out" "$LOGDIR"/reader-*.err 2>/dev/null; then
    echo "  ^ found — see above"
else
    echo "  none found"
fi
