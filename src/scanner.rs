use crate::config::Config;
use crate::database;
use futures_util::Stream;
use sqlx::{Pool, Postgres};
use std::fmt::Debug;
use std::fs::File;
use std::io::BufRead;
use std::time::Duration;
use tokio::process::Command;
use tracing::log::warn;
use tracing::{error, info};

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
	pub fn new() -> ScanBuilder {
		ScanBuilder::default()
	}

	pub async fn start(self) {
		match self.mode {
			Mode::Discovery => Self::masscan(self).await,
			Mode::Rescanner => Self::rescan(self).await,
		}
	}

	pub fn scan_servers_from_stream<T: Stream + Unpin>(_servers: T) {}

	pub fn scan_servers_from_iterator<T: IntoIterator>(_servers: T) {}

	async fn rescan(self) {
		let port_start = self.config.scanner.port_range_start;
		let port_end = self.config.scanner.port_range_end;
		let total_ports = self.config.scanner.total_ports();

		if total_ports > 10 {
			warn!("Large amount of ports! Scans will take exponentially longer for each port to scan!");
		}

		if !self.config.scanner.repeat {
			warn!("Repeat is not enabled in config file! Will only scan once!");
		}

		info!("Scanning port range {port_start} - {port_end} ({total_ports} port(s) per host)");

		loop {
			let servers = database::fetch_servers(&self.pool).await;

			Scanner::scan_servers_from_iterator(servers);

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

	async fn masscan(self) {
		let masscan_config = &self.config.masscan.config_file;
		let masscan_output = &self.config.masscan.output_file;

		if !self.config.scanner.repeat {
			warn!("Repeat is not enabled in config file! Will only scan once!");
		}

		loop {
			let stdout = File::create(masscan_output).unwrap();

			Command::new("sudo")
				.args(["masscan", "-c", masscan_config])
				.stdout(stdout.try_clone().unwrap())
				.spawn()
				.unwrap()
				.wait()
				.await
				.unwrap();

			let reader = std::io::BufReader::new(&stdout).lines();
			Scanner::scan_servers_from_iterator(reader);

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
