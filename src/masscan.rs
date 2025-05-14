use crate::scanner::Scanner;
use std::fs::File;
use std::io::BufRead;
use std::time::Duration;
use tokio::process::Command;
use tracing::{info, warn};

pub async fn start(scanner: Scanner) {
	let masscan_config = &scanner.config.masscan.config_file;
	let masscan_output = &scanner.config.masscan.output_file;

	if !scanner.config.scanner.repeat {
		warn!("Repeat is not enabled in config file! Will only scan once!");
	}

	loop {
		let stdout = File::create(masscan_output).unwrap();

		Command::new("sudo")
			.args(["masscan", "-c", masscan_config])
			.stdout(stdout.try_clone().unwrap())
			.spawn()
			.unwrap()
			.wait()
			.await
			.unwrap();

		let reader = std::io::BufReader::new(&stdout).lines();
		Scanner::scan_servers_from_iterator(reader);

		// Quit if only one scan is requested in config
		if !scanner.config.scanner.repeat {
			info!("Exiting");
			std::process::exit(0);
		}

		// Wait rescan delay before starting a new scan
		if scanner.config.scanner.scan_delay > 0 {
			info!(
				"Waiting {} seconds before starting another scan",
				scanner.config.scanner.scan_delay
			);
			tokio::time::sleep(Duration::from_secs(scanner.config.scanner.scan_delay)).await;
		}
	}
}
