mod database;
mod config;
mod ping;
mod response;
mod colors;

use colors::{GREEN, RED, RESET, YELLOW};
use database::{connect, fetch_servers, update_server};
use ping::ping_server;
use response::parse_response;
use config::{load_config, Config};
use indicatif::{ProgressIterator, ProgressStyle};

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

    let port_start = config.rescanner.port_range_start;
    let port_end = config.rescanner.port_range_end;
    let total_ports = config.rescanner.total_ports();

    if total_ports > 24 {
        println!("{RED}[WARN] Large amount of ports! Scans will take exponentially longer for each port to scan!{RESET}");
    }

    if !config.rescanner.repeat {
        println!("{YELLOW}[WARN] Repeat is not enabled in config file! Will only scan once!{RESET}");
    }

    let pool = connect(database_url.as_str()).await;
    println!("{GREEN}[INFO] Scanning port range {} - {} ({} port(s) per host){RESET}",
             &port_start,
             &port_end,
             total_ports);

    loop {
        // Query servers from database
        let servers = match fetch_servers(&pool).await {
            Ok(servers) => {
                println!("{GREEN}[INFO] Found {} servers to rescan!{RESET}", servers.len());
                servers
            },
            Err(_) => continue
        };

        // Loop over every result
        for address in (&servers).iter().progress_with_style(ProgressStyle::default_bar().progress_chars("##-")) {
            for port in port_start..=port_end {
                // Ping server
                match ping_server(&address, port).await {
                    Ok(server) => {
                        if let Ok(server) = parse_response(&server) {
                            // Update server in database
                            match update_server(server, &pool, &address).await {
                                Ok(_) => (),
                                Err(e) => println!("{RED}{e}{RESET}")
                            }
                        }
                    }
                    Err(_) => continue
                }
            }
        }


        println!("{GREEN}[INFO] Finished pinging all servers{RESET}");
        if !config.rescanner.repeat {
            std::process::exit(0);
        }
    }
}