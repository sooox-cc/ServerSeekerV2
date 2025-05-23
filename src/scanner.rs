use crate::config::Config;
use crate::database;
use crate::protocol::MinecraftServer;
use crate::response::Server;
use crate::utils::RunError;
use futures_util::StreamExt;
use sqlx::types::ipnet::IpNet;
use sqlx::{Pool, Postgres, Row};
use std::fmt::Debug;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

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
					Some(pool) => Arc::new(pool),
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
	pub pool: Arc<Pool<Postgres>>,
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
			let mut servers = database::fetch_servers(&self.pool).await;

			let start_time = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
				Ok(n) => n.as_secs(),
				Err(_) => panic!("SystemTime before UNIX EPOCH!"),
			};

			let mut handles = Vec::new();

			// Iterate over results from Postgres as they become available
			while let Some(Ok(row)) = servers.next().await {
				let address = row.get::<IpNet, _>("address").addr().to_string();
				let port = row.get::<i32, _>("port") as u16;

				// Wait to acquire permit before spawning a new task
				let permit = PERMITS.acquire().await;
				handles.push(task_wrapper(address, port, self.pool.clone()));
			}

			futures_util::future::join_all(handles).await;

			let end_time = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
				Ok(d) => d.as_secs(),
				Err(_) => panic!("SystemTime before UNIX EPOCH!"),
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

				// Spawn a pinging task for each server found
				task_wrapper(address, port, self.pool.clone());
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

fn task_wrapper(
	address: String,
	port: u16,
	conn: Arc<Pool<Postgres>>,
) -> tokio::task::JoinHandle<()> {
	tokio::task::spawn(async move {
		let minecraft_server = MinecraftServer::new(address, port);

		async fn run(minecraft_server: MinecraftServer) -> Result<Server, RunError> {
			// In the future there will be a config option to specify the ping type for
			// a server, as some servers require the hostname and port to be set.
			// Something that this program isn't doing yet.
			let response = minecraft_server.simple_ping().await?;

			// Assign address and port to the server struct
			let mut server: Server = serde_json::from_str(&response)?;
			server.address = minecraft_server.address;
			server.port = minecraft_server.port;

			Ok(server)
		}

		match run(minecraft_server).await {
			Ok(server) => {
				let _ = database::update_server(server, conn).await;
			}
			Err(error) => debug!("Error occurred while pinging server: {error}"),
		}
	})
}
