mod colors;
mod config;
mod database;
mod ping;
mod response;

use colors::{GREEN, RED, RESET, YELLOW};
use config::load_config;
use database::{connect, fetch_servers};
use indicatif::{ProgressBar, ProgressStyle};
use sqlx::{Pool, Postgres};
use std::rc::Rc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Semaphore;

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

		let servers = match fetch_servers(&pool).await {
			Ok(servers) => {
				println!(
					"{GREEN}[INFO] Found {} servers to rescan!{RESET}",
					servers.len()
				);
				servers
			}
			Err(_) => {
				println!("{RED}[ERROR] Failed to fetch servers! Waiting 30 seconds and retrying...{RESET}");
				tokio::time::sleep(Duration::from_secs(30)).await;
				continue;
			}
		};

		let style =
			ProgressStyle::with_template("[{elapsed}] [{bar:40.white/blue}] {pos:>7}/{len:7}")
				.unwrap()
				.progress_chars("=>-");
		let progress_bar = ProgressBar::new(servers.len() as u64).with_style(style);

		let state = Rc::new(State {
			// Pool isn't used anywhere else except for inside the futures so it's safe to move the value
			pool,
			progress_bar,
		});

		let servers = servers
			.iter()
			.flat_map(|ip| {
				(port_start..=port_end)
					.map(|port| run((ip.to_owned(), port), Rc::clone(&state)))
					.collect::<Vec<_>>()
			})
			.collect::<Vec<_>>();

		let results = futures::future::join_all(servers).await;

		let errors = results
			.into_iter()
			.filter_map(Result::err)
			.collect::<Vec<_>>();

		if !errors.is_empty() {
			println!(
				"{YELLOW}[INFO] Scan returned {} errors!{RESET}",
				errors.len()
			);
		}

		println!("{GREEN}[INFO] Finished pinging all servers{RESET}");

		if !config.rescanner.repeat {
			println!("{GREEN}[INFO] Exiting...{RESET}");
			std::process::exit(0);
		}

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
	#[error("error while pinging server")]
	PingServer(#[from] ping::PingServerError),
	#[error("error while parsing response")]
	ParseResponse(#[from] serde_json::Error),
	#[error("error while updating database")]
	DatabaseUpdate(#[from] sqlx::Error),
}

static PING_PERMITS: Semaphore = Semaphore::const_new(1000);

async fn run(host: (String, u16), state: Rc<State>) -> Result<(), RunError> {
	let _permit = PING_PERMITS.acquire().await.unwrap();

	let results = ping::ping_server(&host).await?;
	let response = response::parse_response(results, &host)?;
	database::update(response, &state.pool).await?;
	state.progress_bar.inc(1);

	Ok(())
}
