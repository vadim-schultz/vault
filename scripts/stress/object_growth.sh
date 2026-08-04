#!/usr/bin/env bash
# Dimension 6: repo growth — loose objects accumulate until housekeeping repacks.
# Uses examples/simulate_history for commits and examples/run_housekeeping to trigger repack
# at each milestone without a live daemon.
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

echo "total_commits,object_count,git_dir_kb,show_latency_ms,log_latency_ms,status_latency_ms,loose_after_hk,packs_after_hk,repack_ran"

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

    hk_line=$(cd "$REPO_ROOT" && cargo run --quiet --release --example run_housekeeping -- "$WORKDIR")
    loose_after=$(echo "$hk_line" | sed -n 's/.*loose_after=\([0-9]*\).*/\1/p')
    packs_after=$(echo "$hk_line" | sed -n 's/.*packs_after=\([0-9]*\).*/\1/p')
    repack_ran=$(echo "$hk_line" | sed -n 's/.*repack_ran=\([a-z]*\).*/\1/p')

    echo "$target,$objs,$size_kb,$show_ms,$log_ms,$status_ms,$loose_after,$packs_after,$repack_ran"
done
