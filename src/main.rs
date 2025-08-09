use anyhow::Result;

mod cli;
mod client;
mod config;
mod logging;
mod server;
mod utils;

#[tokio::main]
async fn main() -> Result<()> {
    cli::execute().await
}
