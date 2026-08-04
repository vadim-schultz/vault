---
name: CLI module split — per-command files, zero adapter/business-logic leakage
overview: Split src/cli/mod.rs (293 lines) and src/cli/render.rs (150 lines) — currently two
  files doing arg parsing, adapter wiring, use-case invocation, and output rendering for all nine
  subcommands at once — into one file per command under src/cli/commands/, following the layout
  ontolog/src/ontolog/cli uses (main.py assembles a Typer app from per-command modules; each
  command module owns its own args + handler + output). Along the way, fix a real violation of
  this repo's own architecture.plan.md dependency rule: cli/ currently imports concrete adapters
  (GixObjectStore, SqliteMetaIndex, SystemClock) directly in five separate handlers instead of
  through a single composition-root seam, which is exactly the leakage
  architecture.plan.md's "Composition root" section already anticipated fixing via
  cli::context::build_app_context.
todos:
  - id: cli-context
    content: "Add src/cli/context.rs — the ONLY file under cli/ allowed to name concrete adapter types. Stores { object_store: GixObjectStore, meta_index: SqliteMetaIndex } with Stores::open(&VaultLayout) -> Result<Self, VaultError>, replacing five separate inline GixObjectStore::open + SqliteMetaIndex::open call sites. SystemClock does NOT live on Stores (see cli-clock-location) — it's a separate, narrower seam since only restore needs it"
    status: complete
  - id: cli-clock-location
    content: "Decided: SystemClock does not belong on Stores conceptually (only restore needs a clock; bundling it would make every other command pay conceptual weight for a dependency it doesn't use). Add a separate context::clock() -> SystemClock free function (or a one-line `SystemClock` construction directly in commands/restore.rs, since it's a zero-sized unit struct with no setup) instead of a Stores.clock field"
    status: complete
  - id: cli-support
    content: "Add src/cli/support.rs — adapter-agnostic marshalling helpers shared across commands: run_blocking(), rel_path_from_cli(), and a Global { vault_path, verbose } struct built once in dispatch() and passed to each command's run()"
    status: complete
  - id: cli-commands-dir
    content: "Create src/cli/commands/{mod,init,show,restore,log,diff,status,list,ignore,daemon}.rs. Each file owns: its own clap::Args struct (where the command takes args), its async run(&Global, Args) fn (parse -> Stores::open -> call app::<cmd>::run -> render), and any output-formatting fn specific to that command"
    status: complete
  - id: cli-render-split
    content: "Delete src/cli/render.rs. Move log_report/list_report/restore_report to their respective command files (each is single-use). Move diff_report + render_content_diff + as_utf8_pair into commands/diff.rs (also single-use). Move the Display impls for DaemonStatus/VaultStatus/StatusReport into commands/status.rs, next to the only command that renders them"
    status: complete
  - id: cli-mod-thin
    content: "Rewrite src/cli/mod.rs down to: Cli struct (global flags + subcommand), Command enum (each variant wraps that command's Args type from commands::<name>), run(), and dispatch() — one match arm per command, each a single delegating call. No adapter imports, no bail!/validation logic, no rendering left in this file"
    status: complete
  - id: cli-diff-validation-decision
    content: "Decided: switch to clap's declarative #[arg(long, requires = \"at\")] on DiffArgs.to (removes the imperative bail!\"--to requires --at\" from commands::diff::run entirely). Update tests/diff.rs's stderr predicate from contains(\"--to requires --at\") to clap's actual generated text: \"the following required arguments were not provided:\" / \"--at <AT>\" (confirmed by temporarily building with the attribute — exit code 2, same as the current bail! path)"
    status: complete
  - id: cli-tests-unchanged
    content: "Keep the #[cfg(test)] mod tests in cli/mod.rs (version_matches_cargo_toml, help_lists_subcommands, vault_path_help_does_not_promise_discovery) passing unmodified against the new Cli/Command shape. All output strings byte-identical and tests/{show,restore,log,list,status,init,cli_version}.rs need no changes; tests/diff.rs is the sole exception — update its '--to requires --at' predicate per cli-diff-validation-decision"
    status: complete
  - id: cli-arch-doc-sync
    content: "Update architecture.plan.md's module tree entry for cli/ (currently just mod.rs + render.rs) to reflect context.rs, support.rs, commands/*.rs, and mark the Composition root section's cli::context::build_app_context suggestion as now implemented as cli::context::Stores::open"
    status: complete
