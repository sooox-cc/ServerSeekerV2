use crate::config::Config;
use crate::response::Server;
use crate::utils::RunError;
use crate::{database, ping};
use futures_util::{future, FutureExt};
use indicatif::{ProgressBar, ProgressStyle};
use sqlx::types::ipnet::IpNet;
use sqlx::{Pool, Postgres, Row};
use std::fmt::Debug;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader};
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

static PERMITS: Semaphore = Semaphore::const_new(1000);
const TIMEOUT_SECS: Duration = Duration::from_secs(3);

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
		match self.mode {
			Mode::Discovery => self.masscan().await,
			Mode::Rescanner => self.rescan().await,
		}
	}

	/// Takes a list of ip addresses and ports, either from postgres or masscan
	/// and creates a ping job for every item, then joins on them
	pub async fn scan_servers_from_vec(
		&self,
		servers: Vec<(String, u16)>,
	) -> Vec<Result<Server, RunError>> {
		let style = ProgressStyle::with_template(
			"[{elapsed}] [{bar:40.white/blue}] {pos:>7}/{len:7} ETA {eta}",
		)
		.expect("failed to create progress bar style")
		.progress_chars("=>-");

		let bar = ProgressBar::new(servers.len() as u64).with_style(style);

		let handles = servers
			.into_iter()
			.map(|s| {
				self.run(s.0, s.1).map(|r| {
					bar.inc(1);
					r
				})
			})
			.collect::<Vec<_>>();

		let results = future::join_all(handles).await;

		bar.finish_and_clear();

		results.into_iter().collect()
	}

	/// Rescan servers already found in the database
	async fn rescan(&self) {
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
				.map(|row| {
					(
						row.get::<IpNet, _>(0).addr().to_string(),
						row.get::<i32, _>(1) as u16,
					)
				})
				.collect::<Vec<(_, _)>>();

			let scan_results = self.scan_servers_from_vec(servers).await;
			let completed_servers = self.print_scan_results(scan_results).await;

			database::update_servers_from_vec(completed_servers, self.pool.clone()).await;

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
		let masscan_config = &self.config.masscan.config_file;
		let masscan_output = &self.config.masscan.output_file;

		if !self.config.scanner.repeat {
			warn!("Repeat is not enabled in config file! Will only scan once!");
		}

		loop {
			let file = OpenOptions::new()
				.read(true)
				.open(masscan_output)
				.expect("couldn't open file");

			let _ = Command::new("sudo")
				.args(["masscan", "-c", masscan_config])
				.spawn()
				.expect("error while executing masscan")
				.wait()
				.await;

			info!("Masscan has completed!");

			let reader = BufReader::new(file);
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

			let scan_results = self.scan_servers_from_vec(servers).await;
			let completed_servers = self.print_scan_results(scan_results).await;

			database::update_servers_from_vec(completed_servers, self.pool.clone()).await;

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

	async fn run(&self, address: String, port: u16) -> Result<Server, RunError> {
		// Ping server
		let permit = PERMITS
			.acquire()
			.await
			.expect("failed to acquire a semaphore");
		let pinged_server =
			tokio::time::timeout(TIMEOUT_SECS, ping::ping_server((&*address, port))).await??;
		drop(permit);

		// Parse response
		let mut server = serde_json::from_str::<Server>(&pinged_server)?;
		server.address = address;
		server.port = port;

		Ok(server)
	}

	pub async fn print_scan_results(&self, results: Vec<Result<Server, RunError>>) -> Vec<Server> {
		debug!("results_len = {}", results.len());

		let (servers, errors): (Vec<_>, Vec<_>) = results.into_iter().partition(Result::is_ok);

		// Print errors
		if !errors.is_empty() {
			warn!("Scan returned {} total errors!", errors.len());
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

		servers
			.into_iter()
			.filter_map(Result::ok)
			.collect::<Vec<_>>()
	}
}
