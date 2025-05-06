mod config;
mod database;
mod masscan;
mod ping;
mod response;
mod scan;
mod utils;

use clap::Parser;
use config::load_config;
use indicatif::ProgressStyle;
use sqlx::PgPool;
use tracing::{error, info};

#[derive(clap::ValueEnum, Clone, Debug)]
enum Mode {
	// TODO! Alternate mode: Run masscan, then rescan
	Discovery,
	Rescanner,
}

#[derive(Parser, Debug)]
#[clap(about = "Scans the internet for minecraft servers and indexes them")]
#[clap(rename_all = "kebab-case")]
struct Args {
	#[clap(help = "Specifies the mode to run (Default: discovery)")]
	#[clap(default_value = "rescanner")]
	#[clap(long, short = 'd')]
	mode: Mode,

	#[clap(help = "Specifies the location of the config file")]
	#[clap(default_value = "config.toml")]
	#[clap(long, short = 'c')]
	config_file: String,
}

#[tokio::main]
async fn main() {
	tracing_subscriber::fmt::init();

	let arguments = Args::parse();
	let config = match load_config(&arguments.config_file) {
		Ok(config) => {
			info!("Using config file: {}", arguments.config_file);
			config
		}
		Err(e) => {
			error!("Fatal error loading config file: ({})", e);
			std::process::exit(1);
		}
	};

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

	let style = ProgressStyle::with_template(
		"[{elapsed}] [{bar:40.white/blue}] {pos:>7}/{len:7} ETA {eta}",
	)
	.unwrap()
	.progress_chars("=>-");

	match arguments.mode {
		Mode::Discovery => masscan::start(pool, config, style).await,
		Mode::Rescanner => scan::rescan_servers(pool, config, style).await,
	}
}
