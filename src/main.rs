//! Binary entry point: form → dashboard loop.

use anyhow::Result;
use std::sync::Arc;
use stress_raiser::history;
use stress_raiser::stats::Stats;
use stress_raiser::tui::{run_form, run_tui, RunResult, TestConfig};
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> Result<()> {
    let mut hist = history::load_history();
    let mut init: Option<TestConfig> = None;

    loop {
        let config = match run_form(init, &mut hist).await {
            Ok(c) => c,
            Err(stress_raiser::AppError::UserCancelled) => std::process::exit(0),
            Err(e) => return Err(e.into()),
        };

        let stats = Arc::new(RwLock::new(Stats::default()));
        let concurrency = Arc::new(RwLock::new(config.concurrency));
        let rpm = Arc::new(RwLock::new(config.rpm));
        let running = Arc::new(RwLock::new(true));

        match run_tui(config, stats, concurrency, rpm, running).await {
            Ok(RunResult::Quit) => break,
            Ok(RunResult::BackToForm(cfg)) => {
                init = Some(cfg);
            }
            Err(e) => return Err(e),
        }
    }

    Ok(())
}
