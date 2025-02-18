mod database;
mod config;
mod ping;
mod response;

use std::io::Error;
use config::{load_config, Config};
use std::io::{Read, Write};
use std::str::FromStr;
use crate::database::{connect, fetch_servers, update_server};
use crate::ping::ping_server;
use crate::response::Server;

#[tokio::main]
async fn main() {
    // Handle config file
    let config_file: String = std::env::args().nth(1).unwrap_or("config.toml".to_string());
    let config: &Config = &load_config(config_file);

    // Create database URL
    let database_url = format!("postgresql://{}:{}@{}:{}/{}",
                               config.database.user,
                               config.database.password,
                               config.database.url,
                               config.database.port,
                               config.database.table);

    let mut conn = connect(database_url.as_str()).await;
    let servers = fetch_servers(&mut conn).await;

    for address in servers {
        println!("Pinging: {}", &address);

        let server_response: Result<Server, Error> = ping_server(&address, 25565u16).await;

        if let Ok(server) = server_response {
            if let Err(error) = update_server(&server, &mut conn, address.as_str()).await {
                println!("Failed to update server: {}", error);
            } else {
                println!("Server: {} updated", address);
            }
        }
    }
}