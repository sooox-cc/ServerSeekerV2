use crate::config::Config;
use crate::response::Server;
use crate::utils::RunError;
use crate::{database, ping};
use futures_util::future;
use indicatif::{ProgressBar, ProgressStyle};
use sqlx::{Pool, Postgres, Row};
use std::fmt::Debug;
use std::io::{BufRead, BufReader};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::{Mutex, Semaphore};
use tracing::log::warn;
use tracing::{debug, error, info};

static PERMITS: Semaphore = Semaphore::const_new(1000);
const TIMEOUT_SECS: Duration = Duration::from_secs(5);

#[derive(Debug, Default)]
pub struct ScanBuilder {
	config: Config,
	mode: Mode,
	pool: Option<Pool<Postgres>>,
}

impl ScanBuilder {
	pub fn config(mut self, config: Config) -> ScanBuilder {
		self.config = config;
		self
	}

	pub fn pool(mut self, pool: Option<Pool<Postgres>>) -> ScanBuilder {
		self.pool = pool;
		self
	}

	pub fn mode(mut self, mode: Mode) -> ScanBuilder {
		self.mode = mode;
		self
	}

	pub fn build(self) -> Scanner {
		Scanner {
			config: self.config,
			mode: self.mode,
			pool: {
				match self.pool {
					Some(pool) => pool,
					None => {
						error!("Failed to connect to database!");
						std::process::exit(1);
					}
				}
			},
		}
	}
}

#[derive(clap::ValueEnum, Clone, Debug, Default)]
pub enum Mode {
	#[default]
	Discovery,
	Rescanner,
}

#[derive(Debug)]
pub struct Scanner {
	pub config: Config,
	pub mode: Mode,
	pub pool: Pool<Postgres>,
}

impl Scanner {
	/// Creates a new instance of a ScanBuilder
	pub fn new() -> ScanBuilder {
		ScanBuilder::default()
	}

	/// Starts the scanner based on the selected mode
	pub async fn start(self) {
		match self.mode {
			Mode::Discovery => Self::masscan(self).await,
			Mode::Rescanner => Self::rescan(self).await,
		}
	}

	/// Takes a vector of pingable servers
	pub async fn scan_servers_from_vec(servers: Vec<(String, u16)>) -> Vec<Server> {
		let style = ProgressStyle::with_template(
			"[{elapsed}] [{bar:40.white/blue}] {pos:>7}/{len:7} ETA {eta}",
		)
		.unwrap()
		.progress_chars("=>-");

		let bar = Arc::new(ProgressBar::new(servers.len() as u64).with_style(style));

		let handles = servers
			.into_iter()
			.map(|s| Scanner::run(s.0, s.1, bar.clone()))
			.collect::<Vec<_>>();

		// Wait for all tasks to finish
		let results = future::join_all(handles).await;

		bar.finish_and_clear();

		results
			.into_iter()
			// TODO! Don't flatten here, errors are needed later
			.flatten()
			.collect::<Vec<_>>()
	}

	/// Rescan servers already found in the database
	async fn rescan(self) {
		let port_start = self.config.scanner.port_range_start;
		let port_end = self.config.scanner.port_range_end;
		let total_ports = self.config.scanner.total_ports();

		if total_ports > 10 {
			warn!("Large amount of ports! Each extra port to scan doubles the total time taken!");
		}

		if !self.config.scanner.repeat {
			warn!("Repeat is not enabled in config file! Will only scan once!");
		}

		info!("Scanning port range {port_start} - {port_end} ({total_ports} port(s) per host)");

		loop {
			let servers = database::fetch_servers(&self.pool)
				.await
				.into_iter()
				.map(|row| (row.get::<String, _>(0), row.get::<i32, _>(1) as u16))
				.collect::<Vec<(_, _)>>();

			Self::scan_servers_from_vec(servers).await;

			// Quit if only one scan is requested in config
			if !self.config.scanner.repeat {
				info!("Exiting");
				std::process::exit(0);
			}

			// Wait rescan delay before starting a new scan
			if self.config.scanner.scan_delay > 0 {
				info!(
					"Waiting {} seconds before starting another scan...",
					self.config.scanner.scan_delay
				);
				tokio::time::sleep(Duration::from_secs(self.config.scanner.scan_delay)).await;
			}
		}
	}

