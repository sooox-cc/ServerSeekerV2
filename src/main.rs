mod config;
mod database;
mod masscan;
mod ping;
mod response;
mod scan;

use config::load_config;
use indicatif::ProgressStyle;
use sqlx::PgPool;
use std::time::Duration;
use tracing::error;

enum Mode {
	Discovery,
	Rescan,
}

// TODO: add to config file
const TIMEOUT_SECS: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() {
	tracing_subscriber::fmt::init();

	let config_file = std::env::args().nth(1).unwrap_or("config.toml".to_string());
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

	let pool = match PgPool::connect(&database_url).await {
		Ok(pool) => pool,
		Err(e) => {
			error!("Failed to connect to database: {e}");
			std::process::exit(1);
		}
	};

	let style = ProgressStyle::with_template("[{elapsed}] [{bar:40.white/blue}] {pos:>7}/{len:7}")
		.unwrap()
		.progress_chars("=>-");

	let mode = Mode::Rescan;

	match mode {
		Mode::Discovery => masscan::start(pool, config, style).await,
		Mode::Rescan => scan::rescan_servers(pool, config, style).await,
	}
}
