#!/usr/bin/env bash
# Run GitHub Actions CI steps locally (mirrors .github/workflows/ci.yml).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

JOB="all"

usage() {
    cat <<'EOF'
Usage: scripts/ci.sh [lint|build|docs|all]

Run CI pipeline steps locally (mirrors .github/workflows/ci.yml).

Commands:
  lint   lint-test job: cargo fmt, clippy, test
  build  build-test job: release build, vault --version
  docs   docs job: mdbook build
  all    run lint, build, and docs (default)

Requires Rust stable with rustfmt and clippy components.
For docs: mdbook on PATH or `cargo install mdbook --locked`.
EOF
}

step() {
    echo ""
    echo "=== $1 ==="
}

require_cargo() {
    if ! command -v cargo >/dev/null 2>&1; then
        echo "error: cargo is required; install via https://rustup.rs/" >&2
        exit 1
    fi
}

require_mdbook() {
    if command -v mdbook >/dev/null 2>&1; then
        return
    fi
    echo "error: mdbook is required; install with: cargo install mdbook --locked" >&2
    exit 1
}

run_lint_test() {
    step "cargo fmt --check"
    cargo fmt --all -- --check

    step "cargo clippy"
    cargo clippy -- -D warnings

    step "cargo test"
    cargo test
}

run_build_test() {
    step "cargo build --release"
    cargo build --release

    step "vault --version"
    ./target/release/vault --version
}

run_docs() {
    require_mdbook

    step "mdbook build"
    mdbook build docs/
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -h | --help)
            usage
            exit 0
            ;;
        lint | build | docs | all)
            JOB="$1"
            shift
            ;;
        *)
            echo "error: unknown argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

require_cargo

case "$JOB" in
    lint)
        run_lint_test
        ;;
    build)
        run_build_test
        ;;
    docs)
        run_docs
        ;;
    all)
        run_lint_test
        run_build_test
        run_docs
        ;;
esac
