//! Run git housekeeping once against an initialized vault (no daemon required).
//!
//! Usage: `run_housekeeping <vault-root>`
//!
//! Loads `[gc]` thresholds from `.vault/config.toml`, runs `maybe_run`, and prints
//! loose/pack counts plus whether a repack ran. Used by `scripts/stress/object_growth.sh`.

use std::path::PathBuf;

use vault::config::VaultConfig;
use vault::domain::VaultLayout;
use vault::storage::housekeeping;

fn main() {
    let root: PathBuf = std::env::args()
        .nth(1)
        .expect("usage: run_housekeeping <vault-root>")
        .into();
    let layout = VaultLayout::from_worktree(root);
    let config = VaultConfig::load(&layout.config_path()).expect("load config");
    let before = housekeeping::count_objects(&layout.git_dir_path()).expect("count before");
    let status = housekeeping::maybe_run(&layout, &config.gc).expect("maybe_run");
    println!(
        "loose_before={} packs_before={} loose_after={} packs_after={} repack_ran={}",
        before.loose, before.packs, status.counts.loose, status.counts.packs, status.repack_ran
    );
}
