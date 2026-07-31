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

`init`, `status`, and `ignore` are implemented. `show`, `restore`, `log`, `diff`, and `list`
return **not implemented yet** until Chapter 5.

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
