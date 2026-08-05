# Release binary size stream

Build-output size and compilation profile — distinct from the [benches stream](../benches/README.md)'s
runtime-load benchmarks. Concerned with what ships in `target/release/vault`, not what happens
while it runs.

| Document | Role |
|----------|------|
| [binary_size.plan.md](binary_size.plan.md) | Measured profile-tuning and dependency-feature-trim proposals (draft, pending review) |

**Code:** `Cargo.toml` (`[profile.release]`, dependency feature flags).
