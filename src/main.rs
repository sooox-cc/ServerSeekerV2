#![feature(let_chains)]
#![feature(string_from_utf8_lossy_owned)]

mod config;
mod country_tracking;
mod database;
mod geo_lookup;
mod protocol;
mod response;
mod scanner;
mod utils;

use crate::scanner::Scanner;
use clap::Parser;
use config::load_config;
use scanner::Mode;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::ConnectOptions;
use std::time::Duration;
use tracing::log::LevelFilter;
use tracing::{error, info};

#[derive(Parser, Debug)]
#[clap(about = "Scans the internet for minecraft servers and indexes them")]
#[clap(rename_all = "kebab-case")]
struct Args {
	#[clap(help = "Specifies the mode to run")]
	#[clap(default_value = "rescanner")]
	#[clap(long, short = 'm')]
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
		Ok(config) => config,
		Err(e) => {
			error!("Fatal error loading config file: {}", e);
			std::process::exit(1);
		}
	};

	info!("Using config file: {}", arguments.config_file);

	let options = PgConnectOptions::new()
		.username(&config.database.user)
		.password(&config.database.password)
		.host(&config.database.host)
		.port(config.database.port)
		.database(&config.database.table)
		// Turn off slow statement logging, this clogs the console
		.log_slow_statements(LevelFilter::Off, Duration::from_secs(60));

	let pool = PgPoolOptions::new()
		// Refresh connections every 24 hours
		.max_lifetime(Duration::from_secs(86400))
		.acquire_slow_threshold(Duration::from_secs(60))
		.connect_with(options)
		.await
		.ok();

	if let Some(pool) = &pool {
		if config.country_tracking.enabled {
			// Create tables
			if country_tracking::create_tables(pool).await.is_err() {
				error!("failed to create tables");
				std::process::exit(1);
			}

			// Spawn task to update database
			tokio::task::spawn(country_tracking::country_tracking(
				pool.clone(),
				config.clone(),
			));
		}
	} else {
		error!("Failed to connect to database");
		std::process::exit(1);
	}

	Scanner::new()
		.config(config)
		.mode(arguments.mode)
		.pool(pool)
		.build()
		.start()
		.await;
}
