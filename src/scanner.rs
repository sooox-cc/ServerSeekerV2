use crate::config::Config;
use crate::{masscan, rescan_servers};
use futures_util::Stream;
use sqlx::{Pool, Postgres};
use std::fmt::Debug;
use tracing::error;

#[derive(clap::ValueEnum, Clone, Debug, Default)]
pub enum Mode {
	#[default]
	Discovery,
	Rescanner,
}

#[derive(Debug)]
pub struct Scanner {
	pub config: Config,
	pub mode: Mode,
	pub pool: Pool<Postgres>,
}

impl Scanner {
	pub fn new() -> ScanBuilder {
		ScanBuilder::default()
	}

	pub async fn start(self) {
		match self.mode {
			Mode::Discovery => masscan::start(self).await,
			Mode::Rescanner => rescan_servers::rescan_servers(self).await,
		}
	}

	pub fn scan_servers_from_stream<T: Stream + Unpin>(_servers: T) {}

	pub fn scan_servers_from_iterator<T: IntoIterator>(_servers: T) {}
}

#[derive(Debug, Default)]
pub struct ScanBuilder {
	config: Config,
	mode: Mode,
	pool: Option<Pool<Postgres>>,
}

impl ScanBuilder {
	pub fn config(mut self, config: Config) -> ScanBuilder {
		self.config = config;
		self
	}

	pub fn pool(mut self, pool: Option<Pool<Postgres>>) -> ScanBuilder {
		self.pool = pool;
		self
	}

	pub fn mode(mut self, mode: Mode) -> ScanBuilder {
		self.mode = mode;
		self
	}

	pub fn build(self) -> Scanner {
		Scanner {
			config: self.config,
			mode: self.mode,
			pool: {
				match self.pool {
					Some(pool) => pool,
					None => {
						error!("Failed to connect to database!");
						std::process::exit(1);
					}
				}
			},
		}
	}
}