isProject: false
---

# CLI module split

**Status: implemented** on `feat/cli-refactor`. Both decisions from review landed as planned:
`--to` cross-flag validation moved to clap's declarative `requires`, and `SystemClock` got its
own `context::clock()` accessor rather than a field on `Stores`. `cargo build`, `cargo test`
(all 77 unit tests + every integration test file), and `cargo clippy -- -D warnings` (the repo's
actual CI gate) all pass; `tests/diff.rs` is the one test file with an intentional text change,
per the "Decisions" section below.

## Problem

`src/cli/` is two files doing everything for all nine subcommands:

- `mod.rs` (293 lines): `Cli`/`Command` clap definitions, `dispatch()`, and nine
  `handle_*` functions that each parse args, resolve the vault layout, open adapters, call the
  matching `app::*::run`, and print the result.
- `render.rs` (150 lines): `Display` impls for `app::status` types plus one formatting function
  per read command (`log_report`, `list_report`, `restore_report`, `diff_report`).

Nothing here is *wrong* per se — every `app::*` module already holds the real use-case logic
behind `&dyn ObjectStore` / `&dyn MetaIndex` / `&dyn Clock` trait objects, which is the "library
first" part you want and already have. The problem is organizational: nine unrelated commands are
interleaved in two god-files, so touching `diff` means scrolling past `restore`, `show`, `log`,
and `list` to find it — the opposite of ontolog's `cli/` layout, where `main.py` just assembles a
`Typer` app from `cli/infer/commands.py`-style per-command modules.

There is also one genuine architecture-rule violation worth fixing while we're in here.
`architecture.plan.md`'s dependency table says:

> `cli/`, `daemon/`, `watcher/` may import `app`, `domain`, `ports`, `error` — **must not import
> adapters (except composition root)**

but `cli/mod.rs` does, five times over:

```rust
use crate::adapters::{GixObjectStore, SqliteMetaIndex, SystemClock};
...
let object_store = GixObjectStore::open(&layout)?;
let meta_index = SqliteMetaIndex::open(layout.meta_db_path())?;
```

repeated verbatim in `handle_show`, `handle_restore`, `handle_log`, `handle_diff`, `handle_list`,
plus a bare `&SystemClock` in `handle_restore`. That same architecture doc already names the fix:
a `cli::context::build_app_context`-style composition root. It was never built — this refactor
builds it.

Two smaller things worth folding in since they're adjacent:

