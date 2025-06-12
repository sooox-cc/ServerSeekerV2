use crate::response::Server;
use crate::utils::RunError;
use sqlx::postgres::PgQueryResult;
use sqlx::types::ipnet::{IpNet, Ipv4Net};
use sqlx::types::Uuid;
use sqlx::{PgPool, Row};
use std::net::SocketAddrV4;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct Database(pub PgPool);

impl Database {
	pub fn new(pool: PgPool) -> Self {
		Self(pool)
	}

	/// Gets the count of servers from database
	pub async fn count_servers(&self) -> Result<i64, sqlx::Error> {
		let result = sqlx::query("SELECT COUNT(*) FROM servers")
			.fetch_one(&self.0)
			.await?
			.get("count");

		Ok(result)
	}

	/// Deletes a server from the database
	pub async fn delete_server(&self, address: IpNet) -> Result<PgQueryResult, sqlx::Error> {
		sqlx::query("DELETE FROM servers WHERE address = $1")
			.bind(address)
			.execute(&self.0)
			.await
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
		max_players) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
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
    	max_players = EXCLUDED.max_players",
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
		.execute(&self.0)
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
						.execute(&self.0)
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
					.execute(&self.0)
					.await?;
			}
		}

		Ok(())
	}
}
