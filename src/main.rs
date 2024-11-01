pub mod cli;
pub mod db;
pub mod snapshot;
pub mod utils;

use anyhow::Result;

fn main() -> Result<()> {
    cli::run()
}
