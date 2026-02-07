// main.rs
pub mod cli;
pub mod db;
pub mod mcp;
pub mod snapshot;
pub mod utils;
pub mod web;

use anyhow::Result;

fn main() -> Result<()> {
    // Clean up any orphaned temporary files at startup
    if let Err(e) = snapshot::Snapshot::cleanup_temp_files() {
        eprintln!("Warning: Failed to cleanup temporary files: {}", e);
    }
    
    tokio::runtime::Runtime::new()?.block_on(async {
        cli::run().await
    })
}
