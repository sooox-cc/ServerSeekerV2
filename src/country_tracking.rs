use crate::config::Config;
use anyhow::bail;
use flate2::read::GzDecoder;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::redirect::Policy;
use std::fs::File;
use std::io::{Read, Write};
use std::time::Duration;
use tracing::info;

const DOWNLOAD_URL: &str = "https://ipinfo.io/data/ipinfo_lite.json.gz";

pub async fn download_database(config: &Config) -> anyhow::Result<File> {
	let client = reqwest::ClientBuilder::new()
		.gzip(true)
		.redirect(Policy::limited(3))
		.connect_timeout(Duration::from_secs(10))
		.build()?;

	let url = format!(
		"{}{}{}",
		DOWNLOAD_URL, "?token=", config.country_tracking.ipinfo_token
	);

	// Send request
	let response = client.get(url).send().await?;

	// If response is OK write to file and unzip
	if response.status().is_success() {
		// Content length header is required for the progress bar
		// IPInfo should always supply this
		let content_length = match response.content_length() {
			Some(len) => len,
			None => bail!("Content-Length was not set!"),
		};

		let mut downloaded: u64 = 0;
		let mut output_file = File::create("ipinfo.json.gz")?;
		let mut reader = response.bytes_stream();

		let style = ProgressStyle::with_template(
			"[{elapsed_precise}] [{bar:40.white/blue}] {bytes}/{total_bytes}",
		)
		.expect("failed to create progress bar style")
		.progress_chars("=>-");

		let bar = ProgressBar::new(content_length).with_style(style);

		while let Some(Ok(chunk)) = reader.next().await {
			output_file.write_all(&chunk)?;

			// Update download bar position
			let new = std::cmp::min(downloaded + (chunk.len() as u64), content_length);
			downloaded = new;
			bar.set_position(new);
		}

		// Done
		bar.finish_with_message("Finished downloading IPInfo database to ipinfo.json.gz");
		info!("Decompressing output file...");

		// Decompress file
		let mut decoder = GzDecoder::new(File::open("ipinfo.json.gz")?);
		let mut file = File::create("ipinfo.json")?;
		let mut string = String::new();

		// Write to output file
		decoder.read_to_string(&mut string)?;
		let mut output_file = File::create("ipinfo.json")?;
		output_file.write_all(string.as_bytes())?;
		file.flush()?;

		// Delete compressed file
		std::fs::remove_file("ipinfo.json.gz")?;

		Ok(file)
	} else {
		bail!(
			"IPInfo download failed: {} {:?}",
			response.status(),
			response.status().canonical_reason()
		)
	}
}
