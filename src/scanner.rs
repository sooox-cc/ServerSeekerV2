use crate::config::Config;
use crate::database::Database;
use crate::protocol::PingableServer;
use crate::response::Server;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use sqlx::{Pool, Postgres, Row};
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::net::{IpAddr, Ipv4Addr, SocketAddrV4};
use std::str::FromStr;
use std::sync::Arc;
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
	RangeScanner,
	GeoUpdate,
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
			Mode::RangeScanner => self.range_scan().await,
			Mode::GeoUpdate => self.geo_update().await,
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
				"SELECT (address - '0.0.0.0'::inet) AS address FROM servers ORDER BY last_seen ASC",
			)
			.fetch(&self.database.pool);

			// Spawn a task to produce values and send them down the transmitter
			tokio::spawn(async move {
				// Streams results from database. This works great for memory usage
				while let Some(Ok(row)) = stream.next().await {
					let address = match row.try_get::<i64, _>("address") {
						Ok(a) => Ipv4Addr::from_bits(a as u32),
						Err(_) => continue,
					};

					// Run for each port specified in config
					//
					// NOTE: clone is needed because RangeInclusive<T> doesn't implement copy
					// This should be optimized away anyway
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

			let bar =
				ProgressBar::new((total_servers * self.config.scanner.total_ports() as i64) as u64)
					.with_style(style);

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

	/// Scan IP ranges around existing servers to find new servers
	async fn range_scan(&self) {
		loop {
			let start_time = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
				Ok(n) => n.as_secs(),
				Err(_) => panic!("system time before unix epoch!"),
			};

			// Get unique IP addresses from existing servers (using same query pattern as rescan method)
			let mut stream = sqlx::query(
				"SELECT (address - '0.0.0.0'::inet) AS address FROM servers ORDER BY last_seen ASC",
			)
			.fetch(&self.database.pool);

			let mut base_ips = HashSet::new();
			while let Some(result) = stream.next().await {
				match result {
					Ok(row) => {
						match row.try_get::<i64, _>("address") {
							Ok(address_int) => {
								let ip = Ipv4Addr::from_bits(address_int as u32);
								debug!("Found base IP: {}", ip);
								base_ips.insert(ip);
							}
							Err(e) => {
								error!("Error parsing address from row: {}", e);
							}
						}
					}
					Err(e) => {
						error!("Error fetching row from database: {}", e);
						break;
					}
				}
			}

			let base_ips: Vec<Ipv4Addr> = base_ips.into_iter().collect();

			info!("Starting range scan around {} base IPs", base_ips.len());

			// Generate ranges around each base IP
			let (tx, mut rx) = tokio::sync::mpsc::channel::<SocketAddrV4>(1000);
			let ports = self.config.scanner.port_range_start..=self.config.scanner.port_range_end;

			tokio::spawn(async move {
				for base_ip in base_ips {
					// Generate a /24 subnet around each base IP
					let base_octets = base_ip.octets();
					let subnet_base = [base_octets[0], base_octets[1], base_octets[2], 0];
					
					// Scan the entire /24 subnet (256 IPs)
					for last_octet in 0..=255u8 {
						let target_ip = Ipv4Addr::from([subnet_base[0], subnet_base[1], subnet_base[2], last_octet]);
						
						// Scan all configured ports for this IP
						for port in ports.clone() {
							let socket = SocketAddrV4::new(target_ip, port);
							if tx.send(socket).await.is_err() {
								return; // Receiver closed
							}
						}
					}
				}
			});

			let pb = ProgressBar::new_spinner();
			pb.set_style(
				ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {msg}")
					.unwrap_or(ProgressStyle::default_spinner()),
			);

			let mut scanned = 0u64;

			// Process the generated ranges
			while let Some(socket) = rx.recv().await {
				let permit = PERMITS.acquire().await.expect("semaphore closed");
				let database = self.database.clone();
				let pb_clone = pb.clone();

				tokio::spawn(async move {
					let _permit = permit;
					task_wrapper(socket, database).await;
					pb_clone.set_message(format!("Scanned: {}", socket));
				});

				scanned += 1;
				
				// Break if channel is closed (no more IPs to scan)
				if rx.is_closed() && rx.is_empty() {
					break;
				}
			}

			// Wait for all tasks to complete
			while PERMITS.available_permits() < 1000 {
				tokio::time::sleep(Duration::from_millis(100)).await;
			}

			pb.finish_with_message(format!("Range scan completed! Scanned {} addresses", scanned));

			let end_time = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
				Ok(n) => n.as_secs(),
				Err(_) => panic!("system time before unix epoch!"),
			};

			info!("Range scan completed in {} seconds", end_time - start_time);

			if !self.config.scanner.repeat {
				info!("Exiting");
				std::process::exit(0);
			}

			// Wait scan delay before starting a new scan
			if self.config.scanner.scan_delay > 0 {
				info!(
					"Waiting {} seconds before starting another range scan...",
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
					Some(address) => Ipv4Addr::from_str(address).unwrap(),
					None => continue,
				};

				let pool = self.database.clone();

				// Spawn a pinging task for each server found
				tokio::spawn(async move {
					let socket = SocketAddrV4::new(address, port);

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

impl Scanner {
	/// Updates country information for servers with "Unknown" country using geo lookup API
	async fn geo_update(&self) {
		info!("Starting geo update for servers with unknown countries...");
		
		loop {
			let start_time = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
				Ok(n) => n.as_secs(),
				Err(_) => panic!("system time before unix epoch!"),
			};

			// First, get a count of how many unknown servers we have
			let total_unknown: i64 = sqlx::query_scalar(
				"SELECT COUNT(DISTINCT (address - '0.0.0.0'::inet)) FROM servers WHERE country = 'Unknown' OR country IS NULL"
			)
			.fetch_one(&self.database.pool)
			.await
			.unwrap_or(0);

			if total_unknown == 0 {
				info!("No servers with unknown countries found. Geo update complete!");
				if !self.config.scanner.repeat {
					std::process::exit(0);
				}
				// Wait before checking again
				tokio::time::sleep(Duration::from_secs(300)).await; // Check every 5 minutes
				continue;
			}

			info!("Found {} unique IPs with unknown countries", total_unknown);

			// Create progress bar
			let style = ProgressStyle::with_template(
				"[{elapsed_precise}] [{bar:40.green/blue}] {pos}/{len} {msg} (ETA: {eta})"
			)
			.expect("failed to create progress bar style")
			.progress_chars("=>-");
			let bar = ProgressBar::new(total_unknown as u64).with_style(style);

			// Get all unknown IPs and group them by /24 subnet
			let mut stream = sqlx::query(
				"SELECT DISTINCT (address - '0.0.0.0'::inet) AS address 
				 FROM servers 
				 WHERE country = 'Unknown' OR country IS NULL 
				 ORDER BY address"
			)
			.fetch(&self.database.pool);

			// Group IPs by /24 subnet (first 3 octets)
			let mut subnet_groups: HashMap<(u8, u8, u8), Vec<(IpAddr, i64)>> = HashMap::new();

			while let Some(Ok(row)) = stream.next().await {
				let address_int: i64 = match row.try_get("address") {
					Ok(a) => a,
					Err(_) => continue,
				};
				
				let ip = Ipv4Addr::from_bits(address_int as u32);
				let octets = ip.octets();
				let subnet_key = (octets[0], octets[1], octets[2]);
				
				subnet_groups.entry(subnet_key)
					.or_insert_with(Vec::new)
					.push((IpAddr::V4(ip), address_int));
			}

			info!("Grouped {} IPs into {} /24 subnets", total_unknown, subnet_groups.len());
			bar.set_length(subnet_groups.len() as u64);

			let mut total_updated = 0;
			let semaphore = Arc::new(Semaphore::new(5)); // Allow 5 concurrent lookups
			let mut subnet_tasks = Vec::new();

			// Process each subnet
			for (subnet, ips_in_subnet) in subnet_groups {
				let permit = semaphore.clone().acquire_owned().await.unwrap();
				let database = self.database.clone();
				let bar_clone = bar.clone();
				
				subnet_tasks.push(tokio::spawn(async move {
					let _permit = permit; // Keep permit until task completes
					
					// Pick the first IP from the subnet for geolocation lookup
					let (representative_ip, _) = &ips_in_subnet[0];
					
					bar_clone.set_message(format!(
						"Looking up subnet {}.{}.{}.0/24 ({} IPs)", 
						subnet.0, subnet.1, subnet.2, ips_in_subnet.len()
					));
					
					// Retry logic for failed lookups
					let mut retries = 3;
					let mut lookup_result = None;
					
					while retries > 0 && lookup_result.is_none() {
						match database.geo_lookup.lookup_country(*representative_ip).await {
							Ok((country, _country_code)) if country != "Unknown" => {
								lookup_result = Some(country);
								break;
							}
							Ok(_) => {
								// Got "Unknown" result, might be rate limited or invalid IP
								retries -= 1;
								if retries > 0 {
									tokio::time::sleep(Duration::from_secs(2)).await;
								}
							}
							Err(e) => {
								warn!("Lookup failed for subnet {}.{}.{}.0/24 via {} (retries left: {}): {}", 
									subnet.0, subnet.1, subnet.2, representative_ip, retries - 1, e);
								retries -= 1;
								if retries > 0 {
									tokio::time::sleep(Duration::from_secs(2)).await;
								}
							}
						}
					}

					let mut subnet_updated_count = 0;
					if let Some(country) = lookup_result {
						// Update all IPs in this subnet
						let address_ints: Vec<i64> = ips_in_subnet.iter().map(|(_, addr_int)| *addr_int).collect();
						
						for address_int in address_ints {
							match sqlx::query(
								"UPDATE servers SET country = $1 WHERE (address - '0.0.0.0'::inet) = $2"
							)
							.bind(&country)
							.bind(address_int)
							.execute(&database.pool)
							.await {
								Ok(result) => {
									let rows_affected = result.rows_affected();
									if rows_affected > 0 {
										subnet_updated_count += rows_affected;
									}
								}
								Err(e) => {
									let ip = Ipv4Addr::from_bits(address_int as u32);
									error!("Failed to update country for {}: {}", ip, e);
								}
							}
						}
						
						if subnet_updated_count > 0 {
							info!("Updated {} servers in subnet {}.{}.{}.0/24 to country: {}", 
								subnet_updated_count, subnet.0, subnet.1, subnet.2, country);
						}
					} else {
						debug!("Failed to resolve country for subnet {}.{}.{}.0/24 after retries", 
							subnet.0, subnet.1, subnet.2);
					}

					bar_clone.inc(1);
					subnet_updated_count as usize
				}));

				// Process subnets in batches to avoid overwhelming the API
				if subnet_tasks.len() >= 20 {
					let tasks_to_process = std::mem::replace(&mut subnet_tasks, Vec::new());
					let results = futures_util::future::join_all(tasks_to_process).await;
					let batch_updated: usize = results.into_iter().filter_map(|r| r.ok()).sum();
					total_updated += batch_updated;
					
					// Small delay between subnet batches
					tokio::time::sleep(Duration::from_millis(500)).await;
				}
			}

			// Process any remaining subnet tasks
			if !subnet_tasks.is_empty() {
				let results = futures_util::future::join_all(subnet_tasks).await;
				let remaining_updated: usize = results.into_iter().filter_map(|r| r.ok()).sum();
				total_updated += remaining_updated;
			}

			bar.finish_with_message(format!("Completed! Updated {} IPs", total_updated));

			let end_time = match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
				Ok(n) => n.as_secs(),
				Err(_) => panic!("system time before unix epoch!"),
			};

			info!("Geo update completed in {} seconds. Successfully updated {} IPs out of {} total.", 
				  end_time - start_time, total_updated, total_unknown);

			if !self.config.scanner.repeat {
				info!("Exiting");
				std::process::exit(0);
			}

			// Wait before next full cycle - longer delay since we processed everything
			let delay = if total_updated > 0 { 
				self.config.scanner.scan_delay.max(1800) // At least 30 minutes if we updated something
			} else { 
				300 // 5 minutes if nothing was updated
			};
			
			info!("Waiting {} seconds before next geo update cycle...", delay);
			tokio::time::sleep(Duration::from_secs(delay)).await;
		}
	}
}
