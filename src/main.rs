mod colors;
mod config;
mod database;
mod ping;
mod response;

use crate::database::fetch_count;
use colors::{GREEN, RED, RESET, YELLOW};
use config::load_config;
use database::{connect, fetch_servers};
use futures_util::TryStreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use sqlx::{Pool, Postgres, Row};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{sync::Arc, time::Duration};
use thiserror::Error;
use tokio::task::JoinSet;

struct State {
	pool: Pool<Postgres>,
	progress_bar: ProgressBar,
}

#[tokio::main]
async fn main() {
	let config_file = std::env::args().nth(1).unwrap_or("config.toml".to_string());
	println!("{GREEN}[INFO] Using config file {}{RESET}", config_file);
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
		println!("{RED}[WARN] Large amount of ports! Scans will take exponentially longer for each port to scan!{RESET}");
	}

	if !config.rescanner.repeat {
		println!(
			"{YELLOW}[WARN] Repeat is not enabled in config file! Will only scan once!{RESET}"
		);
	}

	println!(
		"{GREEN}[INFO] Scanning port range {} - {} ({} port(s) per host){RESET}",
		port_start, port_end, total_ports
	);

	loop {
		let pool = connect(database_url.as_str()).await;
		let mut servers = fetch_servers(&pool).await;
		let length = fetch_count(&pool).await as u64;

		let style =
			ProgressStyle::with_template("[{elapsed}] [{bar:40.white/blue}] {pos:>7}/{len:7}")
				.unwrap()
				.progress_chars("=>-");

		// Create state to be passed to each task
		let state = Arc::new(State {
			pool: pool.clone(),
			progress_bar: ProgressBar::new(length).with_style(style),
		});

		let mut ping_set = JoinSet::new();

		let start = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("System time is before the unix epoch")
			.as_secs() as i64;

		// Spawn a new task for every result
		while let Some(row) = servers.try_next().await.unwrap() {
			let address: String = row.get(0);

			for port in port_start..=port_end {
				ping_set.spawn(run((address.to_owned(), port), state.clone()));
			}
		}

		let results = ping_set.join_all().await;
		let errors = results
			.into_iter()
			.filter_map(Result::err)
			.collect::<Vec<_>>();

		// Print scan errors, if any
		if !errors.is_empty() {
			println!(
				"{YELLOW}[INFO] Scan returned {} errors!{RESET}",
				errors.len()
			);
		}

		let end = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("System time is before the unix epoch")
			.as_secs() as i64;

		// Scan results
		println!("{GREEN}[INFO] Finished pinging all servers{RESET}");
		println!("{GREEN}[INFO] Scan took {} seconds{RESET}", end - start);

		// Quit if only one scan is requested in config
		if !config.rescanner.repeat {
			println!("{GREEN}[INFO] Exiting...{RESET}");
			std::process::exit(0);
		}

		// Wait rescan delay before starting a new scan
		if config.rescanner.rescan_delay > 0 {
			println!(
				"{GREEN}[INFO] Waiting {} seconds before starting another scan...{RESET}",
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
	#[error("Server opted out of scanning")]
	ServerOptOut,
}

async fn run(host: (String, u16), state: Arc<State>) -> Result<(), RunError> {
	let results = ping::ping_server(&host).await?;
	let response = response::parse_response(results)?;

	if response.check_opt_out() {
		return Err(RunError::ServerOptOut);
	}

	let _ = database::update(response, &state.pool, &host).await;
	state.progress_bar.inc(1);

	Ok(())
}
