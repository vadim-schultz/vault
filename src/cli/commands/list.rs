//! `vault list` command.

use anyhow::Result;

use crate::app::list;
use crate::cli::context;
use crate::cli::support::{run_blocking, Global};
use crate::domain::TrackedFile;
use crate::paths;

/// Run `vault list`.
pub async fn run(global: &Global) -> Result<()> {
    let layout = paths::resolve_vault(global.vault_path.clone())?;
    let files = run_blocking(move || {
        let meta_index = context::open_meta_index(&layout)?;
        list::run(&meta_index)
    })
    .await?;
    print!("{}", render_report(&files));
    Ok(())
}

fn render_report(files: &[TrackedFile]) -> String {
    if files.is_empty() {
        return "No tracked files.\n".to_string();
    }
    files.iter().map(render_line).collect()
}

fn render_line(file: &TrackedFile) -> String {
    format!("{}  {}\n", file.path.as_str(), file.last_modified)
}
