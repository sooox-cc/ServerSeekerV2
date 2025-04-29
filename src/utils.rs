use crate::{database, ping, response};
use indicatif::ProgressBar;
use sqlx::PgTransaction;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{Mutex, Semaphore};
use tracing::{info, warn};

static PERMITS: Semaphore = Semaphore::const_new(2000);
const TIMEOUT_SECS: Duration = Duration::from_secs(5);

pub fn scan_results(results: Vec<Result<(), RunError>>) {
	let results_len = results.len();

	// Save all errors for statistics
	let errors = results
		.into_iter()
		.filter_map(Result::err)
		.collect::<Vec<_>>();

	let errors_len = errors.len();

	// Print scan errors
	if !errors.is_empty() {
		warn!("Scan returned {} errors!", errors.len());
		let mut counts = [0u32; 4];
		for e in errors {
			let i: usize = e.into();
			counts[i] += 1;
		}

		warn!("{} errors while pinging servers", counts[0]);
		warn!("{} errors while parsing responses", counts[1]);
		warn!("{} errors while updating the database", counts[2]);
		warn!("{} connection timeouts", counts[3]);
	}

	info!("Commiting {} results to database", results_len - errors_len);
}

#[derive(Debug, Error)]
pub enum RunError {
	#[error("Error while pinging server")]
	PingServer(#[from] ping::PingServerError),
	#[error("Error while parsing response")]
	ParseResponse(#[from] serde_json::Error),
	#[error("Error while updating database")]
	DatabaseUpdate(#[from] sqlx::Error),
	#[error("Connection timed out")]
	TimedOut(#[from] tokio::time::error::Elapsed),
}
impl Into<usize> for RunError {
	fn into(self) -> usize {
		match self {
			Self::PingServer(_) => 0,
			Self::ParseResponse(_) => 1,
			Self::DatabaseUpdate(_) => 2,
			Self::TimedOut(_) => 3,
		}
	}
}

pub async fn run(
	host: (String, u16),
	transaction: Arc<Mutex<PgTransaction<'_>>>,
	progress_bar: Arc<ProgressBar>,
) -> Result<(), RunError> {
	async fn run_inner(
		host: (String, u16),
		transaction: Arc<Mutex<PgTransaction<'_>>>,
	) -> Result<(), RunError> {
		let permit = PERMITS.acquire().await.unwrap();
		let results = tokio::time::timeout(TIMEOUT_SECS, ping::ping_server(&host)).await??;
		drop(permit);

		let response = response::parse_response(results)?;

		database::update(response, &host, &mut *transaction.lock().await).await?;

		Ok(())
	}

	let result = run_inner(host, transaction).await;
	progress_bar.inc(1);
	result
}
