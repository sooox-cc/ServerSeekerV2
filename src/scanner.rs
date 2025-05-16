use crate::config::Config;
use crate::response::Server;
use crate::{database, ping};
use futures_util::future;
use indicatif::{ProgressBar, ProgressStyle};
use sqlx::{Pool, Postgres, Row};
use std::fmt::Debug;
use std::io::{BufRead, BufReader};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tracing::log::warn;
use tracing::{error, info};

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

	pub async fn scan_servers_from_vec(servers: Vec<(String, u16)>) {
		let style = ProgressStyle::with_template(
			"[{elapsed}] [{bar:40.white/blue}] {pos:>7}/{len:7} ETA {eta}",
		)
		.unwrap()
		.progress_chars("=>-");

		let bar = Arc::new(ProgressBar::new(servers.len() as u64).with_style(style));

		let handles = servers
			.into_iter()
			.map(|s| tokio::task::spawn(Self::run(s.0, s.1)))
			.collect::<Vec<_>>();

		// Wait for all tasks to finish
		let results = future::join_all(handles).await;
		let completed_servers = results
			.into_iter()
			.flatten()
			.map(|s| s.ok())
			.collect::<Vec<_>>();

		bar.finish_and_clear();
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
				.map(|row| (row.get::<String, _>(0), row.get::<i32, _>(1)))
				.collect::<Vec<(_, _)>>();

			// Self::scan_servers_from_iterator(servers.into_iter()).await;

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
			// let output_file = std::fs::File::create(masscan_output).unwrap();

			let _ = Command::new("sudo")
				.args(["masscan", "-c", masscan_config])
				.spawn()
				.expect("error while executing masscan")
				.wait()
				.await;

			info!("Masscan has completed!");

			let reader = BufReader::new(output_file);
			let mut handles = Vec::new();

			for line in reader.lines() {
				let line = if let Ok(line) = line {
					line
				} else {
					continue;
				};

				let mut full_line = line.split_whitespace();

				let port = if let Some(port) = full_line.nth(2) {
					if let Ok(port) = port.parse::<u16>() {
						port
					} else {
						continue;
					}
				} else {
					continue;
				};

				let address = if let Some(address) = full_line.next() {
					address.to_owned()
				} else {
					continue;
				};

				handles.push(tokio::task::spawn(Self::run(address, port)));
			}

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

	async fn run(address: String, port: u16) -> anyhow::Result<Server> {
		let host = (&*address, port);

		// Ping server
		let permit = PERMITS.acquire().await?;
		let pinged_server = tokio::time::timeout(TIMEOUT_SECS, ping::ping_server(host)).await??;
		drop(permit);

		println!("{pinged_server}");

		// Parse response
		let mut server: Server = serde_json::from_str(&pinged_server)?;
		server.address = address;
		server.port = port;

		Ok(server)
	}
}
