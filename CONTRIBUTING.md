# Contributing

## Setup

Install Rust via [rustup](https://rustup.rs/) (stable toolchain):

```bash
rustup default stable
rustup component add rustfmt clippy
```

Clone and build:

```bash
git clone https://github.com/vadim-schultz/vault.git
cd vault
cargo build
```

## Checks

Run the full CI pipeline locally before pushing:

```bash
./scripts/ci.sh              # all CI jobs (default)
./scripts/ci.sh lint         # fmt, clippy, test
./scripts/ci.sh build        # release build + --version
./scripts/ci.sh docs         # mdbook build
```

Or run individual checks:

```bash
cargo fmt --all -- --check
cargo clippy -- -D warnings
cargo test
cargo build --release
./target/release/vault --version
```

## Docs

Published docs: [https://vadim-schultz.github.io/vault/](https://vadim-schultz.github.io/vault/)

Local preview:

```bash
cargo install mdbook --locked   # if mdbook is not on PATH
mdbook serve docs/              # http://localhost:3000
mdbook build docs/
```

**GitHub Pages (maintainers):** enable Settings → Pages → Build and deployment → Source:
**GitHub Actions**. The `deploy-docs` workflow publishes on every push to `main`.

Pull requests should keep CI green (fmt, clippy, test, release build, mdbook).

## Implementation plans

Chapter-based bootstrap plans are in [.plans/](.plans/).
