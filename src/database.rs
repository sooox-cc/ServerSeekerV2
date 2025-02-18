use std::time::{SystemTime, UNIX_EPOCH};
use sqlx::{Connection, Error, PgConnection, Row};
use sqlx::postgres::PgQueryResult;
use crate::response::Server;

pub async fn connect(database_url: &str) -> PgConnection {
    PgConnection::connect(&database_url).await.expect("Failed to connect to database!")
}

// TODO! Return a stream of results instead of a Vec for performance
pub async fn fetch_servers(conn: &mut PgConnection) -> Vec<String> {
    // Sort results by oldest
    sqlx::query("SELECT address FROM servers ORDER BY lastseen ASC")
        .fetch_all(conn)
        .await
        .unwrap()
        .iter()
        .map(|row| {
            row.try_get(0).unwrap()
        })
        .collect()
}

pub async fn update_server(server: Server, conn: &mut PgConnection, address: &str) -> Result<PgQueryResult, Error> {
    let lastseen = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(t) => t.as_secs() as i32,
        Err(_) => panic!("System clock set before unix epoch!")
    };

    let query = sqlx::query("UPDATE servers SET \
        version = $1, \
        protocol = $2, \
        icon = $3, \
        motd = $4, \
        preventsreports = $5, \
        enforcesecure = $6, \
        lastseen = $7 \
        WHERE address = $8");

    let query = query.bind(&server.version)
        .bind(server.protocol)
        .bind(server.icon)
        .bind(server.motd)
        .bind(server.prevents_reports)
        .bind(server.enforces_secure_chat)
        .bind(lastseen)
        .bind(address);

    query.execute(conn).await
}