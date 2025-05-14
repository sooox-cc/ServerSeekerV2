use crate::database;
use crate::scanner::Scanner;
use futures_util::StreamExt;
use sqlx::Row;
use std::time::Duration;
use tracing::{info, warn};

pub async fn rescan_servers(scanner: Scanner) {
	let port_start = scanner.config.scanner.port_range_start;
	let port_end = scanner.config.scanner.port_range_end;
	let total_ports = scanner.config.scanner.total_ports();

	if total_ports > 10 {
		warn!("Large amount of ports! Scans will take exponentially longer for each port to scan!");
	}

	if !scanner.config.scanner.repeat {
		warn!("Repeat is not enabled in config file! Will only scan once!");
	}

	info!("Scanning port range {port_start} - {port_end} ({total_ports} port(s) per host)");

	loop {
		let mut servers = database::fetch_servers(&scanner.pool).await;
		// Count of all results will always be the first row
		let count = servers.next().await.unwrap().unwrap().get::<i32, _>(0);

		Scanner::scan_servers_from_stream(servers);

		// Quit if only one scan is requested in config
		if !scanner.config.scanner.repeat {
			info!("Exiting");
			std::process::exit(0);
		}

		// Wait rescan delay before starting a new scan
		if scanner.config.scanner.scan_delay > 0 {
			info!(
				"Waiting {} seconds before starting another scan...",
				scanner.config.scanner.scan_delay
			);
			tokio::time::sleep(Duration::from_secs(scanner.config.scanner.scan_delay)).await;
		}
	}
}
