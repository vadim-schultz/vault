#!/usr/bin/env bash
# Narrated walkthrough of every `vault` subcommand against a disposable vault,
# printing the internal git (.vault/.git) and sqlite (.vault/meta.db) state
# after each step. This is a human-facing demo/onboarding tool, not a CI gate
# — see scripts/ci.sh for that.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

KEEP=0
PAUSE=0
VAULT_BIN=""
DEBOUNCE_WAIT=3 # default debounce_ms is 2000; leave headroom

usage() {
    cat <<'EOF'
Usage: scripts/showcase.sh [--keep] [--pause] [--vault-bin PATH]

Drive every vault subcommand against a disposable vault in a tempdir, real
watcher included, printing the git commit graph and sqlite rows after each
state-changing step.

Options:
  --keep            Don't delete the tempdir on exit; print its path instead.
  --pause           Wait for Enter after each inspection block.
  --vault-bin PATH  Use this vault binary instead of building target/release.
  -h, --help        Show this help.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --keep) KEEP=1; shift ;;
        --pause) PAUSE=1; shift ;;
        --vault-bin) VAULT_BIN="$2"; shift 2 ;;
        -h | --help) usage; exit 0 ;;
        *) echo "error: unknown argument: $1" >&2; usage >&2; exit 1 ;;
    esac
done

require() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "error: $1 is required ($2)" >&2
        exit 1
    }
}

require git "needed to inspect .vault/.git"
require sqlite3 "needed to inspect .vault/meta.db (macOS ships this; on Linux: apt install sqlite3)"

if [[ -z "$VAULT_BIN" ]]; then
    require cargo "needed to build vault, or pass --vault-bin"
    echo "Building release binary..."
    cargo build --release --quiet
    VAULT_BIN="$ROOT/target/release/vault"
