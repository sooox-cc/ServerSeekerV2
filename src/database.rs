use crate::colors::{RED, RESET};
use crate::response::Server;
use sqlx::{postgres::PgQueryResult, Error, PgPool, Pool, Postgres, Row};
use std::time::{SystemTime, UNIX_EPOCH};

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

pub async fn update(server: Server, conn: &PgPool) -> anyhow::Result<PgQueryResult> {
    let lastseen = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i32;

    let query = sqlx::query!("UPDATE servers SET \
        version = $1, \
        protocol = $2, \
        icon = $3, \
        motd = $4, \
        preventsreports = $5, \
        enforcesecure = $6, \
        lastseen = $7, \
        onlineplayers = $8, \
        maxplayers = $9 \
        WHERE address = $10 \
        AND port = $11", 
        server.version, 
        server.protocol, 
        server.icon,
        server.motd,
        server.prevents_reports,
        server.enforces_secure_chat,
        lastseen,
        server.online_players,
        server.max_players,
        server.address,
        server.port);

    Ok(query.execute(conn).await?)
}