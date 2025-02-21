mod database;
mod config;
mod ping;
mod response;
mod colors;

use crate::database::{connect, fetch_servers, update_server};
use crate::ping::ping_server;
use crate::response::parse_response;
use config::{load_config, Config};
use std::error::Error;
use std::io::{Read, Write};
use std::str::FromStr;
use crate::colors::{GREEN, RED, RESET, YELLOW};

#[tokio::main]
async fn main() {
    // Handle config file
    let config_file: String = std::env::args().nth(1).unwrap_or("config.toml".to_string());
    println!("{GREEN}[INFO] Using config file {}{RESET}", config_file);
    let config: &Config = &load_config(config_file);

    // Create database URL
    let database_url = format!("postgresql://{}:{}@{}:{}/{}",
                               config.database.user,
                               config.database.password,
                               config.database.url,
                               config.database.port,
                               config.database.table);

    let pool = connect(database_url.as_str()).await;

    loop {
        // Query servers from database
        let servers = match fetch_servers(&pool).await {
            Ok(servers) => servers,
            Err(_) => continue
        };

        // Loop over every result
        for address in servers {

            for port in config.rescanner.port_range_start..=config.rescanner.port_range_end {
                println!("{GREEN}[INFO] Pinging: {address}:{port}{RESET}");

                // Ping server
                match ping_server(&address, port).await {
                    Ok(server) => {
                        if let Ok(server) = parse_response(&server) {
                            // Update server in database
                            match update_server(server, &pool, &address).await {
                                Ok(_) => println!("{GREEN}[INFO] Server: {address} updated in database{RESET}"),
                                Err(e) => println!("{RED}{e}{RESET}")
                            }
                        }
                    }
                    Err(_) => continue
                }
            }
        }

        println!("Finished pinging all servers");
        if !config.rescanner.repeat {
            println!("{YELLOW}[INFO] Repeat is not enabled in config file! Exiting...{RESET}");
            std::process::exit(0);
        }
    }
}