elif [[ "$VAULT_BIN" != /* ]]; then
    VAULT_BIN="$ROOT/$VAULT_BIN"
fi

STATE_DIR="$(mktemp -d)"
WORKDIR="$(mktemp -d)"
export VAULT_STATE_DIR="$STATE_DIR"
VAULT_DIR="$WORKDIR/.vault"
DAEMON_PID=""

cleanup() {
    [[ -n "$DAEMON_PID" ]] && kill "$DAEMON_PID" 2>/dev/null || true
    if [[ "$KEEP" -eq 1 ]]; then
        echo ""
        echo "--keep set: leaving vault at $WORKDIR (state dir: $STATE_DIR)"
    else
        rm -rf "$STATE_DIR" "$WORKDIR"
    fi
}
trap cleanup EXIT

section() {
    echo ""
    echo "=== $1 ==="
}

vlt() {
    echo "+ vault $*"
    "$VAULT_BIN" "$@"
}

inspect_git() {
    section "git: .vault/.git"
    git --git-dir="$VAULT_DIR/.git" log --oneline --stat -8
    echo ""
    echo "tree at HEAD:"
    git --git-dir="$VAULT_DIR/.git" ls-tree -r --name-only HEAD
}

inspect_sqlite() {
    section "sqlite: .vault/meta.db"
    echo "-- snapshots --"
    sqlite3 -column -header "$VAULT_DIR/meta.db" \
        "SELECT id, commit_sha, created_at FROM snapshots ORDER BY id DESC LIMIT 8;"
    echo "-- file_events --"
    sqlite3 -column -header "$VAULT_DIR/meta.db" \
        "SELECT id, snapshot_id, path, event_type FROM file_events ORDER BY id DESC LIMIT 12;"
}

inspect_daemon_state() {
    section "daemon state (VAULT_STATE_DIR)"
    echo "state dir: $STATE_DIR"
    if [[ -f "$STATE_DIR/daemon.json" ]]; then
        echo "-- daemon.json (heartbeat) --"
        cat "$STATE_DIR/daemon.json"
    else
        echo "(no daemon.json yet)"
    fi
    echo ""
    if [[ -f "$STATE_DIR/queue.json" ]]; then
        echo "-- queue.json (pending background tasks, written every 2s) --"
        cat "$STATE_DIR/queue.json"
    else
        echo "(no queue.json yet — heartbeat has not ticked)"
    fi
    echo ""
    if [[ -f "$STATE_DIR/daemon.log" ]]; then
        echo "-- daemon.log (last 5 lines) --"
        tail -n 5 "$STATE_DIR/daemon.log" || true
    fi
}

inspect_queue_status() {
    section "vault status (includes background work queue)"
    vlt status
    echo ""
    echo "The daemon seeds recurring reconcile_walk (every 10 min) and git_housekeeping"
    echo "(every 15 min) tasks per registered vault at startup. While a task is pending,"
    echo "status lists it as 'Queue: N pending' with id, kind, lane, and attempts —"
    echo "sourced from queue.json."
}

inspect() {
    inspect_git
    inspect_sqlite
    pause
}

pause() {
    [[ "$PAUSE" -eq 1 ]] && read -r -p "-- press enter to continue --" _ || true
}

wait_for() {
    local desc="$1"
    shift
    for _ in $(seq 1 100); do
        if "$@" >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.1
    done
    echo "timed out waiting for: $desc" >&2
    exit 1
}

cd "$WORKDIR"

section "1. Seed a file, then vault init"
echo "Hello, Vault!" >notes.md
vlt init --no-service
inspect

section "1b. Second vault — two registered roots means two background reconcile tasks"
mkdir -p nested-vault
echo "nested copy" >nested-vault/readme.md
(
    cd nested-vault
    vlt init --no-service
)
pause

section "2. Start the real background watcher"
"$VAULT_BIN" daemon --foreground &
DAEMON_PID=$!
sleep 0.2
echo "polling queue.json for a pending reconcile_walk (runner may drain it quickly)..."
for _ in $(seq 1 40); do
    if [[ -f "$STATE_DIR/queue.json" ]] &&
        grep -q '"kind": "reconcile_walk"' "$STATE_DIR/queue.json" 2>/dev/null; then
        break
    fi
    sleep 0.05
done
inspect_queue_status
inspect_daemon_state
sleep 2
echo "after heartbeat tick (queue snapshot may now be empty if work finished):"
inspect_queue_status
pause

section "3. Edit notes.md — the watcher auto-commits it"
echo "Hello again, Vault!" >notes.md
wait_for "modify snapshot" bash -c "[[ \$('$VAULT_BIN' log notes.md | grep -cE '^(update|delete|restore|change) ') -ge 2 ]]"
inspect

section "4. Create and then delete draft.md"
echo "scratch" >draft.md
wait_for "draft.md snapshot" bash -c "'$VAULT_BIN' log draft.md | grep -q update"
echo "note: file_events.event_type still stores raw create/modify/delete/restore"
echo "values (see inspect_sqlite's file_events dump below, unchanged) -- it's only"
echo "the humanized 'vault log' header that groups create+modify under 'update',"
echo "matching git's own convention of not distinguishing them in --stat output."
inspect_git
inspect_sqlite
pause
rm draft.md
wait_for "draft.md delete snapshot" bash -c "'$VAULT_BIN' log draft.md | head -n1 | grep -q delete"
echo "draft.md removed from disk -- confirm it drops out of the git tree:"
inspect

section "5. vault status / vault list"
inspect_queue_status
vlt list
pause

section "6. vault log — unscoped and scoped to a path"
vlt log
vlt log notes.md
pause

AT1="$("$VAULT_BIN" log notes.md | grep -E '^(update|delete|restore|change) ' | tail -n1 | awk '{print $NF}')"
AT1_SHA="$(git --git-dir="$VAULT_DIR/.git" log --format='%H %s' --all | grep -F "$AT1" | awk '{print $1}')"
AT2="$("$VAULT_BIN" log notes.md | grep -E '^(update|delete|restore|change) ' | head -n1 | awk '{print $NF}')"
echo "captured AT1=$AT1 (baseline v1, commit $AT1_SHA), AT2=$AT2 (v2 modify)"

section "7. vault show — retrieve v1's exact bytes"
vlt show notes.md --at "$AT1"
echo ""
echo "cross-check via git cat-file directly:"
git --git-dir="$VAULT_DIR/.git" cat-file -p "$AT1_SHA:notes.md"
pause

section "8. vault diff — two snapshots"
vlt diff notes.md --at "$AT1" --to "$AT2"
pause

section "9. vault diff --to without --at is a CLI usage error"
vlt diff notes.md --to "$AT2" || echo "(failed as expected)"
pause

section "10. vault diff — last snapshot vs. an uncommitted edit"
echo "Hello a third time, Vault!" >notes.md
vlt diff notes.md
wait_for "third snapshot" bash -c "[[ \$('$VAULT_BIN' log notes.md | grep -cE '^(update|delete|restore|change) ') -ge 3 ]]"
echo "the watcher just auto-committed that edit too:"
inspect

section "11. vault restore --dry-run — resolves but writes/commits nothing"
vlt restore notes.md --at "$AT1" --dry-run
echo "notes.md on disk is still v3:"
cat notes.md
pause

section "12. vault restore — writes v1 back and commits its own snapshot"
vlt restore notes.md --at "$AT1"
cat notes.md
echo ""
echo "the restore is its own commit, tagged 'restore' (not 'modify'):"
inspect

section "13. vault ignore — a live daemon does not hot-reload it"
vlt ignore '*.tmp'
echo "restarting the daemon so the new ignore pattern actually takes effect"
kill "$DAEMON_PID"
wait "$DAEMON_PID" 2>/dev/null || true
"$VAULT_BIN" daemon --foreground &
DAEMON_PID=$!
sleep 1
echo "scratch" >ignored.tmp
sleep "$DEBOUNCE_WAIT"
echo "ignored.tmp should be absent from both list and the git tree:"
vlt list
inspect

section "14. git housekeeping — conditional repack when loose objects exceed [gc] threshold"
echo "ground truth before housekeeping:"
git --git-dir="$VAULT_DIR/.git" count-objects -v
echo "lowering [gc].loose_object_limit so the next check will repack"
if grep -q '^\[gc\]' "$VAULT_DIR/config.toml"; then
    sed -i.bak '/^\[gc\]/,/^\[/ s/^loose_object_limit = .*/loose_object_limit = 5/' "$VAULT_DIR/config.toml"
    rm -f "$VAULT_DIR/config.toml.bak"
else
    cat >>"$VAULT_DIR/config.toml" <<'EOF'

[gc]
loose_object_limit = 5
EOF
fi
for i in 1 2 3; do
    echo "extra edit $i for housekeeping demo" >>notes.md
    sleep "$DEBOUNCE_WAIT"
done
echo "restarting the daemon so git_housekeeping is re-enqueued immediately at startup"
kill "$DAEMON_PID"
wait "$DAEMON_PID" 2>/dev/null || true
"$VAULT_BIN" daemon --foreground &
DAEMON_PID=$!
sleep 1
echo "polling queue.json for a pending git_housekeeping task..."
for _ in $(seq 1 40); do
    if [[ -f "$STATE_DIR/queue.json" ]] &&
        grep -q '"kind": "git_housekeeping"' "$STATE_DIR/queue.json" 2>/dev/null; then
        break
    fi
    sleep 0.05
done
inspect_queue_status
inspect_daemon_state
wait_for "housekeeping repack" bash -c "[[ \$(git --git-dir='$VAULT_DIR/.git' count-objects -v | awk '/^count:/{print \$2}') -le 5 ]]"
echo "ground truth after housekeeping:"
git --git-dir="$VAULT_DIR/.git" count-objects -v
vlt status
pause

section "15. File size limit — oversized files are skipped, but vault status says so"
echo "writing a 11MB file (default max_file_bytes is 10MB)"
dd if=/dev/zero of=huge.bin bs=1M count=11 2>/dev/null
sleep "$DEBOUNCE_WAIT"
echo "huge.bin was never committed -- confirm it's absent from list and the git tree:"
vlt list
inspect_git
echo ""
echo "vault status enumerates it instead of staying silent about the skip:"
vlt status
pause

section "16. Humanized vault log -- git --stat shape, --verbose for full diffs"
vlt log notes.md
echo ""
echo "cross-check shape against real git log --stat (hash present there, absent above):"
git --git-dir="$VAULT_DIR/.git" log --stat notes.md
pause
echo "--verbose swaps the diffstat block for full unified-diff hunks, like git log -p:"
"$VAULT_BIN" --verbose log notes.md
pause

section "17. Humanized vault show -- whole-vault and directory report modes"
echo "no PATH: report for the resolved commit, full diff always (no --verbose needed):"
vlt show --at "$AT2"
echo ""
echo "cross-check against real git show:"
AT2_SHA="$(git --git-dir="$VAULT_DIR/.git" log --format='%H %s' --all | grep -F "$AT2" | awk '{print $1}')"
git --git-dir="$VAULT_DIR/.git" show "$AT2_SHA"
pause
echo "seed a subdirectory so directory-scoped show has something to filter to:"
mkdir -p sub
echo "sub file" >sub/child.md
wait_for "sub/child.md snapshot" bash -c "'$VAULT_BIN' log sub/child.md | grep -q update"
AT3="$("$VAULT_BIN" log | grep -E '^(update|delete|restore|change) ' | head -n1 | awk '{print $NF}')"
echo "a directory path scopes the same report to that subtree only:"
vlt show sub --at "$AT3"
pause

section "Final recap"
vlt log
inspect_git
inspect_sqlite

echo ""
echo "showcase complete"
