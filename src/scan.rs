use crate::config::Config;
use crate::utils::scan_results;
use crate::{database, ping, response};
use futures_util::TryStreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use sqlx::{PgTransaction, Pool, Postgres, Row};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinSet;
use tracing::{info, warn};

pub async fn rescan_servers(pool: Pool<Postgres>, config: Config, style: ProgressStyle) {
	let port_start = config.scanner.port_range_start;
	let port_end = config.scanner.port_range_end;
	let total_ports = config.scanner.total_ports();

	if total_ports > 10 {
		warn!("Large amount of ports! Scans will take exponentially longer for each port to scan!");
	}

	if !config.scanner.repeat {
		warn!("Repeat is not enabled in config file! Will only scan once!");
	}

	info!("Scanning port range {port_start} - {port_end} ({total_ports} port(s) per host)");

	loop {
		let mut servers = database::fetch_servers(&pool).await;
		let length = database::fetch_count(&pool).await as u64;
		let transaction = Arc::new(Mutex::new(
			pool.begin().await.expect("failed to create transaction"),
		));

		let progress_bar = Arc::new(ProgressBar::new(length).with_style(style.clone()));
		let mut ping_set = JoinSet::new();
		let scan_start = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("system time is before the unix epoch")
			.as_secs();

		// Streams results from the database
		while let Some(row) = servers.try_next().await.unwrap() {
			let address: String = row.get(0);

			for port in port_start..=port_end {
				ping_set.spawn(run(
					(address.to_owned(), port),
					transaction.clone(),
					progress_bar.clone(),
				));
			}
		}

		let results = ping_set.join_all().await;

		// Print information about scan
		scan_results(results);

		Arc::try_unwrap(transaction)
			.unwrap()
			.into_inner()
			.commit()
			.await
			.expect("error while commiting to database");

		let scan_end = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("system time is before the unix epoch")
			.as_secs();

		info!("Scan took {} seconds", scan_end - scan_start);

		// Quit if only one scan is requested in config
		if !config.scanner.repeat {
			info!("Exiting");
			std::process::exit(0);
		}

		// Wait rescan delay before starting a new scan
		if config.scanner.scan_delay > 0 {
			info!(
				"Waiting {} seconds before starting another scan...",
				config.scanner.scan_delay
			);
			tokio::time::sleep(Duration::from_secs(config.scanner.scan_delay)).await;
		}
	}
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

static PERMITS: Semaphore = Semaphore::const_new(2000);

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
		let results = tokio::time::timeout(crate::TIMEOUT_SECS, ping::ping_server(&host)).await??;
		drop(permit);

		let response = response::parse_response(results)?;

		database::update(response, &host, &mut *transaction.lock().await).await?;

		Ok(())
	}

	let result = run_inner(host, transaction).await;
	progress_bar.inc(1);
	result
}
