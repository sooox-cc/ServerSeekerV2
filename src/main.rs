mod config;
mod database;
mod ping;
mod response;

use config::load_config;
use futures_util::TryStreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use sqlx::{PgPool, PgTransaction, Row};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{sync::Arc, time::Duration};
use thiserror::Error;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinSet;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() {
	tracing_subscriber::fmt::init();

	let config_file = std::env::args().nth(1).unwrap_or("config.toml".to_string());
	info!("Using config file {config_file}");
	let config = load_config(config_file);

	// Create database URL
	let database_url = format!(
		"postgresql://{}:{}@{}:{}/{}",
		config.database.user,
		config.database.password,
		config.database.url,
		config.database.port,
		config.database.table
	);

	let port_start = config.rescanner.port_range_start;
	let port_end = config.rescanner.port_range_end;
	let total_ports = config.rescanner.total_ports();

	if total_ports > 10 {
		warn!("Large amount of ports! Scans will take exponentially longer for each port to scan!");
	}

	if !config.rescanner.repeat {
		warn!("Repeat is not enabled in config file! Will only scan once!");
	}

	info!("Scanning port range {port_start} - {port_end} ({total_ports} port(s) per host)",);

	let pool = match PgPool::connect(&database_url).await {
		Ok(pool) => pool,
		Err(e) => {
			error!("Failed to connect to database: {e}");
			std::process::exit(1);
		}
	};

	let style = ProgressStyle::with_template("[{elapsed}] [{bar:40.white/blue}] {pos:>7}/{len:7}")
		.unwrap()
		.progress_chars("=>-");

	loop {
		let mut servers = database::fetch_servers(&pool).await;
		let length = database::fetch_count(&pool).await as u64;
		let transaction = Arc::new(Mutex::new(
			pool.begin().await.expect("failed to create transaction"),
		));

		let progress_bar = Arc::new(ProgressBar::new(length).with_style(style.clone()));

		let mut ping_set = JoinSet::new();

		let start = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("System time is before the unix epoch")
			.as_secs() as i64;

		// Spawn a new task for every result
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
		let errors = results
			.into_iter()
			.filter_map(Result::err)
			.collect::<Vec<_>>();

		// Print scan errors, if any
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

		info!("Commiting results to database...");
		Arc::try_unwrap(transaction)
			.unwrap()
			.into_inner()
			.commit()
			.await
			.expect("error while commiting to database");

		let end = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("System time is before the unix epoch")
			.as_secs() as i64;

		// Scan results
		info!("Finished pinging all servers");
		info!("Scan took {} seconds", end - start);

		// Quit if only one scan is requested in config
		if !config.rescanner.repeat {
			info!("Exiting...");
			std::process::exit(0);
		}

		// Wait rescan delay before starting a new scan
		if config.rescanner.rescan_delay > 0 {
			info!(
				"Waiting {} seconds before starting another scan...",
				config.rescanner.rescan_delay
			);
			tokio::time::sleep(Duration::from_secs(config.rescanner.rescan_delay)).await;
		}
	}
}

#[derive(Debug, Error)]
enum RunError {
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

// TODO: add to config file
const TIMEOUT_SECS: Duration = Duration::from_secs(5);

static PERMITS: Semaphore = Semaphore::const_new(2000);

async fn run(
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
