use crate::response::Server;
#[cfg(feature = "warner")]
use crate::warner::join_server;
use crate::{database, ping, response};
use indicatif::{ProgressBar, ProgressIterator, ProgressStyle};
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{Mutex, Semaphore};
use tokio::task::JoinError;
use tracing::{debug, info, warn};

static PERMITS: Semaphore = Semaphore::const_new(1000);
const TIMEOUT_SECS: Duration = Duration::from_secs(5);

pub async fn handle_scan_results(
	results: Vec<Result<Result<CompletedServer, RunError>, JoinError>>,
	pool: &Pool<Postgres>,
	style: ProgressStyle,
) {
	let results_len = results.len();
	debug!("results_len = {}", results_len);

	let (servers, errors): (Vec<_>, Vec<_>) = results
		.into_iter()
		.filter_map(Result::ok)
		.partition(Result::is_ok);

	let errors_len = errors.len();

	// Print scan errors
	if !errors.is_empty() {
		warn!("Scan returned {} total errors!", errors_len);
		let mut counts = [0u32; 5];
		for e in errors.into_iter().filter_map(Result::err) {
			let i: usize = e.into();
			counts[i] += 1;
		}

		warn!("{} errors while parsing addresses", counts[0]);
		warn!("{} I/0 Errors", counts[1]);
		warn!("{} malformed responses", counts[2]);
		warn!("{} errors while parsing responses", counts[3]);
		warn!("{} servers timed out", counts[4]);
	}

	// Transactions allow adding multiple statements to a single query
	let transaction = Arc::new(Mutex::new(
		pool.begin().await.expect("failed to create transaction"),
	));

	let completed_servers = servers
		.into_iter()
		.filter_map(Result::ok)
		.collect::<Vec<_>>();

	info!(
		"Commiting {} servers to database...",
		completed_servers.len()
	);
	let bar = ProgressBar::new(completed_servers.len() as u64).with_style(style);

	for server in completed_servers.into_iter().progress_with(bar) {
		if let Err(e) = database::update(
			server.server,
			&(server.ip, server.port),
			&mut *transaction.lock().await,
		)
		.await
		{
			warn!("Failed to update database: {}", e);
		}
	}

	Arc::try_unwrap(transaction)
		.unwrap()
		.into_inner()
		.commit()
		.await
		.expect("error while commiting to database");
}

#[derive(Debug, Error)]
pub enum RunError {
	#[error("Failed to parse address")]
	AddressParseError(#[from] std::net::AddrParseError),
	#[error("I/O error")]
	IOError(#[from] std::io::Error),
	#[error("Malformed response")]
	MalformedResponse,
	#[error("Error while parsing response")]
	ParseResponse(#[from] serde_json::Error),
	#[error("Connection timed out")]
	TimedOut(#[from] tokio::time::error::Elapsed),
}
impl Into<usize> for RunError {
	fn into(self) -> usize {
		match self {
			Self::AddressParseError(_) => 0,
			Self::IOError(_) => 1,
			Self::MalformedResponse => 2,
			Self::ParseResponse(_) => 3,
			Self::TimedOut(_) => 4,
		}
	}
}

#[derive(Debug)]
pub struct CompletedServer {
	pub ip: String,
	pub port: u16,
	pub server: Server,
}

pub async fn run(
	host: (String, u16),
	progress_bar: Arc<ProgressBar>,
) -> Result<CompletedServer, RunError> {
	async fn run_inner(host: (&str, u16)) -> Result<CompletedServer, RunError> {
		let permit = PERMITS.acquire().await.unwrap();
		let results = tokio::time::timeout(TIMEOUT_SECS, ping::ping_server(host)).await??;
		drop(permit);

		let response = response::parse_response(results)?;

		let _ = join_server(host.0).await;

		Ok(CompletedServer {
			ip: host.0.to_string(),
			port: host.1,
			server: response,
		})
	}

	let completed_server = run_inner((host.0.as_str(), host.1)).await;

	if let Err(e) = &completed_server {
		debug!("{} threw an error: {}", host.0, e)
	}

	progress_bar.inc(1);
	completed_server
}
