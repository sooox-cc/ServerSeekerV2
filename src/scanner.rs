use crate::config::Config;
use crate::database::Database;
use crate::protocol::PingableServer;
use crate::response::Server;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use sqlx::{Pool, Postgres, Row};
use std::fmt::Debug;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::{Duration, SystemTime};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

pub static PERMITS: Semaphore = Semaphore::const_new(1000);
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
			database: {
				match self.pool {
					Some(pool) => Database::new(pool),
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
	pub database: Database,
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
		loop {
			let start_time = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
				Ok(n) => n.as_secs(),
				Err(_) => panic!("system time before unix epoch!"),
			};

			let ports = self.config.scanner.port_range_start..=self.config.scanner.port_range_end;
			let (tx, mut rx) = tokio::sync::mpsc::channel::<SocketAddrV4>(10);

			let mut stream = sqlx::query(
				"SELECT (address - '0.0.0.0'::inet) AS address FROM servers ORDER BY last_seen DESC LIMIT 1000",
			)
			.fetch(&self.database.0);

			// Spawn a task to produce values and send them down the transmitter
			tokio::spawn(async move {
				// Streams results from database. This works great for performance
				while let Some(Ok(row)) = stream.next().await {
					let address = match row.try_get::<i64, _>("address") {
						Ok(a) => Ipv4Addr::from_bits(a as u32),
						Err(_) => continue,
					};

					// Run for each port specified in config
					for port in ports.clone() {
						match tx.send(SocketAddrV4::new(address, port)).await {
							Ok(_) => {}
							Err(e) => debug!("send channel has been closed! {e}"),
						}
					}
				}
			});

			let total_servers = self
				.database
				.count_servers()
				.await
				.expect("failed to count servers!");

			let style = ProgressStyle::with_template(
				"[{elapsed_precise}] [{bar:40.white/blue}] {human_pos}/{human_len} {msg}",
			)
			.expect("failed to create progress bar style")
			.progress_chars("=>-");

			let bar = ProgressBar::new(1000).with_style(style);

			// Consume values from the receiver
			while let Some(socket) = rx.recv().await {
				let permit = PERMITS.acquire().await;

				let pool = self.database.clone();
				let bar = bar.clone();

				tokio::spawn(async move {
					// Move permit to future so it blocks the task as well
					let _permit = permit;

					task_wrapper(socket, pool).await;
					bar.inc(1);
				});
			}

			// Sleep for 10 seconds to ensure that all tasks finish
			tokio::time::sleep(Duration::from_secs(10)).await;
			bar.finish_and_clear();

			let end_time = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
				Ok(d) => d.as_secs(),
				Err(_) => panic!("system time before unix epoch!"),
			};

			info!("Scan completed in {} seconds", end_time - start_time);

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
			// Spawn masscan
			let mut command = Command::new("sudo")
				.args(["masscan", "-c", &self.config.masscan.config_file])
				.stdout(std::process::Stdio::piped())
				.spawn()
				.expect("error while executing masscan");

			// Verify stdout is valid
			let stdout = match command.stdout.take() {
				Some(o) => o,
				None => {
					error!("Failed to get stdout from masscan!");
					std::process::exit(1);
				}
			};

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

				let pool = self.database.clone();

				// Spawn a pinging task for each server found
				tokio::spawn(async move {
					// In the future there will be different ping types here
					// Such as pinging legacy servers, and bedrock servers
					let socket = SocketAddrV4::new(address.parse().unwrap(), port);
					task_wrapper(socket, pool).await;
				});
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
}

#[inline(always)]
async fn task_wrapper(socket: SocketAddrV4, pool: Database) {
	let server = PingableServer::new(socket);

	if let Ok(response) = server.simple_ping().await {
		if let Ok(server) = serde_json::from_str::<Server>(&response) {
			if let Err(e) = pool.update_server(server, socket).await {
				debug!("Error updating server in database! {e}");
			}
		}
	}
}
