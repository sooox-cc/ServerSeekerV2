use crate::config::Config;
use crate::utils;
use crate::utils::scan_results;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use sqlx::{Pool, Postgres};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tracing::{error, info, warn};

#[derive(Deserialize)]
pub struct Masscan {
	pub ip: String,
	pub ports: Vec<Port>,
}

#[derive(Deserialize)]
pub struct Port {
	port: u16,
}

pub fn start_masscan(masscan_config: &str) {
	info!("Starting masscan...");
	if cfg!(target_os = "windows") {
		Command::new("cmd.exe")
			.args(["masscan", "-c", masscan_config])
			.spawn()
			.expect("failed to execute process")
	} else {
		Command::new("sudo")
			.args(["masscan", "-c", masscan_config])
			.spawn()
			.expect("failed to execute process")
	}
	.wait()
	.expect("failed to wait on child");

	info!("Masscan completed");
}

pub fn parse_output(masscan_output: &str) -> anyhow::Result<Vec<(String, u16)>> {
	let file = std::fs::read_to_string(&masscan_output)?;
	let output = serde_json::from_str::<Vec<Masscan>>(&file)?;

	Ok(output
		.into_iter()
		.flat_map(|entry| {
			entry
				.ports
				.into_iter()
				.map(move |p| (entry.ip.clone(), p.port))
		})
		.collect())
}

pub async fn start(pool: Pool<Postgres>, config: Config, style: ProgressStyle) {
	let masscan_config = config.masscan.config_file.as_str();
	let masscan_output = config.masscan.output_file.as_str();

	let transaction = Arc::new(Mutex::new(
		pool.begin().await.expect("failed to create transaction"),
	));

	if !config.scanner.repeat {
		warn!("Repeat is not enabled in config file! Will only scan once!");
	}

	loop {
		start_masscan(masscan_config);
		let servers = match parse_output(masscan_output) {
			Ok(servers) => servers,
			Err(e) => {
				error!("Failed to parse masscan output: {e:?}");
				std::process::exit(1);
			}
		};

		info!("{} servers found", servers.len());

		let progress_bar =
			Arc::new(ProgressBar::new(servers.len() as u64).with_style(style.clone()));

		let mut join_set = JoinSet::new();

		servers.into_iter().for_each(|(ip, port)| {
			join_set.spawn(utils::run(
				(ip, port),
				transaction.clone(),
				progress_bar.clone(),
			));
		});

		let results = join_set.join_all().await;

		// Print information about scan
		scan_results(results);

		// Quit if only one scan is requested in config
		if !config.scanner.repeat {
			info!("Exiting");
			std::process::exit(0);
		}

		// Wait rescan delay before starting a new scan
		if config.scanner.scan_delay > 0 {
			info!(
				"Waiting {} seconds before starting another scan",
				config.scanner.scan_delay
			);
			tokio::time::sleep(Duration::from_secs(config.scanner.scan_delay)).await;
		}
	}
}
