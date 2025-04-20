use crate::response::Server;
use futures_core::stream::BoxStream;
use sqlx::postgres::{PgQueryResult, PgRow};
use sqlx::{Error, PgPool, Pool, Postgres, Row};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::error;

pub async fn connect(url: &str) -> Pool<Postgres> {
	match sqlx::postgres::PgPoolOptions::new()
		.max_connections(25)
		.connect(url)
		.await
	{
		Ok(pool) => pool,
		Err(e) => {
			error!("Unable to connect to database: {e}");
			std::process::exit(1);
		}
	}
}

pub async fn fetch_servers(pool: &Pool<Postgres>) -> BoxStream<Result<PgRow, Error>> {
	sqlx::query("SELECT address FROM servers ORDER BY lastseen DESC LIMIT 100").fetch(pool)
}

pub async fn fetch_count(pool: &Pool<Postgres>) -> i64 {
	sqlx::query("SELECT COUNT(address) FROM servers")
		.fetch_one(pool)
		.await
		.unwrap()
		.get(0)
}

pub async fn update(server: Server, conn: &PgPool, host: &(String, u16)) -> anyhow::Result<()> {
	let mut transaction = conn.begin().await?;
	let timestamp = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.expect("System time is before the unix epoch")
		.as_secs() as i32;

	let (address, port) = host;
	// TODO: Can't be used in the database yet
	// let server_type = server.get_type();

	let description: String = server
		.description
		.ok_or(anyhow::Error::msg("MOTD is missing!"))?
		.into();

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

	transaction
		.commit()
		.await
		.map_err(|_| anyhow::Error::msg("Failed to commit transaction!"))
}

pub async fn remove_server(address: String, conn: &PgPool) -> Result<PgQueryResult, Error> {
	sqlx::query("DELETE FROM servers WHERE address = $1")
		.bind(address)
		.execute(conn)
		.await
}
