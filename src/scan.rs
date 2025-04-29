use crate::config::Config;
use crate::utils::scan_results;
use crate::{database, utils};
use futures_util::future::join_all;
use indicatif::{ProgressBar, ProgressStyle};
use sqlx::{Pool, Postgres, Row};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tokio::task;
use tracing::{debug, info, warn};

pub async fn rescan_servers(pool: Pool<Postgres>, config: Config, style: ProgressStyle) {
	let port_start = config.scanner.port_range_start;
	let port_end = config.scanner.port_range_end;
	let total_ports = config.scanner.total_ports();

	if total_ports > 10 {
		warn!("Large amount of ports! Scans will take exponentially longer for each port to scan!");
	}

	if !config.scanner.repeat {
		warn!("Repeat is not enabled in config file! Will only scan once!");
	}

	info!("Scanning port range {port_start} - {port_end} ({total_ports} port(s) per host)");

	loop {
		let servers = database::fetch_servers(&pool).await;

		// Transactions allow adding multiple statements to a single query
		// This gets shared between tasks such that every server found
		// is added all at once at the end of the scan
		let transaction = Arc::new(Mutex::new(
			pool.begin().await.expect("failed to create transaction"),
		));

		let progress_bar =
			Arc::new(ProgressBar::new(servers.len() as u64).with_style(style.clone()));
		let mut handles = vec![];
		let scan_start = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("system time is before the unix epoch")
			.as_secs();

		for row in servers {
			let address = row.get::<String, _>(0);

			(port_start..=port_end).into_iter().for_each(|port| {
				handles.push(task::spawn(utils::run(
					(address.to_owned(), port),
					transaction.clone(),
					progress_bar.clone(),
				)));
			});
		}

		let results = join_all(handles)
			.await
			.into_iter()
			.filter_map(|r| r.ok())
			.collect::<Vec<_>>();

		// Print information about scan
		scan_results(results);

		debug!("Attempting to commit results to DB");
		Arc::try_unwrap(transaction)
			.unwrap()
			.into_inner()
			.commit()
			.await
			.expect("error while commiting to database");

		let scan_end = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("system time is before the unix epoch")
			.as_secs();

		info!("Scan took {} seconds", scan_end - scan_start);

		// Quit if only one scan is requested in config
		if !config.scanner.repeat {
			info!("Exiting");
			std::process::exit(0);
		}

		// Wait rescan delay before starting a new scan
		if config.scanner.scan_delay > 0 {
			info!(
				"Waiting {} seconds before starting another scan...",
				config.scanner.scan_delay
			);
			tokio::time::sleep(Duration::from_secs(config.scanner.scan_delay)).await;
		}
	}
}
