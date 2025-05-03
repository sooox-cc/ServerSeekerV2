use crate::response::Server;
use sqlx::postgres::PgRow;
use sqlx::types::ipnet::IpNet;
use sqlx::types::Uuid;
use sqlx::{Error, PgConnection, Pool, Postgres};
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
) -> Result<(), Error> {
	let (address, port) = (
		address.parse::<IpNet>().expect("Failed to parse address"),
		*port as i32,
	);

	let timestamp = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.expect("system time is before the unix epoch")
		.as_secs() as i32;

	// Update server
	sqlx::query(
		"UPDATE servers SET
        version = $1,
        protocol = $2,
        icon = $3,
        description = $4,
        prevents_chat_reports = $5,
        enforces_secure_chat = $6,
        last_seen = $7,
        online_players = $8,
        max_players = $9
        WHERE address = $10
        AND port = $11",
	)
	.bind(address)
	.bind(port)
	.bind(server.version.name)
	.bind(server.version.protocol)
	.bind(server.favicon)
	.bind(server.description)
	.bind(server.prevents_reports)
	.bind(server.enforces_secure_chat)
	.bind(timestamp)
	.bind(server.players.online)
	.bind(server.players.max)
	.execute(&mut *transaction)
	.await?;

	// Update players
	if let Some(sample) = server.players.sample {
		for player in sample {
			if let Ok(uuid) = Uuid::parse_str(&player.id) {
				sqlx::query("INSERT INTO players (address, port, uuid, name, first_seen, last_seen) VALUES ($1, $2, $3, $4, $5, $6)
                    ON CONFLICT (address, port, uuid) DO UPDATE SET
                    last_seen = EXCLUDED.last_seen")
					.bind(address)
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

	// Update mods
	if let Some(mods_sample) = server.forge_data {
		for mods in mods_sample.mods {
			sqlx::query("INSERT INTO mods (address, port, id, mod_marker) VALUES ($1, $2, $3, $4) ON CONFLICT (address, port, id) DO NOTHING")
				.bind(address)
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
