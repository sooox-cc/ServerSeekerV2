use crate::response::Server;
use futures_core::stream::BoxStream;
use sqlx::postgres::{PgConnectOptions, PgQueryResult, PgRow};
use sqlx::{Error, PgPool, Pool, Postgres, Row};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::error;

pub async fn connect(url: &str) -> Pool<Postgres> {
	let options = PgConnectOptions::new()
		.host(url)
		.application_name("ServerSeekerV2-Rust");

	match sqlx::postgres::PgPoolOptions::new()
		.max_connections(50)
		.connect_with(options)
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
	sqlx::query("SELECT address FROM servers ORDER BY lastseen DESC").fetch(pool)
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
		.expect("system time is before the unix epoch")
		.as_secs() as i32;

	let (address, port) = host;
	// TODO: Can't be used in the database yet
	// let server_type = server.get_type();

	let players = server
		.players
		.ok_or(anyhow::Error::msg("Players object is missing!"))?;
	let forge_data = server
		.forge_data
		.ok_or(anyhow::Error::msg("ForgeData object is missing!"))?;

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
	.bind(server.description)
	.bind(server.prevents_reports)
	.bind(server.enforces_secure_chat)
	.bind(timestamp)
	.bind(players.online)
	.bind(players.max)
	.bind(address)
	.bind(*port as i64)
	.execute(&mut *transaction)
	.await?;

	// Update players
	for player in players
		.sample
		.ok_or(anyhow::Error::msg("Players sample is missing!"))?
	{
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

	// Update mods
	for mods in forge_data.mods {
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
