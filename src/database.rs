use crate::response::Server;
use futures_core::stream::BoxStream;
use sqlx::postgres::PgRow;
use sqlx::{Error, PgConnection, Pool, Postgres, Row};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info};

pub async fn fetch_servers(pool: &Pool<Postgres>) -> BoxStream<Result<PgRow, Error>> {
	sqlx::query("SELECT address FROM servers ORDER BY lastseen DESC").fetch(pool)
}

pub async fn fetch_count(pool: &Pool<Postgres>) -> i64 {
	sqlx::query("SELECT COUNT(address) FROM servers")
		.fetch_one(pool)
		.await
		.unwrap()
		.get(0)
}

pub async fn update(
	server: Server,
	(address, port): &(String, u16),
	transaction: &mut PgConnection,
) -> Result<(), Error> {
	// Remove from database if server has opted out
	if server.check_opt_out() {
		sqlx::query("DELETE FROM servers WHERE address = $1")
			.bind(address)
			.execute(&mut *transaction)
			.await?;
		info!("Removing {address} from database due to opt-out");
		return Ok(());
	}

	let timestamp = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.expect("system time is before the unix epoch")
		.as_secs() as i32;

	// TODO: Can't be used in the database yet
	// let server_type = server.get_type();

	let description: String = match server.description {
		Some(description) => description.into(),
		None => {
			debug!("{address}: Missing MOTD");
			return Ok(());
		}
	};

	// Update server
	sqlx::query(
		"UPDATE servers SET
        version = $1,
        protocol = $2,
        icon = $3,
        motd = $4,
        preventsreports = $5,
        enforcesecure = $6,
        lastseen = $7,
        onlineplayers = $8,
        maxplayers = $9
        WHERE address = $10
        AND port = $11",
	)
	.bind(server.version.name)
	.bind(server.version.protocol)
	.bind(server.favicon)
	.bind(description)
	.bind(server.prevents_reports)
	.bind(server.enforces_secure_chat)
	.bind(timestamp)
	.bind(server.players.online)
	.bind(server.players.max)
	.bind(address)
	.bind(*port as i64)
	.execute(&mut *transaction)
	.await?;

	// Update players
	if let Some(sample) = server.players.sample {
		for player in sample {
			sqlx::query("INSERT INTO playerhistory (address, port, playeruuid, playername, firstseen, lastseen) VALUES ($1, $2, $3, $4, $5, $6)
                    ON CONFLICT (address, port, playeruuid) DO UPDATE SET
                    lastseen = EXCLUDED.lastseen,
                    playername = EXCLUDED.playername")
						.bind(address)
						.bind(*port as i64)
						.bind(player.id)
						.bind(player.name)
						.bind(timestamp)
						.bind(timestamp)
						.execute(&mut *transaction)
						.await?;
		}
	}

	// Update mods
	if let Some(mods_sample) = server.forge_data {
		for mods in mods_sample.mods {
			sqlx::query("INSERT INTO mods (address, port, modid, modmarker) VALUES ($1, $2, $3, $4) ON CONFLICT (address, port, modid) DO NOTHING")
				.bind(address)
				.bind(*port as i64)
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
