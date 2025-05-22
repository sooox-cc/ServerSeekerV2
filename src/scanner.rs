use crate::config::Config;
use crate::{database, utils};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use sqlx::types::ipnet::IpNet;
use sqlx::{Pool, Postgres, Row};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::sync::Semaphore;
use tracing::{error, info, warn};

pub static PERMITS: Semaphore = Semaphore::const_new(10000);
pub const TIMEOUT_SECS: Duration = Duration::from_secs(3);

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
	pub async fn start(&self) {
		if !self.config.scanner.repeat {
			warn!("Repeat is not enabled in config file! Will only scan once!");
		}

		match self.mode {
			Mode::Discovery => self.masscan().await,
			Mode::Rescanner => self.rescan().await,
		}
	}

	/// Rescan servers already found in the database
	async fn rescan(&self) {
		let port_start = self.config.scanner.port_range_start;
		let port_end = self.config.scanner.port_range_end;
		let total_ports = self.config.scanner.total_ports();

		if total_ports > 10 {
			warn!("Large amount of ports! Each extra port to scan doubles the total time taken!");
		}

		info!("Scanning port range {port_start} - {port_end} ({total_ports} port(s) per host)");

		loop {
			let mut servers = database::fetch_servers(&self.pool).await;

			// Create transaction to update each server
			let transaction = Arc::new(Mutex::new(
				self.pool
					.begin()
					.await
					.expect("failed to create transaction"),
			));

			// Fetch how many rows are returned from database
			let count: i64 = sqlx::query("SELECT count(address) FROM servers")
				.fetch_one(&self.pool)
				.await
				.expect("Failed to get a valid row count from the database!")
				.get(0);

			let style = ProgressStyle::with_template(
				"[{elapsed}] [{bar:40.white/blue}] {pos:>7}/{len:7} ETA {eta}",
			)
			.expect("failed to create progress bar style")
			.progress_chars("=>-");

			let bar = Arc::new(ProgressBar::new(count as u64).with_style(style));

			info!("Scanning {count} servers from database");

			let mut handles = Vec::with_capacity(count as usize);
			while let Some(Ok(row)) = servers.next().await {
				let address = row.get::<IpNet, _>("address").addr().to_string();
				let port = row.get::<i32, _>("port") as u16;

				handles.push(tokio::task::spawn(utils::run_and_update_with_progress(
					address,
					port,
					transaction.clone(),
					bar.clone(),
				)));
			}

			futures_util::future::join_all(handles).await;

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
	async fn masscan(&self) {
		loop {
			// Create transaction to update each server
			let transaction = Arc::new(Mutex::new(
				self.pool
					.begin()
					.await
					.expect("failed to create transaction"),
			));

			// Spawn masscan
			let mut command = Command::new("sudo")
				.args(["masscan", "-c", &self.config.masscan.config_file])
				.stdout(std::process::Stdio::piped())
				.spawn()
				.expect("error while executing masscan");

			let mut count = 0;

			// Get output from masscan
			if let Some(stdout) = command.stdout.take() {
				let mut reader = BufReader::new(stdout).lines();

				// Iterate over the lines of output from masscan
				while let Ok(Some(line)) = reader.next_line().await {
					let mut line = line.split_whitespace();

					let port = match line
						.nth(3)
						// Split on port/tcp
						.and_then(|p| p.split('/').nth(0))
						// Parse as u16
						.and_then(|s| s.parse::<u16>().ok())
					{
						Some(port) => port,
						None => continue,
					};

					// .nth() consumes all preceding elements so address will be the 2nd
					let address = match line.nth(1) {
						Some(address) => address.to_owned(),
						None => continue,
					};

					// Ping and update server in database
					tokio::task::spawn(utils::run_and_update(address, port, transaction.clone()));
					count += 1;
				}
			} else {
				error!("Failed to get stdout from masscan!");
				std::process::exit(1);
			}

			// Commit transaction to database
			info!("Inserting {count} servers to database");
			Arc::try_unwrap(transaction)
				.unwrap()
				.into_inner()
				.commit()
				.await
				.expect("error while commiting to database");

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
}
