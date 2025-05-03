use crate::response::Server;
use sqlx::postgres::PgRow;
use sqlx::types::ipnet::IpNet;
use sqlx::types::Uuid;
use sqlx::{PgConnection, Pool, Postgres};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

pub async fn fetch_servers(pool: &Pool<Postgres>) -> Vec<PgRow> {
	sqlx::query("SELECT address FROM servers ORDER BY last_seen ASC")
		.fetch_all(pool)
		.await
		.expect("failed to fetch servers")
}

pub async fn update(
	server: Server,
	(address, port): &(String, u16),
	transaction: &mut PgConnection,
) -> anyhow::Result<()> {
	let port = *port as i32;
	// SQLx requires each IP address to be in CIDR notation to add to Postgres
	let address = IpNet::from_str((address.to_owned() + "/32").as_str())?;
	let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i32;

	sqlx::query(
		"INSERT INTO servers (
		address,
		port,
		software,
        version,
		protocol,
		icon,
		description,
		prevents_chat_reports,
		enforces_secure_chat,
		first_seen,
		last_seen,
		online_players,
		max_players) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
    	ON CONFLICT (address, port) DO UPDATE SET
    	software = EXCLUDED.software,
    	version = EXCLUDED.version,
    	protocol = EXCLUDED.protocol,
    	icon = EXCLUDED.icon,
    	description = EXCLUDED.description,
    	prevents_chat_reports = EXCLUDED.prevents_chat_reports,
    	enforces_secure_chat = EXCLUDED.enforces_secure_chat,
    	last_seen = EXCLUDED.last_seen,
    	online_players = EXCLUDED.online_players,
    	max_players = EXCLUDED.max_players",
	)
	.bind(&address)
	.bind(port)
	.bind(server.get_type())
	.bind(server.version.name)
	.bind(server.version.protocol)
	.bind(server.favicon)
	.bind(server.description)
	.bind(server.prevents_reports)
	.bind(server.enforces_secure_chat)
	.bind(timestamp)
	.bind(timestamp)
	.bind(server.players.online)
	.bind(server.players.max)
	.execute(&mut *transaction)
	.await?;

	if let Some(sample) = server.players.sample {
		for player in sample {
			if let Ok(uuid) = Uuid::parse_str(&player.id) {
				sqlx::query("INSERT INTO players (address, port, uuid, name, first_seen, last_seen) VALUES ($1, $2, $3, $4, $5, $6)
	                ON CONFLICT (address, port, uuid) DO UPDATE SET
	                last_seen = EXCLUDED.last_seen")
					.bind(&address)
					.bind(port)
					.bind(uuid)
					.bind(player.name)
					.bind(timestamp)
					.bind(timestamp)
					.execute(&mut *transaction)
					.await?;
			}
		}
	}

	if let Some(mods_sample) = server.forge_data {
		for mods in mods_sample.mods {
			sqlx::query("INSERT INTO mods (address, port, id, mod_marker) VALUES ($1, $2, $3, $4) ON CONFLICT (address, port, id) DO NOTHING")
				.bind(&address)
				.bind(port)
				.bind(mods.id)
				.bind(mods.marker)
				.bind(timestamp)
				.bind(timestamp)
				.execute(&mut *transaction)
				.await?;
		}
	}

	Ok(())
}
