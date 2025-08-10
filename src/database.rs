use crate::geo_lookup::GeoLookup;
use crate::response::Server;
use crate::utils::RunError;
use sqlx::postgres::{PgQueryResult, PgRow};
use sqlx::types::ipnet::{IpNet, Ipv4Net};
use sqlx::types::Uuid;
use sqlx::{FromRow, PgPool, Row};
use std::net::{IpAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
struct AddressInfo {
	country: String,
	asn: String,
}

impl FromRow<'_, PgRow> for AddressInfo {
	fn from_row(row: &PgRow) -> Result<Self, sqlx::Error> {
		Ok(Self {
			country: row.try_get("country")?,
			asn: row.try_get("asn")?,
		})
	}
}

#[derive(Debug, Clone)]
pub struct Database {
	pub pool: PgPool,
	pub geo_lookup: Arc<GeoLookup>,
}

impl Database {
	pub fn new(pool: PgPool) -> Self {
		Self {
			pool,
			geo_lookup: Arc::new(GeoLookup::new()),
		}
	}

	/// Gets the count of servers from database
	pub async fn count_servers(&self) -> Result<i64, sqlx::Error> {
		let result = sqlx::query("SELECT COUNT(*) FROM servers")
			.fetch_one(&self.pool)
			.await?
			.get("count");

		Ok(result)
	}

	/// Deletes a server from the database
	async fn delete_server(&self, address: IpNet) -> Result<PgQueryResult, sqlx::Error> {
		sqlx::query("DELETE FROM servers WHERE address = $1")
			.bind(address)
			.execute(&self.pool)
			.await
	}

	async fn get_country_info(&self, address: &IpNet) -> Result<AddressInfo, sqlx::Error> {
		// First try the local countries database
		let result = sqlx::query_as("SELECT country, asn FROM countries WHERE $1 <<= network")
			.bind(address)
			.fetch_one(&self.pool)
			.await;

		if let Ok(info) = result {
			return Ok(info);
		}

		// If not found in local database, return "Unknown" to avoid rate limiting
		// The geo lookup could be done in a background task later if needed
		Ok(AddressInfo {
			country: "Unknown".to_string(),
			asn: "Unknown".to_string(),
		})
	}

	/// Updates a single server in the database, this includes all mods
	/// and players that come with it. Will also remove a server from the
	/// database if it has requested to be removed
	pub async fn update_server(&self, server: Server, socket: SocketAddrV4) -> anyhow::Result<()> {
		let address = IpNet::from(Ipv4Net::from(*socket.ip()));
		let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i32;

		// Format description if it exists
		let formatted = server
			.description_raw
			.as_ref()
			.map(|v| server.build_formatted_description(v));

		// Delete server if it's opted out
		if server.check_opt_out() {
			self.delete_server(address).await?;
			return Err(RunError::ServerOptOut)?;
		}

		let address_information = self.get_country_info(&address).await?;

		sqlx::query(
			"INSERT INTO servers (
		address,
		port,
		software,
        version,
		protocol,
		icon,
		description_raw,
        description_formatted,
		prevents_chat_reports,
		enforces_secure_chat,
		first_seen,
		last_seen,
		online_players,
		max_players,
        country,
    	asn) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
    	ON CONFLICT (address, port) DO UPDATE SET
    	software = EXCLUDED.software,
    	version = EXCLUDED.version,
    	protocol = EXCLUDED.protocol,
    	icon = EXCLUDED.icon,
    	description_raw = EXCLUDED.description_raw,
    	description_formatted = EXCLUDED.description_formatted,
    	prevents_chat_reports = EXCLUDED.prevents_chat_reports,
    	enforces_secure_chat = EXCLUDED.enforces_secure_chat,
    	last_seen = EXCLUDED.last_seen,
    	online_players = EXCLUDED.online_players,
    	max_players = EXCLUDED.max_players,
    	country = EXCLUDED.country,
    	asn = EXCLUDED.asn",
		)
		.bind(address)
		.bind(socket.port() as i32)
		.bind(server.get_type())
		.bind(server.version.name)
		.bind(server.version.protocol)
		.bind(server.favicon)
		// description_raw is for storing raw JSON descriptions
		// useful for applications that want to parse descriptions in their own way
		.bind(server.description_raw)
		// description_formatted is for pre-formatted descriptions
		// useful for regex searches and for applications that just quickly need a servers description
		.bind(formatted)
		.bind(server.prevents_reports)
		.bind(server.enforces_secure_chat)
		.bind(timestamp)
		.bind(timestamp)
		.bind(server.players.online)
		.bind(server.players.max)
		.bind(address_information.country)
		.bind(address_information.asn)
		.execute(&self.pool)
		.await?;

		if let Some(sample) = server.players.sample {
			for player in sample {
				if let Ok(uuid) = Uuid::parse_str(&player.id) {
					sqlx::query("INSERT INTO players (address, port, uuid, name, first_seen, last_seen) VALUES ($1, $2, $3, $4, $5, $6)
	                ON CONFLICT (address, port, uuid) DO UPDATE SET
	                last_seen = EXCLUDED.last_seen")
						.bind(address)
						.bind(socket.port() as i32)
						.bind(uuid)
						.bind(player.name)
						.bind(timestamp)
						.bind(timestamp)
						.execute(&self.pool)
						.await?;
				}
			}
		}

		if let Some(mods_sample) = server.forge_data {
			for mods in mods_sample.mods {
				sqlx::query("INSERT INTO mods (address, port, id, mod_marker) VALUES ($1, $2, $3, $4) ON CONFLICT (address, port, id) DO NOTHING")
					.bind(address)
					.bind(socket.port() as i32)
					.bind(mods.id)
					.bind(mods.version)
					.bind(timestamp)
					.bind(timestamp)
					.execute(&self.pool)
					.await?;
			}
		}

		Ok(())
	}
}
