//! Binary entry point: form → dashboard loop.

use anyhow::Result;
use std::sync::Arc;
use stress_raiser::history;
use stress_raiser::stats::Stats;
use stress_raiser::tui::{run_form, run_tui, RunResult};
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> Result<()> {
    let mut hist = history::load_history();
    let mut init = None;

    loop {
        let (request, conc_init, rpm_init) = match run_form(init, &mut hist).await {
            Ok(t) => t,
            Err(stress_raiser::AppError::UserCancelled) => std::process::exit(0),
            Err(e) => return Err(e.into()),
        };

        let stats = Arc::new(RwLock::new(Stats::default()));
        let concurrency = Arc::new(RwLock::new(conc_init));
        let rpm = Arc::new(RwLock::new(rpm_init));
        let running = Arc::new(RwLock::new(true));

        match run_tui(request, stats, concurrency, rpm, running).await {
            Ok(RunResult::Quit) => break,
            Ok(RunResult::BackToForm((req, conc, rpm_val))) => {
                init = Some((req, conc, rpm_val));
            }
            Err(e) => return Err(e),
        }
    }

    Ok(())
}
