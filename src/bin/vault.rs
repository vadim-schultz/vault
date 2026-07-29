//! Binary entry point for the `vault` CLI.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    vault::run().await
}