	/// Starts an instance of masscan to find new servers
	async fn masscan(self) {
		let masscan_config = &self.config.masscan.config_file;
		let masscan_output = &self.config.masscan.output_file;

		if !self.config.scanner.repeat {
			warn!("Repeat is not enabled in config file! Will only scan once!");
		}

		loop {
			let output_file = std::fs::File::open(masscan_output).unwrap();

			let _ = Command::new("sudo")
				.args(["masscan", "-c", masscan_config])
				.spawn()
				.expect("error while executing masscan")
				.wait()
				.await;

			info!("Masscan has completed!");

			let reader = BufReader::new(output_file);
			let mut servers = Vec::new();

			for line in reader.lines() {
				let line = match line {
					Ok(line) => line,
					Err(_) => continue,
				};

				let mut line = line.split_whitespace();

				let port = match line.nth(2).and_then(|s| s.parse::<u16>().ok()) {
					Some(port) => port,
					None => continue,
				};

				let address = match line.next() {
					Some(address) => address.to_owned(),
					None => continue,
				};

				servers.push((address, port));
			}

			Self::scan_servers_from_vec(servers).await;

			// Quit if only one scan is requested in config
			if !self.config.scanner.repeat {
				info!("Exiting");
				std::process::exit(0);
			}

			// Wait rescan delay before starting a new scan
			if self.config.scanner.scan_delay > 0 {
				info!(
					"Waiting {} seconds before starting another scan",
					self.config.scanner.scan_delay
				);
				tokio::time::sleep(Duration::from_secs(self.config.scanner.scan_delay)).await;
			}
		}
	}

	async fn run(address: String, port: u16, bar: Arc<ProgressBar>) -> anyhow::Result<Server> {
		async fn inner(address: String, port: u16) -> anyhow::Result<Server> {
			// Ping server
			let permit = PERMITS.acquire().await?;
			let pinged_server = tokio::time::timeout(
				TIMEOUT_SECS,
				// TODO! Fix
				ping::ping_server((&*address.split('/').nth(0).unwrap(), port)),
			)
			.await??;
			drop(permit);

			// Parse response
			let mut server = serde_json::from_str::<Server>(&pinged_server)?;
			server.address = address;
			server.port = port;

			Ok(server)
		}

		let server = inner(address, port).await;
		bar.inc(1);
		server
	}

	pub async fn complete_scan(&self, results: Vec<Result<Server, RunError>>) {
		let results_len = results.len();
		debug!("results_len = {}", results_len);

		let (servers, errors): (Vec<_>, Vec<_>) = results.into_iter().partition(Result::is_ok);

		let errors_len = errors.len();

		// Print scan errors
		if !errors.is_empty() {
			warn!("Scan returned {} total errors!", errors_len);
			let mut counts = [0u32; 6];
			for e in errors.into_iter().filter_map(Result::err) {
				let i: usize = e.into();
				counts[i] += 1;
			}

			warn!("{} errors while parsing addresses", counts[0]);
			warn!("{} I/0 Errors", counts[1]);
			warn!("{} malformed responses", counts[2]);
			warn!("{} errors while parsing responses", counts[3]);
			warn!("{} servers timed out", counts[4]);
			info!("{} servers removed due to opting out", counts[5])
		}

		// Transactions allow adding multiple statements to a single query
		let transaction = Arc::new(Mutex::new(
			self.pool
				.begin()
				.await
				.expect("failed to create transaction"),
		));

		let completed_servers = servers
			.into_iter()
			.filter_map(Result::ok)
			.collect::<Vec<_>>();

		info!(
			"Commiting {} servers to database...",
			completed_servers.len()
		);

		Arc::try_unwrap(transaction)
			.unwrap()
			.into_inner()
			.commit()
			.await
			.expect("error while commiting to database");
	}
}
