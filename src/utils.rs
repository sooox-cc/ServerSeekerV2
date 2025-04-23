use crate::scan::RunError;
use tracing::{info, warn};

pub fn scan_results(results: Vec<Result<(), RunError>>) {
	let results_len = results.len();

	// Save all errors for statistics
	let errors = results
		.into_iter()
		.filter_map(Result::err)
		.collect::<Vec<_>>();

	let errors_len = errors.len();

	// Print scan errors
	if !errors.is_empty() {
		warn!("Scan returned {} errors!", errors.len());
		let mut counts = [0u32; 4];
		for e in errors {
			let i: usize = e.into();
			counts[i] += 1;
		}

		warn!("{} errors while pinging servers", counts[0]);
		warn!("{} errors while parsing responses", counts[1]);
		warn!("{} errors while updating the database", counts[2]);
		warn!("{} connection timeouts", counts[3]);
	}

	info!("Commiting {} results to database", results_len - errors_len);
}
