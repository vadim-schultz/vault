#!/usr/bin/env bash
# Dimension 4: edit burst size — N files touched inside one debounce window (bulk rename,
# branch switch, find-replace, unzip). One commit processes the whole batch sequentially:
# N blob writes + N tree edits before the commit lands. Measures wall-clock to settle and
# whether the daemon ever falls behind badly enough for the OS watch backlog to matter.
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

require_vault_bin

BURST_SIZES=(100 1000 5000 10000 15000 20000)

echo "burst_files,elapsed_ms,final_snapshot_count,tracked_files,expected_tracked_files"

for n in "${BURST_SIZES[@]}"; do
    WORKDIR="$(mktemp -d)/vault"
    DAEMON_PID=""
    cleanup() { stop_daemon "${DAEMON_PID:-}" 2>/dev/null; rm -rf "$(dirname "$WORKDIR")"; }
    trap cleanup EXIT

    new_stress_vault "$WORKDIR"
    DAEMON_PID=$(start_daemon "$WORKDIR")
    sleep 1

    for ((i = 0; i < n; i++)); do
        printf 'burst content %d' "$i" > "$WORKDIR/burst-$(printf '%06d' "$i").md"
    done

    read -r elapsed_ms final_count < <(wait_for_settle "$WORKDIR" 180)
    # Confirm the settle reading is real, not a lucky gap between debounce batches.
    sleep 15
    final_count=$(snapshot_count "$WORKDIR")
    tracked=$( (cd "$WORKDIR" && "$VAULT_BIN" list 2>/dev/null | wc -l) | tr -d ' ')
    echo "$n,$elapsed_ms,$final_count,$tracked,$n"
    if (( tracked < n )); then
        echo "  ! ${n} touched but only ${tracked} tracked — $((n - tracked)) files never landed a snapshot" >&2
    fi

    cleanup
    trap - EXIT
done
