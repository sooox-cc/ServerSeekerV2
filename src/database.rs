use sqlx::{Connection, Error, PgConnection, Postgres, QueryBuilder, Row};
use sqlx::postgres::PgQueryResult;
use crate::response::Server;

pub async fn connect(database_url: &str) -> PgConnection {
    PgConnection::connect(&database_url).await.unwrap()
}

// TODO! Return a stream of results instead of a Vec for performance
pub async fn fetch_servers(conn: &mut PgConnection) -> Vec<String> {
    // Sort results by
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

pub async fn update_server(server: &Server, conn: &mut PgConnection, address: &str) -> Result<PgQueryResult, Error> {
    let mut query: QueryBuilder<Postgres> = QueryBuilder::new("UPDATE servers SET");

    query.push(" version = ");
    query.push_bind(&server.version);

    query.push(", protocol = ");
    query.push_bind(server.protocol);

    query.push(", icon = ");
    query.push_bind(&server.icon);

    query.push(", motd = ");
    query.push_bind(&server.motd);

    query.push(", preventsreports = ");
    query.push_bind(server.prevents_reports);

    query.push(", enforcesecure = ");
    query.push_bind(server.enforces_secure_chat);

    query.push(" WHERE address = ");
    query.push_bind(address);

    let mut query = query.build();
    query.execute(conn).await
}