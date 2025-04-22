mod config;
mod database;
mod ping;
mod response;
mod scan;

use crate::scan::scan_servers;
use config::load_config;
use sqlx::PgPool;
use tracing::error;

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

	match PgPool::connect(&database_url).await {
		Ok(pool) => scan_servers(pool, config).await,
		Err(e) => {
			error!("Failed to connect to database: {e}");
			std::process::exit(1);
		}
	};
}
