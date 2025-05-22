use crate::response::Server;
use crate::utils;
use futures_util::stream::BoxStream;
use futures_util::{future, FutureExt};
use indicatif::{ProgressBar, ProgressStyle};
use sqlx::postgres::{PgQueryResult, PgRow};
use sqlx::types::ipnet::IpNet;
use sqlx::types::Uuid;
use sqlx::{PgConnection, Pool, Postgres, Transaction};
use std::str::FromStr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;
use tracing::info;

/// Returns all servers from the database
pub async fn fetch_servers(pool: &Pool<Postgres>) -> BoxStream<Result<PgRow, sqlx::Error>> {
	sqlx::query("SELECT address, port FROM servers ORDER BY last_seen ASC").fetch(pool)
}

/// Deletes a server from the database
pub async fn delete_server(
	address: String,
	transaction: &mut PgConnection,
) -> Result<PgQueryResult, sqlx::Error> {
	sqlx::query("DELETE FROM servers WHERE address = $1")
		.bind(address)
		.execute(transaction)
		.await
}

/// Updates a single server in the database, this includes all mods
/// and players that come with it. Will also remove a server from the
/// database if it has requested to be removed
pub async fn update_server(
	server: Server,
	transaction: Arc<Mutex<Transaction<'_, Postgres>>>,
) -> anyhow::Result<()> {
	let conn = &mut **transaction.lock().await;

	// SQLx requires each IP address to be in CIDR notation to add to Postgres
	let address = IpNet::from_str(&(server.address.to_string() + "/32"))?;
	let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i32;

	// Handle server descriptions
	let description_raw = server
		.description_raw
		.as_ref()
		.ok_or(utils::RunError::MalformedResponse)?;
	let description_formatted = server.build_server_description(description_raw);

	if server.check_opt_out() {
		let modified_rows = delete_server(address.to_string(), conn)
			.await?
			.rows_affected();
		info!(
			"Removing {} from the database due to opt out! ({} rows modified)",
			address, modified_rows
		);

		return Err(utils::RunError::ServerOptOut)?;
	}

	// description_raw is for storing raw JSON descriptions
	// useful for applications that want to parse descriptions in their own way
	// description_formatted is for pre-formatted descriptions
	// useful for regex searches and for applications that just quickly need a servers description
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
	.bind(&address)
	.bind(server.port as i32)
	.bind(server.get_type())
	.bind(server.version.name)
	.bind(server.version.protocol)
	.bind(server.favicon)
	.bind(description_raw)
	.bind(description_formatted)
	.bind(server.prevents_reports)
	.bind(server.enforces_secure_chat)
	.bind(timestamp)
	.bind(timestamp)
	.bind(server.players.online)
	.bind(server.players.max)
	.execute(&mut *conn)
	.await?;

	if let Some(sample) = server.players.sample {
		for player in sample {
			if let Ok(uuid) = Uuid::parse_str(&player.id) {
				sqlx::query("INSERT INTO players (address, port, uuid, name, first_seen, last_seen) VALUES ($1, $2, $3, $4, $5, $6)
	                ON CONFLICT (address, port, uuid) DO UPDATE SET
	                last_seen = EXCLUDED.last_seen")
                    .bind(&address)
                    .bind(server.port as i32)
                    .bind(uuid)
                    .bind(player.name)
                    .bind(timestamp)
                    .bind(timestamp)
                    .execute(&mut *conn)
                    .await?;
			}
		}
	}

	if let Some(mods_sample) = server.forge_data {
		for mods in mods_sample.mods {
			sqlx::query("INSERT INTO mods (address, port, id, mod_marker) VALUES ($1, $2, $3, $4) ON CONFLICT (address, port, id) DO NOTHING")
                .bind(&address)
                .bind(server.port as i32)
                .bind(mods.id)
                .bind(mods.version)
                .bind(timestamp)
                .bind(timestamp)
                .execute(&mut *conn)
                .await?;
		}
	}

	Ok(())
}
