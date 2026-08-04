#!/usr/bin/env bash
# Dimension 6: repo growth with no GC. Loose objects accumulate in .vault/.git/objects forever
# — there is no git gc/repack anywhere in the codebase. This simulates years of daily edits to
# one file (compressed into seconds via examples/simulate_history, bypassing the daemon's
# debounce entirely) and tracks object count, disk usage, and real CLI latency as it grows.
set -euo pipefail
STRESS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$STRESS_DIR/lib.sh"

require_vault_bin
REPO_ROOT="$(cd "$STRESS_DIR/../.." && pwd)"

WORKDIR="$(mktemp -d)/vault"
mkdir -p "$(dirname "$WORKDIR")"
trap 'rm -rf "$(dirname "$WORKDIR")"' EXIT

new_stress_vault "$WORKDIR"

MILESTONES=(100 1000 5000 20000)
prev=0

echo "total_commits,object_count,git_dir_kb,show_latency_ms,log_latency_ms,status_latency_ms"

for target in "${MILESTONES[@]}"; do
    delta=$((target - prev))
    (cd "$REPO_ROOT" && cargo run --quiet --release --example simulate_history -- "$WORKDIR" "$delta" >/dev/null)
    prev=$target

    objs=$(git_object_count "$WORKDIR")
    size_kb=$(git_dir_bytes "$WORKDIR")
    # `date` truncates to whole seconds, which can sort *before* a commit's own sub-second
    # created_at taken within the same second (plain string comparison in resolve_at). A 1s
    # pad guarantees "now" is unambiguously after every commit just made.
    sleep 1
    now_iso=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

    start=$(now_ms)
    (cd "$WORKDIR" && "$VAULT_BIN" show history-000.md --at "$now_iso" >/dev/null)
    show_ms=$(( $(now_ms) - start ))

    start=$(now_ms)
    (cd "$WORKDIR" && "$VAULT_BIN" log history-000.md >/dev/null)
    log_ms=$(( $(now_ms) - start ))

    start=$(now_ms)
    (cd "$WORKDIR" && "$VAULT_BIN" status >/dev/null)
    status_ms=$(( $(now_ms) - start ))

    echo "$target,$objs,$size_kb,$show_ms,$log_ms,$status_ms"
done
