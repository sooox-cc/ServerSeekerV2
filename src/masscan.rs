use crate::config::Config;
use crate::scan;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use sqlx::{Pool, Postgres};
use std::process::Command;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tracing::info;

#[derive(Deserialize)]
pub struct Masscan {
	pub ip: String,
	pub ports: Vec<Port>,
}

#[derive(Deserialize)]
pub struct Port {
	port: u16,
}

pub fn start_masscan(masscan_config: String) {
	info!("Starting masscan...");
	if cfg!(target_os = "windows") {
		Command::new("cmd.exe")
			.arg("/c")
			.arg(format!("masscan -c {masscan_config}"))
			.spawn()
			.expect("failed to execute process")
	} else {
		Command::new("/bin/sh")
			.arg("-c")
			// Masscan needs to be run as root, nothing I can do about it
			// TODO! Change this to be a toggle in the config for security
			.arg(format!("sudo masscan -c {masscan_config}"))
			.spawn()
			.expect("failed to execute process")
	}
	.wait()
	.expect("failed to wait on child");

	info!("Masscan completed");
}

pub fn parse_output(masscan_output: String) -> Vec<(String, u16)> {
	let file = std::fs::read_to_string(masscan_output).expect("failed to read masscan");
	let output = serde_json::from_str::<Vec<Masscan>>(&file).expect("failed to read masscan");

	output
		.into_iter()
		.flat_map(|entry| {
			let ip = entry.ip;
			entry.ports.into_iter().map(move |p| (ip.clone(), p.port))
		})
		.collect()
}

pub async fn start(pool: Pool<Postgres>, config: Config, style: ProgressStyle) {
	let transaction = Arc::new(Mutex::new(
		pool.begin().await.expect("failed to create transaction"),
	));

	loop {
		// TODO: Config changes needed here
		start_masscan("masscan.conf".to_string());
		let servers = parse_output("minecraft-servers.json".to_string());
		info!("{} servers found", servers.len());

		let progress_bar =
			Arc::new(ProgressBar::new(servers.len() as u64).with_style(style.clone()));

		let mut join_set = JoinSet::new();

		servers.into_iter().for_each(|(ip, port)| {
			join_set.spawn(scan::run(
				(ip, port),
				transaction.clone(),
				progress_bar.clone(),
			));
		});

		join_set.join_all().await;

		// TODO: Delay here
	}
}
