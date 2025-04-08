use crate::colors::{RED, RESET};
use crate::response::Server;
use sqlx::{postgres::PgQueryResult, Error, PgPool, Pool, Postgres, Row};
use std::time::{SystemTime, UNIX_EPOCH};
use futures::future;

pub async fn connect(url: &str) -> Pool<Postgres> {
    match PgPool::connect(url).await {
        Ok(pool) => pool,
        Err(e) => panic!("{RED}Unable to connect to database: {e}{RESET}"),
    }
}

// TODO! Return a stream of results instead of a Vec for performance
pub async fn fetch_servers(pool: &PgPool) -> Result<Vec<String>, Error> {
    // Sort results by oldest
    sqlx::query("SELECT address FROM servers ORDER BY lastseen DESC")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.try_get(0) )
        .collect()
}

pub async fn update(server: Server, conn: &PgPool) -> anyhow::Result<()> {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i32;
    let mut transaction = conn.begin().await?;
    let address: &str = server.address.as_str();

    sqlx::query("UPDATE servers SET
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
        AND port = $11")
        .bind(server.version)
        .bind(server.protocol,)
        .bind(server.icon)
        .bind(server.motd)
        .bind(server.prevents_reports)
        .bind(server.enforces_secure_chat)
        .bind(timestamp)
        .bind(server.online_players)
        .bind(server.max_players)
        .bind(address)
        .bind(server.port)
        .execute(&mut *transaction)
        .await?;

    // Upsert players
    for player in server.players {
        sqlx::query("INSERT INTO playerhistory (address, port, playeruuid, playername, firstseen, lastseen) VALUES ($1, $2, $3, $4, $5, $6)
                    ON CONFLICT (address, port, playeruuid) DO UPDATE SET
                    lastseen = EXCLUDED.lastseen,
                    playername = EXCLUDED.playername")
            .bind(address)
            .bind(server.port)
            .bind(player.uuid)
            .bind(player.username)
            .bind(timestamp)
            .bind(timestamp)
            .execute(&mut *transaction)
            .await?;
    };

    // Update mods
    for mods in server.mods {
        sqlx::query("INSERT INTO mods (address, port, modid, modmarker) VALUES ($1, $2, $3, $4) ON CONFLICT (address, port, modid) DO NOTHING")
            .bind(address)
            .bind(server.port)
            .bind(mods.mod_id)
            .bind(mods.mod_name)
            .bind(timestamp)
            .bind(timestamp)
            .execute(&mut *transaction)
            .await?;
    };
    
    Ok(transaction.commit().await?)
}