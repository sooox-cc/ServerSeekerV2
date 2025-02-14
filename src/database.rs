use crate::response::Server;
use diesel::{Connection, PgConnection};
use diesel::connection::SimpleConnection;

pub fn connect(database_url: &str) -> PgConnection {
    PgConnection::establish(&database_url).unwrap_or_else(|err| {panic!("Fatal Error! {}", err)})
}