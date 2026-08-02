# Getting started

## Prerequisites

Install Rust via [rustup](https://rustup.rs/) (stable toolchain):

```bash
rustup default stable
rustup component add rustfmt clippy
```

## Clone and build

```bash
git clone https://github.com/vadim-schultz/vault.git
cd vault
cargo build --release
./target/release/vault --version
```

Expected output: `vault 0.1.0`.

## Initialize a vault

In a directory containing documents you want to version:

```bash
./target/release/vault init
```

This creates `.vault/` with storage artifacts, registers the vault in the global registry, and
starts the singleton background watcher. A second `vault init` in the same directory fails.

Check watcher health:

```bash
./target/release/vault status
```

## Verify the CLI

```bash
./target/release/vault --help
```

All subcommands are implemented: `init`, `status`, `ignore`, `show`, `restore`, `log`, `diff`,
and `list`. See [cli.md](cli.md) for full usage.

## See it all in action

`scripts/showcase.sh` drives every subcommand against a disposable vault — real background
watcher included — and prints what actually landed in the git object store (`.vault/.git`) and
the sqlite index (`.vault/meta.db`) after each step:

```bash
./scripts/showcase.sh
```

Pass `--pause` to step through it interactively, or `--keep` to leave the vault on disk
afterwards for manual poking. Requires `git` and `sqlite3` on `PATH`.

## Local documentation

Preview this book locally:

```bash
cargo install mdbook --locked   # if mdbook is not on PATH
mdbook serve docs/
```

Open [http://localhost:3000](http://localhost:3000) in your browser.

Build without serving:

```bash
mdbook build docs/
```

Published docs: [https://vadim-schultz.github.io/vault/](https://vadim-schultz.github.io/vault/).

## Contributing

See [CONTRIBUTING.md](https://github.com/vadim-schultz/vault/blob/main/CONTRIBUTING.md) in the
repository. Run the full CI mirror before pushing:

```bash
./scripts/ci.sh all
```

Implementation plans live in [.plans/](https://github.com/vadim-schultz/vault/tree/main/.plans).