1. `handle_diff`'s `bail!("--to requires --at")` is imperative validation living in the CLI
   layer for no good reason other than that's where the flags are matched. Not business logic
   (it doesn't touch use-cases, ports, or domain types), but the more idiomatic clap way to say
   "these two flags are linked" is declaratively, on the arg definition itself — **decided**:
   switch to `#[arg(long, requires = "at")]` on `DiffArgs.to`. This removes the imperative check
   from `commands::diff::run` entirely; clap enforces it during parsing, before the handler ever
   runs. It does change the exact stderr text (see `cli-diff-validation-decision`), so
   `tests/diff.rs` needs its predicate string updated to match.
2. `rel_path_from_cli` — converting a raw `PathBuf` into the domain `RelPath` — is correctly a
   CLI concern (translating untyped user input into a validated domain type is what marshalling
   *is*), but it's currently a free function tucked at the bottom of `mod.rs`; it should live in
   the new shared `support.rs` alongside `run_blocking` since four different commands call it.

## Target layout

```text
src/cli/
├── mod.rs                 # Cli, Command, run(), dispatch() — marshalling only
├── context.rs             # Stores::open() — the ONE place allowed to name concrete adapters
├── support.rs             # run_blocking(), rel_path_from_cli(), Global{vault_path,verbose}
└── commands/
    ├── mod.rs              # pub mod declarations only
    ├── init.rs             # InitArgs, run()
    ├── show.rs             # ShowArgs, run()
    ├── restore.rs          # RestoreArgs, run(), restore_report()
    ├── log.rs              # LogArgs, run(), log_report()
    ├── diff.rs             # DiffArgs, run(), diff_report(), render_content_diff(), as_utf8_pair()
    ├── status.rs           # run(), Display impls for DaemonStatus/VaultStatus/StatusReport
    ├── list.rs             # run(), list_report()
    ├── ignore.rs           # IgnoreArgs, run()
    └── daemon.rs           # DaemonArgs (hidden), run()
```

`src/cli/render.rs` is deleted; every formatting function moves next to its one caller.

## Per-file sketch

**`context.rs`** — the composition root inside `cli/`. `Stores` holds only what every read/write
command needs (object store + meta index); `SystemClock` is intentionally not a field on it —
only `restore` needs a clock, so bundling it onto `Stores` would make every other command carry
a dependency it doesn't use. It gets its own narrow accessor instead:

```rust
pub struct Stores {
    pub object_store: GixObjectStore,
    pub meta_index: SqliteMetaIndex,
}

impl Stores {
    pub fn open(layout: &VaultLayout) -> Result<Self, VaultError> {
        Ok(Self {
            object_store: GixObjectStore::open(layout)?,
            meta_index: SqliteMetaIndex::open(layout.meta_db_path())?,
        })
    }
}

/// The only clock adapter in production use; `SystemClock` is a zero-sized unit struct so this
/// is free, but keeping it as a named seam (rather than `commands/restore.rs` naming
/// `SystemClock` directly) keeps `context.rs` the sole file under `cli/` that imports
/// concrete adapters.
pub fn clock() -> SystemClock {
    SystemClock
}
```

**`support.rs`** — shared plumbing, no adapter types in sight:

```rust
pub struct Global {
    pub vault_path: Option<PathBuf>,
    pub verbose: bool,
}

pub async fn run_blocking<F, T>(f: F) -> Result<T>
where F: FnOnce() -> Result<T, VaultError> + Send + 'static, T: Send + 'static { ... } // unchanged

pub fn rel_path_from_cli(layout: &VaultLayout, path: &Path) -> Result<RelPath, VaultError> { ... } // unchanged
```

**`commands/show.rs`** — one command, fully self-contained:

```rust
#[derive(Debug, clap::Args)]
pub struct ShowArgs {
    pub path: PathBuf,
    #[arg(long)]
    pub at: AtDate,
}

pub async fn run(global: &Global, args: ShowArgs) -> Result<()> {
    let layout = paths::resolve_vault(global.vault_path.clone())?;
    let rel = rel_path_from_cli(&layout, &args.path)?;
    let at = args.at.as_str().to_string();
    let bytes = run_blocking(move || {
        let stores = Stores::open(&layout)?;
        app::show::run(&stores.object_store, &stores.meta_index, &rel, &at)
    })
    .await?;
    std::io::stdout().write_all(&bytes)?;
    Ok(())
}
```

**`commands/diff.rs`** — the one command whose `Args` struct carries the cross-flag rule, now
declarative instead of an imperative `bail!` in the handler body:

```rust
#[derive(Debug, clap::Args)]
pub struct DiffArgs {
    pub path: PathBuf,
    #[arg(long)]
    pub at: Option<AtDate>,
    /// Requires --at: comparing two explicit points needs a starting point as well as an end.
    #[arg(long, requires = "at")]
    pub to: Option<AtDate>,
}
```

`run()` no longer has a validation branch at all — clap rejects `--to` without `--at` during
parsing, before `commands::diff::run` is ever called.

`restore.rs`, `log.rs`, `list.rs` follow the `show.rs` shape, each ending in a call to its
own local render function instead of a shared `render::xxx_report`. `status.rs` and `ignore.rs`
and `init.rs` mirror the current `handle_status`/`handle_ignore`/`handle_init` bodies verbatim,
just relocated and (for status/ignore) routed through `Stores`/`Global` where they touch adapters
or the global flags. `daemon.rs` stays a thin passthrough to `daemon::run_foreground()`.

**`mod.rs`** — after the split, this is the entire marshalling surface:

```rust
#[derive(Debug, Parser)]
#[command(name = "vault", version, about)]
pub struct Cli {
    #[arg(short, long, global = true)]
    pub verbose: bool,
    #[arg(long, global = true)]
    pub vault_path: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Init(commands::init::InitArgs),
    Show(commands::show::ShowArgs),
    Restore(commands::restore::RestoreArgs),
    Log(commands::log::LogArgs),
    Diff(commands::diff::DiffArgs),
    Status,
    List,
    Ignore(commands::ignore::IgnoreArgs),
    #[command(hide = true)]
    Daemon(commands::daemon::DaemonArgs),
}

pub async fn run() -> Result<()> {
    dispatch(Cli::parse()).await
}

async fn dispatch(cli: Cli) -> Result<()> {
    let Some(command) = cli.command else { return Ok(()) };
    let global = support::Global { vault_path: cli.vault_path, verbose: cli.verbose };
    match command {
        Command::Init(args) => commands::init::run(&global, args).await,
        Command::Show(args) => commands::show::run(&global, args).await,
        Command::Restore(args) => commands::restore::run(&global, args).await,
        Command::Log(args) => commands::log::run(&global, args).await,
        Command::Diff(args) => commands::diff::run(&global, args).await,
        Command::Status => commands::status::run().await,
        Command::List => commands::list::run(&global).await,
        Command::Ignore(args) => commands::ignore::run(&global, args).await,
        Command::Daemon(args) => commands::daemon::run(args).await,
    }
}
```

No `use crate::adapters::*`, no `bail!`, no formatting — every arm is a one-line delegation.

## Verifying no leakage

Once split, a mechanical check: `grep -rn "adapters::" src/cli/` should return matches only in
`context.rs`. Anything else in `cli/` that imports a concrete adapter type is leakage that snuck
back in. Similarly, `grep -rn "impl.*for" src/cli/commands/*.rs` (excluding `status.rs`) should
find nothing — no other command should need trait impls, since none of them hold app-layer types
worth a `Display`.

## Decisions (resolved during review)

1. **`--to requires --at`** — switch to clap's declarative `#[arg(long, requires = "at")]`.
   Confirmed by temporarily building with the attribute: clap rejects `diff PATH --to X` (no
   `--at`) with exit code 2 and stderr

   ```
   error: the following required arguments were not provided:
     --at <AT>

   Usage: vault diff --at <AT> --to <TO> <PATH>

   For more information, try '--help'.
   ```

   `tests/diff.rs`'s `predicates::str::contains("--to requires --at")` assertion updates to match
   this text (e.g. `contains("the following required arguments were not provided")` and/or
   `contains("--at <AT>")`) — the only test whose expected text changes in this whole refactor.
2. **Clock placement** — `SystemClock` does not live on `Stores`. `context.rs` exposes a separate
   `clock() -> SystemClock` accessor (or `commands/restore.rs` constructs `SystemClock` directly
   via `context::SystemClock` re-export — implementation will pick whichever reads cleaner) so
   that the one command needing a clock doesn't make every other command's `Stores::open` carry
   a field it never uses.

## Non-goals

- No behavior change, with one deliberate, called-out exception: `vault diff PATH --to X`
  (no `--at`) now fails via clap's declarative validation instead of the app's `bail!`, which
  changes the exact stderr text and requires updating `tests/diff.rs` accordingly. Every other
  subcommand's flags, output text, and exit codes stay identical.
- Not touching `app/*`, `ports/*`, `adapters/*`, `domain/*` — those already hold the real logic
  behind trait objects and are not where the leakage is.
- Not adding new commands or flags.
