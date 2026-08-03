#!/usr/bin/env bash
# Dimension 3: file size. Commit latency and memory for files from 1KB up past the default
# 10MB max_file_bytes limit, and whether the size-limit skip is visible to the user or silent.
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

require_vault_bin

WORKDIR="$(mktemp -d)"
trap 'stop_daemon "${DAEMON_PID:-}" 2>/dev/null; rm -rf "$WORKDIR"' EXIT

new_stress_vault "$WORKDIR"
DAEMON_PID=$(start_daemon "$WORKDIR")
sleep 1

echo "size_bytes,committed,elapsed_ms"

# name:size_bytes pairs. 10485760 = default max_file_bytes (10MB).
SIZES=(
    "1KB:1024"
    "100KB:102400"
    "1MB:1048576"
    "9MB:9437184"
    "10MB_boundary:10485760"
    "11MB_over_limit:11534336"
    "50MB_over_limit:52428800"
)

expected_count=0
for entry in "${SIZES[@]}"; do
    name="${entry%%:*}"
    bytes="${entry##*:}"
    file="$WORKDIR/${name}.bin"
    head -c "$bytes" /dev/urandom > "$file"

    # A file at/under the limit produces exactly one new snapshot; over-limit files produce none.
    under_limit=1
    if (( bytes > 10485760 )); then
        under_limit=0
    fi

    if (( under_limit == 1 )); then
        expected_count=$((expected_count + 1))
        elapsed=$(wait_for_snapshot_count "$WORKDIR" "$expected_count" 15 || echo "TIMEOUT")
        echo "$name,true,$elapsed"
    else
        # Give the daemon a full debounce window plus headroom, then confirm the count
        # did NOT advance — i.e. the skip is real, not just slow.
        sleep "$DEBOUNCE_WAIT"
        actual_count=$(snapshot_count "$WORKDIR")
        if (( actual_count > expected_count )); then
            echo "$name,true,UNEXPECTED_COMMIT"
        else
            echo "$name,false,SKIPPED_NO_ERROR_SURFACED"
        fi
    fi
done

echo ""
echo "vault list output (does an over-limit file show up as tracked?):"
(cd "$WORKDIR" && "$VAULT_BIN" list)

echo ""
echo "vault status (any indication a file was skipped?):"
(cd "$WORKDIR" && "$VAULT_BIN" status)
