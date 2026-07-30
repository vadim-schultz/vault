# Vault

**Automatic version history.**

[![CI](https://github.com/vadim-schultz/vault/actions/workflows/ci.yml/badge.svg)](https://github.com/vadim-schultz/vault/actions/workflows/ci.yml)
[![docs](https://img.shields.io/badge/docs-GitHub%20Pages-blue)](https://vadim-schultz.github.io/vault/)
[![License](https://img.shields.io/github/license/vadim-schultz/vault)](LICENSE)

Run `vault init` once in a docs directory, then forget about it. Weeks later, retrieve any earlier version:

```bash
vault show README.md --at 2026-06-01
vault restore design.md --at "2026-06-01 23:58"
```

No git knowledge required. Vault watches files in the background and records every change.

## Documentation

User guide: [https://vadim-schultz.github.io/vault/](https://vadim-schultz.github.io/vault/)

## Build

```bash
cargo build --release
./target/release/vault --version
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Implementation plans live in [.plans/](.plans/).

## License

MIT — see [LICENSE](LICENSE).
