use crate::response::Server;
use sqlx::{postgres::PgQueryResult, Connection, Error, Executor, PgPool, Pool, Postgres, Row};
use std::time::{SystemTime, UNIX_EPOCH};
use crate::colors::{RED, RESET};

pub async fn connect(database_url: &str) -> Pool<Postgres> {
    match PgPool::connect(&database_url).await {
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
        .iter()
        .map(|row| {
            row.try_get(0)
        }).collect()
}

pub async fn update_server(server: Server, pool: &PgPool, address: &str) -> Result<PgQueryResult, Error> {
    let lastseen = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(t) => t.as_secs(),
        Err(_) => panic!("{RED}System clock set before unix epoch!{RESET}")
    };

    let query = sqlx::query("UPDATE servers SET \
        version = $1, \
        protocol = $2, \
        icon = $3, \
        motd = $4, \
        preventsreports = $5, \
        enforcesecure = $6, \
        lastseen = $7, \
        onlineplayers = $8, \
        maxplayers = $9 \
        WHERE address = $10")
        .bind(&server.version)
        .bind(server.protocol)
        .bind(server.icon)
        .bind(server.motd)
        .bind(server.prevents_reports)
        .bind(server.enforces_secure_chat)
        .bind(lastseen as i32)
        .bind(server.online_players)
        .bind(server.max_players)
        .bind(address);

    query.execute(pool).await
}