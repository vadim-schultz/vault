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

section "2. Start the real background watcher"
"$VAULT_BIN" daemon --foreground &
DAEMON_PID=$!
sleep 1
vlt status
pause

section "3. Edit notes.md — the watcher auto-commits it"
echo "Hello again, Vault!" >notes.md
wait_for "modify snapshot" bash -c "[[ \$('$VAULT_BIN' log notes.md | wc -l) -ge 2 ]]"
inspect

section "4. Create and then delete draft.md"
echo "scratch" >draft.md
wait_for "draft.md snapshot" bash -c "'$VAULT_BIN' log draft.md | grep -q modify"
echo "note: the watcher path always classifies a present file as 'modify', never"
echo "'create' (see PathKind::classify in src/domain/change.rs) -- 'create' only"
echo "ever comes from vault init's baseline walk, as notes.md showed in step 1."
inspect_git
inspect_sqlite
pause
rm draft.md
wait_for "draft.md delete snapshot" bash -c "'$VAULT_BIN' log draft.md | head -n1 | grep -q delete"
echo "draft.md removed from disk -- confirm it drops out of the git tree:"
inspect

section "5. vault status / vault list"
vlt status
vlt list
pause

section "6. vault log — unscoped and scoped to a path"
vlt log
vlt log notes.md
pause

AT1="$("$VAULT_BIN" log notes.md | tail -n1 | awk '{print $2}')"
AT1_SHA="$("$VAULT_BIN" log notes.md | tail -n1 | awk '{print $1}')"
AT2="$("$VAULT_BIN" log notes.md | head -n1 | awk '{print $2}')"
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
wait_for "third snapshot" bash -c "[[ \$('$VAULT_BIN' log notes.md | wc -l) -ge 3 ]]"
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

section "Final recap"
vlt log
inspect_git
inspect_sqlite

echo ""
echo "showcase complete"
