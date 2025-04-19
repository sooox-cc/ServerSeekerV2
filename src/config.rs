use serde::Deserialize;
use std::cmp::max;
use std::fs::File;
use std::io::{ErrorKind, Read};
use tracing::error;

#[derive(Deserialize)]
pub struct Config {
	pub database: Database,
	pub rescanner: Rescanner,
}

#[derive(Deserialize)]
pub struct Database {
	pub url: String,
	pub port: u16,
	pub table: String,
	pub user: String,
	pub password: String,
}

#[derive(Deserialize)]
pub struct Rescanner {
	pub repeat: bool,
	pub rescan_delay: u64,
	pub port_range_start: u16,
	pub port_range_end: u16,
}

impl Default for Rescanner {
	fn default() -> Self {
		Self {
			repeat: true,
			rescan_delay: 60,
			port_range_start: 25560,
			port_range_end: 25570,
		}
	}
}

impl Rescanner {
	pub fn total_ports(&self) -> u16 {
		let start = self.port_range_start;
		let end = self.port_range_end;

		if start > end {
			error!("port_range_start cannot be greater than port_range_end!");
			std::process::exit(1);
		}

		max(1, end - start)
	}
}

pub fn load_config(path: String) -> Config {
	let mut file = match File::open(&path) {
		Ok(file) => file,
		Err(error) => match error.kind() {
			ErrorKind::NotFound => panic!("{} not found!", &path),
			ErrorKind::PermissionDenied => panic!("File permissions invalid for {}!", &path),
			_ => panic!("Unknown config error!"),
		},
	};

	let mut contents = String::new();
	file.read_to_string(&mut contents).unwrap_or_default();

	toml::from_str(&contents).expect("Failed to parse config!")
}
