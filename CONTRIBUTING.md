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

```bash
cargo install mdbook --locked   # if mdbook is not on PATH
mdbook build docs/
```

Pull requests should keep CI green (fmt, clippy, test, release build, mdbook).

## Implementation plans

Chapter-based bootstrap plans are in [.plans/](.plans/).
